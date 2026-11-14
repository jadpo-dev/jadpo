//! Caches in the plan (area 5, "Caches"; chunk Q9): each route's display
//! read of a query with `cache:`, with the entities its statements read,
//! and what each committed change clears.
//!
//! An entity's statements name its table as an identifier
//! (`jadpo_sql::ident`), so the entities a read depends on are those whose
//! table any of its statements names: its own, an include's, and each one
//! its policy reads. A write changes the entities its statement names, a
//! delete every entity referring to one of them too, which its references'
//! `on_delete:` may change in turn, and an erasure everything every cache
//! holds.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, Project};
use jadpo_std::caches::{Clear, RouteCache};
use jadpo_std::read::ReadPlan;
use jadpo_std::write::{PurgePlan, WritePlan};

use crate::queries::QueryPlan;

/// The application's entities' tables, and what refers to each.
pub(crate) struct Tables {
    /// Each entity's name and its table as a statement names it.
    tables: Vec<(String, String)>,
    /// Each entity's name, with the entities whose references point at it.
    referrers: BTreeMap<String, BTreeSet<String>>,
}

impl Tables {
    pub(crate) fn of(db: &dyn Db, project: Project) -> Tables {
        let model = jadpo_check::data::data_model(db, project);
        let mut referrers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for entity in &model.entities {
            for field in &entity.fields {
                if let Some(reference) = &field.reference {
                    referrers
                        .entry(reference.target.clone())
                        .or_default()
                        .insert(entity.name.clone());
                }
            }
        }
        Tables {
            tables: model
                .entities
                .iter()
                .map(|entity| (entity.name.clone(), jadpo_sql::ident(&entity.table)))
                .collect(),
            referrers,
        }
    }

    /// The entities whose tables `statements` name, in name order.
    fn named_in<'s>(&self, statements: impl IntoIterator<Item = &'s str>) -> BTreeSet<String> {
        let statements: Vec<&str> = statements.into_iter().collect();
        self.tables
            .iter()
            .filter(|(_, table)| statements.iter().any(|sql| sql.contains(table.as_str())))
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// `entities` with every entity referring to one of them, and to
    /// those, to the end.
    fn with_referrers(&self, mut entities: BTreeSet<String>) -> BTreeSet<String> {
        let mut pending: Vec<String> = entities.iter().cloned().collect();
        while let Some(entity) = pending.pop() {
            for referrer in self.referrers.get(&entity).into_iter().flatten() {
                if entities.insert(referrer.clone()) {
                    pending.push(referrer.clone());
                }
            }
        }
        entities
    }

    /// The entities `read` depends on.
    pub(crate) fn read_by(&self, read: &ReadPlan) -> Vec<String> {
        self.named_in(read.statements()).into_iter().collect()
    }
}

/// The cache a route's display read of `query` keeps, when the query says
/// `cache:` and reads no sensitive field, whose every read is recorded.
pub(crate) fn route_cache(
    db: &dyn Db,
    project: Project,
    tables: &Tables,
    query: &QueryPlan,
) -> Option<RouteCache> {
    let cache = jadpo_check::caches::query_cache(db, project, &query.name)?;
    if query.read.reads_sensitive() {
        return None;
    }
    // The read plan's every part, statements and codecs alike: a build that
    // reads differently keeps its entries apart.
    let digest = blake3::hash(format!("{:?}", query.read).as_bytes()).to_hex();
    Some(RouteCache {
        store: cache.store,
        keep_ms: cache.keep_ms,
        tags: tables.read_by(&query.read),
        key: format!(
            "{} {}",
            query.key.as_str(),
            digest.get(..32).unwrap_or(digest.as_str())
        ),
    })
}

/// What the application's caches read: each cache's entities, by store.
pub(crate) struct Caching<'t> {
    tables: &'t Tables,
    by_store: BTreeMap<String, BTreeSet<String>>,
    longest: Option<u64>,
}

impl<'t> Caching<'t> {
    /// From every cache the application keeps.
    pub(crate) fn of(
        tables: &'t Tables,
        caches: impl IntoIterator<Item = RouteCache>,
    ) -> Caching<'t> {
        let mut by_store: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut longest = None;
        for cache in caches {
            by_store
                .entry(cache.store.clone())
                .or_default()
                .extend(cache.tags.iter().cloned());
            longest = longest.max(Some(cache.keep_ms));
        }
        Caching {
            tables,
            by_store,
            longest,
        }
    }

    /// The longest `keep:` of any cache: how long a write's revision
    /// cookie lasts; `None` with no cache.
    pub(crate) fn longest_keep(&self) -> Option<u64> {
        self.longest
    }

    /// What a change to `changed` clears, by store.
    fn clears(&self, changed: &BTreeSet<String>) -> Vec<Clear> {
        self.by_store
            .iter()
            .filter_map(|(store, read)| {
                let tags: Vec<String> = read.intersection(changed).cloned().collect();
                (!tags.is_empty()).then(|| Clear {
                    store: store.clone(),
                    tags,
                })
            })
            .collect()
    }

    /// Everything every cache holds: what an erasure clears.
    pub(crate) fn everything(&self) -> Vec<Clear> {
        self.by_store
            .iter()
            .map(|(store, read)| Clear {
                store: store.clone(),
                tags: read.iter().cloned().collect(),
            })
            .collect()
    }

    /// What `write` clears once it commits.
    pub(crate) fn of_write(&self, write: &WritePlan) -> Vec<Clear> {
        if self.by_store.is_empty() {
            return Vec::new();
        }
        if !write.erased.is_empty() {
            return self.everything();
        }
        let statements = std::iter::once(write.sql.as_str()).chain(write.unpoliced.as_deref());
        let changed = self.tables.named_in(statements);
        let changed = if write.op == jadpo_std::write::WriteOp::Delete {
            self.tables.with_referrers(changed)
        } else {
            changed
        };
        self.clears(&changed)
    }

    /// What a batch of `purge` clears once it commits.
    pub(crate) fn of_purge(&self, purge: &PurgePlan) -> Vec<Clear> {
        if self.by_store.is_empty() {
            return Vec::new();
        }
        let changed = BTreeSet::from([purge.entity.clone()]);
        self.clears(&self.tables.with_referrers(changed))
    }
}

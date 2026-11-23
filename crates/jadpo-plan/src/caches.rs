//! Caches in the plan (area 5, "Caches"; chunks Q9 and Q9b): each route's
//! display read of a query with `cache:`, with the entities its statements
//! read, or of one record by `id` through its entity's record cache, and
//! what each committed change clears.
//!
//! An entity's statements name its table as an identifier
//! (`jadpo_sql::ident`), so the entities a read depends on are those whose
//! table any of its statements names: its own, an include's, and each one
//! its policy reads. A write changes the entities its statement names, a
//! delete every entity referring to one of them too, which its references'
//! `on_delete:` may change in turn, and an erasure everything every cache
//! holds.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_check::data::EntityModel;
use jadpo_check::data::queries::{QueryModel, Source as QuerySource};
use jadpo_check::policy::Effect;
use jadpo_db::{Db, Project};
use jadpo_sql::statement as text;
use jadpo_std::caches::{Clear, KeptField, RecordRead, RouteCache};
use jadpo_std::read::{Cardinality, Field, ReadPlan, Record, Select, Source, SqlType};
use jadpo_std::redis::Element;
use jadpo_std::write::{PurgePlan, WritePlan};

use crate::queries::{Columns, Planner, QueryPlan, Reach, Statement, borns};

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

/// The cache a route's display read of `query` keeps: the query's own,
/// when it says `cache:` and reads no sensitive field, whose every read is
/// recorded; else its entity's record cache, when the query finds one
/// record by `id` and gives only what that keeps.
pub(crate) fn route_cache(
    db: &dyn Db,
    project: Project,
    tables: &Tables,
    query: &QueryPlan,
) -> Option<RouteCache> {
    let Some(cache) = jadpo_check::caches::query_cache(db, project, &query.name) else {
        let records = query.records.as_deref()?;
        let cache = jadpo_check::caches::record_cache(db, project, &records.entity)?;
        // What a copy is read with and holds: every query of the entity
        // shares its copies, and a build that keeps records differently
        // reads none of an earlier one's.
        let digest =
            blake3::hash(format!("{:?}", (&records.fill, &records.fields)).as_bytes()).to_hex();
        return Some(RouteCache {
            key: format!(
                "{} records {}",
                records.entity,
                digest.get(..32).unwrap_or(digest.as_str())
            ),
            records: Some(Box::new(records.clone())),
            ..entity_cache(&records.entity, &cache)
        });
    };
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
        records: None,
    })
}

/// What entity `entity`'s record cache `cache` reads, as what a change to
/// the entity clears: its records, in its store.
fn entity_cache(entity: &str, cache: &jadpo_check::caches::RecordCache) -> RouteCache {
    RouteCache {
        store: cache.store.clone(),
        keep_ms: cache.keep_ms,
        tags: vec![entity.to_string()],
        key: String::new(),
        records: None,
    }
}

/// Every entity's record cache, whether or not a route reads it, since
/// each write of the entity clears it.
pub(crate) fn record_caches(db: &dyn Db, project: Project) -> Vec<RouteCache> {
    jadpo_check::data::entity_names(db, project)
        .iter()
        .filter_map(|entity| {
            let cache = jadpo_check::caches::record_cache(db, project, entity)?;
            Some(entity_cache(entity, &cache))
        })
        .collect()
}

impl Planner<'_> {
    /// How a route's display read of `query`, a query of `entity`, is
    /// served from the entity's record cache, when it is (area 5, "A record
    /// cache"; chunk Q9b): the `id` it looks for, the fields a copy keeps,
    /// the entity's `read:` grants as a record read whole is tested, what
    /// the record becomes, and the read that fills a copy.
    pub(crate) fn record_read(
        &mut self,
        query: &QueryModel,
        entity: &EntityModel,
    ) -> Option<RecordRead> {
        let (db, project) = (self.lookup.db(), self.lookup.project());
        let (cache, value) = jadpo_check::caches::record_lookup(db, project, query)?;
        let id = self.redis_given(entity, "id", value)?;
        let element = Element::Picked {
            ty: query.element.ty.clone(),
            fields: query
                .element
                .fields
                .iter()
                .map(|(name, source)| match source {
                    QuerySource::Column(from) => Some((name.clone(), from.clone())),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?,
        };
        let mut fields = Vec::new();
        for name in &cache.kept {
            let keyed = self.keyed(entity, name)?;
            fields.push(KeptField {
                field: keyed.field,
                born: keyed.born,
                codec: keyed.codec,
            });
        }
        let fill = self.record_fill(entity, &cache.kept)?;
        Some(RecordRead {
            entity: entity.name.clone(),
            id,
            fields,
            grants: self.record_grants(entity, Effect::Read),
            element,
            fill: Box::new(fill),
        })
    }

    /// The read that fills a copy of one of `entity`'s records: its fields
    /// `kept`, by `id`, its one argument, as `visible:` lets reads see it,
    /// with no policy, which each read of the copy applies.
    fn record_fill(&mut self, entity: &EntityModel, kept: &[String]) -> Option<ReadPlan> {
        let store = entity.store.clone()?;
        let mut columns = Columns::default();
        let mut fields = Vec::new();
        for field in entity
            .fields
            .iter()
            .filter(|field| kept.contains(&field.name))
        {
            let kind = self.column(entity, field)?;
            let at = columns.add(&field.name, kind);
            fields.push((field.name.clone(), Field::Column(at)));
        }
        let mut statement = Statement::default();
        let id = statement.param(
            Source::Arg {
                index: 0,
                path: Vec::new(),
            },
            SqlType::Uuid,
        );
        let mut conditions = vec![text::equal("id", &id)];
        conditions.extend(self.access(&mut statement, entity, Reach::Read)?);
        let mut sql = text::select(&borns(entity, &columns.names), &entity.table, "");
        text::where_clause(&mut sql, &conditions, "");
        let held = self.rules_held(entity, &columns.names, &columns.kinds);
        Some(ReadPlan {
            store,
            settings: Vec::new(),
            cardinality: Cardinality::Optional,
            select: Select {
                sql,
                unpoliced: None,
                permitted: None,
                params: statement.into_params(),
                columns: columns.kinds,
                held,
                record: Record {
                    ty: entity.name.clone(),
                    fields,
                },
                includes: Vec::new(),
                versions: None,
                sensitive: None,
            },
            page: None,
            missing: None,
            recheck: None,
            layer: false,
            decided: None,
            gate: None,
            redis: None,
            every: None,
        })
    }
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

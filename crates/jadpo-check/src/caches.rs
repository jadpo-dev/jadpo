//! Caches (area 5, "Caches"; area 16, "Cache drivers and what a cache may
//! hold"; chunk Q9): a query's `cache:`, and each `Driver.memory`
//! connection.
//!
//! ```text
//! query category_page(category_id: Category.id) -> List<ProductCard> {
//!     from: Product
//!     where: Product.category_id == category_id
//!     cache: {
//!         store: cache
//!         keep: 10m
//!     }
//! }
//! ```
//!
//! `store:` names a cache connection: `Driver.redis`, or `Driver.memory`,
//! each server's own memory, which is declinable (`connection.memory_cache`)
//! since a change on one server doesn't clear another's. `keep:` is the
//! oldest an entry can be, from a second to a day. A query whose records
//! give a sensitive field, whose every read is recorded, or a `File`,
//! which goes out as a link that lasts five minutes, can't be cached; any
//! other problem with the block is `query.cache_invalid`.

use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{AstNode, Decl, MemberItem};

use crate::data::ConnectionKind;
use crate::data::queries::{Element, Include, QueryModel, Source};
use jadpo_db::{ItemKind, item_tree};

use crate::lookups::{Lookup, Name, NameView};
use crate::tree::{self, name_text, node_span};
use crate::types::Const;

/// What a query's `cache:` holds.
const SETTINGS: [&str; 2] = ["store", "keep"];

const REASON: &str = "A query's `cache:` keeps its results for the routes that run it: `store:` names a cache connection, one with `driver: Driver.redis` or `driver: Driver.memory`, and `keep:` the oldest an entry can be, from `1s` to `1d`.";

/// A query's `cache:`, read cleanly.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct QueryCache {
    /// The cache connection.
    pub store: String,
    /// `keep:`, in milliseconds.
    pub keep_ms: u64,
}

/// One query's `cache:` as read: what it says, when it reads cleanly, and
/// the problems with it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CacheRead {
    pub cache: Option<QueryCache>,
    pub problems: Vec<Diagnostic>,
}

/// The `cache:` of the query called `name`, read.
#[salsa::tracked(returns(ref))]
pub fn cache_read<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> CacheRead {
    let mut out = CacheRead::default();
    let lookup = Lookup::new(db, project);
    let Some(symbol) = lookup
        .declared(name.text(db))
        .iter()
        .find(|symbol| symbol.kind == ItemKind::Query)
    else {
        return out;
    };
    let Some(file) = crate::lookups::file_at(db, project, Name::new(db, symbol.path.clone()))
    else {
        return out;
    };
    let Some(source) = tree::source(db, file) else {
        return out;
    };
    let Some(Decl::QueryDecl(decl)) = tree::decl_at(&source, symbol.index) else {
        return out;
    };
    let path = symbol.path.as_str();
    let query = name.text(db);
    let Some(member) = decl
        .member_list()
        .iter()
        .flat_map(|list| list.member_items())
        .find_map(|item| match item {
            MemberItem::Member(member)
                if member
                    .member_key()
                    .and_then(|key| key.names().next())
                    .and_then(|name| tree::name_text(&name))
                    .as_deref()
                    == Some("cache") =>
            {
                Some(member)
            }
            _ => None,
        })
    else {
        return out;
    };
    let key = member.member_key().map_or_else(
        || node_span(path, member.syntax()),
        |key| node_span(path, key.syntax()),
    );
    let Some(block) = member.block() else {
        out.problems.push(
            invalid(
                format!("`{query}`'s `cache:` holds no block"),
                key,
                "not a block",
            )
            .next("Write `cache: { store: <connection> keep: 10m }`"),
        );
        return out;
    };
    let connections = crate::data::connections(db, project);
    let (mut store, mut keep) = (None, None);
    let mut shaped = true;
    // The settings a misspelt one was taken to mean, which aren't then
    // reported missing too.
    let mut meant: Vec<&str> = Vec::new();
    for (setting, entry) in tree::members(Some(block)) {
        let word = name_text(&setting).unwrap_or_default();
        let setting_span = node_span(path, setting.syntax());
        let value_span = entry.value().map_or_else(
            || setting_span.clone(),
            |value| node_span(path, value.syntax()),
        );
        match word.as_str() {
            "store" => {
                let written = entry
                    .value()
                    .map(|value| tree::written(value.syntax()))
                    .unwrap_or_default();
                match connections.kind(&written) {
                    Some(ConnectionKind::Redis | ConnectionKind::Memory) => store = Some(written),
                    found => {
                        shaped = false;
                        let caches: Vec<&str> = connections
                            .declared
                            .iter()
                            .filter(|(_, kind)| {
                                matches!(kind, ConnectionKind::Redis | ConnectionKind::Memory)
                            })
                            .map(|(name, _)| name.as_str())
                            .collect();
                        let what = match found {
                            Some(_) => format!("`{written}` isn't a cache connection"),
                            None => {
                                format!("`app.jadpo` declares no connection called `{written}`")
                            }
                        };
                        let problem = invalid(
                            format!(
                                "`{query}` keeps its cache in `{written}`, which isn't a cache"
                            ),
                            value_span.clone(),
                            "not a cache",
                        )
                        .reason(format!("{REASON} {what}."));
                        out.problems.push(match (nearest(&written, caches.iter().copied()), caches.first()) {
                            (Some(near), _) => problem
                                .next(format!("Use `store: {near}`"))
                                .edit(value_span, near),
                            (None, Some(first)) => problem
                                .next(format!("Name a cache connection, such as `store: {first}`")),
                            (None, None) => problem.next(
                                "Declare a connection with `driver: Driver.redis` in `app.jadpo`, and name it in `store:`",
                            ),
                        });
                    }
                }
            }
            "keep" => {
                let millis = entry.value().and_then(|value| {
                    let Const::Duration(_) = Const::from_expr(&value) else {
                        return None;
                    };
                    let millis = jadpo_std::Dur::parse(&tree::written(value.syntax()))?.millis;
                    u64::try_from(millis).ok()
                });
                match millis {
                    Some(millis)
                        if (jadpo_std::caches::LEAST_KEEP_MS..=jadpo_std::caches::MOST_KEEP_MS)
                            .contains(&millis) =>
                    {
                        keep = Some(millis);
                    }
                    _ => {
                        shaped = false;
                        out.problems.push(
                            invalid(
                                format!("`{query}`'s `keep:` must be a time from `1s` to `1d`"),
                                value_span,
                                "not from 1s to 1d",
                            )
                            .next("Write how old an entry may get, such as `keep: 10m`"),
                        );
                    }
                }
            }
            other => {
                shaped = false;
                let problem = invalid(
                    format!("`{other}` isn't a cache setting"),
                    setting_span.clone(),
                    "not a setting",
                );
                out.problems.push(match nearest(other, SETTINGS) {
                    Some(near) => {
                        meant.push(near);
                        problem
                            .next(format!("Use `{near}:`"))
                            .edit(setting_span, near)
                    }
                    None => problem.next(format!("Remove `{other}:`")),
                });
            }
        }
    }
    for (setting, given) in [("store", store.is_some()), ("keep", keep.is_some())] {
        let written = tree::members(member.block())
            .iter()
            .any(|(name, _)| name_text(name).as_deref() == Some(setting));
        if !given && !written && !meant.contains(&setting) {
            shaped = false;
            out.problems.push(
                invalid(
                    format!("`{query}`'s `cache:` doesn't give `{setting}:`"),
                    key.clone(),
                    "incomplete",
                )
                .next(match setting {
                    "store" => "Add `store: <connection>`, a cache connection",
                    _ => "Add `keep:`, how old an entry may get, such as `keep: 10m`",
                }),
            );
        }
    }
    if let Some(model) = crate::data::queries::query_model(db, project, name).as_deref()
        && let Some((field, why)) = uncacheable(db, project, &lookup, model)
    {
        shaped = false;
        out.problems.push(
            invalid(
                format!("`{query}` can't be cached: its records give `{field}`"),
                key.clone(),
                "can't be kept",
            )
            .reason(format!("{REASON} {why}"))
            .next(format!(
                "Remove `cache:` from `{query}`, or leave `{field}` out of what it gives"
            )),
        );
    }
    if let (Some(store), Some(keep_ms), true) = (store, keep, shaped) {
        out.cache = Some(QueryCache { store, keep_ms });
    }
    out
}

/// The first field `model`'s records give that a cache can't keep, and
/// why: a sensitive field, whose every read is recorded (area 15,
/// "Sensitive fields"), or a `File`, which goes out as a link that lasts
/// five minutes (area 16, "Downloading").
fn uncacheable(
    db: &dyn Db,
    project: Project,
    lookup: &Lookup<'_>,
    model: &QueryModel,
) -> Option<(String, &'static str)> {
    fn walk(
        db: &dyn Db,
        project: Project,
        lookup: &Lookup<'_>,
        entity: &str,
        element: &Element,
        includes: &[Include],
    ) -> Option<(String, &'static str)> {
        let entities = crate::data::data_model(db, project);
        let found = entities.entities.iter().find(|e| e.name == entity)?;
        let marks = crate::ai_personal::marks(db, project);
        for (_, source) in &element.fields {
            let field = match source {
                Source::Column(field) => field,
                Source::Entry {
                    field: Some(field), ..
                } => field,
                _ => continue,
            };
            let Some(model) = found.fields.iter().find(|f| f.name == *field) else {
                continue;
            };
            let named = format!("{entity}.{field}");
            if model.file.is_some() {
                return Some((
                    named,
                    "A `File` goes out as a signed link that lasts five minutes, which a cache would keep long after it ends.",
                ));
            }
            let category = marks
                .mark(entity, field)
                .and_then(|mark| mark.category.clone())
                .or_else(|| {
                    crate::ai_personal::standard_personal(lookup, &model.ty)
                        .first()
                        .map(|(_, category)| category.clone())
                });
            if category.is_some_and(|category| crate::ai_personal::is_sensitive(&category)) {
                return Some((
                    named,
                    "A sensitive field is never put in a cache, since every read of one is recorded in the access records.",
                ));
            }
        }
        for include in includes {
            if let Some(found) = walk(
                db,
                project,
                lookup,
                &include.entity,
                &include.element,
                &include.includes,
            ) {
                return Some(found);
            }
        }
        None
    }
    walk(
        db,
        project,
        lookup,
        &model.entity,
        &model.element,
        &model.includes,
    )
}

/// The caches that may hold a copy of `entity.field`, by connection, in
/// name order: each cached query whose records give it, directly or
/// through an include (area 15, "Where personal data goes"; chunk Q9).
pub fn caches_holding(db: &dyn Db, project: Project, entity: &str, field: &str) -> Vec<String> {
    fn gives(
        entity: &str,
        field: &str,
        from: &str,
        element: &Element,
        includes: &[Include],
    ) -> bool {
        (from == entity
            && element.fields.iter().any(|(_, source)| match source {
                Source::Column(name) => name == field,
                Source::Entry {
                    field: Some(name), ..
                } => name == field,
                _ => false,
            }))
            || includes.iter().any(|include| {
                gives(
                    entity,
                    field,
                    &include.entity,
                    &include.element,
                    &include.includes,
                )
            })
    }
    let mut out: Vec<String> = crate::data::queries::query_models(db, project)
        .queries
        .iter()
        .filter(|query| {
            gives(
                entity,
                field,
                &query.entity,
                &query.element,
                &query.includes,
            )
        })
        .filter_map(|query| query_cache(db, project, &query.name).map(|cache| cache.store))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The `cache:` of the query called `name`, when it reads cleanly.
pub fn query_cache(db: &dyn Db, project: Project, name: &str) -> Option<QueryCache> {
    cache_read(db, project, Name::new(db, name.to_string()))
        .cache
        .clone()
}

/// The problems with the `cache:` of each query `file` declares.
#[salsa::tracked(returns(ref))]
pub fn cache_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let lookup = Lookup::new(db, project);
    for item in &item_tree(db, file).items {
        if item.kind != ItemKind::Query || !seen.insert(item.name.as_str()) {
            continue;
        }
        let declared_here = lookup
            .declared(&item.name)
            .iter()
            .find(|symbol| symbol.kind == ItemKind::Query)
            .is_some_and(|symbol| symbol.path == *path);
        if !declared_here {
            continue;
        }
        out.extend(
            cache_read(db, project, Name::new(db, item.name.clone()))
                .problems
                .iter()
                .cloned(),
        );
    }
    out
}

/// Each connection with `driver: Driver.memory`, refused unless declined
/// (`connection.memory_cache`): each server keeps a cache of its own,
/// emptied on every restart, and a change one server clears isn't cleared
/// on another. One that gives a setting is `connection.key_invalid`, since
/// it takes none.
#[salsa::tracked(returns(ref))]
pub fn memory_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for found in crate::config::singletons(db, project, "application") {
        let path = &found.path;
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("connections") {
                continue;
            }
            for (name, entry) in tree::members(member.block()) {
                let Some(connection) = name_text(&name) else {
                    continue;
                };
                if crate::data::connection_kind(entry.block()) != ConnectionKind::Memory {
                    continue;
                }
                for (setting, _) in tree::members(entry.block()) {
                    let word = name_text(&setting).unwrap_or_default();
                    if word == "driver" {
                        continue;
                    }
                    out.push(
                        Diagnostic::new(
                            &rule::CONNECTION_KEY_INVALID,
                            format!("A connection with `driver: Driver.memory` takes no `{word}:`"),
                        )
                        .at(node_span(path, setting.syntax()), "not a setting")
                        .reason("`Driver.memory` keeps a cache in each server's own memory, which needs no address and no other setting.")
                        .next(format!("Remove `{word}:`")),
                    );
                }
                out.push(
                    Diagnostic::new(
                        &rule::CONNECTION_MEMORY_CACHE,
                        format!("`{connection}` keeps a cache in each server's own memory"),
                    )
                    .at(node_span(path, name.syntax()), "each server's own cache")
                    .reason(format!(
                        "`{connection}` has `driver: Driver.memory`, so each server keeps a cache of its own, lost on every restart, and a change one server clears from its cache is still served by the others until their entries reach their `keep:`. One server, or a Redis connection, keeps every copy current."
                    ))
                    .next(format!(
                        "Give `{connection}` `driver: Driver.redis` and a `url:`, which every server shares"
                    )),
                );
            }
        }
    }
    out
}

fn invalid(message: impl Into<String>, at: Span, label: &str) -> Diagnostic {
    Diagnostic::new(&rule::QUERY_CACHE_INVALID, message)
        .at(at, label)
        .reason(REASON)
}

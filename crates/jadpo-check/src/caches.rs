//! Caches (area 5, "Caches"; area 16, "Cache drivers and what a cache may
//! hold"; chunks Q9 and Q9b): a query's `cache:`, an entity's `caches: {
//! records: }`, and each `Driver.memory` connection.
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
//!
//! An entity's record cache keeps each record whole, by `id`, one copy for
//! every caller, with the same two settings (`data.cache_invalid`). It
//! leaves out a sensitive field, an encrypted one and a `File`
//! ([`left_out`]), and serves a route's query that finds one record by
//! `id` and gives only what it keeps ([`record_lookup`]). Policy is applied
//! to the copy each time it is read, by what the record holds, so a role,
//! a tenant, or a user field it leaves out is refused
//! ([`record_cache_read`]).

use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, Rule, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Decl, MemberItem};

use crate::data::queries::{Compare, Condition, Element, Include, QueryModel, Source, Value};
use crate::data::{ConnectionKind, Connections, EntityModel, FieldModel};
use jadpo_db::{ItemKind, item_tree};

use crate::lookups::{Lookup, Name, NameView};
use crate::policy::{Effect, Grant};
use crate::tree::{self, name_text, node_span};
use crate::types::Const;

/// What a query's `cache:` and an entity's `records:` hold.
const SETTINGS: [&str; 2] = ["store", "keep"];

const REASON: &str = "A query's `cache:` keeps its results for the routes that run it: `store:` names a cache connection, one with `driver: Driver.redis` or `driver: Driver.memory`, and `keep:` the oldest an entry can be, from `1s` to `1d`.";

const RECORDS_REASON: &str = "An entity's `caches: { records: }` keeps a copy of each of its records, by `id`, for the routes that read one: `store:` names a cache connection, one with `driver: Driver.redis` or `driver: Driver.memory`, and `keep:` the oldest a copy can be, from `1s` to `1d`.";

/// What an entity's `caches:` may keep.
const CACHES: [&str; 1] = ["records"];

/// A query's `cache:`, or an entity's `records:`, read cleanly.
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

/// Whose cache a block of `store:` and `keep:` is, in the words its
/// problems use: a query's `cache:` or an entity's `records:`.
struct Owner {
    rule: &'static Rule,
    reason: &'static str,
    /// `` `products`'s `cache:` ``.
    block: String,
    /// `` `products` keeps its cache ``.
    keeps: String,
    /// `` `products`'s ``.
    whose: String,
    /// What one thing it keeps is: `an entry`.
    copy: &'static str,
    /// A `keep:` to suggest: `10m`.
    example: &'static str,
    /// How the whole block is written.
    shape: &'static str,
}

impl Owner {
    fn invalid(&self, message: impl Into<String>, at: Span, label: &str) -> Diagnostic {
        Diagnostic::new(self.rule, message)
            .at(at, label)
            .reason(self.reason)
    }
}

/// `member`, a block of `store:` and `keep:` whose key is at `key`, read
/// for `owner`: what it says when it reads cleanly, its problems pushed to
/// `out` otherwise.
fn settings(
    owner: &Owner,
    path: &str,
    connections: &Connections,
    member: &ast::Member,
    key: &Span,
    out: &mut Vec<Diagnostic>,
) -> Option<QueryCache> {
    let Some(block) = member.block() else {
        out.push(
            owner
                .invalid(
                    format!("{} holds no block", owner.block),
                    key.clone(),
                    "not a block",
                )
                .next(owner.shape),
        );
        return None;
    };
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
                        let problem = owner
                            .invalid(
                                format!("{} in `{written}`, which isn't a cache", owner.keeps),
                                value_span.clone(),
                                "not a cache",
                            )
                            .reason(format!("{} {what}.", owner.reason));
                        out.push(match (nearest(&written, caches.iter().copied()), caches.first()) {
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
                        out.push(
                            owner
                                .invalid(
                                    format!(
                                        "{} `keep:` must be a time from `1s` to `1d`",
                                        owner.whose
                                    ),
                                    value_span,
                                    "not from 1s to 1d",
                                )
                                .next(format!(
                                    "Write how old {} may get, such as `keep: {}`",
                                    owner.copy, owner.example
                                )),
                        );
                    }
                }
            }
            other => {
                shaped = false;
                let problem = owner.invalid(
                    format!("`{other}` isn't a cache setting"),
                    setting_span.clone(),
                    "not a setting",
                );
                out.push(match nearest(other, SETTINGS) {
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
            out.push(
                owner
                    .invalid(
                        format!("{} doesn't give `{setting}:`", owner.block),
                        key.clone(),
                        "incomplete",
                    )
                    .next(match setting {
                        "store" => "Add `store: <connection>`, a cache connection".to_string(),
                        _ => format!(
                            "Add `keep:`, how old {} may get, such as `keep: {}`",
                            owner.copy, owner.example
                        ),
                    }),
            );
        }
    }
    match (store, keep, shaped) {
        (Some(store), Some(keep_ms), true) => Some(QueryCache { store, keep_ms }),
        _ => None,
    }
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
    let owner = Owner {
        rule: &rule::QUERY_CACHE_INVALID,
        reason: REASON,
        block: format!("`{query}`'s `cache:`"),
        keeps: format!("`{query}` keeps its cache"),
        whose: format!("`{query}`'s"),
        copy: "an entry",
        example: "10m",
        shape: "Write `cache: { store: <connection> keep: 10m }`",
    };
    let connections = crate::data::connections(db, project);
    let read = settings(&owner, path, connections, &member, &key, &mut out.problems);
    if member.block().is_none() {
        return out;
    }
    let mut shaped = read.is_some();
    if let Some(model) = crate::data::queries::query_model(db, project, name).as_deref()
        && let Some((field, why)) = uncacheable(db, project, &lookup, model)
    {
        shaped = false;
        out.problems.push(
            owner
                .invalid(
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
    if shaped {
        out.cache = read;
    }
    out
}

/// An entity's `caches:`, its key at `key`, read once its store is (area
/// 5, "A record cache"; chunk Q9b): its `records:`, when it reads cleanly,
/// its problems pushed to `out` otherwise. An entity kept in Redis keeps
/// none, since its records are there already.
pub(crate) fn records_read(
    connections: &Connections,
    path: &str,
    entity: &EntityModel,
    (member, key): (&ast::Member, &Span),
    out: &mut Vec<Diagnostic>,
) -> Option<QueryCache> {
    let name = &entity.name;
    let invalid = |message: String, at: Span, label: &str| {
        Diagnostic::new(&rule::DATA_CACHE_INVALID, message)
            .at(at, label)
            .reason(RECORDS_REASON)
    };
    if entity.redis {
        out.push(
            invalid(
                format!("`{name}` is kept in Redis already"),
                key.clone(),
                "a copy of what Redis holds",
            )
            .reason(format!(
                "`{}` is a Redis connection, read in one round trip, and a cache would only keep a second copy of each record there, which a write would then have to clear.",
                entity.store.as_deref().unwrap_or_default()
            ))
            .next(format!("Remove `caches:` from `{name}`")),
        );
        return None;
    }
    let Some(block) = member.block() else {
        out.push(
            invalid(
                format!("`{name}`'s `caches:` holds no block"),
                key.clone(),
                "not a block",
            )
            .next("Write `caches: { records: { store: <connection> keep: 1h } }`"),
        );
        return None;
    };
    let owner = Owner {
        rule: &rule::DATA_CACHE_INVALID,
        reason: RECORDS_REASON,
        block: format!("`{name}`'s `records:`"),
        keeps: format!("`{name}` caches its records"),
        whose: format!("`{name}`'s"),
        copy: "a copy",
        example: "1h",
        shape: "Write `records: { store: <connection> keep: 1h }`",
    };
    let mut found = None;
    let mut shaped = true;
    let mut records = false;
    for (kept, entry) in tree::members(Some(block)) {
        let word = name_text(&kept).unwrap_or_default();
        let at = node_span(path, kept.syntax());
        if word == "records" {
            records = true;
            found = settings(&owner, path, connections, &entry, &at, out);
            continue;
        }
        shaped = false;
        let problem = invalid(
            format!("`{word}` isn't a cache an entity keeps"),
            at.clone(),
            "not a cache",
        )
        .reason("An entity's `caches:` holds `records:`, a copy of each of its records by `id`. A query's results are kept by the query's own `cache:`.");
        out.push(match nearest(&word, CACHES) {
            Some(near) => problem.next(format!("Use `{near}:`")).edit(at, near),
            None => problem.next(format!("Remove `{word}:`")),
        });
    }
    if !records && shaped {
        out.push(
            invalid(
                format!("`{name}`'s `caches:` keeps nothing"),
                key.clone(),
                "empty",
            )
            .next("Add `records: { store: <connection> keep: 1h }`"),
        );
    }
    found.filter(|_| shaped)
}

/// An entity's record cache, once its policy can be held to a copy:
/// where, for how long, and the fields it keeps of each record.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RecordCache {
    pub store: String,
    pub keep_ms: u64,
    /// The fields it keeps, in declared order: each but those [`left_out`]
    /// names.
    pub kept: Vec<String>,
}

impl RecordCache {
    /// Where it is kept and for how long.
    pub fn settings(&self) -> QueryCache {
        QueryCache {
            store: self.store.clone(),
            keep_ms: self.keep_ms,
        }
    }
}

/// One entity's record cache as read: what it keeps, when its policy can
/// be held to a copy, and the problems with that policy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordCacheRead {
    pub cache: Option<RecordCache>,
    pub problems: Vec<Diagnostic>,
}

/// Why a record cache leaves `entity`'s field `field` out, when it does:
/// a sensitive field, whose every read is recorded (area 15, "Sensitive
/// fields"); an encrypted one, which no cache may hold opened; and a
/// `File`, which goes out as a link that lasts five minutes.
pub fn left_out(
    db: &dyn Db,
    project: Project,
    lookup: &Lookup<'_>,
    entity: &EntityModel,
    field: &FieldModel,
) -> Option<&'static str> {
    if field.file.is_some() {
        return Some("a `File`, which goes out as a link that lasts five minutes");
    }
    if field.sealed.is_some() {
        return Some("encrypted, so no cache may hold it opened");
    }
    let category = crate::ai_personal::marks(db, project)
        .mark(&entity.name, &field.name)
        .and_then(|mark| mark.category.clone())
        .or_else(|| {
            crate::ai_personal::standard_personal(lookup, &field.ty)
                .first()
                .map(|(_, category)| category.clone())
        });
    category
        .is_some_and(|category| crate::ai_personal::is_sensitive(&category))
        .then_some("sensitive, and every read of one is recorded")
}

/// The record cache of the entity called `name`, read (area 9, "Caches and
/// personal data"; chunk Q9b): one copy of each record serves every caller,
/// held to policy as it is read by what the record holds, so a role and a
/// tenant, which read memberships, and a user field the cache leaves out
/// are refused (`data.cache_invalid`).
#[salsa::tracked(returns(ref))]
pub fn record_cache_read<'db>(
    db: &'db dyn Db,
    project: Project,
    name: Name<'db>,
) -> RecordCacheRead {
    let mut out = RecordCacheRead::default();
    let Some(read) = crate::data::entity_read(db, project, name) else {
        return out;
    };
    let entity = read.model.as_ref();
    let (Some(cache), Some(at)) = (&entity.records_cache, &read.spans.caches) else {
        return out;
    };
    let lookup = Lookup::new(db, project);
    let kept: Vec<String> = entity
        .fields
        .iter()
        .filter(|field| left_out(db, project, &lookup, entity, field).is_none())
        .map(|field| field.name.clone())
        .collect();
    let entity_name = &entity.name;
    let refused = |what: String, why: &str| {
        Diagnostic::new(
            &rule::DATA_CACHE_INVALID,
            format!("`{entity_name}`'s records can't be cached: {what}"),
        )
        .at(at.clone(), "can't apply its policy")
        .reason(format!(
            "A record cache keeps one copy of each record for every caller, and applies policy to it each time it is read, by what the record holds: `Access.public`, `Access.authenticated`, `Access.system` and a user field. {why}"
        ))
        .next(format!(
            "Remove `caches:` from `{entity_name}`, and give the query a route reads it through a `cache:` of its own, kept per caller"
        ))
    };
    if let Some(policy) = crate::policy::policy_read(db, project, name) {
        let grants = policy.model.grants(Effect::Read);
        if grants
            .iter()
            .any(|grant| matches!(grant, Grant::Role { .. }))
        {
            out.problems.push(refused(
                "its `read:` grants name a role".to_string(),
                "A role is a membership, read from the database as policy is checked.",
            ));
        }
        if policy.model.tenancy.is_some() {
            out.problems.push(refused(
                "it belongs to a tenant".to_string(),
                "A tenant's records are reached through the caller's memberships, read from the database as policy is checked.",
            ));
        }
        for grant in grants {
            if let Grant::Caller { field, .. } = grant
                && !kept.contains(field)
            {
                out.problems.push(refused(
                    format!("its `read:` grants go through `{field}`, which it leaves out"),
                    "A sensitive field is never put in a cache, so a copy can't say whose it is.",
                ));
            }
        }
    }
    if out.problems.is_empty() {
        out.cache = Some(RecordCache {
            store: cache.store.clone(),
            keep_ms: cache.keep_ms,
            kept,
        });
    }
    out
}

/// The record cache of the entity called `entity`, when it has one whose
/// policy a copy can be held to.
pub fn record_cache(db: &dyn Db, project: Project, entity: &str) -> Option<RecordCache> {
    record_cache_read(db, project, Name::new(db, entity.to_string()))
        .cache
        .clone()
}

/// The value `query` looks one record up by, and the record cache a
/// route's display read of it is served from (area 5, "A record cache";
/// chunk Q9b): a query of an entity with a record cache, with no `cache:`
/// of its own, that finds at most one record by `where: Entity.id ==
/// <argument>` alone, with no include, and gives only fields the cache
/// keeps. Any other read goes to the store.
pub fn record_lookup<'q>(
    db: &dyn Db,
    project: Project,
    query: &'q QueryModel,
) -> Option<(RecordCache, &'q Value)> {
    use jadpo_std::read::Cardinality;
    let cache = record_cache(db, project, &query.entity)?;
    if query_cache(db, project, &query.name).is_some()
        || !matches!(query.cardinality, Cardinality::One | Cardinality::Optional)
        || !query.includes.is_empty()
        || query.grouping.is_some()
        || query.nearest.is_some()
        || query.distance.is_some()
        || query.geometry.is_some()
        || query.include_erased
        || query.include_hidden
        || query.limit.is_some()
        || query.after.is_some()
        || query.order.iter().any(|order| !order.added)
    {
        return None;
    }
    let [
        Condition::Compare {
            field,
            op: Compare::Equal,
            value: value @ Value::Param { .. },
        },
    ] = query.filter.as_slice()
    else {
        return None;
    };
    if field != "id" {
        return None;
    }
    let gives_kept = query.element.fields.iter().all(|(_, source)| match source {
        Source::Column(field) => cache.kept.contains(field),
        _ => false,
    });
    gives_kept.then_some((cache, value))
}

/// The problems with the record cache of each entity `file` declares whose
/// policy a copy can't be held to.
#[salsa::tracked(returns(ref))]
pub fn record_cache_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for item in &item_tree(db, file).items {
        if item.kind != ItemKind::Entity || !seen.insert(item.name.as_str()) {
            continue;
        }
        let name = Name::new(db, item.name.clone());
        if crate::data::entity_read(db, project, name)
            .as_ref()
            .is_none_or(|read| read.model.path != *path)
        {
            continue;
        }
        out.extend(
            record_cache_read(db, project, name)
                .problems
                .iter()
                .cloned(),
        );
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
/// through an include (area 15, "Where personal data goes"; chunk Q9), and
/// the entity's record cache when it keeps the field (chunk Q9b).
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
    if let Some(cache) = record_cache(db, project, entity)
        && cache.kept.iter().any(|kept| kept == field)
    {
        out.push(cache.store);
    }
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

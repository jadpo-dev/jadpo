//! Entities in Redis (area 5, "Entities in Redis"; area 6, "Reading and
//! writing entities in Redis"; chunk Q10): what an entity kept in Redis
//! may refer to and grant, which queries can read one, and what an action
//! writing one may do. What the entity declares on its own is read with
//! it ([`crate::data`]); what needs the rest of the project is here.
//!
//! - **References.** A record in Redis keeps an `id` it holds as a value,
//!   with no foreign key: a reference may point only at the principal's
//!   user or service entity, a user field policy can grant through, and
//!   never at a person, whose erasure finds their records through
//!   references only a database can search.
//! - **Policy** is applied to each record once it is read, so a grant is
//!   `Access.public`, `Access.authenticated`, `Access.system` or a user
//!   field: equality on the record's own fields. A role, a tenant and a
//!   field's own `policy:` read records a database keeps.
//! - **Queries** ([`lookup_of`], which the plan reads too) find records by
//!   `id`, by a unique field, or by equality on one field `indexes:`
//!   lists, a page in creation order or its count. Anything else is
//!   `query.redis_unsupported`, naming PostgreSQL.
//! - **An action writing one** commits its writes in Redis, so it emits
//!   no event, keeps no record of several changing calls, and no batch
//!   walks a query of one: each of those is kept in a database, in the
//!   same transaction as the writes.

use jadpo_db::{Db, File, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::Kind;
use jadpo_syntax::ast::{self, AstNode, Decl, MemberItem};

use crate::data::queries::{
    AggregateKind, Cardinality, Compare, Condition, Entry, QueryModel, Value,
};
use crate::data::{EntityModel, data_model, redis_unsupported};
use crate::lookups::{Lookup, Name, NameView};
use crate::policy::{Effect, Grant};
use crate::tree::{self, node_span};

/// How a query of an entity in Redis finds its records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedisLookup<'q> {
    /// By `id`.
    Id(&'q Value),
    /// By the key kept for a unique field's value.
    Unique { field: &'q str, value: &'q Value },
    /// The records holding a value in a field `indexes:` lists, in
    /// creation order.
    Index { field: &'q str, value: &'q Value },
    /// How many records hold it: an aggregate whose one total, `total`, is
    /// `count()`.
    Count {
        field: &'q str,
        value: &'q Value,
        total: &'q str,
    },
}

/// Why a query can't read an entity in Redis: the section that asks, what
/// it asks, and why only a database can.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unserved {
    pub section: &'static str,
    pub what: String,
    pub why: &'static str,
}

fn unserved(section: &'static str, what: impl Into<String>, why: &'static str) -> Unserved {
    Unserved {
        section,
        what: what.into(),
        why,
    }
}

const KEYS: &str = "Jadpo keeps a key for each value of a unique field and a set for each value of a field `indexes:` lists, and finds a record by nothing else.";

/// Whether `entity` keeps a set for each value of `field`.
pub fn indexed(entity: &EntityModel, field: &str) -> bool {
    entity
        .indexes
        .iter()
        .any(|rule| rule.fields.len() == 1 && rule.fields[0] == field && rule.kind.is_none())
}

/// Whether `entity`'s field `field` has `unique: true`.
pub fn unique(entity: &EntityModel, field: &str) -> bool {
    entity
        .fields
        .iter()
        .any(|f| f.name == field && f.unique && f.name != "id")
}

/// How `query`, a query of `entity` kept in Redis, finds its records, or
/// why it can't.
pub fn lookup_of<'q>(
    entity: &EntityModel,
    query: &'q QueryModel,
) -> Result<RedisLookup<'q>, Unserved> {
    if !query.includes.is_empty() {
        return Err(unserved(
            "include",
            "an `include:`",
            "Redis keeps no references to follow: each record is read whole by its own key.",
        ));
    }
    if query.nearest.is_some() {
        return Err(unserved(
            "order",
            "a nearest-first search",
            "A vector search reads an index only PostgreSQL keeps.",
        ));
    }
    if query.distance.is_some() {
        return Err(unserved(
            "order",
            "an order by `geo.distance`",
            "Redis keeps no spatial index, and pages come only in creation order.",
        ));
    }
    if query.geometry.is_some() {
        return Err(unserved(
            "geometry",
            "a map layer",
            "A layer lists records by where they are, which only a spatial index finds.",
        ));
    }
    if query.include_erased {
        return Err(unserved(
            "include_erased",
            "`include_erased: true`",
            "Erasure keeps nothing in Redis: a record there is only ever there or gone.",
        ));
    }
    if query.order.iter().any(|order| !order.added) {
        return Err(unserved(
            "order",
            "an `order:`",
            "Pages come in creation order, by `id`, from the set Jadpo keeps for each value; Redis sorts by nothing else.",
        ));
    }
    let (field, value) = match query.filter.as_slice() {
        [] => {
            return Err(unserved(
                "from",
                "a read of every record",
                "Redis keeps no list of every record: a query finds them by `id`, by a unique field or by `==` on a field `indexes:` lists.",
            ));
        }
        [condition] => equality(condition)?,
        [_, _, ..] => {
            return Err(unserved(
                "where",
                "more than one condition",
                "Jadpo finds a record in Redis by one key or one set, so a query tests one field with `==`.",
            ));
        }
    };
    let found = if field == "id" {
        RedisLookup::Id(value)
    } else if unique(entity, field) {
        RedisLookup::Unique { field, value }
    } else if indexed(entity, field) {
        RedisLookup::Index { field, value }
    } else {
        return Err(unserved(
            "where",
            format!("`==` on `{field}`, which isn't unique or indexed"),
            KEYS,
        ));
    };
    let Some(grouping) = &query.grouping else {
        return Ok(found);
    };
    if grouping.grouped {
        return Err(unserved(
            "group",
            "a `group:`",
            "Jadpo counts the records in one set, kept for one value, and keeps no groups.",
        ));
    }
    match (grouping.entries.as_slice(), found) {
        (
            [
                (
                    total,
                    Entry::Aggregate {
                        kind: AggregateKind::Count,
                        field: None,
                    },
                ),
            ],
            RedisLookup::Index { field, value },
        ) if query.cardinality == Cardinality::One => Ok(RedisLookup::Count {
            field,
            value,
            total,
        }),
        ([_], RedisLookup::Index { .. }) | ([_, _, ..], _) | ([], _) => Err(unserved(
            "aggregate",
            "an aggregate other than one `count()`",
            "Jadpo counts the records in the set it keeps for a value, and works out nothing else from them.",
        )),
        ([_], _) => Err(unserved(
            "aggregate",
            "a count by `id` or a unique field",
            "At most one record holds a unique value, so there is nothing to count: read it instead.",
        )),
    }
}

/// Whether `condition` is a search inside text, on a `where:` or a
/// `where_present:` line.
fn searches_inside_text(condition: &Condition) -> bool {
    match condition {
        Condition::Call { call, .. } => call.op == jadpo_std::Op::TextContainsIgnoringCase,
        Condition::Present { condition, .. } => searches_inside_text(condition),
        _ => false,
    }
}

/// The field and value `condition` tests with `==`, or why Redis can't
/// test it.
fn equality(condition: &Condition) -> Result<(&str, &Value), Unserved> {
    const ORDER: &str = "Redis keeps no order of a field's values, only a set for each one, so it finds records holding a value and nothing between two.";
    match condition {
        Condition::Compare {
            field,
            op: Compare::Equal,
            value,
        } if !matches!(value, Value::List(_)) => Ok((field.as_str(), value)),
        Condition::Compare {
            op: Compare::NotEqual,
            ..
        } => Err(unserved(
            "where",
            "`!=`",
            "Jadpo keeps a set of the records holding each value, not of those that don't.",
        )),
        Condition::Compare { op, .. } if op.is_ordering() => {
            Err(unserved("where", "a range", ORDER))
        }
        Condition::Compare { .. } | Condition::In { .. } => Err(unserved(
            "where",
            "`in`",
            "Each value is a set of its own, and Jadpo reads one set for a query.",
        )),
        Condition::IsNone { .. } => Err(unserved(
            "where",
            "a test for `none`",
            "Jadpo keeps no key or set for a field that holds nothing.",
        )),
        Condition::And(_) | Condition::Or(_) | Condition::Not(_) => Err(unserved(
            "where",
            "`and`, `or` or `not`",
            "Jadpo finds a record in Redis by one key or one set, so a query tests one field with `==`.",
        )),
        // A search inside text, on either line (area 6, "Searching inside
        // text").
        Condition::Call { .. } | Condition::Present { .. } if searches_inside_text(condition) => {
            Err(unserved(
                if matches!(condition, Condition::Present { .. }) {
                    "where_present"
                } else {
                    "where"
                },
                "a search inside text, `text.contains_ignoring_case`",
                "A search inside text reads a trigram index, which only PostgreSQL keeps.",
            ))
        }
        Condition::Present { .. } => Err(unserved(
            "where_present",
            "`where_present:`",
            "A condition that applies only when its value is there reads every record when it isn't, and Redis keeps no list of them.",
        )),
        Condition::Call { .. } => Err(unserved(
            "where",
            "a standard operation, such as a `geo.` test",
            "Redis keeps no index an operation's result can be found by.",
        )),
        Condition::Matches(_) => Err(unserved(
            "where",
            "`matches`",
            "A search reads a full-text index, which only PostgreSQL keeps.",
        )),
    }
}

/// The names of the entities kept in Redis, in name order: what every
/// file's checks here read first, so a project with none, or an edit that
/// leaves them as they were, checks nothing again.
#[salsa::tracked(returns(ref))]
pub fn redis_entities(db: &dyn Db, project: Project) -> Vec<String> {
    let mut out: Vec<String> = data_model(db, project)
        .entities
        .iter()
        .filter(|entity| entity.redis)
        .map(|entity| entity.name.clone())
        .collect();
    out.sort();
    out
}

/// The problems with the queries `file` declares that read an entity kept
/// in Redis (`query.redis_unsupported`), and with a `cache:` on one, which
/// would keep a copy of what Redis already holds (`query.cache_invalid`).
#[salsa::tracked(returns(ref))]
pub fn redis_query_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let redis = redis_entities(db, project);
    if redis.is_empty() {
        return Vec::new();
    }
    let lookup = Lookup::new(db, project);
    let mut out = Vec::new();
    for item in &item_tree(db, file).items {
        if item.kind != ItemKind::Query {
            continue;
        }
        let Some(read) =
            crate::data::queries::query_read(db, project, Name::new(db, item.name.clone()))
        else {
            continue;
        };
        if read.path != *path {
            continue;
        }
        let Some(query) = read
            .model
            .as_ref()
            .filter(|query| redis.contains(&query.entity))
        else {
            continue;
        };
        let Some(entity) =
            crate::data::entity_model(db, project, Name::new(db, query.entity.clone()))
                .clone()
                .filter(|entity| entity.redis)
        else {
            continue;
        };
        let entity = entity.as_ref();
        let Some(decl) = query_decl(&lookup, db, project, &query.name) else {
            continue;
        };
        if let Err(unserved) = lookup_of(entity, query) {
            let at = section_span(path, &decl, unserved.section)
                .or_else(|| section_span(path, &decl, "from"))
                .or_else(|| decl.name().map(|name| node_span(path, name.syntax())));
            if let Some(at) = at {
                out.push(
                    Diagnostic::new(
                        &rule::QUERY_REDIS_UNSUPPORTED,
                        format!(
                            "`{}` reads `{}`, kept in Redis, with {}",
                            query.name, entity.name, unserved.what
                        ),
                    )
                    .at(at, "only a database can")
                    .reason(format!(
                        "`{}` is a Redis connection. {} A query of an entity in Redis finds records by `id`, by a field with `unique: true`, or by `==` on one field `indexes:` lists, a page in creation order or its `count()`.",
                        entity.store.as_deref().unwrap_or_default(),
                        unserved.why
                    ))
                    .next(format!(
                        "Keep `{}` in a PostgreSQL store, or find its records by `id`, a unique field or an indexed one",
                        entity.name
                    )),
                );
            }
        }
        if let Some(at) = section_span(path, &decl, "cache") {
            out.push(
                Diagnostic::new(
                    &rule::QUERY_CACHE_INVALID,
                    format!("`{}` caches `{}`, which is kept in Redis already", query.name, entity.name),
                )
                .at(at, "a copy of what Redis holds")
                .reason(format!(
                    "`{}` is a Redis connection, read in one round trip, and a cache would only keep a second copy of each record there, which a write would then have to clear.",
                    entity.store.as_deref().unwrap_or_default()
                ))
                .next(format!("Remove `cache:` from `{}`", query.name)),
            );
        }
    }
    out
}

/// The query declaration called `name`.
fn query_decl(
    lookup: &Lookup<'_>,
    db: &dyn Db,
    project: Project,
    name: &str,
) -> Option<ast::QueryDecl> {
    let symbol = lookup
        .declared(name)
        .iter()
        .find(|symbol| symbol.kind == ItemKind::Query)?;
    let file = crate::lookups::file_at(db, project, Name::new(db, symbol.path.clone()))?;
    let source = tree::source(db, file)?;
    match tree::decl_at(&source, symbol.index) {
        Some(Decl::QueryDecl(decl)) => Some(decl),
        _ => None,
    }
}

/// Where the section `word` of `decl` is written: its key.
fn section_span(path: &str, decl: &ast::QueryDecl, word: &str) -> Option<Span> {
    decl.member_list()
        .iter()
        .flat_map(|list| list.member_items())
        .find_map(|item| match item {
            MemberItem::Member(member)
                if member
                    .member_key()
                    .and_then(|key| key.names().next())
                    .and_then(|name| tree::name_text(&name))
                    .as_deref()
                    == Some(word) =>
            {
                member.member_key().map(|key| node_span(path, key.syntax()))
            }
            _ => None,
        })
}

/// What the entities kept in Redis that `file` declares refer to and
/// grant, past what each reads alone: each reference, which only the
/// principal's user or service entity may be the target of, and never a
/// person; and each grant, role, tenant and field's `policy:`.
#[salsa::tracked(returns(ref))]
pub fn redis_entity_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let redis = redis_entities(db, project);
    if !item_tree(db, file)
        .items
        .iter()
        .any(|item| item.kind == ItemKind::Entity && redis.contains(&item.name))
    {
        return Vec::new();
    }
    let identity = crate::identity::identity(db, project);
    let principals: Vec<String> = [
        jadpo_std::http::PrincipalKind::User,
        jadpo_std::http::PrincipalKind::Service,
    ]
    .into_iter()
    .filter_map(|kind| identity.variant(kind).map(|variant| variant.entity.clone()))
    .collect();
    let mut out = Vec::new();
    for item in &item_tree(db, file).items {
        if item.kind != ItemKind::Entity {
            continue;
        }
        let name = Name::new(db, item.name.clone());
        let Some(read) = crate::data::entity_read(db, project, name) else {
            continue;
        };
        let entity = &read.model;
        if read.model.path != *path || !entity.redis {
            continue;
        }
        for (field, at) in entity.fields.iter().zip(&read.spans.fields) {
            let Some(reference) = &field.reference else {
                continue;
            };
            let target =
                crate::data::entity_model(db, project, Name::new(db, reference.target.clone()));
            if target.as_ref().is_some_and(|target| target.person) {
                out.push(redis_unsupported(
                    entity,
                    at.clone(),
                    "a reference to a person",
                    &format!("`{}`, a reference to `{}`, a person", field.name, reference.target),
                    "Erasing a person finds their records by the references to them, which only a database can search, so a record in Redis that names one would outlive their erasure.",
                ));
            } else if !principals.contains(&reference.target) {
                out.push(redis_unsupported(
                    entity,
                    at.clone(),
                    "a reference",
                    &format!("`{}`, a reference to `{}`", field.name, reference.target),
                    "A reference is a foreign key, which only a database keeps. A record in Redis may hold the `id` of the user or service it belongs to, as a user field its policy grants through, and keeps it until it expires.",
                ));
            }
        }
        let Some(policy) = crate::policy::policy_read(db, project, name) else {
            continue;
        };
        let place = |word: &str| {
            policy
                .places
                .iter()
                .find(|(written, _)| written == word)
                .map(|(_, span)| span.clone())
                .unwrap_or_else(|| read.spans.name.clone())
        };
        for effect in Effect::ALL {
            if policy
                .model
                .grants(effect)
                .iter()
                .any(|grant| matches!(grant, Grant::Role { .. }))
            {
                out.push(redis_unsupported(
                    entity,
                    place(effect.word()),
                    "a role",
                    &format!("a role in its `{}:` grants", effect.word()),
                    "A role is a membership, read from a database as policy is checked; a record in Redis is held only to what it holds itself.",
                ));
            }
        }
        if policy.model.tenancy.is_some() {
            out.push(redis_unsupported(
                entity,
                place("tenant"),
                "a tenant",
                "a tenant",
                "A tenant's records are reached through the caller's memberships, read from a database as policy is checked.",
            ));
        }
        for field in &policy.model.fields {
            out.push(redis_unsupported(
                entity,
                place(&format!("field {}", field.field)),
                "a field's own policy",
                &format!("`{}`'s own `policy:`", field.field),
                "A record in Redis is read and written whole, so its grants are the record's alone.",
            ));
        }
    }
    out
}

/// The problems with what the actions in `file` do besides writing an
/// entity kept in Redis: an event emitted, whose outbox is in a database;
/// a record of several changing calls, kept beside the writes; and a batch
/// walking a query of one, whose run is kept in the query's store.
#[salsa::tracked(returns(ref))]
pub fn redis_write_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    if redis_entities(db, project).is_empty() {
        return Vec::new();
    }
    let model = data_model(db, project);
    let path = file.path(db);
    let connections = crate::data::connections(db, project);
    let redis_store =
        |store: &str| connections.kind(store) == Some(crate::data::ConnectionKind::Redis);
    let written = crate::types::written_stores(db, project);
    let records = crate::types::record_plans(db, project);
    let mut out = Vec::new();
    for callable in crate::types::callables_of(db, file) {
        let checked = crate::types::typed_body(db, project, callable);
        let Some(key) = checked
            .key
            .as_ref()
            .filter(|key| matches!(key.kind(), Some(Kind::Action | Kind::Fixture)))
        else {
            continue;
        };
        let name = key.name();
        let facts = crate::types::facts_at(&checked.body);
        // Each batch over a query of an entity in Redis.
        for (fact, (start, end)) in &facts {
            if let crate::types::Fact::Batch(batch) = fact
                && let Some(query) = &batch.query
                && let Some(entity) =
                    crate::data::queries::query_model(db, project, Name::new(db, query.clone()))
                        .as_ref()
                        .and_then(|query| model.entity(&query.entity).cloned())
                        .filter(|entity| entity.redis)
            {
                out.push(writer_refused(
                    (name, &entity, entity.store.as_deref().unwrap_or_default()),
                    Span::new(path, *start, *end),
                    "a batch over Redis",
                    &format!("`{name}` starts a batch over `{query}`, which reads `{}`, kept in Redis", entity.name),
                    "A batch's run is kept in its query's store, beside the writes of each of its items, and Redis keeps no run.",
                ));
            }
        }
        let Some((store, entity)) = written
            .get(name)
            .and_then(|stores| stores.iter().find(|(store, _)| redis_store(store)))
            .and_then(|(store, entity)| Some((store.clone(), model.entity(entity)?.clone())))
        else {
            continue;
        };
        let event = facts.iter().find_map(|(fact, at)| match fact {
            crate::types::Fact::Emits(event) => Some((event, *at)),
            _ => None,
        });
        if let Some((event, (start, end))) = event {
            let event = event.name().rsplit('.').next().unwrap_or_default();
            out.push(writer_refused(
                (name, &entity, &store),
                Span::new(path, start, end),
                "an event",
                &format!("`{name}` writes `{}`, kept in Redis, and emits `{event}`", entity.name),
                "An event is kept in an outbox in the same transaction as the writes that emit it, until its subscribers have it, and a write to Redis commits with no database.",
            ));
        }
        if let Some(plan) = records.get(key)
            && redis_store(&plan.store)
            && let Some(at) = facts
                .iter()
                .find(|(fact, _)| matches!(fact, crate::types::Fact::Writes(_)))
                .or_else(|| facts.first())
                .map(|(_, (start, end))| Span::new(path, *start, *end))
        {
            out.push(writer_refused(
                (name, &entity, &store),
                at,
                "several changing calls",
                &format!("`{name}` writes `{}`, kept in Redis, and makes several changing calls", entity.name),
                "An action making several changing calls keeps a record of them beside its writes, in the same transaction, so a rerun replays them, and a write to Redis commits with no database.",
            ));
        }
    }
    out
}

/// `data.redis_unsupported` at an action (chunk Q10): `action`, which
/// writes or reads `entity`, kept in Redis store `store`, does what
/// `headline` says, which a database keeps beside the writes, for the
/// reason `why` gives.
fn writer_refused(
    (action, entity, store): (&str, &EntityModel, &str),
    at: Span,
    label: &str,
    headline: &str,
    why: &str,
) -> Diagnostic {
    let name = &entity.name;
    Diagnostic::new(&rule::DATA_REDIS_UNSUPPORTED, headline.to_string())
    .at(at, label)
    .reason(format!("`{store}` is a Redis connection. {why}"))
    .next(format!(
        "Keep `{name}` in a PostgreSQL store, or move what `{action}` does besides into an action of its own"
    ))
}

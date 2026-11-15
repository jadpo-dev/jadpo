//! The plans of reads and writes of an entity kept in Redis (area 5,
//! "Entities in Redis"; area 6, "Reading and writing entities in Redis";
//! chunk Q10), as [`jadpo_std::redis`] says the runtime runs them: what
//! every read and write knows of the entity ([`Planner::redis_entity`]),
//! how a query finds its records, the one [`jadpo_check::redis::lookup_of`]
//! decided, and an entity's `get`. A write's plan is [`crate::writes`]'s.

use jadpo_check::data::EntityModel;
use jadpo_check::data::queries::{Element as QueryElement, QueryModel, Source as QuerySource};
use jadpo_check::policy::{Effect, Grant};
use jadpo_check::redis::RedisLookup;
use jadpo_graph::{Kind, TypeRef};
use jadpo_std::policy::{Narrowed, RecordGrant, RecordGrants};
use jadpo_std::read::{
    Cardinality, MissingValue, PageRead, ReadPlan, Record, Select, Size, Source, SqlType,
};
use jadpo_std::redis::{Element, Given, KeyedField, Lookup, RecordPolicy, RedisEntity, RedisRead};

use crate::queries::{EntityPlan, Planner, QueryPlan, field, tag};

/// A read plan of `store` that runs no statement: one whose `redis` says
/// how it finds its records.
pub(crate) fn redis_read(
    store: String,
    cardinality: Cardinality,
    page: Option<PageRead>,
    missing: Option<jadpo_std::read::MissingPlan>,
    redis: RedisRead,
) -> ReadPlan {
    ReadPlan {
        store,
        settings: Vec::new(),
        cardinality,
        select: Select {
            sql: String::new(),
            unpoliced: None,
            permitted: None,
            params: Vec::new(),
            columns: Vec::new(),
            held: Vec::new(),
            record: Record {
                ty: redis.entity.entity.clone(),
                fields: Vec::new(),
            },
            includes: Vec::new(),
            versions: None,
            sensitive: None,
        },
        page,
        missing,
        recheck: None,
        layer: false,
        decided: None,
        gate: None,
        redis: Some(Box::new(redis)),
    }
}

impl Planner<'_> {
    /// The key of `entity`'s field `name`.
    fn field_key(&self, entity: &EntityModel, name: &str) -> Option<jadpo_graph::Key> {
        self.types
            .graph()
            .named(Kind::Field, &format!("{}.{name}", entity.name))
            .map(|id| self.types.graph().node(id).key.clone())
    }

    /// A field Jadpo keeps a key or a set for, its value written by its
    /// type's codec with the names it was born with.
    fn keyed(&self, entity: &EntityModel, name: &str) -> Option<KeyedField> {
        let field = field(entity, name)?;
        let key = self.field_key(entity, name)?;
        Some(KeyedField {
            field: field.name.clone(),
            born: field.born.clone(),
            codec: self.json_number(crate::routes::with_born_names(TypeRef::Field(key)))?,
        })
    }

    /// Who may do `effect` to `entity`'s records, as a record read whole
    /// is tested: the grants a statement would compile, and the scope a
    /// narrowed token needs.
    pub(crate) fn record_grants(&self, entity: &EntityModel, effect: Effect) -> RecordGrants {
        let grants = self
            .grants(entity, effect)
            .into_iter()
            .filter_map(|grant| {
                Some(match grant {
                    Grant::Public => RecordGrant::Anyone,
                    Grant::Signed => RecordGrant::Signed,
                    Grant::System => RecordGrant::System,
                    Grant::Caller { field, kind } => RecordGrant::Caller {
                        field,
                        part: crate::policy::caller_part(kind),
                    },
                    // A role reads a membership a database keeps, which the
                    // check refuses on an entity in Redis.
                    Grant::Role { .. } => return None,
                })
            })
            .collect();
        let narrowed = if !self.access.narrows {
            Narrowed::Untouched
        } else {
            match self
                .access
                .entity(&entity.name)
                .and_then(|policy| policy.scope(effect))
            {
                Some(token) => Narrowed::Needs(token.to_string()),
                None => Narrowed::Refused,
            }
        };
        RecordGrants { grants, narrowed }
    }

    /// What every read and write of `entity`, kept in Redis, knows of it.
    pub(crate) fn redis_entity(&self, entity: &EntityModel) -> Option<RedisEntity> {
        let key = self.types.named(&entity.name)?;
        let codec = self.json_number(crate::routes::with_born_names(TypeRef::Node(key)))?;
        let mut unique = Vec::new();
        for field in entity
            .fields
            .iter()
            .filter(|field| field.unique && field.name != "id")
        {
            unique.push(self.keyed(entity, &field.name)?);
        }
        let mut indexed = Vec::new();
        for rule in &entity.indexes {
            if let [name] = rule.fields.as_slice()
                && rule.kind.is_none()
            {
                indexed.push(self.keyed(entity, name)?);
            }
        }
        Some(RedisEntity {
            entity: entity.name.clone(),
            store: entity.store.clone()?,
            keyspace: entity.table.clone(),
            codec,
            expires_ms: entity.expires_after_ms?,
            unique,
            indexed,
            policy: RecordPolicy {
                read: self.record_grants(entity, Effect::Read),
                create: self.record_grants(entity, Effect::Create),
                update: self.record_grants(entity, Effect::Update),
                delete: self.record_grants(entity, Effect::Delete),
            },
        })
    }

    /// Where a query's value comes from, as a read of a Redis record takes
    /// it: an argument, a constant made for `field`'s type, or the clock.
    fn redis_given(
        &self,
        entity: &EntityModel,
        name: &str,
        value: &jadpo_check::data::queries::Value,
    ) -> Option<Given> {
        let field = field(entity, name)?;
        Some(match self.given(entity, field, value)? {
            Source::Arg { index, path } => Given::Arg { index, path },
            Source::Constant(constant) => Given::Constant(constant),
            Source::Now => Given::Now,
            _ => return None,
        })
    }

    /// The plan of `query`, a query of `entity` kept in Redis.
    pub(crate) fn redis_query(
        &mut self,
        query: &QueryModel,
        entity: &EntityModel,
    ) -> Option<QueryPlan> {
        let found = jadpo_check::redis::lookup_of(entity, query).ok()?;
        let redis_entity = self.redis_entity(entity)?;
        let position = |fields: &[KeyedField], name: &str| -> Option<u32> {
            u32::try_from(fields.iter().position(|keyed| keyed.field == name)?).ok()
        };
        let lookup = match found {
            RedisLookup::Id(value) => Lookup::Id(self.redis_given(entity, "id", value)?),
            RedisLookup::Unique { field, value } => Lookup::Unique {
                field: position(&redis_entity.unique, field)?,
                value: self.redis_given(entity, field, value)?,
            },
            RedisLookup::Index { field, value } => Lookup::Index {
                field: position(&redis_entity.indexed, field)?,
                value: self.redis_given(entity, field, value)?,
            },
            RedisLookup::Count {
                field,
                value,
                total,
            } => Lookup::Count {
                field: position(&redis_entity.indexed, field)?,
                value: self.redis_given(entity, field, value)?,
                ty: query.element.ty.clone(),
                total: total.to_string(),
            },
        };
        let element = picked(entity, &query.element)?;
        let source = |value: &jadpo_check::data::queries::Value| -> Option<Source> {
            match value {
                jadpo_check::data::queries::Value::Param { index, path } => Some(Source::Arg {
                    index: *index,
                    path: path.clone(),
                }),
                _ => None,
            }
        };
        let size = |bound: &jadpo_check::data::queries::Bound| -> Option<Size> {
            Some(match bound {
                jadpo_check::data::queries::Bound::Literal(n) => Size::Fixed(*n),
                jadpo_check::data::queries::Bound::Value(value) => Size::From(source(value)?),
            })
        };
        let page = match query.cardinality {
            Cardinality::Page => Some(PageRead {
                size: size(query.limit.as_ref()?)?,
                after: source(query.after.as_ref()?)?,
                cursor: vec![(0, SqlType::Uuid)],
                tag: tag(&query.key),
            }),
            _ => None,
        };
        let limit = match (query.cardinality, &query.limit) {
            (Cardinality::List, Some(bound)) => Some(size(bound)?),
            _ => None,
        };
        let missing = match &query.missing {
            Some(missing) => Some(self.missing(missing)?),
            None => None,
        };
        let caller = query
            .params
            .iter()
            .position(|(_, ty)| crate::holds_caller_ty(ty))
            .and_then(|at| u32::try_from(at).ok());
        let store = redis_entity.store.clone();
        let mut read = redis_read(
            store,
            query.cardinality,
            page,
            missing,
            RedisRead {
                entity: redis_entity,
                lookup,
                element,
                limit,
            },
        );
        read.gate = self.query_gate(&query.name);
        Some(QueryPlan {
            key: query.key.clone(),
            name: query.name.clone(),
            caller,
            read,
        })
    }

    /// `entity`'s `get`, kept in Redis: its record by `id`, or its
    /// not-found failure given the `id` looked for.
    pub(crate) fn redis_get(&mut self, entity: &EntityModel) -> Option<EntityPlan> {
        let key = self.types.named(&entity.name)?;
        let redis_entity = self.redis_entity(entity)?;
        let not_found = self.not_found(
            entity,
            MissingValue::Arg {
                index: 0,
                path: Vec::new(),
            },
        );
        let get = redis_read(
            redis_entity.store.clone(),
            Cardinality::One,
            None,
            not_found,
            RedisRead {
                entity: redis_entity,
                lookup: Lookup::Id(Given::Arg {
                    index: 0,
                    path: Vec::new(),
                }),
                element: Element::Record,
                limit: None,
            },
        );
        Some(EntityPlan {
            key,
            name: entity.name.clone(),
            get,
        })
    }
}

/// What each record a query reads becomes: the entity itself, or a type
/// whose fields pick the record's.
fn picked(entity: &EntityModel, element: &QueryElement) -> Option<Element> {
    if element.ty == entity.name {
        return Some(Element::Record);
    }
    let mut fields = Vec::new();
    for (name, source) in &element.fields {
        match source {
            QuerySource::Column(from) => fields.push((name.clone(), from.clone())),
            // An aggregate's total is the count itself.
            QuerySource::Entry { .. } => {}
            QuerySource::Include(_) => return None,
        }
    }
    Some(Element::Picked {
        ty: element.ty.clone(),
        fields,
    })
}

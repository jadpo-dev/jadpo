//! The SQL each `create`, `update` and `delete` runs on PostgreSQL (area 6,
//! "The operations every entity gets"), one plan per site, as
//! [`jadpo_std::write`] plans both executors run.
//!
//! - A create inserts every column, with a new identity and the clock for
//!   generated fields, and returns the row it made.
//! - An update sets the fields `set:` gives, and every `on_change` field
//!   to the clock, on the record loaded earlier or the one with the `id`
//!   it is given, and returns the row. By `id`, a value that reads stored
//!   fields (`Post.view_count + 1`) is worked out by the database in the
//!   same statement. A patch writes each of its input's fields as `CASE
//!   WHEN <supplied> THEN <value> ELSE <column> END`, so a field left out
//!   keeps its value and the text never changes.
//! - A delete removes the record.
//!
//! Each write lists the constraints a refusal can name: each `conflict:`
//! arm's rule, and each reference it writes, which names a record that
//! isn't there (that entity's not-found failure) or, when its value was
//! read from a record loaded earlier, one deleted since.

use std::collections::HashMap;

use jadpo_check::data::{DataModel, EntityModel, foreign_key, unique_constraint};
use jadpo_graph::body::{BinaryOp, Body, Callee, Expr, Node, SiteId, UnaryOp};
use jadpo_graph::schema::object_name;
use jadpo_graph::{Key, Kind, TypeRef};
use jadpo_std::read::MissingValue;
use jadpo_std::read::{Field, Record, SqlType};
use jadpo_std::write::{
    Guard, Raises, Refusal, Returned, RowCheck, WriteOp, WriteParam, WritePlan, WriteSource,
};

use jadpo_sql::Bind;
use jadpo_sql::statement as text;

use crate::policy::{CallerBind, Row};
use crate::queries::{Planner, Reach, column_type, field};
use crate::types::{Types, ValidatorId};
use jadpo_check::policy::Effect;
use jadpo_std::policy::CallerPart;
use jadpo_std::write::{FieldProbe, PolicyRefusal};

/// What one write site runs: its plan, which says what its refusals and
/// a missing record raise, and the sites the executor works out first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteSitePlan {
    pub plan: WritePlan,
    /// The sites worked out before it runs, in order: its record, then
    /// the values its block gives. A value a field stores is kept the way
    /// its field's type keeps it (a decimal at its scale), with that
    /// field's validator.
    pub inputs: Vec<(SiteId, Option<ValidatorId>)>,
}

/// One write's statement as it is built.
#[derive(Default)]
struct Writing {
    params: Vec<WriteParam>,
    inputs: Vec<(SiteId, Option<ValidatorId>)>,
}

impl Writing {
    /// `$n` for `source` sent as `ty`, sent once however often it is used.
    fn param(&mut self, source: WriteSource, ty: SqlType) -> String {
        let param = WriteParam { source, ty };
        let at = match self.params.iter().position(|p| *p == param) {
            Some(at) => at,
            None => {
                self.params.push(param);
                self.params.len() - 1
            }
        };
        jadpo_sql::placeholder(at)
    }

    /// The input `site`, worked out before the write runs.
    fn input(&mut self, site: SiteId, validator: Option<ValidatorId>) -> u32 {
        self.inputs.push((site, validator));
        (self.inputs.len() - 1) as u32
    }
}

impl Bind for Writing {
    fn bind(&mut self, value: &jadpo_sql::Value, ty: SqlType) -> Option<String> {
        match value {
            jadpo_sql::Value::Now => Some(self.param(WriteSource::Now, ty)),
            _ => None,
        }
    }

    /// A literal or an enum value, bound as the storage codec made it
    /// (chunk 5.5w3b).
    fn stored(&mut self, value: jadpo_std::SqlValue) -> Option<String> {
        let ty = value.ty();
        Some(self.param(WriteSource::Stored(value), ty))
    }
}

impl CallerBind for Writing {
    fn caller(&mut self, part: CallerPart) -> String {
        self.param(WriteSource::Caller(part), part.ty())
    }
}

impl Planner<'_> {
    /// The plan of the write at `site` of `body`.
    pub(crate) fn write(&mut self, body: &Body, site: SiteId) -> Option<WriteSitePlan> {
        let Node::Expr(Expr::Write {
            entity,
            op,
            transition,
            target,
            by_id,
            set,
            patch,
            when_supplied,
            conflicts,
            missing,
            not_found,
            ..
        }) = &body.site(site).node
        else {
            return None;
        };
        let name = self.types.name(entity).to_string();
        let model: &DataModel = self.model;
        let entity_model = model.entity(&name)?;
        let store = entity_model.store.clone()?;
        let table = entity_model.table.as_str();
        let lifecycle = entity_model.lifecycle.as_ref();
        // The lifecycle transition it runs, as the lifecycle declares it.
        let transition = match transition {
            Some(key) => {
                let name = self.types.name(key).to_string();
                let (_, short) = name.split_once('.')?;
                Some(lifecycle?.transition(short)?)
            }
            None => None,
        };
        let mut writing = Writing::default();
        let mut guard: Option<(String, Option<String>, Vec<WriteParam>)> = None;
        let unpoliced: Option<String>;
        let policy: Option<PolicyRefusal>;
        // What keeps the references it sets inside the tenant (area 9).
        let mut kept: Vec<String> = Vec::new();
        // Where each reference's value comes from, for a refusal's failure.
        let mut written_refs: Vec<(String, MissingValue)> = Vec::new();
        // An update's or a delete's row, by where its `id` comes from.
        let mut row: Option<RowCheck> = None;
        let sql = match op {
            WriteOp::Create => {
                let given: Vec<(String, u32)> = set
                    .iter()
                    .map(|(key, value)| {
                        let validator = self.types.validator_id(&TypeRef::Field(key.clone()));
                        let input = writing.input(*value, Some(validator));
                        (self.types.field_name(key).to_string(), input)
                    })
                    .collect();
                let mut names: Vec<String> = Vec::new();
                let mut values = Vec::new();
                // Each field's value, as the new record's grants read it.
                let mut new: Vec<(String, String)> = Vec::new();
                for field in &entity_model.fields {
                    // Every field is stored, or there is no statement.
                    column_type(field)?;
                    if let Some(lifecycle) = lifecycle.filter(|l| l.owns(&field.name)) {
                        // A field the lifecycle owns starts at its
                        // `initial:` value, or `none`.
                        let value = lifecycle
                            .initial
                            .iter()
                            .find(|(name, _)| *name == field.name)
                            .map_or_else(
                                || Some(jadpo_sql::NULL.to_string()),
                                |(_, value)| jadpo_sql::value(&mut writing, value, &field.column()),
                            )?;
                        names.push(field.born.clone());
                        new.push((field.name.clone(), value.clone()));
                        values.push(value);
                        continue;
                    }
                    let source = if field.name == "id" {
                        WriteSource::NewId
                    } else if field.generated.is_some() {
                        WriteSource::Now
                    } else {
                        let (_, input) = given.iter().find(|(name, _)| *name == field.name)?;
                        if field.reference.is_some() {
                            written_refs.push((
                                field.name.clone(),
                                MissingValue::Arg {
                                    index: *input,
                                    path: Vec::new(),
                                },
                            ));
                        }
                        WriteSource::Input(*input)
                    };
                    // A compound type goes into its several columns (area
                    // 19).
                    let json = self.json_codec(entity_model, field);
                    let parts = field_columns(&mut writing, field, &source, json)?;
                    let first = parts.first().map(|(_, value)| value.clone())?;
                    for (column, value) in parts {
                        names.push(column);
                        values.push(value);
                    }
                    new.push((field.name.clone(), first));
                }
                // The new record's grants hold of its values, in the
                // same statement (area 9).
                let condition =
                    self.policy(&mut writing, entity_model, Effect::Create, &Row::New(&new))?;
                // A field whose own `policy:` narrows who may set it, set
                // by the request: setting nothing is never refused.
                let mut narrowed = Vec::new();
                for (name, _) in &given {
                    let Some(who) = self.field_policy(
                        &mut writing,
                        entity_model,
                        name,
                        Effect::Create,
                        &Row::New(&new),
                    )?
                    else {
                        continue;
                    };
                    let value = new.iter().find(|(n, _)| n == name)?.1.clone();
                    narrowed.push(if field(entity_model, name)?.nullable {
                        jadpo_sql::policy::unless_none(&value, &who)
                    } else {
                        who
                    });
                }
                let condition = all_of(condition, narrowed);
                // Each reference it sets inside the tenant points at a
                // record of the new record's tenant, with or without
                // policy (area 9).
                let set_names: Vec<String> = given.iter().map(|(name, _)| name.clone()).collect();
                let names: Vec<&str> = names.iter().map(String::as_str).collect();
                let new_value =
                    |name: &str| new.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone());
                kept = self.kept_in_tenant(entity_model, &set_names, &new_value, true)?;
                let condition = all_of(condition, kept.clone());
                let plain = if kept.is_empty() {
                    text::insert(table, &names, &values, &returning(entity_model))
                } else {
                    jadpo_sql::policy::insert_when(
                        table,
                        &names,
                        &values,
                        &all_of(jadpo_sql::policy::ANYONE.to_string(), kept.clone()),
                        &returning(entity_model),
                    )
                };
                if condition == jadpo_sql::policy::ANYONE {
                    unpoliced = None;
                    policy = None;
                    plain
                } else {
                    unpoliced = Some(plain);
                    policy = Some(PolicyRefusal::Create);
                    jadpo_sql::policy::insert_when(
                        table,
                        &names,
                        &values,
                        &condition,
                        &returning(entity_model),
                    )
                }
            }
            WriteOp::Update | WriteOp::Delete => {
                let record = writing.input((*target)?, None);
                let id_source = if *by_id {
                    WriteSource::Input(record)
                } else {
                    WriteSource::Field {
                        input: record,
                        field: "id".to_string(),
                    }
                };
                let mut assignments: Vec<String> = Vec::new();
                // Each field it changes, with its value once written, as
                // SQL the `WHERE` can test.
                let mut changed: Vec<(String, String)> = Vec::new();
                // What each field's own `policy:` adds for the fields the
                // write changes (area 9, "Field restrictions").
                let mut narrowed: Vec<String> = Vec::new();
                if let Some(transition) = transition {
                    // A transition sets its fields to their fixed values,
                    // and every field generated on change to the clock.
                    for (name, value) in &transition.set {
                        let field = field(entity_model, name)?;
                        let value = jadpo_sql::value(&mut writing, value, &field.column())?;
                        assignments.push(text::equal(&field.born, &value));
                        changed.push((name.clone(), value.clone()));
                        if *op == WriteOp::Update
                            && let Some(who) = self.field_policy(
                                &mut writing,
                                entity_model,
                                name,
                                Effect::Update,
                                &Row::Table,
                            )?
                        {
                            narrowed.push(who);
                        }
                    }
                    for field in &entity_model.fields {
                        if field.generated == Some(jadpo_std::Generated::OnChange) {
                            let ty = column_type(field)?.param();
                            let now = writing.param(WriteSource::Now, ty);
                            assignments.push(text::equal(&field.born, &now));
                        }
                    }
                } else if *op == WriteOp::Update {
                    // `set:`, in the order written.
                    for (key, value) in set {
                        let field_name = self.types.field_name(key).to_string();
                        if let Some(who) = self.field_policy(
                            &mut writing,
                            entity_model,
                            &field_name,
                            Effect::Update,
                            &Row::Table,
                        )? {
                            narrowed.push(who);
                        }
                        let field = field(entity_model, &field_name)?;
                        let ty = column_type(field)?.param();
                        let validator = self.types.validator_id(&TypeRef::Field(key.clone()));
                        let parts = if *by_id && stored_in(body, *value) {
                            // The database works out no compound type in
                            // its several columns (area 19), nor a value
                            // held as JSON (chunk 5.5w2b).
                            if field.compound.is_some() || field.json.is_some() {
                                return None;
                            }
                            let sql =
                                self.stored_sql(body, *value, entity_model, ty, &mut writing)?;
                            vec![(field.born.clone(), sql)]
                        } else {
                            let input = writing.input(*value, Some(validator));
                            if field.reference.is_some() {
                                written_refs.push((
                                    field_name.clone(),
                                    MissingValue::Arg {
                                        index: input,
                                        path: Vec::new(),
                                    },
                                ));
                            }
                            let json = self.json_codec(entity_model, field);
                            field_columns(&mut writing, field, &WriteSource::Input(input), json)?
                        };
                        let first = parts
                            .first()
                            .map(|(_, sql)| sql.clone())
                            .unwrap_or_default();
                        for (column, sql) in parts {
                            assignments.push(text::equal(&column, &sql));
                        }
                        changed.push((field_name.clone(), first));
                    }
                    // `patch:` and `when_supplied:`.
                    let mut supplied_fields: Vec<String> = Vec::new();
                    let mut patch_input = None;
                    if let Some(patch) = patch {
                        let input = writing.input(*patch, None);
                        patch_input = Some(input);
                        let input_ty = match &body.site(*patch).ty {
                            TypeRef::Node(key) => Some(key.clone()),
                            _ => None,
                        };
                        let input_fields: Vec<String> = input_ty
                            .and_then(|key| self.types.shape_plan(&key))
                            .map(|shape| shape.fields.iter().map(|f| f.name.clone()).collect())
                            .unwrap_or_default();
                        for name in input_fields {
                            let Some(field) = field(entity_model, &name) else {
                                continue;
                            };
                            let ty = column_type(field)?.param();
                            let supplied = writing.param(
                                WriteSource::Supplied {
                                    input,
                                    field: name.clone(),
                                },
                                SqlType::Bool,
                            );
                            let json = self.json_codec(entity_model, field);
                            let parts = field_columns(
                                &mut writing,
                                field,
                                &WriteSource::Field {
                                    input,
                                    field: name.clone(),
                                },
                                json,
                            )?;
                            let _ = ty;
                            let (first_column, first) = parts.first().cloned()?;
                            for (column, value) in &parts {
                                assignments.push(text::set_when(column, &supplied, value));
                            }
                            changed.push((
                                name.clone(),
                                supplied_value(table, &first_column, &supplied, &first),
                            ));
                            if let Some(who) = self.field_policy(
                                &mut writing,
                                entity_model,
                                &name,
                                Effect::Update,
                                &Row::Table,
                            )? {
                                narrowed
                                    .push(jadpo_sql::policy::unless_unsupplied(&supplied, &who));
                            }
                            if field.reference.is_some() {
                                written_refs.push((
                                    name.clone(),
                                    MissingValue::Arg {
                                        index: input,
                                        path: vec![name.clone()],
                                    },
                                ));
                            }
                            supplied_fields.push(name);
                        }
                        for (supplied_key, fields) in when_supplied {
                            let supplied_name = self.types.field_name(supplied_key).to_string();
                            let supplied = writing.param(
                                WriteSource::Supplied {
                                    input,
                                    field: supplied_name,
                                },
                                SqlType::Bool,
                            );
                            for (key, value) in fields {
                                let field_name = self.types.field_name(key).to_string();
                                let field = field(entity_model, &field_name)?;
                                let ty = column_type(field)?.param();
                                let validator =
                                    self.types.validator_id(&TypeRef::Field(key.clone()));
                                let at = writing.input(*value, Some(validator));
                                if field.reference.is_some() {
                                    written_refs.push((
                                        field_name.clone(),
                                        MissingValue::Arg {
                                            index: at,
                                            path: Vec::new(),
                                        },
                                    ));
                                }
                                let _ = ty;
                                let json = self.json_codec(entity_model, field);
                                let parts = field_columns(
                                    &mut writing,
                                    field,
                                    &WriteSource::Input(at),
                                    json,
                                )?;
                                let (first_column, first) = parts.first().cloned()?;
                                for (column, value) in &parts {
                                    assignments.push(text::set_when(column, &supplied, value));
                                }
                                changed.push((
                                    field_name.clone(),
                                    supplied_value(table, &first_column, &supplied, &first),
                                ));
                                if let Some(who) = self.field_policy(
                                    &mut writing,
                                    entity_model,
                                    &field_name,
                                    Effect::Update,
                                    &Row::Table,
                                )? {
                                    narrowed.push(jadpo_sql::policy::unless_unsupplied(
                                        &supplied, &who,
                                    ));
                                }
                            }
                        }
                    }
                    // The fields generated on every change.
                    for field in &entity_model.fields {
                        if field.generated != Some(jadpo_std::Generated::OnChange) {
                            continue;
                        }
                        let ty = column_type(field)?.param();
                        let now = writing.param(WriteSource::Now, ty);
                        if let Some(input) = patch_input {
                            let any = writing.param(
                                WriteSource::AnySupplied {
                                    input,
                                    fields: supplied_fields.clone(),
                                },
                                SqlType::Bool,
                            );
                            assignments.push(text::set_when(&field.born, &any, &now));
                        } else {
                            assignments.push(text::equal(&field.born, &now));
                        }
                    }
                    if assignments.is_empty() {
                        assignments.push(text::equal("id", &text::column("id")));
                    }
                }
                // A record moved to another tenant (only a listed action
                // may) needs the caller's membership there, as a create
                // there would (area 9, "Tenancy").
                if *op == WriteOp::Update
                    && let Some(tenancy) = self
                        .access
                        .entity(&entity_model.name)
                        .and_then(|policy| policy.tenancy.as_ref())
                    && let Some((_, to)) = changed.iter().rev().find(|(n, _)| *n == tenancy.via)
                    && let Some(member) = self.moved_to(&mut writing, entity_model, to)?
                {
                    narrowed.push(member);
                }
                // Each reference it changes inside the tenant points at a
                // record of the record's tenant, with or without policy
                // (area 9).
                if *op == WriteOp::Update {
                    let names: Vec<String> = changed.iter().map(|(n, _)| n.clone()).collect();
                    let new_value = |name: &str| match changed.iter().rev().find(|(n, _)| n == name)
                    {
                        Some((_, value)) => Some(value.clone()),
                        None => Some(jadpo_sql::policy::qualified(
                            table,
                            &field(entity_model, name)?.born,
                        )),
                    };
                    kept = self.kept_in_tenant(entity_model, &names, &new_value, false)?;
                }
                let id = writing.param(id_source.clone(), SqlType::Uuid);
                let mut conditions = vec![text::equal("id", &id)];
                conditions.extend(self.access(&mut writing, entity_model, Reach::Write)?);
                // A delete needs `delete` whether it removes the record or
                // runs the lifecycle's `delete` transition; every other
                // transition is an update (area 9, "Grants").
                let effect = if *op == WriteOp::Delete {
                    Effect::Delete
                } else {
                    Effect::Update
                };
                let granted = self.policy(&mut writing, entity_model, effect, &Row::Table)?;
                let open =
                    granted == jadpo_sql::policy::ANYONE && narrowed.is_empty() && kept.is_empty();
                // Writing no row asks whether the record is there under its
                // lifecycle: then policy refused it. With a field's own
                // policy, it asks again under the record's grants: then the
                // field's refused it.
                policy = if open {
                    None
                } else {
                    let mut asking = Writing::default();
                    let id = asking.param(id_source.clone(), SqlType::Uuid);
                    let mut asked = vec![text::equal("id", &id)];
                    asked.extend(self.access(&mut asking, entity_model, Reach::Write)?);
                    let mut probe = text::select_one(table);
                    text::where_clause(&mut probe, &asked, "");
                    let fields_probe = if narrowed.is_empty() && kept.is_empty() {
                        None
                    } else {
                        let mut asking = Writing::default();
                        let id = asking.param(id_source.clone(), SqlType::Uuid);
                        let mut asked = vec![text::equal("id", &id)];
                        asked.extend(self.access(&mut asking, entity_model, Reach::Write)?);
                        let record = self.policy(&mut asking, entity_model, effect, &Row::Table)?;
                        if record != jadpo_sql::policy::ANYONE {
                            asked.push(record);
                        }
                        let mut probe = text::select_one(table);
                        text::where_clause(&mut probe, &asked, "");
                        Some(FieldProbe {
                            sql: probe,
                            params: asking.params,
                        })
                    };
                    Some(PolicyRefusal::Probe {
                        sql: probe,
                        params: asking.params,
                        fields: fields_probe,
                    })
                };
                // A transition's `when:` joins the `WHERE` when it can
                // refuse a visible record; writing no row then asks which
                // it was, the record or the condition.
                if let Some(transition) = transition
                    && transition.guarded
                    && let Some(when) = &transition.when
                {
                    let mut asking = Writing::default();
                    let id = asking.param(id_source.clone(), SqlType::Uuid);
                    let mut asked = vec![text::equal("id", &id)];
                    asked.extend(self.access(&mut asking, entity_model, Reach::Write)?);
                    let mut bare = text::select_one(table);
                    text::where_clause(&mut bare, &asked, "");
                    let asked_policy =
                        self.policy(&mut asking, entity_model, effect, &Row::Table)?;
                    // A field the caller may not change refuses the write
                    // before its condition can: the probe asks with the
                    // fields' grants too, so a refused field is 403, not
                    // the condition's failure (area 9, "Field
                    // restrictions").
                    let mut fields = Vec::new();
                    if *op == WriteOp::Update {
                        for (name, _) in &transition.set {
                            if let Some(who) = self.field_policy(
                                &mut asking,
                                entity_model,
                                name,
                                Effect::Update,
                                &Row::Table,
                            )? {
                                fields.push(who);
                            }
                        }
                    }
                    if asked_policy == jadpo_sql::policy::ANYONE && fields.is_empty() {
                        guard = Some((bare, None, asking.params));
                    } else {
                        if asked_policy != jadpo_sql::policy::ANYONE {
                            asked.push(asked_policy);
                        }
                        asked.extend(fields);
                        let mut probe = text::select_one(table);
                        text::where_clause(&mut probe, &asked, "");
                        guard = Some((probe, Some(bare), asking.params));
                    }
                    conditions.push(jadpo_sql::condition(
                        &mut writing,
                        entity_model,
                        when,
                        false,
                    )?);
                }
                // A version is runtime state, not an application input. Reserve
                // its own slot (never deduplicate it with a literal NULL).
                let version = writing.params.len() as u32;
                writing.params.push(WriteParam {
                    source: WriteSource::Stored(jadpo_std::SqlValue::Null(SqlType::Text)),
                    ty: SqlType::Text,
                });
                let expected = jadpo_sql::placeholder(version as usize);
                conditions.push(text::holds_row_version(&expected));
                row = Some(RowCheck {
                    table: table.to_string(),
                    id: id_source,
                    version,
                    sql: text::row_version(table),
                });
                let write = |conditions: &[String]| {
                    let mut sql = match op {
                        _ if transition.is_some() => text::update(table, &assignments),
                        WriteOp::Update => text::update(table, &assignments),
                        _ => text::delete(table),
                    };
                    text::where_clause(&mut sql, conditions, "");
                    if *op == WriteOp::Update {
                        text::returning(&mut sql, &returning(entity_model));
                    }
                    sql
                };
                if open {
                    unpoliced = None;
                    write(&conditions)
                } else {
                    let mut plain = conditions.clone();
                    plain.extend(kept.iter().cloned());
                    unpoliced = Some(write(&plain));
                    if granted != jadpo_sql::policy::ANYONE {
                        conditions.push(granted);
                    }
                    conditions.extend(narrowed);
                    conditions.extend(kept.iter().cloned());
                    write(&conditions)
                }
            }
        };
        // What each refusal raises: an arm's failure, a missing record's
        // not-found failure, or a record deleted since it was loaded.
        let mut refusals: Vec<Refusal> = Vec::new();
        let mut guarded_at = None;
        for (at, arm) in conflicts.iter().enumerate() {
            if arm.rule.kind() == Some(Kind::Transition) {
                // The transition's `when:` refusing the record.
                guarded_at = Some(refusals.len() as u32);
                refusals.push(Refusal {
                    constraint: format!("when: {}", self.types.name(&arm.rule)),
                    raises: Raises::Arm(at as u32),
                });
                continue;
            }
            let Some(constraint) = self.constraint(&arm.rule) else {
                continue;
            };
            refusals.push(Refusal {
                constraint,
                raises: Raises::Arm(at as u32),
            });
        }
        for (field_name, value) in written_refs {
            let Some(field) = field(entity_model, &field_name) else {
                continue;
            };
            let Some(reference) = &field.reference else {
                continue;
            };
            let constraint = foreign_key(entity_model, field);
            if refusals
                .iter()
                .any(|refusal| refusal.constraint == constraint)
            {
                continue;
            }
            let may_be_missing = missing
                .iter()
                .any(|(key, _)| self.types.field_name(key) == field_name);
            let raises = if may_be_missing {
                let target = model.entity(&reference.target)?;
                Raises::NotFound(self.not_found(target, value)?)
            } else {
                Raises::Gone(format!(
                    "the `{}` this {} refers to was deleted after it was loaded",
                    reference.target,
                    op.word()
                ))
            };
            refusals.push(Refusal { constraint, raises });
        }
        let missing = match not_found {
            Some(_) => Some(self.not_found(
                entity_model,
                MissingValue::Arg {
                    index: 0,
                    path: Vec::new(),
                },
            )?),
            None => None,
        };
        let returns = (*op != WriteOp::Delete).then(|| self.returned(entity_model));
        let returns = match returns {
            Some(returned) => Some(returned?),
            None => None,
        };
        Some(WriteSitePlan {
            plan: WritePlan {
                store,
                op: *op,
                sql,
                unpoliced,
                policy,
                guard: match (guard, guarded_at) {
                    (Some((sql, unpoliced, params)), Some(refusal)) => Some(Guard {
                        sql,
                        unpoliced,
                        params,
                        refusal,
                    }),
                    _ => None,
                },
                params: writing.params,
                returns,
                by_id: *by_id,
                refusals,
                missing,
                kept_in_tenant: !kept.is_empty(),
                row,
            },
            inputs: writing.inputs,
        })
    }

    /// A value an update by `id` works out from stored fields, as SQL: a
    /// stored field is its column, `+` (`||` joining text), `-` and a
    /// construction (which the
    /// checker proved can't fail) are worked out by the database, and
    /// anything else is an input the executor works out first.
    fn stored_sql(
        &mut self,
        body: &Body,
        site: SiteId,
        entity: &EntityModel,
        ty: SqlType,
        writing: &mut Writing,
    ) -> Option<String> {
        if !stored_in(body, site) {
            let input = writing.input(site, None);
            return Some(writing.param(WriteSource::Input(input), ty));
        }
        Some(match &body.site(site).node {
            Node::Expr(Expr::Stored { field: key }) => {
                let field = field(entity, self.types.field_name(key))?;
                text::column(&field.born)
            }
            Node::Expr(Expr::Binary {
                op: op @ (BinaryOp::Add | BinaryOp::Subtract),
                lhs,
                rhs,
            }) => {
                let lhs = self.stored_sql(body, *lhs, entity, ty, writing)?;
                let rhs = self.stored_sql(body, *rhs, entity, ty, writing)?;
                // Jadpo's `+` joins text, which SQL writes `||`.
                match op {
                    BinaryOp::Add => text::plus(&lhs, &rhs, ty == SqlType::Text),
                    _ => text::minus(&lhs, &rhs),
                }
            }
            Node::Expr(Expr::Unary {
                op: UnaryOp::Negate,
                operand,
            }) => text::negate(&self.stored_sql(body, *operand, entity, ty, writing)?),
            Node::Expr(Expr::Call {
                callee: Callee::Construct(_),
                args,
                ..
            }) => {
                let [Some(inner)] = args.as_slice() else {
                    return None;
                };
                self.stored_sql(body, *inner, entity, ty, writing)?
            }
            _ => return None,
        })
    }

    /// The constraint the database knows a rule by: a unique field's or a
    /// named unique rule's key, or a reference's foreign key.
    fn constraint(&self, rule: &Key) -> Option<String> {
        let name = self.types.name(rule).to_string();
        let (holder, part) = name.split_once('.')?;
        let entity = self.model.entity(holder)?;
        match rule.kind()? {
            Kind::Field => {
                let field = field(entity, part)?;
                Some(object_name(&[&entity.table, &field.born], "key"))
            }
            Kind::UniqueRule => Some(unique_constraint(entity, part)),
            Kind::Reference => Some(foreign_key(entity, field(entity, part)?)),
            _ => None,
        }
    }

    /// The record a written row becomes: every column of the entity.
    fn returned(&mut self, entity: &EntityModel) -> Option<Returned> {
        let mut columns = Vec::new();
        let mut held = Vec::new();
        let mut fields = Vec::new();
        for field in &entity.fields {
            let at = columns.len() as u32;
            let column = self.column(entity, field)?;
            // A compound type comes back in its several columns (area 19).
            let parts = field.columns().len().max(1);
            // Every stored field is read back, held to its rules (chunk
            // 5.9d); a JSON-held one by its codec.
            held.push(
                (!matches!(column, jadpo_std::read::Column::Json(_)))
                    .then(|| self.held_field(entity, field)),
            );
            columns.push(column);
            for _ in 1..parts {
                columns.push(jadpo_std::read::Column::Part);
                held.push(None);
            }
            fields.push((field.name.clone(), Field::Column(at)));
        }
        Some(Returned {
            columns,
            held,
            record: Record {
                ty: entity.name.clone(),
                fields,
            },
        })
    }
}

/// A patched field's value once written, as the `WHERE` of its update
/// reads it: the value when the request supplies it, the column otherwise.
fn supplied_value(table: &str, column: &str, supplied: &str, value: &str) -> String {
    jadpo_sql::policy::supplied_value(table, column, supplied, value)
}

/// `condition` and each of `fields`: [`jadpo_sql::policy::ANYONE`] when
/// both let anyone in.
fn all_of(condition: String, fields: Vec<String>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if condition != jadpo_sql::policy::ANYONE {
        parts.push(condition);
    }
    parts.extend(fields);
    jadpo_sql::policy::all_of(&parts)
}

/// Every column of an entity, as `RETURNING` lists them: a compound
/// type's several (area 19).
fn returning(entity: &EntityModel) -> Vec<String> {
    entity
        .fields
        .iter()
        .flat_map(|field| field.columns().into_iter().map(|(column, _)| column))
        .collect()
}

/// The columns `field` is written to with the value from `source`, each
/// with its value as SQL: one, or each of a compound type's (`Money`'s
/// minor units and currency code, area 19).
fn field_columns(
    writing: &mut Writing,
    field: &jadpo_check::data::FieldModel,
    source: &WriteSource,
    json: Option<u32>,
) -> Option<Vec<(String, String)>> {
    let ty = column_type(field)?.param();
    // A JSON-held standard value, written as its contract's JSON by the
    // route codec (chunk 5.5w2b).
    if let Some(codec) = json {
        let source = WriteSource::Json {
            of: Box::new(source.clone()),
            codec,
        };
        return Some(vec![(field.born.clone(), writing.param(source, ty))]);
    }
    if field.compound.is_none() {
        return Some(vec![(
            field.born.clone(),
            writing.param(source.clone(), ty),
        )]);
    }
    field
        .columns()
        .into_iter()
        .enumerate()
        .map(|(index, (column, sql_type))| {
            let part = WriteSource::Part {
                of: Box::new(source.clone()),
                index: u32::try_from(index).ok()?,
            };
            let ty = SqlType::of_column(&sql_type)?.param();
            Some((column, writing.param(part, ty)))
        })
        .collect()
}

/// Whether the value at `site` reads a stored field.
fn stored_in(body: &Body, site: SiteId) -> bool {
    match &body.site(site).node {
        Node::Expr(Expr::Stored { .. }) => true,
        Node::Expr(Expr::Binary { lhs, rhs, .. }) => stored_in(body, *lhs) || stored_in(body, *rhs),
        Node::Expr(Expr::Unary { operand, .. }) => stored_in(body, *operand),
        Node::Expr(Expr::Call { args, .. }) => {
            args.iter().flatten().any(|arg| stored_in(body, *arg))
        }
        _ => false,
    }
}

/// Every write of a project's bodies, planned: the plans, in the order the
/// callables are numbered, and where each callable's write sites are.
pub(crate) fn plans(
    db: &dyn jadpo_db::Db,
    project: jadpo_db::Project,
    types: &mut Types,
    bodies: &[(Key, &Body)],
    json: &std::cell::RefCell<Vec<TypeRef>>,
) -> (Vec<WriteSitePlan>, HashMap<(Key, u32), u32>) {
    let model = jadpo_check::data::data_model(db, project);
    let lookup = jadpo_check::lookups::Lookup::new(db, project);
    let access = jadpo_check::policy::access(db, project);
    let mut planner = Planner {
        model,
        lookup: &lookup,
        types,
        access,
        action: None,
        json,
    };
    let mut out = Vec::new();
    let mut at = HashMap::new();
    for (key, body) in bodies {
        planner.action = (key.kind() == Some(Kind::Action)).then(|| key.name().to_string());
        for (site, _, _) in body.writes() {
            if let Some(plan) = planner.write(body, site) {
                at.insert((key.clone(), site.0), out.len() as u32);
                out.push(plan);
            }
        }
    }
    (out, at)
}

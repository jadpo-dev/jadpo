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
use jadpo_std::read::{Field, Record, SqlType};
use jadpo_std::write::{Returned, WriteOp, WriteParam, WritePlan, WriteSource};

use crate::queries::{
    MissingPlan, MissingValue, Planner, column_type, field, quoted, where_clause,
};
use crate::types::{Types, ValidatorId};

/// What one write site runs, and what its refusals raise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteSitePlan {
    pub plan: WritePlan,
    /// The sites worked out before it runs, in order: its record, then
    /// the values its block gives. A value a field stores is kept the way
    /// its field's type keeps it (a decimal at its scale), with that
    /// field's validator.
    pub inputs: Vec<(SiteId, Option<ValidatorId>)>,
    /// What each of the plan's refusals raises, in the same order.
    pub refusals: Vec<Refusal>,
    /// A write by `id` that finds no record: the entity's not-found
    /// failure, given the `id`.
    pub missing: Option<MissingPlan>,
}

/// What a refused write raises.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The failure the `conflict:` arm at this position names.
    Arm(u32),
    /// A reference naming a record that isn't there: that entity's
    /// not-found failure.
    NotFound(MissingPlan),
    /// A reference read from a record loaded earlier, whose record was
    /// deleted since: a fault saying so.
    Gone(String),
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
        format!("${}", at + 1)
    }

    /// The input `site`, worked out before the write runs.
    fn input(&mut self, site: SiteId, validator: Option<ValidatorId>) -> u32 {
        self.inputs.push((site, validator));
        (self.inputs.len() - 1) as u32
    }
}

impl Planner<'_> {
    /// The plan of the write at `site` of `body`.
    pub(crate) fn write(&mut self, body: &Body, site: SiteId) -> Option<WriteSitePlan> {
        let Node::Expr(Expr::Write {
            entity,
            op,
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
        let table = quoted(&entity_model.table);
        let mut writing = Writing::default();
        // Where each reference's value comes from, for a refusal's failure.
        let mut written_refs: Vec<(String, MissingValue)> = Vec::new();
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
                let mut names = Vec::new();
                let mut values = Vec::new();
                for field in &entity_model.fields {
                    let ty = column_type(field)?.param();
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
                    names.push(quoted(&field.born));
                    values.push(writing.param(source, ty));
                }
                format!(
                    "INSERT INTO {table} ({})\nVALUES ({})\nRETURNING {}",
                    names.join(", "),
                    values.join(", "),
                    returning(entity_model)
                )
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
                if *op == WriteOp::Update {
                    // `set:`, in the order written.
                    for (key, value) in set {
                        let field_name = self.types.field_name(key).to_string();
                        let field = field(entity_model, &field_name)?;
                        let ty = column_type(field)?.param();
                        let validator = self.types.validator_id(&TypeRef::Field(key.clone()));
                        let sql = if *by_id && stored_in(body, *value) {
                            self.stored_sql(body, *value, entity_model, ty, &mut writing)?
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
                            writing.param(WriteSource::Input(input), ty)
                        };
                        assignments.push(format!("{} = {sql}", quoted(&field.born)));
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
                            let value = writing.param(
                                WriteSource::Field {
                                    input,
                                    field: name.clone(),
                                },
                                ty,
                            );
                            let column = quoted(&field.born);
                            assignments.push(format!(
                                "{column} = CASE WHEN {supplied} THEN {value} ELSE {column} END"
                            ));
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
                                let value = writing.param(WriteSource::Input(at), ty);
                                let column = quoted(&field.born);
                                assignments.push(format!(
                                    "{column} = CASE WHEN {supplied} THEN {value} ELSE {column} END"
                                ));
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
                        let column = quoted(&field.born);
                        if let Some(input) = patch_input {
                            let any = writing.param(
                                WriteSource::AnySupplied {
                                    input,
                                    fields: supplied_fields.clone(),
                                },
                                SqlType::Bool,
                            );
                            assignments.push(format!(
                                "{column} = CASE WHEN {any} THEN {now} ELSE {column} END"
                            ));
                        } else {
                            assignments.push(format!("{column} = {now}"));
                        }
                    }
                    if assignments.is_empty() {
                        assignments.push("\"id\" = \"id\"".to_string());
                    }
                }
                let id = writing.param(id_source, SqlType::Uuid);
                let mut conditions = vec![format!("\"id\" = {id}")];
                conditions.extend(self.access(entity_model));
                let mut sql = match op {
                    WriteOp::Update => {
                        format!("UPDATE {table}\nSET {}", assignments.join(",\n    "))
                    }
                    _ => format!("DELETE FROM {table}"),
                };
                where_clause(&mut sql, &conditions, "");
                if *op == WriteOp::Update {
                    sql.push_str(&format!("\nRETURNING {}", returning(entity_model)));
                }
                sql
            }
        };
        // What each refusal raises: an arm's failure, a missing record's
        // not-found failure, or a record deleted since it was loaded.
        let mut refusals: Vec<String> = Vec::new();
        let mut raises: Vec<Refusal> = Vec::new();
        for (at, arm) in conflicts.iter().enumerate() {
            let Some(constraint) = self.constraint(&arm.rule) else {
                continue;
            };
            refusals.push(constraint);
            raises.push(Refusal::Arm(at as u32));
        }
        for (field_name, value) in written_refs {
            let Some(field) = field(entity_model, &field_name) else {
                continue;
            };
            let Some(reference) = &field.reference else {
                continue;
            };
            let constraint = foreign_key(entity_model, field);
            if refusals.contains(&constraint) {
                continue;
            }
            let may_be_missing = missing
                .iter()
                .any(|(key, _)| self.types.field_name(key) == field_name);
            let raise = if may_be_missing {
                let target = model.entity(&reference.target)?;
                Refusal::NotFound(self.not_found(target, value)?)
            } else {
                Refusal::Gone(format!(
                    "the `{}` this {} refers to was deleted after it was loaded",
                    reference.target,
                    op.word()
                ))
            };
            refusals.push(constraint);
            raises.push(raise);
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
                params: writing.params,
                returns,
                by_id: *by_id,
                refusals,
            },
            inputs: writing.inputs,
            refusals: raises,
            missing,
        })
    }

    /// A value an update by `id` works out from stored fields, as SQL: a
    /// stored field is its column, `+`, `-` and a construction (which the
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
                quoted(&field.born)
            }
            Node::Expr(Expr::Binary {
                op: op @ (BinaryOp::Add | BinaryOp::Subtract),
                lhs,
                rhs,
            }) => {
                let lhs = self.stored_sql(body, *lhs, entity, ty, writing)?;
                let rhs = self.stored_sql(body, *rhs, entity, ty, writing)?;
                let op = if *op == BinaryOp::Add { "+" } else { "-" };
                format!("({lhs} {op} {rhs})")
            }
            Node::Expr(Expr::Unary {
                op: UnaryOp::Negate,
                operand,
            }) => format!(
                "(-{})",
                self.stored_sql(body, *operand, entity, ty, writing)?
            ),
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
        let (owner, part) = name.split_once('.')?;
        let entity = self.model.entity(owner)?;
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
    fn returned(&self, entity: &EntityModel) -> Option<Returned> {
        let mut columns = Vec::new();
        let mut fields = Vec::new();
        for (at, field) in entity.fields.iter().enumerate() {
            columns.push(self.column(entity, field)?);
            fields.push((field.name.clone(), Field::Column(at as u32)));
        }
        Some(Returned {
            columns,
            record: Record {
                ty: entity.name.clone(),
                fields,
            },
        })
    }
}

/// Every column of an entity, as `RETURNING` lists them.
fn returning(entity: &EntityModel) -> String {
    entity
        .fields
        .iter()
        .map(|field| quoted(&field.born))
        .collect::<Vec<_>>()
        .join(", ")
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
) -> (Vec<WriteSitePlan>, HashMap<(Key, u32), u32>) {
    let model = jadpo_check::data::data_model(db, project);
    let lookup = jadpo_check::lookups::Lookup::new(db, project);
    let mut planner = Planner {
        model,
        lookup: &lookup,
        types,
    };
    let mut out = Vec::new();
    let mut at = HashMap::new();
    for (key, body) in bodies {
        for (site, _, _) in body.writes() {
            if let Some(plan) = planner.write(body, site) {
                at.insert((key.clone(), site.0), out.len() as u32);
                out.push(plan);
            }
        }
    }
    (out, at)
}

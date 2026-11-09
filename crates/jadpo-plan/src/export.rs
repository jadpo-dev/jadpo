//! A person's export (area 15, "Export"): what `User.export(id)` reads,
//! which both executors run through the runtime
//! (`jadpo_runtime::export`). It starts where erasure does, from the walk
//! `jadpo_sql`'s `erased` writes, read rather than deleted: every record
//! erasure deletes, the person's and those it reaches, and each record it
//! keeps that refers to one of them, but another person's own, which holds
//! that person's data and not this one's. Hidden records are in it, as
//! they are for erasure, and a person erased and kept field by field is
//! not: nothing is held about them any more.

use std::collections::BTreeMap;

use jadpo_check::ai_personal::Holder;
use jadpo_check::data::{DataModel, EntityModel};
use jadpo_check::erasure::{Erasure, exported_fields};
use jadpo_check::lookups::Lookup;
use jadpo_check::personal_data::{Sent, sent};
use jadpo_check::policy::Effect;
use jadpo_graph::{Key, Kind, TypeRef};
use jadpo_sql::statement as text;
use jadpo_std::erasure::{ExportKind, ExportPart, ExportPlan, ExportRetention};
use jadpo_std::read::{Cardinality, Column, Field, ReadPlan, Record, Select, Source, SqlType};

use crate::policy::Row;
use crate::queries::{Columns, Planner, Statement, borns, both, field};
use crate::types::Types;

/// Each person entity's `export` that `keep` keeps, by the entity's key, in
/// key order; not one the check refuses as not built yet. `setting` numbers
/// a retention's setting.
pub(crate) fn plans(
    db: &dyn jadpo_db::Db,
    project: jadpo_db::Project,
    types: &mut Types,
    json: &std::cell::RefCell<Vec<TypeRef>>,
    keep: &dyn Fn(&Key) -> bool,
    setting: &dyn Fn(&str) -> Option<u32>,
) -> Vec<(Key, ExportPlan)> {
    let model = jadpo_check::data::data_model(db, project);
    let erasures = jadpo_check::erasure::erasures(db, project);
    let lookup = Lookup::new(db, project);
    let access = jadpo_check::policy::access(db, project);
    let sent = sent(db, project);
    let mut planner = Planner {
        model,
        lookup: &lookup,
        types,
        access,
        action: None,
        json,
        sign_in: Default::default(),
        retired: Default::default(),
        provider_keys: Default::default(),
    };
    let mut out: Vec<(Key, ExportPlan)> = model
        .entities
        .iter()
        .filter(|entity| entity.person)
        .filter_map(|person| {
            let erasure = erasures.people.get(&person.name)?;
            if !erasure.export_unbuilt.is_empty() {
                return None;
            }
            let key = planner.types.named(&person.name)?;
            if !keep(&key) {
                return None;
            }
            Some((key, planner.export(model, person, erasure, sent, setting)?))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

impl Planner<'_> {
    fn export(
        &mut self,
        model: &DataModel,
        person: &EntityModel,
        erasure: &Erasure,
        sent: &[Sent],
        setting: &dyn Fn(&str) -> Option<u32>,
    ) -> Option<ExportPlan> {
        let store = person.store.clone()?;
        let access = self.export_access(person, erasure, &store)?;
        let mut parts = Vec::new();
        // The person's store first, so their own record is first.
        let mut stores: Vec<(&String, &Erasure)> = erasure.stores.iter().collect();
        stores.sort_by_key(|(name, _)| **name != store);
        for (name, here) in stores {
            let own = *name == store;
            let mut deleted: Vec<&EntityModel> = Vec::new();
            for entity in &here.deleted {
                deleted.push(model.entity(entity)?);
            }
            let follows = here
                .follows
                .iter()
                .map(|follow| {
                    let holder = deleted.get(follow.to)?;
                    Some(text::Followed {
                        from: follow.from,
                        to: follow.to,
                        table: holder.table.as_str(),
                        column: field(holder, &follow.field)?.born.as_str(),
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            // What picks each entity's records, in the order they are found.
            let mut picks: Vec<(&EntityModel, Vec<String>)> = Vec::new();
            let mut note = |entity: &'_ EntityModel, condition: String| match picks
                .iter_mut()
                .find(|(known, _)| known.name == entity.name)
            {
                Some((_, conditions)) => conditions.push(condition),
                None => picks.push((
                    model.entity(&entity.name).expect("in the model"),
                    vec![condition],
                )),
            };
            // Elsewhere the person's own table isn't there, so the first of
            // `deleted`, the person, is only the walk's root.
            for (at, entity) in deleted.iter().enumerate().skip(usize::from(!own)) {
                note(entity, text::found_in("id", at));
            }
            for kept in &here.kept {
                let holder = model.entity(&kept.entity)?;
                if holder.person {
                    continue;
                }
                for reference in &kept.references {
                    let column = field(holder, &reference.field)?.born.as_str();
                    note(holder, text::found_in(column, reference.target));
                }
            }
            for (entity, conditions) in picks {
                parts.push(self.export_part(
                    entity,
                    name,
                    &deleted,
                    &follows,
                    conditions,
                    erasure_keeps_own(here, &deleted, entity),
                )?);
            }
        }
        let entities: Vec<&str> = parts.iter().map(|part| part.entity.as_str()).collect();
        let mut kinds = Vec::new();
        let mut listed: Vec<&str> = Vec::new();
        for entity in entities {
            if listed.contains(&entity) {
                continue;
            }
            listed.push(entity);
            kinds.push(export_kind(model.entity(entity)?, sent, setting)?);
        }
        Some(ExportPlan {
            entity: person.name.clone(),
            access,
            parts,
            kinds,
        })
    }

    /// The person's `id`, when the caller may read their record and they
    /// haven't been erased: what an export asks first, under `read`, hidden
    /// or not, as erasure reaches them under `delete`.
    fn export_access(
        &mut self,
        person: &EntityModel,
        erasure: &Erasure,
        store: &str,
    ) -> Option<ReadPlan> {
        let mut statement = Statement::default();
        let id = statement.param(
            Source::Arg {
                index: 0,
                path: Vec::new(),
            },
            SqlType::Uuid,
        );
        let mut conditions = vec![text::equal("id", &id)];
        let keeps_own = erasure.own.iter().any(|own| own.entity == 0);
        if keeps_own {
            conditions.push(text::is_null(jadpo_std::ERASED_AT));
        }
        let policy = self.policy(&mut statement, person, Effect::Read, &Row::Table)?;
        let names = ["id"];
        let (sql, unpoliced) = both(
            &|conditions: &[String]| {
                let mut sql = text::select(&names, &person.table, "");
                text::where_clause(&mut sql, conditions, "");
                sql
            },
            conditions,
            policy,
        );
        Some(ReadPlan {
            store: store.to_string(),
            settings: Vec::new(),
            cardinality: Cardinality::Optional,
            select: Select {
                sql,
                unpoliced,
                permitted: None,
                params: statement.into_params(),
                columns: vec![Column::Uuid],
                held: Vec::new(),
                record: Record {
                    ty: person.name.clone(),
                    fields: vec![("id".to_string(), Field::Column(0))],
                },
                includes: Vec::new(),
                versions: None,
                sensitive: None,
            },
            page: None,
            missing: None,
            recheck: None,
            layer: false,
            gate: None,
        })
    }

    /// One entity's records in one store: those `conditions` pick among
    /// what the walk found, with its exported fields, each as the plan's
    /// JSON table writes it.
    fn export_part(
        &mut self,
        entity: &EntityModel,
        store: &str,
        deleted: &[&EntityModel],
        follows: &[text::Followed<'_>],
        conditions: Vec<String>,
        keeps_own: bool,
    ) -> Option<ExportPart> {
        let mut columns = Columns::default();
        let mut record = Vec::new();
        let mut fields = Vec::new();
        let lookup: &Lookup<'_> = self.lookup;
        for field in exported_fields(lookup, entity) {
            let kind = self.column(entity, field)?;
            let at = columns.add(&field.name, kind);
            record.push((field.name.clone(), Field::Column(at)));
            let id = self
                .types
                .graph()
                .named(Kind::Field, &format!("{}.{}", entity.name, field.name))?;
            // A `File` is exported as its signed link (area 16,
            // "Downloading"), written by the link's codec.
            let number = match &field.file {
                Some(_) => {
                    let link = TypeRef::Std(jadpo_std::files::FILE_LINK.to_string(), Vec::new());
                    let nullable = matches!(field.ty, jadpo_check::types::Ty::Nullable(_));
                    self.json_number(if nullable {
                        TypeRef::Nullable(Box::new(link))
                    } else {
                        link
                    })?
                }
                None => {
                    self.json_number(TypeRef::Field(self.types.graph().node(id).key.clone()))?
                }
            };
            fields.push((field.name.clone(), number));
        }
        let mut statement = Statement::default();
        let id = statement.param(
            Source::Arg {
                index: 0,
                path: Vec::new(),
            },
            SqlType::Uuid,
        );
        let tables: Vec<&str> = deleted.iter().map(|entity| entity.table.as_str()).collect();
        let picked = text::export_pick(&conditions, keeps_own.then_some(jadpo_std::ERASED_AT));
        let held = self.rules_held(entity, &columns.names, &columns.kinds);
        let sql = text::export_records(
            text::EraseRoot::Given(&id),
            &tables,
            follows,
            &borns(entity, &columns.names),
            &entity.table,
            &[picked],
        );
        Some(ExportPart {
            entity: entity.name.clone(),
            read: ReadPlan {
                store: store.to_string(),
                settings: Vec::new(),
                cardinality: Cardinality::List,
                select: Select {
                    sql,
                    unpoliced: None,
                    permitted: None,
                    params: statement.into_params(),
                    columns: columns.kinds,
                    held,
                    record: Record {
                        ty: entity.name.clone(),
                        fields: record,
                    },
                    includes: Vec::new(),
                    versions: None,
                    sensitive: None,
                },
                page: None,
                missing: None,
                recheck: None,
                layer: false,
                gate: None,
            },
            fields,
        })
    }
}

/// Whether records of `entity` in this store's walk may be someone erased
/// and kept field by field, who holds nothing now: a person whose own
/// `Erasure.retained` field keeps theirs.
fn erasure_keeps_own(here: &Erasure, deleted: &[&EntityModel], entity: &EntityModel) -> bool {
    here.own.iter().any(|own| {
        deleted
            .get(own.entity)
            .is_some_and(|held| held.name == entity.name)
    })
}

/// How long `entity`'s data is kept and which connections receive it, as
/// the personal-data report has them.
fn export_kind(
    entity: &EntityModel,
    sent: &[Sent],
    setting: &dyn Fn(&str) -> Option<u32>,
) -> Option<ExportKind> {
    let mut retention = Vec::new();
    if let Some(kept) = &entity.retention {
        retention.push(ExportRetention {
            field: None,
            setting: setting(&kept.setting)?,
            since: kept.since.clone(),
        });
    }
    for field in &entity.fields {
        if let Some(kept) = &field.retention {
            retention.push(ExportRetention {
                field: Some(field.name.clone()),
                setting: setting(&kept.setting)?,
                since: kept.since.clone(),
            });
        }
    }
    let mut recipients: BTreeMap<&str, ()> = BTreeMap::new();
    for sent in sent {
        if let Holder::Field { host, .. } = &sent.holder
            && *host == entity.name
        {
            recipients.insert(&sent.connection, ());
        }
    }
    Some(ExportKind {
        entity: entity.name.clone(),
        retention,
        recipients: recipients.keys().map(|name| name.to_string()).collect(),
    })
}

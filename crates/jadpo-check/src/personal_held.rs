//! What the personal-data report says the app itself holds (area 15, "The
//! personal-data report", "Where data is stored" and "What this does not
//! cover"): for each person entity every marked field with its category,
//! store and who may read it, its retention, what erasure does to each of
//! their records and the queries that say `include_erased: true`; then
//! each store with where it is set, the step records of actions with
//! several changing calls, and what the language covers.
//!
//! [`crate::personal_data::report`] puts these beside where personal data
//! goes, and each section's JSON sits beside `connections`.

use std::collections::BTreeMap;

use jadpo_db::{Db, Project};
use jadpo_graph::json::Value;
use jadpo_std::Erasure as Outcome;

use crate::ai_personal::{self, Holding};
use crate::data::{self, ConnectionKind, DataModel, EntityModel, FieldModel};
use crate::erasure::Erasure;
use crate::lookups::Lookup;
use crate::policy::{self, report::who_reads};

/// A list as a report writes it: `` `a`, `b` ``.
fn fields(names: &[String]) -> String {
    names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a field holds, for a report: `` `contact` (an `Email`) ``, or
/// `` `health`, sensitive ``.
fn category_cell(holding: &Holding) -> String {
    holding
        .categories
        .iter()
        .map(|(category, from)| {
            let mut cell = format!("`{category}`");
            if ai_personal::is_sensitive(category) {
                cell.push_str(", sensitive");
            }
            if let Some(ty) = from {
                cell.push_str(&format!(" ({})", ai_personal::a_type(ty)));
            }
            cell
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// How long a field is kept: its own retention or its entity's.
fn kept(entity: &EntityModel, field: &FieldModel) -> (String, Option<Value>) {
    match field.retention.as_ref().or(entity.retention.as_ref()) {
        Some(retention) => {
            let mut json = Value::object();
            json.set("after", retention.after_written.as_str());
            json.set("since", retention.since.as_str());
            (
                format!("`{}` since `{}`", retention.after_written, retention.since),
                Some(json),
            )
        }
        None => ("kept until erased".to_string(), None),
    }
}

/// One marked field, as a report's row and its JSON: the cells are the
/// category, the store, who may read it and how long it is kept.
struct FieldRow {
    cells: [String; 5],
    json: Value,
}

fn field_row(
    db: &dyn Db,
    project: Project,
    entity: &EntityModel,
    field: &FieldModel,
    holding: &Holding,
) -> FieldRow {
    let access = policy::access(db, project);
    let readers = who_reads(access.entity(&entity.name), &field.name);
    let store = entity.store.clone().unwrap_or_default();
    let (keep, retention) = kept(entity, field);
    let cells = [
        format!("`{}.{}`", entity.name, field.name),
        category_cell(holding),
        format!("`{store}`"),
        readers.clone(),
        keep.clone(),
    ];
    let mut json = Value::object();
    json.set("entity", entity.name.as_str());
    json.set("field", field.name.as_str());
    let categories: Vec<String> = holding.categories.keys().cloned().collect();
    json.set("categories", categories.clone());
    json.set(
        "sensitive",
        categories.iter().any(|c| ai_personal::is_sensitive(c)),
    );
    json.set("store", store);
    // No entity can have a cache yet (area 5, `caches:` is refused).
    json.set("caches", Vec::<Value>::new());
    json.set("read_by", readers);
    match retention {
        Some(retention) => json.set("retention", retention),
        None => json.set("kept_until_erased", true),
    }
    FieldRow { cells, json }
}

/// The marked fields of `entity`, each with what it holds.
fn held<'m>(
    lookup: &Lookup<'_>,
    marks: &ai_personal::Marks,
    entity: &'m EntityModel,
) -> Vec<(&'m FieldModel, Holding)> {
    entity
        .fields
        .iter()
        .filter_map(|field| {
            let holding = ai_personal::field_holding(lookup, marks, &entity.name, &field.name)?;
            (!holding.categories.is_empty()).then_some((field, holding))
        })
        .collect()
}

/// One row of "what erasure does": its record, how it got there, the store
/// and what happens.
struct ErasureRow {
    entity: String,
    through: Vec<String>,
    store: String,
    outcome: &'static str,
    said: String,
    clears: Vec<String>,
    keeps: Vec<String>,
}

impl ErasureRow {
    fn json(&self) -> Value {
        let mut json = Value::object();
        json.set("entity", self.entity.as_str());
        json.set("through", self.through.clone());
        json.set("store", self.store.as_str());
        json.set("outcome", self.outcome);
        json.set("clears", self.clears.clone());
        json.set("keeps", self.keeps.clone());
        json
    }
}

/// What erasing one `person` does to each record that belongs to them.
fn erasure_rows(model: &DataModel, person: &str, erasure: &Erasure) -> Vec<ErasureRow> {
    let store_of = |entity: &str| {
        model
            .entity(entity)
            .and_then(|entity| entity.store.clone())
            .unwrap_or_default()
    };
    let home = store_of(person);
    let after_commit = |store: &str| {
        if store == home {
            ""
        } else {
            ", once the person's own commit has gone through"
        }
    };
    let mut rows = Vec::new();
    for (at, entity) in erasure.deleted.iter().enumerate() {
        let through: Vec<String> = erasure
            .follows
            .iter()
            .filter(|follow| follow.to == at)
            .map(|follow| follow.field.clone())
            .collect();
        let store = store_of(entity);
        let own = erasure.own.iter().find(|own| own.entity == at);
        let (outcome, said, clears, keeps) = match own {
            Some(own) => (
                "kept_field_by_field",
                format!(
                    "Kept, hidden from every query that doesn't say `include_erased: true`. Keeps {}. Clears {}",
                    fields(&own.retained),
                    if own.clears.is_empty() {
                        "nothing".to_string()
                    } else {
                        fields(&own.clears)
                    }
                ),
                own.clears.clone(),
                own.retained.clone(),
            ),
            None => ("deleted", "Deleted".to_string(), Vec::new(), Vec::new()),
        };
        rows.push(ErasureRow {
            entity: entity.clone(),
            through,
            said: format!("{said}{}.", after_commit(&store)),
            outcome,
            store,
            clears,
            keeps,
        });
    }
    for kept in &erasure.kept {
        let store = store_of(&kept.entity);
        for reference in &kept.references {
            let (outcome, opening) = match reference.outcome {
                Outcome::Retained => (
                    "retained",
                    "Kept until its retention ends. Clears the reference",
                ),
                _ => ("anonymised", "Anonymised. Clears the reference"),
            };
            let also = if kept.clears.is_empty() {
                String::new()
            } else if kept.through.is_empty() {
                format!(" and {}", fields(&kept.clears))
            } else {
                format!(
                    ", and {} once no other person it belongs to remains",
                    fields(&kept.clears)
                )
            };
            rows.push(ErasureRow {
                entity: kept.entity.clone(),
                through: vec![reference.field.clone()],
                store: store.clone(),
                outcome,
                said: format!("{opening}{also}{}.", after_commit(&store)),
                clears: kept.clears.clone(),
                keeps: Vec::new(),
            });
        }
    }
    rows
}

/// The queries that say `include_erased: true` on `person`.
fn reading_kept(db: &dyn Db, project: Project, person: &str) -> Vec<String> {
    data::queries::query_models(db, project)
        .queries
        .iter()
        .filter(|query| query.include_erased && query.entity == person)
        .map(|query| query.name.clone())
        .collect()
}

/// A table with the given header and rows.
fn table(head: &[&str], rows: impl IntoIterator<Item = Vec<String>>) -> String {
    let mut out = format!("| {} |\n|{}\n", head.join(" | "), "---|".repeat(head.len()));
    for row in rows {
        out.push_str(&format!("| {} |\n", row.join(" | ")));
    }
    out
}

/// The sections for each person entity, and its JSON array.
pub fn people(db: &dyn Db, project: Project) -> (String, Vec<Value>) {
    let model = data::data_model(db, project);
    let erasures = crate::erasure::erasures(db, project);
    let lookup = Lookup::new(db, project);
    let marks = ai_personal::marks(db, project);
    let mut markdown = String::new();
    let mut json = Vec::new();
    for person in model.entities.iter().filter(|entity| entity.person) {
        let Some(erasure) = erasures.people.get(&person.name) else {
            continue;
        };
        let mut rows = Vec::new();
        let mut field_json = Vec::new();
        for entity in crate::erasure::exported_entities(model, erasure) {
            for (field, holding) in held(&lookup, marks, entity) {
                let row = field_row(db, project, entity, field, &holding);
                rows.push(row.cells.to_vec());
                field_json.push(row.json);
            }
        }
        markdown.push_str(&format!("## Person `{}`\n\n", person.name));
        if rows.is_empty() {
            markdown.push_str("No field of their records is marked as personal data.\n\n");
        } else {
            markdown.push_str(&table(
                &["Field", "Category", "Store", "Who may read it", "Kept for"],
                rows,
            ));
            markdown.push('\n');
        }
        let erasure_rows = erasure_rows(model, &person.name, erasure);
        markdown.push_str(&format!("### Erasing a `{}`\n\n", person.name));
        markdown.push_str(&table(
            &["Record", "Reached through", "Store", "What happens"],
            erasure_rows.iter().map(|row| {
                vec![
                    format!("`{}`", row.entity),
                    if row.through.is_empty() {
                        "the person".to_string()
                    } else {
                        fields(&row.through)
                    },
                    format!("`{}`", row.store),
                    row.said.clone(),
                ]
            }),
        ));
        markdown.push('\n');
        for unbuilt in &erasure.unbuilt {
            markdown.push_str(&format!(
                "Erasing them isn't possible yet: {unbuilt}. Calling `{}.erase` is refused.\n\n",
                person.name
            ));
        }
        let queries = reading_kept(db, project, &person.name);
        if !queries.is_empty() {
            markdown.push_str(&format!(
                "Queries that read what erasure keeps: {}.\n\n",
                fields(&queries)
            ));
        }
        let mut entry = Value::object();
        entry.set("entity", person.name.as_str());
        entry.set("fields", field_json);
        entry.set(
            "erasure",
            erasure_rows
                .iter()
                .map(ErasureRow::json)
                .collect::<Vec<_>>(),
        );
        entry.set("include_erased", queries);
        json.push(entry);
    }
    (markdown, json)
}

/// Personal data on an entity no person's records reach: a shop's own
/// phone number, which only a standard type makes personal data. A field
/// marked `personal:` here is `data.personal_unreachable`.
pub fn no_person(db: &dyn Db, project: Project) -> (String, Vec<Value>) {
    let model = data::data_model(db, project);
    let erasures = crate::erasure::erasures(db, project);
    let lookup = Lookup::new(db, project);
    let marks = ai_personal::marks(db, project);
    let mut rows = Vec::new();
    let mut json = Vec::new();
    for entity in &model.entities {
        if erasures.reaches(entity) {
            continue;
        }
        for (field, holding) in held(&lookup, marks, entity) {
            let row = field_row(db, project, entity, field, &holding);
            rows.push(row.cells.to_vec());
            json.push(row.json);
        }
    }
    if rows.is_empty() {
        return (String::new(), json);
    }
    let mut markdown = String::from(
        "## Personal data that belongs to no person\n\nA standard type makes these personal data, but no person's records hold them, so erasing someone doesn't touch them.\n\n",
    );
    markdown.push_str(&table(
        &["Field", "Category", "Store", "Who may read it", "Kept for"],
        rows,
    ));
    markdown.push('\n');
    (markdown, json)
}

/// Each store with what says where it is (area 15, "Where data is stored").
pub fn stores(db: &dyn Db, project: Project) -> (String, Vec<Value>) {
    let connections = data::connections(db, project);
    let urls = crate::config::connection_urls(db, project);
    let mut found: BTreeMap<String, &'static str> = BTreeMap::new();
    for store in connections.databases() {
        found.insert(store, "database");
    }
    for store in connections.files() {
        found.insert(store, "files");
    }
    for (store, kind) in &connections.declared {
        if *kind == ConnectionKind::Redis {
            found.insert(store.clone(), "cache");
        }
    }
    let mut rows = Vec::new();
    let mut json = Vec::new();
    for (store, kind) in found {
        let setting = urls
            .urls
            .iter()
            .find(|url| url.connection == store)
            .map(|url| url.setting.clone());
        let mut entry = Value::object();
        entry.set("store", store.as_str());
        entry.set("kind", kind);
        let where_set = match (&setting, kind) {
            (Some(setting), _) => {
                entry.set("setting", setting.as_str());
                format!("`config.{setting}`")
            }
            (None, "files") => "the `path:` its connection gives in `app.jadpo`".to_string(),
            (None, _) => "not set: no setting says where it is".to_string(),
        };
        rows.push(vec![format!("`{store}`"), kind.to_string(), where_set]);
        json.push(entry);
    }
    let mut markdown = String::from(
        "## Stores\n\nEach store is one database in one place. An app that must keep data in a region runs one deployment per region, each with its own stores.\n\n",
    );
    markdown.push_str(&table(&["Store", "Holds", "Where it is set"], rows));
    markdown.push('\n');
    (markdown, json)
}

/// The step records of actions with several changing calls.
pub fn step_records(db: &dyn Db, project: Project) -> (String, Vec<Value>) {
    let steps = crate::types::action_steps(db, project);
    let actions: Vec<(String, Option<String>)> = steps
        .iter()
        .filter(|(_, steps)| steps.several())
        .map(|(key, steps)| (key.name().to_string(), steps.first.clone()))
        .collect();
    let mut markdown = String::from(
        "## Step records\n\nAn action with several changing calls records each call's answer, encrypted, until the action commits. An answer over 1 MB is a fault.\n\n",
    );
    if actions.is_empty() {
        markdown.push_str("No action makes several changing calls.\n\n");
    } else {
        markdown.push_str(&table(
            &["Action", "First changing call"],
            actions.iter().map(|(action, first)| {
                vec![
                    format!("`{action}`"),
                    first
                        .as_deref()
                        .map_or_else(String::new, |call| format!("`{call}`")),
                ]
            }),
        ));
        markdown.push('\n');
    }
    let json = actions
        .into_iter()
        .map(|(action, first)| {
            let mut entry = Value::object();
            entry.set("action", action);
            if let Some(first) = first {
                entry.set("first_changing_call", first);
            }
            entry
        })
        .collect();
    (markdown, json)
}

/// What a report always says about grants, which aren't built yet.
pub const OAUTH: &str =
    "## OAuth grants\n\nNo connection holds an OAuth grant: grants aren't built yet.\n\n";

/// What the language covers, what is the app's choice and what isn't code
/// (area 15, "What this does not cover").
const COVERAGE: [(&str, &str, &[&str]); 4] = [
    (
        "covered",
        "Covered by the language",
        &[
            "minimisation and storage limitation",
            "access",
            "rectification, by ordinary updates",
            "erasure",
            "portability",
            "privacy by design",
            "records of processing, most of Article 30",
            "security of processing",
        ],
    ),
    (
        "app_choice",
        "Supported, but the app's choice",
        &["restriction and objection", "consent"],
    ),
    (
        "not_code",
        "Not code at all",
        &[
            "the lawful basis",
            "the privacy notice",
            "data protection impact assessments (DPIAs)",
            "processor contracts",
            "the data protection officer (DPO)",
        ],
    ),
    (
        "to_add",
        "Still to add",
        &[
            "breach notification, which lists the people whose marked data a breached store or provider held",
            "international transfers, where provider packages declare where data is processed and this report shows it",
        ],
    ),
];

/// The coverage section and its JSON.
pub fn coverage() -> (String, Value) {
    let mut markdown = String::from("## What this report covers\n\n");
    let mut json = Value::object();
    for (key, heading, items) in COVERAGE {
        markdown.push_str(&format!("- **{heading}:** {}.\n", items.join(", ")));
        json.set(
            key,
            items
                .iter()
                .map(|item| item.to_string())
                .collect::<Vec<_>>(),
        );
    }
    markdown.push('\n');
    markdown.push_str("This is not legal advice. A privacy professional reviews this report.\n");
    (markdown, json)
}

/// A person's JSON as the lines a review compares.
pub fn person_lines(person: &Value) -> Vec<String> {
    let text = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let strings = |value: &Value, key: &str| -> Vec<String> {
        value
            .get(key)
            .and_then(Value::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    };
    let mut lines = Vec::new();
    for field in person
        .get("fields")
        .and_then(Value::as_array)
        .unwrap_or_default()
    {
        let mut categories = strings(field, "categories");
        if field.get("sensitive") == Some(&Value::Bool(true)) {
            categories.push("sensitive".to_string());
        }
        let kept = match field.get("retention") {
            Some(retention) => format!(
                "kept {} since {}",
                text(retention, "after"),
                text(retention, "since")
            ),
            None => "kept until erased".to_string(),
        };
        lines.push(format!(
            "`{}.{}` ({}): in `{}`, read by {}, {kept}",
            text(field, "entity"),
            text(field, "field"),
            categories.join(", "),
            text(field, "store"),
            text(field, "read_by"),
        ));
    }
    for row in person
        .get("erasure")
        .and_then(Value::as_array)
        .unwrap_or_default()
    {
        let mut line = format!(
            "erasing `{}` through {}: {}",
            text(row, "entity"),
            strings(row, "through").join(", "),
            text(row, "outcome"),
        );
        let clears = strings(row, "clears");
        if !clears.is_empty() {
            line.push_str(&format!(", clears {}", clears.join(", ")));
        }
        lines.push(line);
    }
    for query in strings(person, "include_erased") {
        lines.push(format!("query `{query}` reads what erasure keeps"));
    }
    lines.sort();
    lines
}

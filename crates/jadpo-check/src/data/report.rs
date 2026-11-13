//! The schema report (area 14, "The `build/` folder"; area 5, "Indexes"):
//! `build/report/database.md` and its `.json` twin, every table, column,
//! key and index each store holds, each index with the queries that read
//! it. And what the query report reads from here: for each statement a
//! query sends, the table it reads and the key or index that serves it,
//! found as the compiler found the indexes the queries need, so the two
//! can't disagree.

use std::collections::BTreeMap;

use jadpo_db::{Db, Project};
use jadpo_graph::json::Value;
use jadpo_graph::schema::{Holds, Table};
use jadpo_std::read::Cardinality;

use super::queries::{self, Bound, Condition, Include, IncludeKind, OrderField, QueryModel};
use super::{DataModel, EntityModel, data_model, schema};
use crate::lookups::Lookup;
use crate::types::{Ty, TypeView};

/// A report: its Markdown and its JSON twin.
#[derive(Clone, Debug)]
pub struct Report {
    pub markdown: String,
    pub json: Value,
}

// --- the columns a read needs --------------------------------------------------

/// The fields a read tests with `==` or `in` on every record, in the
/// order its lines name them.
fn equal_fields(filter: &[Condition], out: &mut Vec<String>) {
    for line in filter {
        let mut push = |field: &String| {
            if !out.contains(field) {
                out.push(field.clone());
            }
        };
        match line {
            Condition::Compare {
                field,
                op: queries::Compare::Equal,
                ..
            }
            | Condition::In { field, .. } => push(field),
            Condition::And(parts) => equal_fields(parts, out),
            _ => {}
        }
    }
}

/// The fields an index for a read starts with: `prefix`, then those it
/// always tests for equality, then its order's; and how many of them lead
/// as equality tests (area 5, "Indexes").
pub(crate) fn wanted_columns(
    prefix: &[String],
    filter: &[Condition],
    order: &[OrderField],
) -> (Vec<String>, usize) {
    let mut out = prefix.to_vec();
    equal_fields(filter, &mut out);
    let equal = out.len();
    for field in order {
        if !out.contains(&field.field) {
            out.push(field.field.clone());
        }
    }
    (out, equal)
}

/// The columns an index for a read of `fields` holds, the first `equal`
/// of them tested for equality: each field's own column, every column of
/// a standard type kept in several that is compared and those its order
/// reads of one sorted by, and none of a value kept whole as its JSON, since a B-tree's entry holds about 2.7 kB, far less than a stored
/// value may (area 5, "Indexes"). With the text columns sorted after those
/// tested for equality, which an index keeps in code-point order as the
/// read sorts them, and the columns tested for equality. `None` when a
/// field isn't the entity's.
pub(crate) fn index_columns(
    lookup: &Lookup<'_>,
    entity: &EntityModel,
    fields: &[String],
    equal: usize,
) -> Option<(Vec<String>, Vec<String>, Vec<String>)> {
    let mut born = Vec::new();
    let mut collate = Vec::new();
    let mut tested = Vec::new();
    for (at, name) in fields.iter().enumerate() {
        let field = entity.fields.iter().find(|field| field.name == *name)?;
        let columns: Vec<String> = match (field.json.is_some(), field.compound) {
            (true, _) => Vec::new(),
            // Compared, every column; sorted, the columns its order reads,
            // an amount's minor units alone.
            (false, Some(entry)) => {
                let all: Vec<String> = field
                    .columns()
                    .into_iter()
                    .map(|(column, _)| column)
                    .collect();
                if at < equal {
                    all
                } else {
                    jadpo_sql::ordered_parts(entry)
                        .into_iter()
                        .filter_map(|part| all.get(part as usize).cloned())
                        .collect()
                }
            }
            (false, None) => vec![field.column.clone()],
        };
        if at < equal {
            tested.extend(columns.iter().cloned());
        } else if field.compound.is_none()
            && lookup.representation(&Ty::Field(entity.name.clone(), vec![name.clone()]))
                == Some(jadpo_std::Builtin::Text)
        {
            collate.extend(columns.iter().cloned());
        }
        born.extend(columns);
    }
    Some((born, collate, tested))
}

/// The key or index of `table` that serves a read of the columns `born`:
/// one that starts with them, kept in the same order, or a unique key all
/// of whose columns the read tests for equality, since at most one record
/// matches. Keys come first, as the database prefers them.
pub(crate) fn serving(
    table: &Table,
    born: &[String],
    collate: &[String],
    tested: &[String],
) -> Option<String> {
    let serves = |existing: &[String], sorted: &[String]| {
        existing.len() >= born.len()
            && existing[..born.len()] == born[..]
            && born
                .iter()
                .all(|column| collate.contains(column) == sorted.contains(column))
    };
    let keys = || {
        std::iter::once((&table.primary_key.name, &table.primary_key.columns)).chain(
            table
                .unique
                .iter()
                .map(|(name, unique)| (name, &unique.columns)),
        )
    };
    keys()
        .find(|(_, columns)| serves(columns, &[]))
        .map(|(name, _)| name.clone())
        .or_else(|| {
            table
                .indexes
                .iter()
                .find(|(_, index)| {
                    index.expression.is_none() && serves(&index.columns, &index.collate)
                })
                .map(|(name, _)| name.clone())
        })
        .or_else(|| {
            keys()
                .find(|(_, columns)| columns.iter().all(|column| tested.contains(column)))
                .map(|(name, _)| name.clone())
        })
}

// --- what each statement reads -----------------------------------------------------

/// One statement a query sends: the query itself, or one of its includes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementRead {
    /// The query that sends it.
    pub query: String,
    /// The includes it is for, outermost first; empty for the query's own.
    pub include: Vec<String>,
    /// The entity it reads, and that entity's store and table.
    pub entity: String,
    pub store: Option<String>,
    pub table: String,
    /// The most records it returns, in words: `1`, `0 or 1`, `up to 20`,
    /// or for an include, per record it is included in.
    pub rows: String,
    /// The most rows, as a number, when there is one.
    pub most: Option<u64>,
    /// The key or index that serves it; `None` when it reads every record.
    pub index: Option<String>,
}

/// Every statement each query sends, in query-name order, each include
/// after the statement it is included in.
pub fn reads(db: &dyn Db, project: Project) -> Vec<StatementRead> {
    let model = data_model(db, project);
    let schema = schema(db, project);
    let lookup = Lookup::new(db, project);
    let mut queries: Vec<&QueryModel> = queries::query_models(db, project).queries.iter().collect();
    queries.sort_by(|a, b| a.name.cmp(&b.name));
    let mut out = Vec::new();
    for query in queries {
        let reader = Reader {
            model,
            schema,
            lookup: &lookup,
            query,
        };
        let (wanted, equal) = wanted_columns(&[], &query.filter, &query.order);
        let (rows, most) = query_rows(&lookup, query);
        out.push(StatementRead {
            query: query.name.clone(),
            include: Vec::new(),
            entity: query.entity.clone(),
            store: model.entity(&query.entity).and_then(|e| e.store.clone()),
            table: model
                .entity(&query.entity)
                .map(|e| e.table.clone())
                .unwrap_or_default(),
            rows,
            most,
            index: reader.index(&query.entity, &wanted, equal),
        });
        reader.includes(&query.entity, &query.includes, &[], &mut out);
    }
    out
}

struct Reader<'a> {
    model: &'a DataModel,
    schema: &'a jadpo_graph::schema::Schema,
    lookup: &'a Lookup<'a>,
    query: &'a QueryModel,
}

impl Reader<'_> {
    fn table(&self, entity: &EntityModel) -> Option<&Table> {
        self.schema
            .stores
            .get(entity.store.as_deref()?)?
            .tables
            .get(&entity.table)
    }

    /// The key or index serving a read of `entity` by the fields `wanted`.
    fn index(&self, entity: &str, wanted: &[String], equal: usize) -> Option<String> {
        let entity = self.model.entity(entity)?;
        let table = self.table(entity)?;
        let columns = index_columns(self.lookup, entity, wanted, equal);
        let born = columns
            .as_ref()
            .map(|(born, _, _)| born.clone())
            .unwrap_or_default();
        let found = columns
            .filter(|(born, _, _)| !born.is_empty())
            .and_then(|(born, collate, tested)| serving(table, &born, &collate, &tested));
        // A search, an operation or a nearest-neighbour order is served by
        // an index of its own, made for this query.
        found.or_else(|| {
            let reason = format!("query {}", self.query.name);
            table
                .indexes
                .iter()
                .find(|(_, index)| {
                    index.reason == reason
                        && (index.expression.is_some()
                            || index.method.is_some()
                            || index.columns.first() == born.first())
                })
                .map(|(name, _)| name.clone())
        })
    }

    fn includes(
        &self,
        parent: &str,
        includes: &[Include],
        under: &[String],
        out: &mut Vec<StatementRead>,
    ) {
        for include in includes {
            let mut path = under.to_vec();
            path.push(include.name.clone());
            let entity = self.model.entity(&include.entity);
            let (index, rows, most) = match include.kind {
                IncludeKind::Parent => (
                    entity
                        .and_then(|e| self.table(e))
                        .map(|t| t.primary_key.name.clone()),
                    format!("1 per `{parent}`"),
                    None,
                ),
                IncludeKind::Child => (
                    self.index(&include.entity, std::slice::from_ref(&include.via), 1),
                    format!("0 or 1 per `{parent}`"),
                    None,
                ),
                IncludeKind::Children => {
                    let (wanted, equal) = wanted_columns(
                        std::slice::from_ref(&include.via),
                        &include.filter,
                        &include.order,
                    );
                    let most = include
                        .limit
                        .as_ref()
                        .and_then(|bound| bound_most(self.lookup, self.query, bound));
                    let rows = match (&include.limit, most) {
                        (_, Some(n)) => format!("up to {n} per `{parent}`"),
                        (Some(_), None) => format!("up to its `limit:` per `{parent}`"),
                        (None, None) => format!("every one per `{parent}`"),
                    };
                    (self.index(&include.entity, &wanted, equal), rows, None)
                }
            };
            out.push(StatementRead {
                query: self.query.name.clone(),
                include: path.clone(),
                entity: include.entity.clone(),
                store: entity.and_then(|e| e.store.clone()),
                table: entity.map(|e| e.table.clone()).unwrap_or_default(),
                rows,
                most,
                index,
            });
            self.includes(&include.entity, &include.includes, &path, out);
        }
    }
}

/// The most a `limit:` or a page's `size:` lets through: its number, or
/// the `max:` of its value's type.
fn bound_most(lookup: &Lookup<'_>, query: &QueryModel, bound: &Bound) -> Option<u64> {
    match bound {
        Bound::Literal(n) => Some(*n),
        Bound::Value(jadpo_sql::Value::Param { index, path }) => {
            let (_, ty) = query.params.get(*index as usize)?;
            let ty = match (path.is_empty(), ty.strip_nullable()) {
                (true, ty) => ty.clone(),
                (false, Ty::Named(name)) => Ty::Field(name.clone(), path.clone()),
                _ => return None,
            };
            lookup
                .rule_chain(&ty)
                .iter()
                .find_map(|(_, rules)| rules.max)
                .and_then(|max| max.to_string().parse().ok())
        }
        Bound::Value(_) => None,
    }
}

/// The most records a query returns, in words and as a number.
fn query_rows(lookup: &Lookup<'_>, query: &QueryModel) -> (String, Option<u64>) {
    if let Some(grouping) = &query.grouping
        && !grouping.grouped
    {
        return ("1".to_string(), Some(1));
    }
    let most = match &query.nearest {
        Some(nearest) => Some(nearest.most),
        None => query
            .limit
            .as_ref()
            .and_then(|bound| bound_most(lookup, query, bound)),
    };
    let what = if query.grouping.is_some() {
        " groups"
    } else {
        ""
    };
    match query.cardinality {
        Cardinality::One => ("1".to_string(), Some(1)),
        Cardinality::Optional => ("0 or 1".to_string(), Some(1)),
        Cardinality::Page => match most {
            Some(n) => (format!("a page of up to {n}"), Some(n)),
            None => ("a page".to_string(), None),
        },
        Cardinality::List => match most {
            Some(n) => (format!("up to {n}{what}"), Some(n)),
            None => (format!("up to its `limit:`{what}"), None),
        },
    }
}

/// What a statement is for, as the reports name it: `notes_by_author`, or
/// `notes_by_author` include `author`.
pub fn statement_name(read: &StatementRead) -> String {
    if read.include.is_empty() {
        format!("`{}`", read.query)
    } else {
        format!("`{}` include `{}`", read.query, read.include.join("."))
    }
}

// --- the schema report ---------------------------------------------------------------

/// The schema report: every store's tables, each column and each key and
/// index with why it exists, an index with the queries that read it.
pub fn database(db: &dyn Db, project: Project) -> Report {
    let schema = schema(db, project);
    let reads = reads(db, project);
    // Which statements each key or index serves, by store, table and name.
    let mut readers: BTreeMap<(String, String, String), Vec<String>> = BTreeMap::new();
    for read in &reads {
        if let (Some(store), Some(index)) = (&read.store, &read.index) {
            let entry = readers
                .entry((store.clone(), read.table.clone(), index.clone()))
                .or_default();
            let name = statement_name(read);
            if !entry.contains(&name) {
                entry.push(name);
            }
        }
    }
    let read_by = |store: &str, table: &str, name: &str| -> Vec<String> {
        readers
            .get(&(store.to_string(), table.to_string(), name.to_string()))
            .cloned()
            .unwrap_or_default()
    };
    let mut out = String::from(
        "# Database\n\n\
         Every table, column, key and index each store holds, from the entities and the \
         queries that read them, as the migrations in `migrations/` make them. Each index \
         says why it exists and which queries read it. Written by `jadpo check`.\n\n",
    );
    if schema.stores.is_empty() {
        out.push_str("This project stores nothing yet.\n");
    }
    let mut json_stores = Vec::new();
    for (store_name, store) in &schema.stores {
        out.push_str(&format!("## `{store_name}`\n\n"));
        let mut json_tables = Vec::new();
        // The entities' tables first, then the runtime's own.
        let mut tables: Vec<(&String, &Table)> = store.tables.iter().collect();
        tables.sort_by_key(|(name, table)| (matches!(table.holds, Holds::System(_)), *name));
        // Jadpo's own tables are the runtime's, the same in every app, so
        // the Markdown names them and only the JSON twin lists their
        // columns.
        let own: Vec<String> = tables
            .iter()
            .filter_map(|(name, table)| match &table.holds {
                Holds::System(what) => Some(format!("`{name}` ({})", what.replace('_', " "))),
                Holds::Entity(_) => None,
            })
            .collect();
        for (table_name, table) in tables {
            let mut markdown = String::new();
            let shown = matches!(table.holds, Holds::Entity(_));
            let holds = match &table.holds {
                Holds::Entity(entity) => format!("entity `{entity}`"),
                Holds::System(what) => format!("Jadpo's {}", what.replace('_', " ")),
            };
            let retired = if table.retired { " · retired" } else { "" };
            markdown.push_str(&format!("### `{table_name}` · {holds}{retired}\n\n"));
            markdown.push_str("| column | type | null | field |\n|---|---|---|---|\n");
            let mut json_columns = Vec::new();
            for (column_name, column) in table.ordered() {
                let mut ty = column.sql_type.clone();
                if column.generated.is_some() {
                    ty.push_str(", generated");
                }
                if column.sealed {
                    ty.push_str(", sealed");
                }
                let field = match (&column.field, column.retired) {
                    (_, true) => "retired".to_string(),
                    (Some(field), false) => format!("`{field}`"),
                    (None, false) => "–".to_string(),
                };
                markdown.push_str(&format!(
                    "| `{column_name}` | `{ty}` | {} | {field} |\n",
                    if column.nullable { "yes" } else { "no" }
                ));
                let mut entry = Value::object();
                entry.set("name", column_name.as_str());
                entry.set("type", column.sql_type.as_str());
                entry.set("nullable", column.nullable);
                entry.set_some("field", column.field.as_deref());
                entry.set_some("generated", column.generated.is_some());
                entry.set_some("sealed", column.sealed);
                entry.set_some("retired", column.retired);
                json_columns.push(entry);
            }
            markdown.push_str("\n| key or index | columns | why | read by |\n|---|---|---|---|\n");
            let columns_text = |columns: &[String]| -> String {
                columns
                    .iter()
                    .map(|c| format!("`{c}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let readers_text = |names: &[String]| -> String {
                if names.is_empty() {
                    "–".to_string()
                } else {
                    names.join(", ")
                }
            };
            let mut json_indexes = Vec::new();
            let key_readers = read_by(store_name, table_name, &table.primary_key.name);
            markdown.push_str(&format!(
                "| `{}` | {} | primary key | {} |\n",
                table.primary_key.name,
                columns_text(&table.primary_key.columns),
                readers_text(&key_readers)
            ));
            json_indexes.push(index_json(
                &table.primary_key.name,
                "primary_key",
                &table.primary_key.columns,
                "primary key",
                &key_readers,
            ));
            for (name, unique) in &table.unique {
                let names = read_by(store_name, table_name, name);
                let why = format!("unique `{}`", unique.rule);
                markdown.push_str(&format!(
                    "| `{name}` | {} | {why} | {} |\n",
                    columns_text(&unique.columns),
                    readers_text(&names)
                ));
                json_indexes.push(index_json(name, "unique", &unique.columns, &why, &names));
            }
            for (name, key) in &table.foreign_keys {
                let why = format!(
                    "refers to `{}`, on delete {}",
                    key.table,
                    key.on_delete.sql().to_lowercase()
                );
                markdown.push_str(&format!(
                    "| `{name}` | {} | {why} | – |\n",
                    columns_text(&key.columns)
                ));
                json_indexes.push(index_json(name, "foreign_key", &key.columns, &why, &[]));
            }
            for (name, index) in &table.indexes {
                let names = read_by(store_name, table_name, name);
                let mut shown = match &index.expression {
                    Some(expression) => format!("`{expression}`"),
                    None => index
                        .columns
                        .iter()
                        .map(|c| {
                            if index.collate.contains(c) {
                                format!("`{c}` in code-point order")
                            } else {
                                format!("`{c}`")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                };
                if let Some(method) = &index.method {
                    shown.push_str(&format!(" by `{}`", method.name));
                }
                if let Some(predicate) = &index.predicate {
                    shown.push_str(&format!(" where `{predicate}`"));
                }
                let why = reason_text(&index.reason);
                markdown.push_str(&format!(
                    "| `{name}` | {shown} | {why} | {} |\n",
                    readers_text(&names)
                ));
                let mut entry = index_json(name, "index", &index.columns, &index.reason, &names);
                entry.set_some("expression", index.expression.as_deref());
                entry.set_some("where", index.predicate.as_deref());
                entry.set_some(
                    "method",
                    index.method.as_ref().map(|method| method.name.as_str()),
                );
                json_indexes.push(entry);
            }
            markdown.push('\n');
            if shown {
                out.push_str(&markdown);
            }
            let mut entry = Value::object();
            entry.set("name", table_name.as_str());
            match &table.holds {
                Holds::Entity(entity) => entry.set("entity", entity.as_str()),
                Holds::System(what) => entry.set("system", what.as_str()),
            }
            entry.set_some("retired", table.retired);
            entry.set("columns", json_columns);
            entry.set("indexes", json_indexes);
            json_tables.push(entry);
        }
        if !own.is_empty() {
            out.push_str(&format!(
                "Jadpo's own tables, whose columns are the runtime's and listed in `database.json`: {}.\n\n",
                own.join(", ")
            ));
        }
        let mut entry = Value::object();
        entry.set("name", store_name.as_str());
        entry.set("driver", store.driver.as_str());
        entry.set("tables", json_tables);
        json_stores.push(entry);
    }
    let mut json = Value::object();
    json.set("format", jadpo_graph::manifest::FORMAT);
    json.set("stores", json_stores);
    Report {
        markdown: one_final_newline(out),
        json,
    }
}

/// Why an index exists, as the report says it.
fn reason_text(reason: &str) -> String {
    match reason.split_once(' ') {
        Some(("query", query)) => format!("needed by query `{query}`"),
        Some(("reference", field)) => format!("reference `{field}`"),
        Some(("index", index)) => format!("declared in `indexes:` as `{index}`"),
        Some(("search", entity)) => format!("`{entity}`'s search"),
        Some(("system", _)) => "Jadpo's own".to_string(),
        _ => reason.to_string(),
    }
}

fn index_json(name: &str, kind: &str, columns: &[String], why: &str, readers: &[String]) -> Value {
    let mut entry = Value::object();
    entry.set("name", name);
    entry.set("kind", kind);
    entry.set(
        "columns",
        columns.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    entry.set("why", why);
    entry.set_some(
        "readBy",
        readers
            .iter()
            .map(|name| name.replace('`', ""))
            .collect::<Vec<_>>(),
    );
    entry
}

fn one_final_newline(mut text: String) -> String {
    while text.ends_with("\n\n") {
        text.pop();
    }
    text
}

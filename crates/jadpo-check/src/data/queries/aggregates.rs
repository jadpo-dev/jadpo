//! Aggregate queries (area 6, "Counts, sums and groups"; chunk 5.6c): a
//! query with `aggregate:` works totals out from the records it finds,
//! and with `group:` gives one row per group of them, instead of the
//! records themselves.
//!
//! - A group is a field whose stored values are equal exactly when Jadpo's
//!   are, or a time bucket: `temporal.year`, `temporal.month` or
//!   `temporal.day` of a date, through its SQL form (area 19).
//! - An aggregate is `count()`, `sum(F)`, `min(F)`, `max(F)` or
//!   `average(F, scale:, rounding:)`, each giving what code working from
//!   the records gives (`list.count`, `list.sum`, `money.sum`,
//!   `decimal.divide`), with a field's `none` values left out.
//! - Each row is a value of an `Output` or `Object` type whose fields are
//!   exactly the groups and aggregates, by name. With `group:` the query
//!   returns `List<T>`, ordered by its entries' names with the groups it
//!   doesn't name added as the tie-break; without, it returns `T`, the one
//!   row over every record.

use super::*;
use jadpo_std::{Op, Rounding};

/// What an aggregate query's rows are grouped by.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GroupKey {
    /// The entity's field.
    Field(String),
    /// A time bucket: a standard operation of a date field, through its
    /// SQL form (`temporal.year`, `temporal.month`, `temporal.day`).
    Bucket { op: Op, field: String },
}

/// What an aggregate works out from a group's records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AggregateKind {
    /// How many records there are.
    Count,
    /// A field's values added up, its `none` values left out.
    Sum,
    /// A field's smallest value.
    Min,
    /// A field's largest value.
    Max,
    /// A field's exact mean, rounded once to `scale` places.
    Average { scale: u32, rounding: Rounding },
}

impl AggregateKind {
    /// As it is written: `sum`.
    pub fn word(self) -> &'static str {
        match self {
            AggregateKind::Count => "count",
            AggregateKind::Sum => "sum",
            AggregateKind::Min => "min",
            AggregateKind::Max => "max",
            AggregateKind::Average { .. } => "average",
        }
    }
}

/// One entry of an aggregate query: a group or an aggregate, filling the
/// result's field of its name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Entry {
    Group(GroupKey),
    Aggregate {
        kind: AggregateKind,
        /// The field it reads; `None` for `count()`.
        field: Option<String>,
    },
}

impl Entry {
    /// The entity's field it reads, if any.
    pub fn field(&self) -> Option<&str> {
        match self {
            Entry::Group(GroupKey::Field(field) | GroupKey::Bucket { field, .. }) => Some(field),
            Entry::Aggregate { field, .. } => field.as_deref(),
        }
    }

    pub fn is_group(&self) -> bool {
        matches!(self, Entry::Group(_))
    }
}

/// One entry of an aggregate query's order.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EntryOrder {
    /// The entry's position among the query's entries.
    pub entry: usize,
    pub descending: bool,
    /// Added by the compiler as the tie-break, rather than written.
    pub added: bool,
}

/// An aggregate query's groups and aggregates.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Grouping {
    /// Whether it has `group:`: one row per group, rather than one row
    /// over every record.
    pub grouped: bool,
    /// The groups, then the aggregates, each with its name, as written.
    pub entries: Vec<(String, Entry)>,
    /// Its order, the groups it doesn't name added last; empty without
    /// `group:`.
    pub order: Vec<EntryOrder>,
}

/// The aggregates, as written.
const AGGREGATES: [&str; 5] = ["count", "sum", "min", "max", "average"];

/// The time buckets a group can be, each a `temporal` operation of a date.
const BUCKETS: [&str; 3] = ["year", "month", "day"];

/// What `query` has worked out about an aggregate query before reading its
/// sections.
pub(super) struct Shape<'s> {
    pub name: &'s str,
    pub span: &'s Span,
    pub result: &'s Ty,
    pub result_span: &'s Span,
    pub cardinality: Cardinality,
    pub element: &'s str,
    /// It has a `nearest` order or returns `Nearest<T>`.
    pub nearest: bool,
}

/// An entry read, with what checking the result and the order needs.
struct Read {
    name: String,
    entry: Entry,
    /// What it gives.
    ty: Ty,
    /// Where its name is written.
    span: Span,
    /// As written, for messages: `sum(ApiCall.tokens_spent)`.
    written: String,
    /// Whether rows can be sorted by it.
    sortable: bool,
}

/// Whether a value built on `prim` is one a query sorts by (area 6, "Order
/// and pages").
fn sortable_prim(prim: Prim) -> bool {
    matches!(
        prim,
        Prim::Int
            | Prim::Decimal
            | Prim::Instant
            | Prim::CalendarDate
            | Prim::Duration
            | Prim::Text
            | Prim::Uuid
            | Prim::Quantity(_)
            | Prim::Percentage
    )
}

/// Another word an agent may reach for, and the aggregate it means.
fn aggregate_alias(word: &str) -> Option<&'static str> {
    Some(match word {
        "avg" | "mean" => "average",
        "total" | "add" => "sum",
        "minimum" | "least" | "smallest" => "min",
        "maximum" | "greatest" | "largest" => "max",
        "size" | "length" | "number" | "tally" => "count",
        _ => return None,
    })
}

impl Reader<'_> {
    /// An aggregate query: its shape, its conditions, its groups and
    /// aggregates, the result they fill, and its order and limit.
    pub(super) fn aggregate_query(
        &mut self,
        symbol: &Symbol,
        decl: &ast::QueryDecl,
        scope: &Scope<'_>,
        entity: &EntityModel,
        sections: &Sections,
        shape: &Shape<'_>,
    ) -> Option<QueryModel> {
        let path = scope.path;
        let name = shape.name;
        let grouped = sections.first.contains_key("group");
        let key_span = |section: &str| -> Span {
            sections
                .first
                .get(section)
                .and_then(|m| m.member_key())
                .map_or_else(|| shape.span.clone(), |k| Self::span(path, &k))
        };
        let lead = if grouped { "group" } else { "aggregate" };
        let result = shape.result;
        let element = shape.element;

        // A nearest-neighbour query returns records with their distances.
        if shape.nearest {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_SECTION_UNEXPECTED,
                    format!("A nearest-neighbour query takes no `{lead}:`"),
                )
                .at(key_span(lead), "not for this query")
                .reason("A nearest-neighbour query returns the records nearest a vector, each with its distance; counting or adding up records is a query of its own.")
                .next(format!("Remove `{lead}:`, or write the aggregate as a query of its own")),
            );
            return None;
        }

        // The result: a list of groups, or the one row over every record.
        let mut ok = true;
        match (grouped, shape.cardinality) {
            (true, Cardinality::List) | (false, Cardinality::One) => {}
            (true, _) => {
                ok = false;
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_SECTION_UNEXPECTED,
                        format!("A query returning `{result}` takes no `group:`"),
                    )
                    .at(key_span("group"), "not for this result")
                    .reason("`group:` gives one row for each group of records, so the query returns them as a list, ordered and limited: `List<T>`.")
                    .next(format!("Return `List<{element}>`")),
                );
            }
            (false, Cardinality::List | Cardinality::Page) => {
                ok = false;
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_SECTION_REQUIRED,
                        format!("`{name}` returns `{result}` but has no `group:`"),
                    )
                    .at(shape.span.clone(), "no `group:`")
                    .reason(format!(
                        "Without `group:`, an aggregate query is one row over every record it finds, so it returns `{element}`; `group:` splits the records into a row for each group."
                    ))
                    .next(format!("Add `group: {{ ... }}`, or return `{element}`")),
                );
            }
            (false, Cardinality::Optional) => {
                ok = false;
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_SECTION_UNEXPECTED,
                        format!("A query returning `{result}` takes no `aggregate:`"),
                    )
                    .at(key_span("aggregate"), "not for this result")
                    .reason(format!(
                        "An aggregate over every record is always there, even when nothing matches, so the query returns `{element}`, not `{result}`."
                    ))
                    .next(format!("Return `{element}`")),
                );
            }
        }

        // The sections it needs and those it refuses.
        if ok {
            if grouped {
                for section in ["order", "limit"] {
                    if !sections.first.contains_key(section)
                        && !sections.suggested.contains(section)
                    {
                        ok = false;
                        self.section_required(name, shape.span, shape.cardinality, result, section);
                    }
                }
            }
            let refused: &[&str] = if grouped {
                &["page", "missing", "include"]
            } else {
                &["order", "limit", "page", "missing", "include"]
            };
            for section in refused {
                let Some(member) = sections.first.get(*section) else {
                    continue;
                };
                ok = false;
                self.aggregate_unexpected(path, member, section);
            }
        }

        // Conditions. The database counts, and a `geo.` condition's
        // records are decided in Jadpo's code (area 20, chunk 5.5u5), so
        // an aggregate can't hold one yet.
        let mut filter = Vec::new();
        for (present, member) in &sections.filters {
            if let Some(condition) = self.filter_line(scope, entity, member, *present) {
                if holds_geo(&condition) {
                    ok = false;
                    let at = member.value().map_or_else(
                        || Self::span(path, member),
                        |value| Self::span(path, &value),
                    );
                    self.out.push(
                        Diagnostic::new(
                            &rule::TOOLCHAIN_NOT_BUILT_YET,
                            "A `geo.` condition in an aggregate query isn't built yet".to_string(),
                        )
                        .at(at, "in an aggregate")
                        .reason("A `geo.` condition's records are found through a GiST index and each is held to Jadpo's own test as the query runs (area 20, \"Queries\"); an aggregate query's totals are worked out by the database, which doesn't decide as the code does.")
                        .next("List the records with a query of their own, and count them in the action"),
                    );
                    continue;
                }
                filter.push(condition);
            }
        }

        // The groups, then the aggregates. An entry already reported is
        // remembered by name, so the field it fills and the order naming
        // it aren't reported again.
        let mut reads: Vec<Read> = Vec::new();
        let mut reported: Vec<String> = Vec::new();
        if let Some(member) = sections.first.get("group") {
            let entries = self.entries(path, member, "group");
            if entries.is_empty() {
                ok = false;
            }
            for (entry_name, entry) in entries {
                match self.group_entry(path, entity, &entry_name, &entry) {
                    Some(read) => reads.push(read),
                    None => {
                        ok = false;
                        reported.extend(tree::name_text(&entry_name));
                    }
                }
            }
        }
        if let Some(member) = sections.first.get("aggregate") {
            let entries = self.entries(path, member, "aggregate");
            if entries.is_empty() {
                ok = false;
            }
            for (entry_name, entry) in entries {
                match self.aggregate_entry(path, entity, &entry_name, &entry, grouped) {
                    Some(read) => reads.push(read),
                    None => {
                        ok = false;
                        reported.extend(tree::name_text(&entry_name));
                    }
                }
            }
        }
        // Each name fills one field.
        let mut named: Vec<Read> = Vec::new();
        for read in reads {
            if let Some(earlier) = named.iter().find(|r| r.name == read.name) {
                ok = false;
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_RESULT_FIELD,
                        format!("`{}` is given twice", read.name),
                    )
                    .at(read.span.clone(), "given again")
                    .also(earlier.span.clone(), "first given here")
                    .reason(format!(
                        "Each group and aggregate fills the result's field of its name, so `{}` would fill one field twice.",
                        read.name
                    ))
                    .next(format!("Rename one of them, and add a field for it to `{element}`")),
                );
                continue;
            }
            named.push(read);
        }
        let reads = named;

        // The result's fields, each filled by the entry of its name.
        let element_model = self.aggregate_element(name, entity, &reads, &reported, shape);
        if element_model.is_none() {
            ok = false;
        }

        // Order and limit.
        let order = match sections.first.get("order") {
            Some(member) if grouped => self.entry_order(path, name, member, &reads, &reported),
            _ => None,
        };
        let limit = match sections.first.get("limit") {
            Some(member) if grouped => self.bound(scope, member.value(), member, "limit"),
            _ => None,
        };
        if grouped && (order.is_none() || limit.is_none()) {
            ok = false;
        }
        self.fails(path, decl, name, None, None);

        let element = element_model?;
        if !ok {
            return None;
        }
        let key = self.lookup.registry().key(Kind::Query, name, None);
        let _ = symbol;
        Some(QueryModel {
            name: name.to_string(),
            key,
            path: path.to_string(),
            entity: entity.name.clone(),
            params: scope.params.to_vec(),
            cardinality: shape.cardinality,
            element,
            filter,
            order: Vec::new(),
            limit,
            after: None,
            includes: Vec::new(),
            missing: None,
            nearest: None,
            distance: None,
            grouping: Some(Grouping {
                grouped,
                entries: reads
                    .into_iter()
                    .map(|read| (read.name, read.entry))
                    .collect(),
                order: order.unwrap_or_default(),
            }),
            geometry: None,
        })
    }

    /// A section an aggregate query doesn't take.
    fn aggregate_unexpected(&mut self, path: &str, member: &ast::Member, section: &str) {
        let Some(key) = member.member_key() else {
            return;
        };
        let reason = match section {
            "include" => {
                "An aggregate query's rows are groups and totals, not records, so there are no related records to include."
            }
            "missing" => {
                "An aggregate query always has its rows, one for each group or one over every record, so no failure is raised for finding nothing."
            }
            "page" => "An aggregate query's rows come back in one list, bounded by `limit:`.",
            _ => {
                "Without `group:`, an aggregate query is one row over every record it finds, so there is nothing to order or limit."
            }
        };
        let removal = Self::span(path, member);
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_SECTION_UNEXPECTED,
                format!("An aggregate query takes no `{section}:`"),
            )
            .at(Self::span(path, &key), "not for this query")
            .reason(reason)
            .next(format!("Remove `{section}:`"))
            .edit(removal, ""),
        );
    }

    /// The entries of a `group:` or `aggregate:` block, each named.
    fn entries(
        &mut self,
        path: &str,
        member: &ast::Member,
        section: &str,
    ) -> Vec<(ast::Name, ast::Member)> {
        let key_span = member
            .member_key()
            .map_or_else(|| Span::new(path, 0, 0), |k| Self::span(path, &k));
        let rule = if section == "group" {
            &rule::QUERY_GROUP_KEY
        } else {
            &rule::QUERY_AGGREGATE_UNKNOWN
        };
        let example = if section == "group" {
            "`group: { <name>: <Entity>.<field> }`"
        } else {
            "`aggregate: { <name>: count() }`"
        };
        let Some(block) = member.block() else {
            self.out.push(
                Diagnostic::new(rule, format!("`{section}:` is a block"))
                    .at(key_span, "not a block")
                    .reason(format!(
                        "`{section}:` names each of its entries after the result field it fills, in a block: {example}."
                    ))
                    .next(format!("Write {example}")),
            );
            return Vec::new();
        };
        let entries = tree::members(Some(block));
        if entries.is_empty() {
            self.out.push(
                Diagnostic::new(rule, format!("`{section}:` names nothing"))
                    .at(key_span, "empty")
                    .reason(format!(
                        "`{section}:` names each of its entries after the result field it fills: {example}."
                    ))
                    .next(format!("Add an entry, or remove `{section}:`")),
            );
        }
        entries
    }

    /// One `group:` entry: a field, or a time bucket of a date field.
    fn group_entry(
        &mut self,
        path: &str,
        entity: &EntityModel,
        name: &ast::Name,
        member: &ast::Member,
    ) -> Option<Read> {
        let entry_name = tree::name_text(name)?;
        let span = Self::span(path, name);
        let value = member.value()?;
        let written = shown(value.syntax());
        let value_span = Self::span(path, &value);
        let not_a_group = |reason: String| {
            Diagnostic::new(
                &rule::QUERY_GROUP_KEY,
                format!("`{written}` isn't a field or a time bucket"),
            )
            .at(value_span.clone(), "not a group")
            .reason(reason)
            .next(format!(
                "Group by a field of `{}`, or a time bucket of a date",
                entity.name
            ))
        };
        let shape = "A group is a field, or a time bucket: `temporal.year`, `temporal.month` or `temporal.day` of a date field.".to_string();
        // A time bucket.
        if let Expr::CallExpr(call) = unparen(&value) {
            let bucket = call
                .expr()
                .and_then(|callee| dotted(&callee))
                .and_then(|(head, rest)| {
                    let [op] = rest.as_slice() else {
                        return None;
                    };
                    (tree::name_ref_text(&head)? == "temporal")
                        .then(|| tree::name_ref_text(op))
                        .flatten()
                })
                .filter(|op| BUCKETS.contains(&op.as_str()));
            let Some(bucket) = bucket else {
                self.out.push(not_a_group(shape));
                return None;
            };
            let args: Vec<ast::Arg> = call
                .arg_list()
                .map(|list| list.args().collect())
                .unwrap_or_default();
            let field_expr = match args.as_slice() {
                [arg] if arg.name().is_none() => arg.expr(),
                _ => None,
            };
            let Some(field_expr) = field_expr.filter(|e| dotted(e).is_some()) else {
                self.out.push(not_a_group(format!(
                    "`temporal.{bucket}` here takes one field of `{}`, a date: `temporal.{bucket}({}.<field>)`.",
                    entity.name, entity.name
                )));
                return None;
            };
            let field = self.field_ref(path, entity, &field_expr)?;
            let ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
            if self.lookup.representation(&ty) != Some(Prim::CalendarDate) {
                let field_written = format!("{}.{}", entity.name, field.name);
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_GROUP_KEY,
                        format!(
                            "`temporal.{bucket}` groups by a date, and `{field_written}` is `{}`",
                            field_type_name(&field.ty)
                        ),
                    )
                    .at(Self::span(path, &field_expr), "not a date")
                    .reason("A time bucket is worked out by the database, which gives exactly what code gives only for a date: an instant has a day only in a zone, and PostgreSQL's time zone database can differ from the one built into Jadpo.")
                    .next("Keep the day as a `CalendarDate` field, and group by a time bucket of it"),
                );
                return None;
            }
            let op = jadpo_std::OPERATIONS
                .iter()
                .find(|o| o.namespace == "temporal" && o.name == bucket)
                .map(|o| o.op)?;
            let out = Ty::Prim(Prim::Int);
            return Some(Read {
                name: entry_name,
                entry: Entry::Group(GroupKey::Bucket {
                    op,
                    field: field.name.clone(),
                }),
                ty: if field.nullable { out.nullable() } else { out },
                span,
                written,
                sortable: true,
            });
        }
        // A field.
        if dotted(&value).is_none_or(|(_, rest)| rest.len() != 1) {
            self.out.push(not_a_group(shape));
            return None;
        }
        let field = self.field_ref(path, entity, &value)?;
        let ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
        let held = field.sql_type.as_deref() != Some("jsonb") && field.compound.is_none();
        let groupable = held
            && (matches!(
                self.lookup.representation(&ty),
                Some(Prim::Bool | Prim::Int | Prim::Text | Prim::Uuid | Prim::CalendarDate)
            ) || self.plain_enum(&ty));
        if !groupable {
            let field_written = format!("{}.{}", entity.name, field.name);
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_GROUP_KEY,
                    format!("Records can't be grouped by `{field_written}`"),
                )
                .at(value_span, format!("`{}`", field_type_name(&field.ty)))
                .reason(format!(
                    "A group is a field whose stored values are equal exactly when Jadpo's are: one built on `Bool`, `Int`, `Text`, `Uuid` or `CalendarDate`, or a plain enum. `{field_written}` is `{}`.",
                    field.ty
                ))
                .next("Group by another field, or by a time bucket of a date"),
            );
            return None;
        }
        Some(Read {
            name: entry_name,
            entry: Entry::Group(GroupKey::Field(field.name.clone())),
            ty: if field.nullable { ty.nullable() } else { ty },
            span,
            written,
            sortable: true,
        })
    }

    /// Whether `ty` is an enum whose variants carry nothing.
    fn plain_enum(&self, ty: &Ty) -> bool {
        match self.lookup.structure(ty) {
            Ty::Named(name) => matches!(
                self.lookup.def(&name),
                Some(Def::Enum { variants }) if variants.iter().all(|v| v.fields.is_none())
            ),
            _ => false,
        }
    }

    /// One `aggregate:` entry: `count()`, `sum(F)`, `min(F)`, `max(F)` or
    /// `average(F, scale:, rounding:)`.
    fn aggregate_entry(
        &mut self,
        path: &str,
        entity: &EntityModel,
        name: &ast::Name,
        member: &ast::Member,
        grouped: bool,
    ) -> Option<Read> {
        let entry_name = tree::name_text(name)?;
        let span = Self::span(path, name);
        let value = member.value()?;
        let written = shown(value.syntax());
        let value_span = Self::span(path, &value);
        let listing = format!(
            "An aggregate is `count()`, `sum(F)`, `min(F)`, `max(F)` or `average(F, scale:, rounding:)`, each `F` a field of `{}`.",
            entity.name
        );
        let Expr::CallExpr(call) = unparen(&value) else {
            // A field's own value is a group.
            let problem = Diagnostic::new(
                &rule::QUERY_AGGREGATE_UNKNOWN,
                format!("`{written}` isn't an aggregate"),
            )
            .at(value_span, "not an aggregate")
            .reason(format!(
                "{listing} A field's own value, the same for every record of a row, is a group."
            ));
            self.out
                .push(if self.field_named(entity, &value).is_some() {
                    problem.next(format!("Write `sum({written})`, or move it to `group:`"))
                } else {
                    problem.next("Write one of the aggregates")
                });
            return None;
        };
        let callee = call.expr()?;
        let callee_written = shown(callee.syntax());
        let callee_span = Self::span(path, &callee);
        let word = match &callee {
            Expr::NameRef(name) => tree::name_ref_text(name),
            _ => None,
        };
        let kind_word = word
            .as_deref()
            .filter(|w| AGGREGATES.contains(w))
            .map(str::to_string);
        let Some(kind_word) = kind_word else {
            let last = dotted(&callee)
                .and_then(|(head, rest)| tree::name_ref_text(rest.last().unwrap_or(&head)))
                .unwrap_or_default();
            let near = aggregate_alias(&last)
                .or_else(|| AGGREGATES.iter().copied().find(|a| *a == last))
                .or_else(|| nearest(&last, AGGREGATES.iter().copied()));
            let problem = Diagnostic::new(
                &rule::QUERY_AGGREGATE_UNKNOWN,
                format!("`{callee_written}` isn't an aggregate"),
            )
            .at(callee_span.clone(), "not an aggregate")
            .reason(format!(
                "{listing}{}",
                near.map(|n| format!(" Did you mean `{n}`?"))
                    .unwrap_or_default()
            ));
            self.out.push(match near {
                Some(near) => problem
                    .next(format!("Use `{near}`"))
                    .edit(callee_span, near),
                None => problem.next("Write one of the aggregates"),
            });
            return None;
        };
        let args: Vec<ast::Arg> = call
            .arg_list()
            .map(|list| list.args().collect())
            .unwrap_or_default();
        let positional: Vec<&ast::Arg> = args.iter().filter(|a| a.name().is_none()).collect();
        let named: Vec<(String, &ast::Arg)> = args
            .iter()
            .filter_map(|a| Some((tree::name_text(&a.name()?)?, a)))
            .collect();
        let argument = |title: String, reason: String, next: String| {
            Diagnostic::new(&rule::QUERY_AGGREGATE_ARGUMENT, title)
                .at(value_span.clone(), "wrong arguments")
                .reason(reason)
                .next(next)
        };
        if kind_word == "count" {
            if !args.is_empty() {
                let problem = argument(
                    "`count` takes no arguments".to_string(),
                    format!(
                        "`count()` counts the records in each row, whatever their fields hold; to count only the records whose field has a value, add `where: {}.<field> != none`.",
                        entity.name
                    ),
                    "Write `count()`".to_string(),
                );
                self.out.push(problem.edit(value_span.clone(), "count()"));
                return None;
            }
            return Some(Read {
                name: entry_name,
                entry: Entry::Aggregate {
                    kind: AggregateKind::Count,
                    field: None,
                },
                ty: Ty::Prim(Prim::Int),
                span,
                written,
                sortable: true,
            });
        }
        let average = kind_word == "average";
        let usage = if average {
            format!(
                "`average({}.<field>, scale: 2, rounding: Rounding.half_even)`",
                entity.name
            )
        } else {
            format!("`{kind_word}({}.<field>)`", entity.name)
        };
        // One field, named with its entity.
        let field_expr = match positional.as_slice() {
            [arg] => arg
                .expr()
                .filter(|e| dotted(e).is_some_and(|(_, rest)| rest.len() == 1)),
            _ => None,
        };
        let unknown_named: Vec<&String> = named
            .iter()
            .map(|(n, _)| n)
            .filter(|n| !average || !matches!(n.as_str(), "scale" | "rounding"))
            .collect();
        let Some(field_expr) = field_expr.filter(|_| unknown_named.is_empty()) else {
            self.out.push(argument(
                format!("`{kind_word}` takes one field of `{}`", entity.name),
                format!(
                    "`{kind_word}` works out over one field of the records, named with its entity: {usage}."
                ),
                format!("Write {usage}"),
            ));
            return None;
        };
        // An average's places and rounding, both written out.
        let kind = if average {
            let scale = named
                .iter()
                .find(|(n, _)| n == "scale")
                .and_then(|(_, a)| a.expr());
            let rounding = named
                .iter()
                .find(|(n, _)| n == "rounding")
                .and_then(|(_, a)| a.expr());
            let (Some(scale), Some(rounding)) = (scale, rounding) else {
                self.out.push(argument(
                    "`average` needs `scale:` and `rounding:`".to_string(),
                    format!(
                        "An average rarely comes out exact, so it says how many places it keeps and how the last one is rounded, both written out: {usage}."
                    ),
                    "Add `scale:` and `rounding:`".to_string(),
                ));
                return None;
            };
            let places = match Const::from_expr(&scale) {
                Const::Number {
                    value: n,
                    decimal: false,
                } if n.compare(jadpo_std::Num::whole(0)).is_ge()
                    && n.compare(jadpo_std::Num::whole(38)).is_le() =>
                {
                    u32::try_from(n.mantissa).ok()
                }
                _ => None,
            };
            let Some(places) = places else {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_AGGREGATE_ARGUMENT,
                        format!("`scale: {}` isn't a number of places", shown(scale.syntax())),
                    )
                    .at(Self::span(path, &scale), "not 0 to 38")
                    .reason("`scale:` is how many places the average keeps: a whole number written out, from 0 to 38, as a decimal holds at most 38 digits.")
                    .next("Write `scale: 2`, or the places the report needs"),
                );
                return None;
            };
            let mode = dotted(&rounding).and_then(|(head, rest)| {
                let [variant] = rest.as_slice() else {
                    return None;
                };
                (tree::name_ref_text(&head)? == Rounding::NAME)
                    .then(|| Rounding::from_name(&tree::name_ref_text(variant)?))
                    .flatten()
            });
            let Some(mode) = mode else {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_AGGREGATE_ARGUMENT,
                        format!("`rounding: {}` isn't a rounding", shown(rounding.syntax())),
                    )
                    .at(Self::span(path, &rounding), "not a `Rounding`")
                    .reason("`rounding:` is a `Rounding` written out: `Rounding.half_even`, `Rounding.half_up`, `Rounding.up` or `Rounding.down`.")
                    .next("Write `rounding: Rounding.half_even`, or the rounding the report needs"),
                );
                return None;
            };
            AggregateKind::Average {
                scale: places,
                rounding: mode,
            }
        } else {
            match kind_word.as_str() {
                "sum" => AggregateKind::Sum,
                "min" => AggregateKind::Min,
                _ => AggregateKind::Max,
            }
        };
        let field = self.field_ref(path, entity, &field_expr)?;
        let field_written = format!("{}.{}", entity.name, field.name);
        let field_ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
        let prim = if field.sql_type.as_deref() == Some("jsonb") {
            None
        } else {
            self.lookup.representation(&field_ty)
        };
        let shown_ty = field.ty.to_string();
        let refuse = |this: &mut Self, title: String, reason: String, next: String| {
            this.out.push(
                Diagnostic::new(&rule::QUERY_AGGREGATE_FIELD, title)
                    .at(
                        Self::span(path, &field_expr),
                        format!("`{}`", field_type_name(&field.ty)),
                    )
                    .reason(reason)
                    .next(next),
            );
        };
        // `min`, `max` and `average` of no values are `none`.
        let may_be_none = field.nullable || !grouped;
        let (ty, sortable) = match kind {
            AggregateKind::Sum => {
                let given = match prim {
                    Some(Prim::Int) => Some(Prim::Int),
                    Some(Prim::Decimal) => Some(Prim::Decimal),
                    Some(Prim::Duration) => Some(Prim::Duration),
                    Some(Prim::Quantity(kind)) => Some(Prim::Quantity(kind)),
                    Some(Prim::Money) => {
                        if !field.fixed.iter().any(|(part, _)| *part == "currency") {
                            refuse(
                                self,
                                format!(
                                    "`sum` adds up money in one currency, and `{field_written}`'s type doesn't fix one"
                                ),
                                format!(
                                    "Amounts in two currencies don't add up (`currency_mismatch`), and a sum of no records needs a currency for its zero, so the field's type fixes its currency: `{shown_ty}` doesn't."
                                ),
                                format!("Give `{shown_ty}` a `currency:`"),
                            );
                            return None;
                        }
                        Some(Prim::Money)
                    }
                    _ => None,
                };
                let Some(given) = given else {
                    refuse(
                        self,
                        format!("`sum` can't add up `{field_written}`"),
                        format!(
                            "`sum` adds up numbers, durations, quantities and money in one currency, as `+` does; `{field_written}` is `{shown_ty}`."
                        ),
                        "Add up another field, or count the records with `count()`".to_string(),
                    );
                    return None;
                };
                (Ty::Prim(given), given != Prim::Money)
            }
            AggregateKind::Min | AggregateKind::Max => {
                let fits = prim.is_some_and(|p| sortable_prim(p) && p != Prim::Uuid);
                if !fits {
                    refuse(
                        self,
                        format!("`{kind_word}` can't compare `{field_written}`"),
                        format!(
                            "`min` and `max` take a field a query can sort by, other than an identity: numbers, times, dates, durations, quantities, percentages and text, by code point. `{field_written}` is `{shown_ty}`."
                        ),
                        format!("Take the `{kind_word}` of another field"),
                    );
                    return None;
                }
                (
                    if may_be_none {
                        field_ty.nullable()
                    } else {
                        field_ty
                    },
                    true,
                )
            }
            AggregateKind::Average { scale, .. } => {
                match prim {
                    Some(Prim::Int) => {}
                    // A stored decimal always keeps a fixed number of
                    // places (`type.decimal_scale_required`).
                    Some(Prim::Decimal) => {
                        if let Some(places) = field.scale.filter(|places| *places > scale) {
                            refuse(
                                self,
                                format!("`average` keeps {scale} places, and `{field_written}` keeps {places}"),
                                "The average is the exact mean rounded once to `scale:` places, so it keeps at least the places each value has.".to_string(),
                                format!("Write `scale: {places}` or more"),
                            );
                            return None;
                        }
                    }
                    _ => {
                        refuse(
                            self,
                            format!("`average` can't work out a mean of `{field_written}`"),
                            format!(
                                "`average` takes a field built on `Int` or `Decimal`; `{field_written}` is `{shown_ty}`."
                            ),
                            "Average another field, or add it up with `sum`".to_string(),
                        );
                        return None;
                    }
                }
                let out = Ty::Prim(Prim::Decimal);
                (if may_be_none { out.nullable() } else { out }, true)
            }
            AggregateKind::Count => (Ty::Prim(Prim::Int), true),
        };
        Some(Read {
            name: entry_name,
            entry: Entry::Aggregate {
                kind,
                field: Some(field.name.clone()),
            },
            ty,
            span,
            written,
            sortable,
        })
    }

    /// The rows' type, each field filled by the group or aggregate of its
    /// name.
    fn aggregate_element(
        &mut self,
        name: &str,
        entity: &EntityModel,
        reads: &[Read],
        reported: &[String],
        shape: &Shape<'_>,
    ) -> Option<Element> {
        let ty = shape.element;
        if ty == entity.name {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_RESULT_TYPE,
                    format!("`{name}` counts `{ty}` records but returns them"),
                )
                .at(shape.result_span.clone(), "records, not totals")
                .reason(format!(
                    "An aggregate query's rows are groups and totals, not `{ty}` records: values of an `Output` type with a field for each group and aggregate, by name."
                ))
                .next(format!(
                    "Return an `Output` type with a field for each group and aggregate, such as `type {ty}Totals = Output {{ ... }}`"
                )),
            );
            return None;
        }
        let declared = self
            .lookup
            .fields(ty)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let mut ok = true;
        for read in reads {
            if !declared.iter().any(|f| f.name == read.name) {
                ok = false;
                let what = if read.entry.is_group() {
                    "group"
                } else {
                    "aggregate"
                };
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_RESULT_FIELD,
                        format!("`{ty}` has no field `{}` to hold the {what}", read.name),
                    )
                    .at(read.span.clone(), "nowhere to put it")
                    .reason(format!(
                        "Each group and aggregate fills the result's field of its name, and `{ty}` declares none called `{}`.",
                        read.name
                    ))
                    .next(format!("Add `{}: ...` to `{ty}`, or rename the {what}", read.name)),
                );
            }
        }
        let mut fields = Vec::new();
        for field in &declared {
            let Some(at) = reads.iter().position(|r| r.name == field.name) else {
                ok = false;
                if reported.contains(&field.name) {
                    continue;
                }
                let names: Vec<String> = reads.iter().map(|r| format!("`{}`", r.name)).collect();
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_RESULT_FIELD,
                        format!("`{ty}.{}` isn't a group or an aggregate of `{name}`", field.name),
                    )
                    .at(shape.result_span.clone(), format!("`{}` can't be filled", field.name))
                    .reason(format!(
                        "An aggregate query fills its result's fields from its groups and aggregates of the same name ({}); nothing gives `{}`, and a field of `{}` has no one value for a row's records.",
                        crate::and_list(&names),
                        field.name,
                        entity.name
                    ))
                    .next(format!(
                        "Remove `{}` from `{ty}`, or add `{}: ...` to `group:` or `aggregate:`",
                        field.name, field.name
                    )),
                );
                continue;
            };
            let read = &reads[at];
            if !field.declared.has_error() && !self.lookup.assignable(&read.ty, &field.declared) {
                ok = false;
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_RESULT_FIELD,
                        format!(
                            "`{ty}.{}` is `{}`, which `{}` can't fill",
                            field.name, field.declared, read.written
                        ),
                    )
                    .at(read.span.clone(), format!("gives `{}`", read.ty))
                    .reason(format!(
                        "`{}` gives `{}`, and a total goes into the result as it is, so the result's field takes that type or one it widens to.",
                        read.written, read.ty
                    ))
                    .next(format!("Declare `{}` as `{}`", field.name, read.ty)),
                );
                continue;
            }
            fields.push((
                field.name.clone(),
                Source::Entry {
                    at,
                    field: read.entry.field().map(str::to_string),
                },
            ));
        }
        ok.then(|| Element {
            ty: ty.to_string(),
            fields,
        })
    }

    /// A grouped query's `order:`: its entries by name, then each group it
    /// doesn't name, ascending, as the tie-break.
    fn entry_order(
        &mut self,
        path: &str,
        name: &str,
        member: &ast::Member,
        reads: &[Read],
        reported: &[String],
    ) -> Option<Vec<EntryOrder>> {
        let value = member.value()?;
        let names: Vec<String> = reads.iter().map(|r| format!("`{}`", r.name)).collect();
        let listing = crate::and_list(&names);
        let Expr::ListExpr(list) = &value else {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_ORDER_FIELD,
                    "`order:` is a list of groups and aggregates".to_string(),
                )
                .at(Self::span(path, &value), "not a list")
                .reason(format!(
                    "An aggregate query's `order:` lists its groups and aggregates by name in brackets, each with `asc` or `desc`: {listing}."
                ))
                .next("Write `order: [<name> desc]`"),
            );
            return None;
        };
        let mut ok = true;
        let mut out: Vec<EntryOrder> = Vec::new();
        let mut spans: Vec<Span> = Vec::new();
        for item in list.exprs() {
            let item_span = Self::span(path, &item);
            let Expr::OrderedExpr(ordered) = &item else {
                ok = false;
                let written = shown(item.syntax());
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_ORDER_DIRECTION,
                        format!("`{written}` says no direction"),
                    )
                    .at(item_span.clone(), "`asc` or `desc`?")
                    .reason("Each entry of `order:` says which way it sorts: `asc` from the smallest, `desc` from the largest.")
                    .next(format!("Write `{written} asc` or `{written} desc`"))
                    .edit(Span::new(path, item_span.end, item_span.end), " asc"),
                );
                continue;
            };
            let Some(inner) = ordered.expr() else {
                ok = false;
                continue;
            };
            let descending = ordered.direction().is_some_and(|d| d.text() == "desc");
            let written = shown(inner.syntax());
            let entry_name = match &inner {
                Expr::NameRef(name) => tree::name_ref_text(name),
                _ => None,
            };
            let Some(at) = entry_name
                .as_ref()
                .and_then(|n| reads.iter().position(|r| r.name == *n))
            else {
                ok = false;
                if entry_name.as_ref().is_some_and(|n| reported.contains(n)) {
                    continue;
                }
                let near = entry_name
                    .as_deref()
                    .and_then(|n| nearest(n, reads.iter().map(|r| r.name.as_str())))
                    .map(str::to_string);
                let reason = if dotted(&inner).is_some_and(|(_, rest)| !rest.is_empty()) {
                    format!(
                        "Each row is a group of records, which have no one `{written}`; an aggregate query's `order:` names its groups and aggregates: {listing}."
                    )
                } else {
                    format!(
                        "An aggregate query's `order:` names its groups and aggregates: {listing}.{}",
                        near.as_ref()
                            .map(|n| format!(" Did you mean `{n}`?"))
                            .unwrap_or_default()
                    )
                };
                let inner_span = Self::span(path, &inner);
                let problem = Diagnostic::new(
                    &rule::QUERY_ORDER_FIELD,
                    format!("`{written}` isn't a group or an aggregate of `{name}`"),
                )
                .at(inner_span.clone(), "not an entry")
                .reason(reason);
                self.out.push(match near {
                    Some(near) => problem.next(format!("Use `{near}`")).edit(inner_span, near),
                    None => problem.next("Order by a group or an aggregate"),
                });
                continue;
            };
            if let Some(earlier) = out.iter().position(|o| o.entry == at) {
                ok = false;
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_ORDER_FIELD,
                        format!("`{written}` is in `order:` twice"),
                    )
                    .at(item_span.clone(), "again")
                    .also(spans[earlier].clone(), "first here")
                    .reason("Rows equal on an entry are sorted by the entries after it, so an entry listed again never changes the order.")
                    .next(format!("Remove the second `{written}`")),
                );
                continue;
            }
            let read = &reads[at];
            if !read.sortable {
                ok = false;
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_ORDER_FIELD,
                        format!("Rows can't be sorted by `{written}`"),
                    )
                    .at(item_span.clone(), format!("`{}`", read.ty))
                    .reason(format!(
                        "Rows sort by numbers, times, dates, durations, quantities, percentages, text and identities, which the database and Jadpo order the same way; `{written}` is `{}`, which has no order across currencies.",
                        read.ty
                    ))
                    .next("Sort by another group or aggregate"),
                );
                continue;
            }
            spans.push(item_span);
            out.push(EntryOrder {
                entry: at,
                descending,
                added: false,
            });
        }
        if list.exprs().next().is_none() {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_ORDER_FIELD,
                    "`order:` lists nothing".to_string(),
                )
                .at(Self::span(path, &value), "empty")
                .reason(format!(
                    "`order:` names at least one group or aggregate to sort by ({listing}); the compiler adds the groups it doesn't name."
                ))
                .next("Write `order: [<name> desc]`"),
            );
            return None;
        }
        if !ok {
            return None;
        }
        // The groups tell the rows apart.
        for (at, read) in reads.iter().enumerate() {
            if read.entry.is_group() && !out.iter().any(|o| o.entry == at) {
                out.push(EntryOrder {
                    entry: at,
                    descending: false,
                    added: true,
                });
            }
        }
        Some(out)
    }
}

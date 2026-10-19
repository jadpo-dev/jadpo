//! Area 6's queries (docs/compiler-runtime.md §2.3: "Queries are
//! declarative nodes, not bodies"): each `query` declaration read into a
//! [`QueryModel`], the one description its SQL, its indexes and its row
//! codecs are made from, with the problems found on the way.
//!
//! A query is one read (area 6, "Named queries"): `from:` names its
//! entity; stacked `where:` lines, each a condition every record must
//! meet, and `where_present:` lines that apply only when their value is
//! there; `order:`; `limit:` or `page:`; `include:` for related records;
//! and `missing:` for a query that returns exactly one record. Its result
//! type says how many records come back: `T`, `T?`, `List<T>` or
//! `Page<T>`, where `T` is the entity or an object type whose fields it
//! fills; or a map layer, `FeatureCollection<G, P>`, a bounded list of
//! features whose geometry is the field `geometry:` names and whose
//! properties `P` its fields fill (chunk 5.5u8). Values in a query are parameters and their fields, literals,
//! enum values, lists of those and `clock.now`: a query computes nothing.
//!
//! Each query is read by name ([`query_read`]) into a model without
//! positions ([`query_model`]) and the problems found in it, reading the
//! entities, types and failures it names by name too, so an edit reads
//! again only the query it is in, or a query that names what changed;
//! [`query_problems`] gives one file's share, and [`query_models`] every
//! model, for the indexes and the plan.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jadpo_db::{Db, File, ItemKind, Ownership, Project, Symbol, item_tree, symbols};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::{Key, Kind};
use jadpo_syntax::ast::{self, AstNode, Decl, Expr, MemberItem};

use super::{Entities, EntityModel, FieldModel};
use crate::case::starts_upper;
use crate::lookups::{FailureView, Lookup, Name, NameView, SigView, file_at};
use crate::tree::{self, dotted, node_span};
use crate::types::{Const, Def, FieldsBase, Prim, Ty, TypeView};

mod aggregates;
mod conditions;
mod geo;
mod includes;
mod missing;
mod operations;
mod order;
mod read;
mod values;
mod vectors;

use geo::{distance_entry, geo_condition_name, geo_kind};
use operations::{Asked, standard_operation};
use vectors::{is_distance, nearest_entry};
pub(crate) use vectors::{metric, vector_type};

pub use aggregates::{AggregateKind, Entry, EntryOrder, GroupKey, Grouping};

/// The sections a query declaration holds (area 6, "Named queries").
pub const SECTIONS: [&str; 11] = [
    "from",
    "geometry",
    "where",
    "where_present",
    "group",
    "aggregate",
    "order",
    "limit",
    "page",
    "include",
    "missing",
];

/// The settings of an include written as a block (area 6, "Children and
/// parents").
pub const INCLUDE_SETTINGS: [&str; 6] =
    ["via", "where", "where_present", "order", "limit", "include"];

/// How deep includes nest (area 6: "Includes nest up to two levels").
pub const INCLUDE_LEVELS: usize = 2;

/// The type a page query returns: `Page<T>`.
const PAGE: &str = "Page";

/// The type each record of a nearest-neighbour query is (area 21):
/// `Nearest<T>`, holding `item: T` and `distance: Decimal`.
pub const NEAREST: &str = "Nearest";

/// The most records a nearest-neighbour query returns (area 21, "The
/// nearest-neighbour query").
pub const MOST_NEAREST: u64 = 100;

/// The longest vector pgvector's HNSW index takes at full precision; a
/// longer one is indexed at half precision (area 21, "The index").
pub const MOST_INDEXED_DIMENSIONS: u64 = 2_000;

/// The longest vector pgvector's HNSW index takes at half precision, as a
/// `halfvec`; a longer one is searched exactly (area 21, "The index").
pub const MOST_HALF_INDEXED_DIMENSIONS: u64 = 4_000;

/// How a nearest-neighbour query searches, from its vector's length (area
/// 21, "The index"; chunk 5.5v1c).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VectorSearch {
    /// Through an HNSW index on the column: up to 2,000 dimensions.
    Index,
    /// Through an HNSW index on the column cast to `halfvec(n)`, the query
    /// ordering by the same expression: 2,001 to 4,000 dimensions.
    HalfPrecision(u64),
    /// Every record measured and sorted, which `query.nearest_exact`
    /// names and a decline accepts: over 4,000 dimensions.
    Exact,
}

impl VectorSearch {
    /// How a vector of `dimensions` is searched.
    pub fn of(dimensions: u64) -> VectorSearch {
        if dimensions <= MOST_INDEXED_DIMENSIONS {
            VectorSearch::Index
        } else if dimensions <= MOST_HALF_INDEXED_DIMENSIONS {
            VectorSearch::HalfPrecision(dimensions)
        } else {
            VectorSearch::Exact
        }
    }
}

/// A count as a sentence writes it, thousands apart: `5,000`.
pub(crate) fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

// --- the model ---------------------------------------------------------------

/// How many records a query returns, from its result type: the read
/// plans' own, decided once in `jadpo-std`.
pub use jadpo_std::read::Cardinality;

/// A condition on one entity's fields, its comparisons and the values it
/// reads: the SQL renderer's model, which the plan's statements and the
/// partial indexes render (`jadpo-sql`).
pub use jadpo_sql::form::Held;
pub use jadpo_sql::{Call, CallArg, Compare, Condition, Lowering, Test, Value, lowering};

/// A comparison's operator from its token: `==`, `<=`.
fn compare_token(token: &str) -> Option<Compare> {
    Some(match token {
        "==" => Compare::Equal,
        "!=" => Compare::NotEqual,
        "<" => Compare::Less,
        "<=" => Compare::LessEqual,
        ">" => Compare::Greater,
        ">=" => Compare::GreaterEqual,
        _ => return None,
    })
}

/// One field of an order: the entity's field and its direction.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OrderField {
    pub field: String,
    pub descending: bool,
    /// Added by the compiler as the tie-break, rather than written.
    pub added: bool,
}

/// How many records a list or a page holds at most.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Bound {
    Literal(u64),
    /// A value whose type has a `max:`.
    Value(Value),
}

/// Where a result type's field gets its value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// The entity's field of that name.
    Column(String),
    /// The include at this position in its level's list.
    Include(usize),
    /// The group or aggregate at this position among an aggregate query's
    /// entries (area 6, "Counts, sums and groups"), with the field it
    /// reads, which `count()` has none of.
    Entry { at: usize, field: Option<String> },
}

/// What a query's records become: the entity itself, or an object type
/// whose fields come from the entity's fields and its includes, in the
/// type's declared order.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Element {
    /// The type the records are values of.
    pub ty: String,
    pub fields: Vec<(String, Source)>,
}

/// How an include relates to the records it is included in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IncludeKind {
    /// Through one of this entity's own references: the record it points
    /// at.
    Parent,
    /// Through another entity's reference to this one: a list.
    Children,
    /// Through another entity's unique reference to this one: at most one.
    Child,
}

impl IncludeKind {
    pub fn word(self) -> &'static str {
        match self {
            IncludeKind::Parent => "parent",
            IncludeKind::Children => "children",
            IncludeKind::Child => "child",
        }
    }
}

/// An include: related records under one field of the result.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Include {
    /// The result type's field it fills.
    pub name: String,
    pub kind: IncludeKind,
    /// The entity it reads.
    pub entity: String,
    /// The reference it follows: a field of the including entity for a
    /// parent, of the included one otherwise.
    pub via: String,
    /// For a parent: whether the reference may be `none`.
    pub optional: bool,
    pub element: Element,
    pub filter: Vec<Condition>,
    pub order: Vec<OrderField>,
    pub limit: Option<Bound>,
    pub includes: Vec<Include>,
    /// The names written in its own `include:` block, right or wrong, so
    /// a wrong one isn't reported again as a field nothing fills.
    pub written: Vec<String>,
}

/// The failure a query for exactly one record raises when nothing
/// matches, with its fields.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Missing {
    pub failure: String,
    pub fields: Vec<(String, Value)>,
}

/// `order: [Entity.field nearest value]` (area 21, "The nearest-neighbour
/// query"): records nearest the value first, by the field's vector type's
/// distance, each with its distance.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NearestOrder {
    /// The vector field, of a type with `dimensions:`.
    pub field: String,
    /// The vector it measures from: a parameter of the field's type.
    pub value: Value,
    /// How the field's type measures.
    pub metric: jadpo_std::VectorDistance,
    /// The most records the query returns: its `limit:`, or the most its
    /// value's type allows.
    pub most: u64,
    /// The field's type's `dimensions:`.
    pub dimensions: u64,
    /// How it searches, from them (chunk 5.5v1c).
    pub search: VectorSearch,
}

/// `order: [geo.distance(Entity.field, point) asc]` (area 20, question 4):
/// records nearest the point first, by the distance `geo.distance` gives,
/// beside a `geo.within(Entity.field, point, distance:)` line on the same
/// field and point; `id` breaks ties.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DistanceOrder {
    /// The `Point` field.
    pub field: String,
    /// The point it measures from: a parameter, or a field inside one.
    pub point: Value,
}

/// One query, as its SQL and codecs are made from it, without positions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryModel {
    pub name: String,
    pub key: Key,
    pub path: String,
    /// The entity it reads.
    pub entity: String,
    pub params: Vec<(String, Ty)>,
    pub cardinality: Cardinality,
    pub element: Element,
    /// Every `where:` and `where_present:` line, in order; a record meets
    /// all of them.
    pub filter: Vec<Condition>,
    pub order: Vec<OrderField>,
    /// `limit:`, or a page's `size:`.
    pub limit: Option<Bound>,
    /// A page's `after:`.
    pub after: Option<Value>,
    pub includes: Vec<Include>,
    pub missing: Option<Missing>,
    /// For a nearest-neighbour query, its order, in place of `order`: each
    /// record is a `Nearest<T>` of `element`.
    pub nearest: Option<NearestOrder>,
    /// For a query ordered by `geo.distance`, its order, in place of
    /// `order` (area 20, chunk 5.5u5).
    pub distance: Option<DistanceOrder>,
    /// For an aggregate query, its groups, aggregates and their order, in
    /// place of `order`: each row is a value of `element`, filled from
    /// them.
    pub grouping: Option<Grouping>,
    /// For a map layer, a `FeatureCollection<G, P>` (chunk 5.5u8): the
    /// field each feature's geometry is, its properties being `element`.
    pub geometry: Option<String>,
}

/// Every query of a project, without positions: for what reads them all,
/// the indexes the queries need and the plan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Queries {
    pub queries: Vec<QueryModel>,
}

impl Queries {
    pub fn get(&self, name: &str) -> Option<&QueryModel> {
        self.queries.iter().find(|query| query.name == name)
    }

    pub fn by_key(&self, key: &Key) -> Option<&QueryModel> {
        self.queries.iter().find(|query| query.key == *key)
    }
}

/// One query read from its file: its model, and the problems found in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryRead {
    /// The file it is declared in.
    pub path: String,
    /// `None` when a problem leaves it without one: reported here, or by
    /// resolution (a type that names nothing).
    pub model: Option<Arc<QueryModel>>,
    pub problems: Vec<Diagnostic>,
}

/// The names of the project's queries, in name order.
#[salsa::tracked(returns(ref))]
pub fn query_names(db: &dyn Db, project: Project) -> Vec<String> {
    symbols(db, project)
        .declarations
        .iter()
        .filter(|(_, found)| found.iter().any(|s| s.kind == ItemKind::Query))
        .map(|(name, _)| name.clone())
        .collect()
}

/// The query called `name`, read from the first file that declares one: a
/// query of its own, which reads that file and the names and entities the
/// query uses, so an edit elsewhere never reads it again.
#[salsa::tracked(returns(ref))]
pub fn query_read<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> Option<QueryRead> {
    let lookup = Lookup::new(db, project);
    let symbol = lookup
        .declared(name.text(db))
        .iter()
        .find(|s| s.kind == ItemKind::Query)?;
    let file = file_at(db, project, Name::new(db, symbol.path.clone()))?;
    let source = tree::source(db, file)?;
    let Some(Decl::QueryDecl(decl)) = tree::decl_at(&source, symbol.index) else {
        return None;
    };
    let mut reader = Reader {
        lookup: &lookup,
        entities: Entities::new(db, project),
        out: Vec::new(),
        under: Vec::new(),
        layer: false,
    };
    let model = reader.query(symbol, &decl).map(Arc::new);
    Some(QueryRead {
        path: symbol.path.clone(),
        model,
        problems: reader.out,
    })
}

/// The query called `name`, without positions: what every check that
/// reads a query depends on.
#[salsa::tracked(returns(ref))]
pub fn query_model<'db>(
    db: &'db dyn Db,
    project: Project,
    name: Name<'db>,
) -> Option<Arc<QueryModel>> {
    query_read(db, project, name).as_ref()?.model.clone()
}

/// Whether the query called `name` read without a problem and has a model.
#[salsa::tracked(returns(copy))]
fn query_clean<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> bool {
    query_read(db, project, name)
        .as_ref()
        .is_none_or(|read| read.problems.is_empty() && read.model.is_some())
}

/// Whether every query read without a problem, so the indexes the queries
/// ask for are all known.
pub(super) fn queries_clean(db: &dyn Db, project: Project) -> bool {
    query_names(db, project)
        .iter()
        .all(|name| query_clean(db, project, Name::new(db, name.clone())))
}

/// The project's queries, without positions, in name order.
#[salsa::tracked(returns(ref))]
pub fn query_models(db: &dyn Db, project: Project) -> Arc<Queries> {
    Arc::new(Queries {
        queries: query_names(db, project)
            .iter()
            .filter_map(|name| {
                query_model(db, project, Name::new(db, name.clone()))
                    .as_deref()
                    .cloned()
            })
            .collect(),
    })
}

/// Area 6's query problems in one file: those of the queries it declares.
#[salsa::tracked(returns(ref))]
pub fn query_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for item in &item_tree(db, file).items {
        if item.kind != ItemKind::Query || !seen.insert(item.name.as_str()) {
            continue;
        }
        let Some(read) = query_read(db, project, Name::new(db, item.name.clone())) else {
            continue;
        };
        if read.path != *path {
            continue;
        }
        out.extend(read.problems.iter().cloned());
    }
    out
}

// --- reading a query ---------------------------------------------------------

struct Reader<'a> {
    lookup: &'a Lookup<'a>,
    entities: Entities<'a>,
    out: Vec<Diagnostic>,
    /// The `not` or `or` a condition being read is under, innermost last:
    /// a `geo.` condition stands on its own line or joined by `and`.
    under: Vec<&'static str>,
    /// Whether the query read is a map layer, whose properties come from
    /// its records' own fields alone (chunk 5.5u8).
    layer: bool,
}

/// The sections of one query or include block, as written.
#[derive(Default)]
struct Sections {
    first: BTreeMap<String, ast::Member>,
    filters: Vec<(bool, ast::Member)>,
    /// Sections an unknown one was reported as probably meaning, which
    /// aren't reported missing as well.
    suggested: BTreeSet<String>,
}

/// What a query is reading: its file, its parameters and the entity whose
/// fields its conditions name.
struct Scope<'s> {
    path: &'s str,
    params: &'s [(String, Ty)],
}

/// A type's name in backticks after "a" or "an".
fn a_or_an(name: &str) -> String {
    let an = name
        .chars()
        .next()
        .is_some_and(|c| "AEIOUaeiou".contains(c));
    format!("{} `{name}`", if an { "an" } else { "a" })
}

/// Field `field` of a standard object type (an `InstantRange`'s `start`
/// or `end`), by its type.
fn range_field(ty: &Ty, field: &str) -> Option<Ty> {
    match ty.strip_nullable() {
        Ty::Opaque(name, args) if args.is_empty() => crate::types::standard_fields(name)?
            .iter()
            .find(|f| f.name == field)
            .map(|f| f.declared.clone()),
        _ => None,
    }
}

pub(crate) fn ordered(lookup: &dyn TypeView, ty: &Ty) -> bool {
    matches!(
        lookup.representation(ty),
        Some(
            Prim::Int
                | Prim::Decimal
                | Prim::Instant
                | Prim::CalendarDate
                | Prim::Duration
                | Prim::Quantity(_)
                | Prim::Percentage
        )
    )
}

/// Whether a value of type `a` can be compared with field type `b`:
/// the same type after widening, or two named types with a common
/// ancestor, as `==` allows in a body (area 3).
pub(crate) fn comparable(lookup: &dyn TypeView, a: &Ty, b: &Ty) -> bool {
    let (a, b) = (a.strip_nullable(), b.strip_nullable());
    if a.has_error() || b.has_error() {
        return true;
    }
    if lookup.assignable(a, b) || lookup.assignable(b, a) {
        return true;
    }
    if matches!(a, Ty::Prim(_)) || matches!(b, Ty::Prim(_)) {
        return match (lookup.representation(a), lookup.representation(b)) {
            (Some(x), Some(y)) => x == y || (x.is_number() && y.is_number()),
            _ => false,
        };
    }
    // A common ancestor: a named type, or a built-in type other than
    // `Uuid`, since two entities' identities are different things
    // (area 2, "sibling mismatch") while two instants are not.
    let ancestors = |ty: &Ty| {
        let mut found = Vec::new();
        let mut current = Some(ty.clone());
        for _ in 0..64 {
            let Some(next) = current else { break };
            let next = next.strip_nullable().clone();
            if !matches!(next, Ty::Prim(Prim::Uuid)) {
                found.push(next.clone());
            }
            current = lookup.parent(&next);
        }
        found
    };
    let mine = ancestors(a);
    ancestors(b).iter().any(|t| mine.contains(t))
}

/// Adds `id` as the last field of an order whose fields could tie: records
/// equal on every written field never swap places between pages (area 6,
/// question 3). A unique field last already decides every tie.
pub fn tie_break(entity: &EntityModel, order: &mut Vec<OrderField>) {
    let Some(last) = order.last() else {
        return;
    };
    let decided = last.field == "id"
        || entity
            .fields
            .iter()
            .any(|f| f.name == last.field && f.unique && !f.nullable);
    if decided {
        return;
    }
    let descending = last.descending;
    order.push(OrderField {
        field: "id".to_string(),
        descending,
        added: true,
    });
}

fn unparen(expr: &Expr) -> Expr {
    let mut current = expr.clone();
    while let Expr::ParenExpr(paren) = &current {
        match paren.expr() {
            Some(inner) => current = inner,
            None => break,
        }
    }
    current
}

/// The archived compiler's section names, and the ones they became.
fn renamed(written: &str) -> Option<&'static str> {
    match written {
        "order_by" => Some("order"),
        "and" => Some("where"),
        "project" => Some("include"),
        _ => None,
    }
}

/// The first two words of a declaration or entry, to name it by.
fn first_words(node: &jadpo_syntax::SyntaxNode) -> String {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .take(2)
        .map(|token| token.text().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn field_type_name(ty: &Ty) -> String {
    ty.to_string()
}

/// An expression as written, its spaces kept and its lines joined.
fn shown(node: &jadpo_syntax::SyntaxNode) -> String {
    node.text()
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A constant's value as a query holds it: a literal, a plain variant or a
/// list of those (area 3). An object, or a variant carrying data, isn't
/// one a condition compares with.
fn query_value(value: &crate::constants::Value) -> Option<Value> {
    use crate::constants::Value as Fixed;
    Some(match value {
        Fixed::Number { value, decimal } => Value::Literal(jadpo_graph::body::Literal::Number {
            value: *value,
            decimal: *decimal,
        }),
        Fixed::Text(text) => Value::Literal(jadpo_graph::body::Literal::Text(text.clone())),
        Fixed::Bool(value) => Value::Literal(jadpo_graph::body::Literal::Bool(*value)),
        Fixed::Duration(text) => Value::Literal(jadpo_graph::body::Literal::Duration(text.clone())),
        Fixed::None => Value::Literal(jadpo_graph::body::Literal::None),
        Fixed::Variant(host, variant, fields) if fields.is_empty() => Value::Variant {
            ty: crate::types::shown(host).to_string(),
            variant: variant.clone(),
        },
        Fixed::List(items) => Value::List(items.iter().map(query_value).collect::<Option<_>>()?),
        Fixed::Variant(..) | Fixed::Object(..) => return None,
    })
}

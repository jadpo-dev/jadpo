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
//! fills. Values in a query are parameters and their fields, literals,
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

/// The sections a query declaration holds (area 6, "Named queries").
pub const SECTIONS: [&str; 8] = [
    "from",
    "where",
    "where_present",
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

// --- the model ---------------------------------------------------------------

/// How many records a query returns, from its result type: the read
/// plans' own, decided once in `jadpo-std`.
pub use jadpo_std::read::Cardinality;

/// A condition on one entity's fields, its comparisons and the values it
/// reads: the SQL renderer's model, which the plan's statements and the
/// partial indexes render (`jadpo-sql`).
pub use jadpo_sql::{Compare, Condition, Value};

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

impl<'a> Reader<'a> {
    fn span(path: &str, node: &impl AstNode) -> Span {
        node_span(path, node.syntax())
    }

    fn query(&mut self, symbol: &Symbol, decl: &ast::QueryDecl) -> Option<QueryModel> {
        let path = symbol.path.as_str();
        let name_node = decl.name()?;
        let name = tree::name_text(&name_node)?;
        let span = Self::span(path, &name_node);
        let sig = self.lookup.signature(&name)?.clone();
        let scope = Scope {
            path,
            params: &sig.params,
        };
        let list = decl.member_list();
        let sections = self.sections(path, list.clone(), &SECTIONS, "query", &name);

        // `from:` names the entity read.
        let Some(from) = sections.first.get("from") else {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_SECTION_REQUIRED,
                    format!("`{name}` doesn't say what it reads"),
                )
                .at(span.clone(), "no `from:`")
                .reason("A query reads one entity, named in `from:`; its conditions, order and includes are about that entity's records.")
                .next("Add `from: <Entity>` as the query's first line"),
            );
            return None;
        };
        let entity = self.from(path, from, symbol)?;

        // The result type says how many records come back.
        let result_span = decl
            .ret_type()
            .and_then(|ret| ret.ty())
            .map_or_else(|| span.clone(), |ty| Self::span(path, &ty));
        let (cardinality, element_ty) = self.result(&name, &sig.result, entity, &result_span)?;

        // What each cardinality needs and refuses.
        let (needs, refuses): (&[&str], &[&str]) = match cardinality {
            Cardinality::One => (&["missing"], &["order", "limit", "page"]),
            Cardinality::Optional => (&[], &["missing", "order", "limit", "page"]),
            Cardinality::List => (&["order", "limit"], &["page", "missing"]),
            Cardinality::Page => (&["order", "page"], &["limit", "missing"]),
        };
        let mut complete = true;
        for section in needs {
            if sections.first.contains_key(*section) {
                continue;
            }
            complete = false;
            if !sections.suggested.contains(*section) {
                self.section_required(&name, &span, cardinality, &sig.result, section);
            }
        }
        for section in refuses {
            if let Some(member) = sections.first.get(*section) {
                self.section_unexpected(path, member, section, cardinality, &sig.result);
            }
        }

        // Conditions.
        let mut filter = Vec::new();
        for (present, member) in &sections.filters {
            if let Some(condition) = self.filter_line(&scope, entity, member, *present) {
                filter.push(condition);
            }
        }
        if matches!(cardinality, Cardinality::One | Cardinality::Optional) {
            self.unique(path, entity, &sections, &filter, &span, &sig.result);
        }

        // Order, limit and page.
        let order = match sections.first.get("order") {
            Some(member) if !refuses.contains(&"order") => {
                self.order(path, entity, member, cardinality == Cardinality::Page)
            }
            _ => Vec::new(),
        };
        let limit = match (cardinality, sections.first.get("limit")) {
            (Cardinality::List, Some(member)) => {
                self.bound(&scope, member.value(), member, "limit")
            }
            _ => None,
        };
        let (size, after) = match (cardinality, sections.first.get("page")) {
            (Cardinality::Page, Some(member)) => self.page(&scope, member),
            _ => (None, None),
        };

        // Includes and the result's fields.
        let includes = match sections.first.get("include") {
            Some(member) => self.includes(&scope, entity, member, 1, symbol),
            None => Vec::new(),
        };
        let element = self.element(
            path,
            &element_ty,
            entity,
            &includes,
            sections.first.get("include"),
            &result_span,
        );
        // The includes with their own records' fields filled in.
        let (element, includes) = match element {
            Some((element, filled)) => (Some(element), filled),
            None => (None, includes),
        };

        // `missing:` and the `fails` list.
        let missing = match (cardinality, sections.first.get("missing")) {
            (Cardinality::One, Some(member)) => self.missing(&scope, member),
            _ => None,
        };
        self.fails(
            path,
            decl,
            &name,
            missing.as_ref(),
            sections.first.get("missing"),
        );

        let element = element?;
        if !complete {
            return None;
        }
        let key = self.lookup.registry().key(Kind::Query, &name, None);
        Some(QueryModel {
            name,
            key,
            path: path.to_string(),
            entity: entity.name.clone(),
            params: sig.params.clone(),
            cardinality,
            element,
            filter,
            order,
            limit: limit.or(size),
            after,
            includes,
            missing,
        })
    }

    /// The members of a query's or include's block, each section once
    /// except the condition lines, which stack.
    fn sections(
        &mut self,
        path: &str,
        list: Option<ast::MemberList>,
        known: &[&str],
        what: &str,
        name: &str,
    ) -> Sections {
        let mut out = Sections::default();
        for item in list.iter().flat_map(|list| list.member_items()) {
            let (node, written) = match &item {
                MemberItem::Member(member) => {
                    let Some(key) = member.member_key() else {
                        continue;
                    };
                    let names: Vec<String> =
                        key.names().filter_map(|n| tree::name_text(&n)).collect();
                    match names.as_slice() {
                        [word] if known.contains(&word.as_str()) => {
                            if word == "where" || word == "where_present" {
                                out.filters.push((word == "where_present", member.clone()));
                            } else if let Some(earlier) = out.first.get(word) {
                                let earlier = earlier.member_key().map_or_else(
                                    || Span::new(path, 0, 0),
                                    |k| Self::span(path, &k),
                                );
                                self.out.push(
                                    Diagnostic::new(
                                        &rule::QUERY_REPEATED_SECTION,
                                        format!("`{word}:` is given twice"),
                                    )
                                    .at(Self::span(path, &key), "given again")
                                    .also(earlier, "first given here")
                                    .reason(format!(
                                        "A {what} has one `{word}:`; only `where:` and `where_present:` lines stack, each one more condition."
                                    ))
                                    .next(format!("Remove one `{word}:`")),
                                );
                            } else {
                                out.first.insert(word.clone(), member.clone());
                            }
                            continue;
                        }
                        _ => (key.syntax().clone(), names.join(".")),
                    }
                }
                MemberItem::BareMember(bare) => {
                    (bare.syntax().clone(), tree::written(bare.syntax()))
                }
                MemberItem::Decl(decl) => (decl.syntax().clone(), first_words(decl.syntax())),
                MemberItem::RouteEntry(entry) => {
                    (entry.syntax().clone(), first_words(entry.syntax()))
                }
                MemberItem::Elision(_) => continue,
            };
            let span = node_span(path, &node);
            let near = nearest(&written, known.iter().copied())
                .map(str::to_string)
                .or_else(|| renamed(&written).map(str::to_string));
            if let Some(near) = &near {
                out.suggested.insert(near.clone());
            }
            let listed: Vec<String> = known.iter().map(|s| format!("`{s}:`")).collect();
            let problem = Diagnostic::new(
                &rule::QUERY_UNKNOWN_SECTION,
                format!("`{written}` isn't something a {what} holds"),
            )
            .at(span.clone(), "not a section")
            .reason(format!(
                "A {what}'s block holds {}.{}",
                crate::and_list(&listed),
                near.as_ref()
                    .map(|n| format!(" Did you mean `{n}:`?"))
                    .unwrap_or_default()
            ));
            self.out.push(match near {
                Some(near) if matches!(item, MemberItem::Member(_)) => {
                    problem.next(format!("Use `{near}:`")).edit(span, near)
                }
                _ => problem.next(format!("Remove it from `{name}`")),
            });
        }
        out
    }

    /// `from: Entity`, checked: an entity of the query's own domain.
    fn from(
        &mut self,
        path: &str,
        member: &ast::Member,
        query: &Symbol,
    ) -> Option<&'a EntityModel> {
        let value = member.value();
        let span = value.as_ref().map_or_else(
            || {
                member
                    .member_key()
                    .map_or_else(|| Span::new(path, 0, 0), |k| Self::span(path, &k))
            },
            |v| Self::span(path, v),
        );
        let written = value
            .as_ref()
            .map(|v| tree::written(v.syntax()))
            .unwrap_or_default();
        let named = match &value {
            Some(Expr::NameRef(name)) => tree::name_ref_text(name),
            _ => None,
        };
        let symbol = named.as_ref().and_then(|name| {
            self.lookup
                .declared(name)
                .iter()
                .find(|symbol| symbol.library.is_none())
                .cloned()
        });
        match symbol {
            Some(symbol) if symbol.kind == ItemKind::Entity => {
                if let (Ownership::Domain(theirs), Ownership::Domain(ours)) =
                    (&symbol.ownership, &query.ownership)
                    && theirs != ours
                {
                    self.out.push(
                        Diagnostic::new(
                            &rule::QUERY_OTHER_DOMAIN,
                            format!("`{}` reads `{written}`, an entity of domain `{theirs}`", query.name),
                        )
                        .at(span, format!("domain `{theirs}`"))
                        .reason(format!(
                            "A query reads its own domain's entities only; another domain's data comes from that domain's public queries, so `{theirs}` decides what it shares."
                        ))
                        .next(format!(
                            "Call a public query of `{theirs}`, or move `{}` into `{theirs}`",
                            query.name
                        )),
                    );
                    return None;
                }
                self.entities.get(&symbol.name)
            }
            found => {
                let what = match found.as_ref().map(|s| &s.kind) {
                    Some(ItemKind::Type(_)) => format!("`{written}` is a type"),
                    Some(kind) => format!("`{written}` is {}", kind.with_a()),
                    None if named.is_some() => format!("Nothing called `{written}` is declared"),
                    None => format!("`{written}` isn't a name"),
                };
                let all = self.entities.all();
                let near = named
                    .as_deref()
                    .and_then(|n| nearest(n, all.entities.iter().map(|e| e.name.as_str())))
                    .map(str::to_string);
                let problem = Diagnostic::new(
                    &rule::QUERY_FROM_NOT_ENTITY,
                    format!("`from:` names `{written}`, which isn't an entity"),
                )
                .at(span.clone(), "not an entity")
                .reason(format!(
                    "{what}; a query reads stored records, so `from:` names an entity declared in the domain's `entities/` folder."
                ));
                self.out.push(match near {
                    Some(near) => problem.next(format!("Use `from: {near}`")).edit(span, near),
                    None => problem.next("Name the entity the query reads"),
                });
                None
            }
        }
    }

    /// The cardinality the result type gives, and the type of its records.
    fn result(
        &mut self,
        name: &str,
        result: &Ty,
        entity: &EntityModel,
        span: &Span,
    ) -> Option<(Cardinality, String)> {
        if result.has_error() {
            return None;
        }
        let (cardinality, element) = match result {
            Ty::List(inner) => (Cardinality::List, inner.as_ref()),
            Ty::Opaque(page, args) if page == PAGE && args.len() == 1 => {
                (Cardinality::Page, &args[0])
            }
            Ty::Nullable(inner) => (Cardinality::Optional, inner.as_ref()),
            other => (Cardinality::One, other),
        };
        if element.has_error() {
            return None;
        }
        let fits = match element {
            Ty::Named(ty) if *ty == entity.name => true,
            Ty::Named(ty) => matches!(
                self.lookup.def(ty),
                Some(Def::Fields {
                    base: FieldsBase::Object | FieldsBase::Output,
                    ..
                })
            ),
            _ => false,
        };
        if let (true, Ty::Named(ty)) = (fits, element) {
            return Some((cardinality, ty.clone()));
        }
        let shown = if matches!(result, Ty::Prim(Prim::Unit)) {
            "nothing".to_string()
        } else {
            format!("`{result}`")
        };
        let entity_name = &entity.name;
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_RESULT_TYPE,
                format!("`{name}` reads `{entity_name}` records but returns {shown}"),
            )
            .at(span.clone(), "not records of the entity")
            .reason(format!(
                "A query's result is `{entity_name}` or an object type its fields fill (`type {entity_name}View = Output {{ pick: [...] }}`), as `T` for exactly one record, `T?` for at most one, `List<T>` or `Page<T>`."
            ))
            .next(format!(
                "Return `{entity_name}`, or an `Output` type that picks the fields you need"
            )),
        );
        None
    }

    fn section_required(
        &mut self,
        name: &str,
        span: &Span,
        cardinality: Cardinality,
        result: &Ty,
        section: &str,
    ) {
        let (reason, next) = match section {
            "missing" => (
                format!(
                    "`{result}` is exactly one record, so the query says which failure a caller gets when no record matches: a `not_found` failure in `missing:`. A query that may find nothing returns `{result}?`."
                ),
                "Add `missing: <Entity>NotFound`, or return an optional result".to_string(),
            ),
            "order" => (
                format!(
                    "A {} of records comes back in a fixed order, so its pages and items never change places between calls; `order:` names the fields to sort by.",
                    cardinality.word()
                ),
                "Add `order: [<Entity>.<field> asc]`".to_string(),
            ),
            "limit" => (
                "Every list is bounded: `limit:` is a number, or a value whose type has a `max:`.".to_string(),
                "Add `limit: <number>`, or return `Page<T>` to read it a page at a time".to_string(),
            ),
            _ => (
                "A page query reads one page at a time: `page:` gives its `size:` and the cursor to continue `after:`.".to_string(),
                "Add `page: { size: ... after: ... }`".to_string(),
            ),
        };
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_SECTION_REQUIRED,
                format!("`{name}` returns `{result}` but has no `{section}:`"),
            )
            .at(span.clone(), format!("no `{section}:`"))
            .reason(reason)
            .next(next),
        );
    }

    fn section_unexpected(
        &mut self,
        path: &str,
        member: &ast::Member,
        section: &str,
        cardinality: Cardinality,
        result: &Ty,
    ) {
        let Some(key) = member.member_key() else {
            return;
        };
        let reason = match (section, cardinality) {
            ("missing", Cardinality::Optional) => format!(
                "`{result}` already says the record may be absent: the caller gets `none`, so there is no failure to name."
            ),
            ("missing", _) => format!(
                "`missing:` names the failure for a query that returns exactly one record; `{result}` holds as many as match, none included."
            ),
            ("order" | "limit", Cardinality::One | Cardinality::Optional) => format!(
                "`{result}` is at most one record, found by a unique field, so there is nothing to order or limit."
            ),
            ("limit", _) => {
                "A page's size is its `page:` block's `size:`; `limit:` bounds a `List<T>`."
                    .to_string()
            }
            ("page", Cardinality::List) => format!(
                "`page:` belongs to a query returning `Page<T>`; `{result}` is one bounded list, sized by `limit:`."
            ),
            _ => format!("`{result}` is at most one record, so it has no pages."),
        };
        let removal = Self::span(path, member);
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_SECTION_UNEXPECTED,
                format!("A query returning `{result}` takes no `{section}:`"),
            )
            .at(Self::span(path, &key), "not for this result")
            .reason(reason)
            .next(format!("Remove `{section}:`"))
            .edit(removal, ""),
        );
    }

    // --- conditions ------------------------------------------------------------

    /// One `where:` or `where_present:` line.
    fn filter_line(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        member: &ast::Member,
        present: bool,
    ) -> Option<Condition> {
        let value = member.value()?;
        if !present {
            return self.condition(scope, entity, &value, false);
        }
        // `where_present:` holds one comparison with a value that may be
        // absent.
        let expr = unparen(&value);
        let comparison = match &expr {
            Expr::BinExpr(binary) => binary
                .op()
                .is_some_and(|op| compare_token(op.text()).is_some() || op.text() == "in"),
            _ => false,
        };
        if !comparison {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_CONDITION_UNSUPPORTED,
                    "`where_present:` holds one comparison".to_string(),
                )
                .at(Self::span(scope.path, &value), "not one comparison")
                .reason("`where_present:` applies its condition only when the value on the right is there, which is how a request's optional filters are written: one field compared with one optional value, `<Entity>.<field> == filter.<field>`.")
                .next("Write one comparison, or move the condition to `where:`"),
            );
            return None;
        }
        let condition = self.condition(scope, entity, &expr, true)?;
        let (value_ref, value_expr) = match (&condition, &expr) {
            (Condition::Compare { value, .. } | Condition::In { value, .. }, Expr::BinExpr(b)) => {
                (value.clone(), b.rhs())
            }
            _ => return None,
        };
        let optional = value_expr
            .as_ref()
            .and_then(|rhs| self.value_type(scope, rhs))
            .is_some_and(|ty| ty.has_error() || self.lookup.is_nullable(&ty));
        if !optional {
            let written = value_expr
                .as_ref()
                .map(|e| tree::written(e.syntax()))
                .unwrap_or_default();
            let span = value_expr.as_ref().map_or_else(
                || Self::span(scope.path, &value),
                |e| Self::span(scope.path, e),
            );
            let key = member
                .member_key()
                .map_or_else(|| span.clone(), |k| Self::span(scope.path, &k));
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_OPTIONAL_VALUE,
                    format!("`{written}` is always there, so `where_present:` would always apply"),
                )
                .at(span, "never absent")
                .reason("`where_present:` is for a value that may be absent, such as an optional field of a request's `Input`; a condition that always applies is a `where:` line.")
                .next("Write `where:` instead")
                .edit(key, "where"),
            );
            return None;
        }
        Some(Condition::Present {
            value: value_ref,
            condition: Box::new(condition),
        })
    }

    /// A condition on `entity`'s fields. `optional` allows the compared
    /// value to be absent (`where_present:`).
    fn condition(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        expr: &Expr,
        optional: bool,
    ) -> Option<Condition> {
        let expr = unparen(expr);
        match &expr {
            Expr::BinExpr(binary) => {
                let op = binary.op()?.text().to_string();
                let lhs = binary.lhs()?;
                let rhs = binary.rhs()?;
                match op.as_str() {
                    "and" | "or" => {
                        let left = self.condition(scope, entity, &lhs, optional);
                        let right = self.condition(scope, entity, &rhs, optional);
                        let (left, right) = (left?, right?);
                        let mut parts = Vec::new();
                        for part in [left, right] {
                            match (op.as_str(), part) {
                                ("and", Condition::And(inner)) | ("or", Condition::Or(inner)) => {
                                    parts.extend(inner)
                                }
                                (_, other) => parts.push(other),
                            }
                        }
                        Some(if op == "and" {
                            Condition::And(parts)
                        } else {
                            Condition::Or(parts)
                        })
                    }
                    "in" => {
                        let field = self.condition_field(scope, entity, &lhs, &expr)?;
                        let field_ty = self.field_ty(entity, field);
                        let wanted = Ty::List(Box::new(field_ty.strip_nullable().clone()));
                        let (value, ty) = self.value(scope, &rhs, &wanted)?;
                        if !optional && self.lookup.is_nullable(&ty) {
                            self.optional_in_where(scope, &rhs);
                            return None;
                        }
                        let element = match self.lookup.structure(&ty) {
                            Ty::List(inner) | Ty::Set(inner) => Some(*inner),
                            _ => None,
                        };
                        match element {
                            Some(element) if self.comparable(&element, &field_ty) => {}
                            _ => {
                                self.mismatch(scope, &rhs, &ty, &wanted);
                                return None;
                            }
                        }
                        Some(Condition::In {
                            field: field.name.clone(),
                            value,
                        })
                    }
                    token => {
                        let Some(compare) = compare_token(token) else {
                            self.unsupported(
                                scope,
                                &expr,
                                format!("`{token}` isn't one of a query's operators: conditions compare a field with `==`, `!=`, `<`, `<=`, `>`, `>=` or `in`, and join with `and`, `or` and `not`."),
                                "Compare the field with `==`, `!=`, `<`, `<=`, `>`, `>=` or `in`".to_string(),
                            );
                            return None;
                        };
                        let field = self.condition_field(scope, entity, &lhs, &expr)?;
                        let field_ty = self.field_ty(entity, field);
                        if matches!(Const::from_expr(&rhs), Const::None)
                            && matches!(compare, Compare::Equal | Compare::NotEqual)
                        {
                            if !field.nullable {
                                let written = format!("{}.{}", entity.name, field.name);
                                self.out.push(
                                    Diagnostic::new(
                                        &rule::TYPE_INCOMPARABLE,
                                        format!("`{written}` is never `none`"),
                                    )
                                    .at(Self::span(scope.path, &expr), format!("`{field_ty}` compared with `none`"))
                                    .reason(format!(
                                        "Only a nullable field, declared `T?`, can be `none`; `{written}` is `{field_ty}`."
                                    ))
                                    .next("Remove the condition, since it is always the same"),
                                );
                                return None;
                            }
                            return Some(Condition::IsNone {
                                field: field.name.clone(),
                                negated: compare == Compare::NotEqual,
                            });
                        }
                        if compare.is_ordering() && !self.ordered(&field_ty) {
                            let written = format!("{}.{}", entity.name, field.name);
                            self.unsupported(
                                scope,
                                &expr,
                                format!("`{token}` orders numbers, times, dates, durations and quantities, as it does in a body, and `{written}` is `{}`.", field.ty.strip_nullable()),
                                format!("Compare `{written}` with `==`, `!=` or `in`"),
                            );
                            return None;
                        }
                        let wanted = field_ty.strip_nullable().clone();
                        let (value, ty) = self.value(scope, &rhs, &wanted)?;
                        if !optional && self.lookup.is_nullable(&ty) {
                            self.optional_in_where(scope, &rhs);
                            return None;
                        }
                        if !matches!(value, Value::Literal(_)) && !self.comparable(&ty, &field_ty) {
                            self.mismatch(scope, &rhs, &ty, &wanted);
                            return None;
                        }
                        Some(Condition::Compare {
                            field: field.name.clone(),
                            op: compare,
                            value,
                        })
                    }
                }
            }
            Expr::PrefixExpr(prefix) if prefix.op().is_some_and(|op| op.text() == "not") => {
                let inner = self.condition(scope, entity, &prefix.expr()?, optional)?;
                Some(Condition::Not(Box::new(inner)))
            }
            _ => {
                let (reason, next) = match self.field_named(entity, &expr) {
                    Some(field) => (
                        format!(
                            "A condition compares a field with a value; `{}.{}` alone says nothing to compare it with.",
                            entity.name, field.name
                        ),
                        format!("Write `{}.{} == true`", entity.name, field.name),
                    ),
                    None => (
                        "A condition compares one of the entity's fields with a value, so the database can use an index for it.".to_string(),
                        format!("Compare a field of `{}` with a value", entity.name),
                    ),
                };
                self.unsupported(scope, &expr, reason, next);
                None
            }
        }
    }

    /// The field a comparison tests: `Todo.status`, on the left.
    fn condition_field<'e>(
        &mut self,
        scope: &Scope<'_>,
        entity: &'e EntityModel,
        lhs: &Expr,
        whole: &Expr,
    ) -> Option<&'e FieldModel> {
        let lhs = unparen(lhs);
        if let Expr::CallExpr(call) = &lhs
            && call
                .arg_list()
                .into_iter()
                .flat_map(|list| list.args())
                .filter_map(|arg| arg.expr())
                .any(|arg| self.field_named(entity, &arg).is_some())
        {
            self.unsupported(
                scope,
                whole,
                "A function applied to a field changes it before it is compared, so no index on the field can serve the condition.".to_string(),
                "Compare the field itself, and work the value out before the query".to_string(),
            );
            return None;
        }
        if let Some((first, rest)) = dotted(&lhs)
            && rest.len() == 1
            && starts_upper(&tree::name_ref_text(&first).unwrap_or_default())
        {
            let field = self.field_ref(scope.path, entity, &lhs)?;
            // A compound type is stored in several columns, which a
            // query's condition doesn't compare yet (area 19).
            if field.compound.is_some() {
                let written = shown(whole.syntax());
                self.out.push(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_NOT_BUILT_YET,
                        format!("Comparing `{}.{}` in a query isn't built yet", entity.name, field.name),
                    )
                    .at(
                        Self::span(scope.path, whole),
                        format!("a `{}`", field.compound.unwrap_or_default()),
                    )
                    .reason(format!(
                        "`{}.{}` is a `{}`, stored in several columns (an amount of money in its minor units and its currency, area 19), and `{written}` would compare them all, which this compiler's queries don't do yet.",
                        entity.name,
                        field.name,
                        field.compound.unwrap_or_default()
                    ))
                    .next("Filter the records after the query for now"),
                );
                return None;
            }
            return Some(field);
        }
        let reason = match self.field_in(scope, entity, whole) {
            true => {
                "Write the field on the left and the value on the right: `<Entity>.<field> > value`."
                    .to_string()
            }
            false => format!(
                "A condition compares one of `{}`'s fields, named with its entity (`{}.field`), with a value.",
                entity.name, entity.name
            ),
        };
        self.unsupported(
            scope,
            whole,
            reason,
            format!("Write `{}.<field>` on the left", entity.name),
        );
        None
    }

    /// Whether a comparison names a field of `entity` on its right.
    fn field_in(&self, _scope: &Scope<'_>, entity: &EntityModel, expr: &Expr) -> bool {
        match unparen(expr) {
            Expr::BinExpr(binary) => binary
                .rhs()
                .is_some_and(|rhs| self.field_named(entity, &rhs).is_some()),
            _ => false,
        }
    }

    /// `expr` as one of `entity`'s fields, without reporting anything.
    fn field_named<'e>(&self, entity: &'e EntityModel, expr: &Expr) -> Option<&'e FieldModel> {
        let (first, rest) = dotted(&unparen(expr))?;
        let [field] = rest.as_slice() else {
            return None;
        };
        if tree::name_ref_text(&first)? != entity.name {
            return None;
        }
        let field = tree::name_ref_text(field)?;
        entity.fields.iter().find(|f| f.name == field)
    }

    /// `Entity.field`, checked against the entity the query or include
    /// reads.
    fn field_ref<'e>(
        &mut self,
        path: &str,
        entity: &'e EntityModel,
        expr: &Expr,
    ) -> Option<&'e FieldModel> {
        let (first, rest) = dotted(expr)?;
        let [field_node] = rest.as_slice() else {
            return None;
        };
        let qualifier = tree::name_ref_text(&first)?;
        let field = tree::name_ref_text(field_node)?;
        let span = Self::span(path, expr);
        if qualifier != entity.name {
            let other = self.entities.get(&qualifier).is_some();
            let problem = Diagnostic::new(
                &rule::QUERY_UNKNOWN_FIELD,
                format!("`{qualifier}.{field}` isn't a field of `{}`", entity.name),
            )
            .at(Self::span(path, &first), format!("not `{}`", entity.name))
            .reason(if other {
                format!(
                    "This part of the query reads `{}` records, and `{qualifier}` is another entity; a related entity's records come in through `include:`.",
                    entity.name
                )
            } else {
                format!(
                    "Fields are named with their entity, and this part of the query reads `{}`.",
                    entity.name
                )
            });
            self.out
                .push(if entity.fields.iter().any(|f| f.name == field) {
                    problem
                        .next(format!("Write `{}.{field}`", entity.name))
                        .edit(Self::span(path, &first), entity.name.clone())
                } else {
                    problem.next(format!("Name a field of `{}`", entity.name))
                });
            return None;
        }
        match entity.fields.iter().find(|f| f.name == field) {
            Some(found) => Some(found),
            None => {
                let names: Vec<&str> = entity.fields.iter().map(|f| f.name.as_str()).collect();
                let near = nearest(&field, names.iter().copied());
                let listed: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
                let field_span = Self::span(path, field_node);
                let problem = Diagnostic::new(
                    &rule::QUERY_UNKNOWN_FIELD,
                    format!("`{}` has no field `{field}`", entity.name),
                )
                .at(field_span.clone(), "not declared")
                .reason(format!(
                    "`{}`'s fields are {}.{}",
                    entity.name,
                    crate::and_list(&listed),
                    near.map(|n| format!(" Did you mean `{n}`?"))
                        .unwrap_or_default()
                ));
                let _ = span;
                self.out.push(match near {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{field}`"))
                        .edit(field_span, near),
                    None => problem.next(format!("Name a field of `{}`", entity.name)),
                });
                None
            }
        }
    }

    /// A field's type as a condition compares it: `Todo.status`, or
    /// `Todo.status?` for a nullable field.
    fn field_ty(&self, entity: &EntityModel, field: &FieldModel) -> Ty {
        // A field whose type names nothing absorbs what is compared with it.
        if field.ty.has_error() {
            return Ty::Error;
        }
        let ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
        if field.nullable { ty.nullable() } else { ty }
    }

    /// Whether `<` and the rest order values of `ty`, as they do in a body
    /// (area 3): numbers, times, dates, durations and quantities.
    fn ordered(&self, ty: &Ty) -> bool {
        ordered(self.lookup, ty)
    }

    fn comparable(&self, a: &Ty, b: &Ty) -> bool {
        comparable(self.lookup, a, b)
    }

    fn unsupported(&mut self, scope: &Scope<'_>, expr: &Expr, reason: String, next: String) {
        let written = shown(expr.syntax());
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_CONDITION_UNSUPPORTED,
                format!("`{written}` isn't a condition a query can use"),
            )
            .at(Self::span(scope.path, expr), "not an indexable condition")
            .reason(reason)
            .next(next),
        );
    }

    fn optional_in_where(&mut self, scope: &Scope<'_>, value: &Expr) {
        let written = shown(value.syntax());
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_OPTIONAL_VALUE,
                format!("`{written}` may be absent, so this `where:` line can't always be met"),
            )
            .at(Self::span(scope.path, value), "may be `none`")
            .reason("A `where:` line applies to every call; a condition that should apply only when its value is there, as a request's optional filter, is written with `where_present:`.")
            .next("Write `where_present:` for this condition"),
        );
    }

    fn mismatch(&mut self, scope: &Scope<'_>, value: &Expr, actual: &Ty, expected: &Ty) {
        let written = shown(value.syntax());
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{written}` can't be compared with `{expected}`"),
            )
            .at(Self::span(scope.path, value), format!("a `{actual}`"))
            .reason(format!(
                "A condition compares a field with a value of the same type, after widening; `{written}` is `{actual}`."
            ))
            .next(format!("Compare with a `{expected}` value")),
        );
    }

    // --- values ----------------------------------------------------------------

    /// A value used in a query, checked against the type it meets. Gives
    /// the value and its type.
    fn value(&mut self, scope: &Scope<'_>, expr: &Expr, expected: &Ty) -> Option<(Value, Ty)> {
        let expr = unparen(expr);
        let constant = Const::from_expr(&expr);
        match &expr {
            Expr::Literal(_) | Expr::PrefixExpr(_)
                if !matches!(constant, Const::Other | Const::Path(_)) =>
            {
                if let Err(problem) =
                    crate::types::literal::check(self.lookup, expected.strip_nullable(), &constant)
                {
                    self.out.push(crate::types::literal::diagnostic(
                        self.lookup,
                        problem,
                        &shown(expr.syntax()),
                        expected.strip_nullable(),
                        Self::span(scope.path, &expr),
                        false,
                    ));
                    return None;
                }
                return Some((
                    Value::Literal(constant.literal()?),
                    expected.strip_nullable().clone(),
                ));
            }
            Expr::ListExpr(list) => {
                let element = match self.lookup.structure(expected) {
                    Ty::List(inner) | Ty::Set(inner) => *inner,
                    _ => {
                        self.out.push(
                            Diagnostic::new(
                                &rule::TYPE_MISMATCH,
                                format!("A list can't be used where `{expected}` is required"),
                            )
                            .at(Self::span(scope.path, &expr), "a list")
                            .reason(format!("A list holds several values, and this compares with one `{expected}`; `in` tests a field against a list."))
                            .next("Use `in` to compare with a list"),
                        );
                        return None;
                    }
                };
                let mut items = Vec::new();
                for item in list.exprs() {
                    let (value, ty) = self.value(scope, &item, &element)?;
                    if !matches!(value, Value::Literal(_)) && !self.comparable(&ty, &element) {
                        self.mismatch(scope, &item, &ty, &element);
                        return None;
                    }
                    if self.lookup.is_nullable(&ty) {
                        self.out.push(
                            Diagnostic::new(
                                &rule::QUERY_OPTIONAL_VALUE,
                                format!("`{}` may be absent, and a list's items can't be", shown(item.syntax())),
                            )
                            .at(Self::span(scope.path, &item), "may be `none`")
                            .reason("Each item of a list a field is tested `in` is a value; an absent one would match nothing.")
                            .next("Use a value that is always there"),
                        );
                        return None;
                    }
                    items.push(value);
                }
                return Some((Value::List(items), Ty::List(Box::new(element))));
            }
            _ => {}
        }
        let Some((first, rest)) = dotted(&expr) else {
            self.value_unsupported(scope, &expr);
            return None;
        };
        let head = tree::name_ref_text(&first)?;
        let rest: Vec<String> = rest.iter().filter_map(tree::name_ref_text).collect();
        if starts_upper(&head) {
            if let ([variant], Some(Def::Enum { variants })) =
                (rest.as_slice(), self.lookup.def(&head))
            {
                if variants.iter().any(|v| v.name == *variant) {
                    return Some((
                        Value::Variant {
                            ty: head.clone(),
                            variant: variant.clone(),
                        },
                        Ty::Named(head),
                    ));
                }
                let listed: Vec<String> =
                    variants.iter().map(|v| format!("`{}`", v.name)).collect();
                let near = nearest(variant, variants.iter().map(|v| v.name.as_str()));
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_UNKNOWN_ENUM_VARIANT,
                        format!("`{head}` has no variant `{variant}`"),
                    )
                    .at(Self::span(scope.path, &expr), "not a variant")
                    .reason(format!(
                        "`{head}`'s variants are {}.{}",
                        crate::and_list(&listed),
                        near.map(|n| format!(" Did you mean `{n}`?"))
                            .unwrap_or_default()
                    ))
                    .next(match near {
                        Some(near) => format!("Use `{head}.{near}`"),
                        None => format!("Use one of `{head}`'s variants"),
                    }),
                );
                return None;
            }
            // A declaration's name is no value (area 3); an entity's field
            // is a field, which has its own error.
            if let Some(named) = crate::not_value::classify(self.lookup, self.lookup, &head)
                && (rest.is_empty() || !matches!(named, crate::not_value::Named::Entity))
            {
                self.out.push(crate::not_value::diagnostic(
                    &head,
                    &named,
                    Self::span(scope.path, &first),
                ));
                return None;
            }
            if self.entities.get(&head).is_some() {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_CONDITION_UNSUPPORTED,
                        format!("`{}` compares a field with a field", shown(expr.syntax())),
                    )
                    .at(Self::span(scope.path, &expr), "a field, not a value")
                    .reason("A condition compares a field with a value the call supplies; comparing two fields of a record can't be served by an index.")
                    .next("Compare the field with a parameter, a literal or an enum value"),
                );
                return None;
            }
            self.value_unsupported(scope, &expr);
            return None;
        }
        if head == "clock" && rest == ["now"] {
            return Some((Value::Now, Ty::Prim(Prim::Instant)));
        }
        if !scope.params.iter().any(|(name, _)| *name == head)
            && let Some(found) = self.constant(scope, &expr, &head, &rest, expected)
        {
            return found;
        }
        let Some(index) = scope.params.iter().position(|(name, _)| *name == head) else {
            if let Some(named) = crate::not_value::classify(self.lookup, self.lookup, &head) {
                self.out.push(crate::not_value::diagnostic(
                    &head,
                    &named,
                    Self::span(scope.path, &first),
                ));
                return None;
            }
            if rest.is_empty()
                && let Some(problem) = crate::foreign_value(&head, Self::span(scope.path, &first))
            {
                self.out.push(problem);
                return None;
            }
            let names: Vec<&str> = scope.params.iter().map(|(n, _)| n.as_str()).collect();
            let near = nearest(&head, names.iter().copied());
            let span = Self::span(scope.path, &first);
            let problem = Diagnostic::new(
                &rule::SEMANTIC_UNKNOWN_NAME,
                format!("`{head}` isn't declared"),
            )
            .at(span.clone(), "not a parameter")
            .reason(format!(
                "A query's values come from its parameters{}, literals, constants, enum values and `clock.now`.{}",
                if names.is_empty() {
                    String::new()
                } else {
                    format!(
                        " ({})",
                        crate::and_list(&names.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>())
                    )
                },
                near.map(|n| format!(" Did you mean `{n}`?")).unwrap_or_default()
            ));
            self.out.push(match near {
                Some(near) => problem.next(format!("Use `{near}`")).edit(span, near),
                None => problem.next(format!("Add `{head}` to the query's parameters")),
            });
            return None;
        };
        let mut ty = scope.params[index].1.clone();
        for field in &rest {
            if self.lookup.is_nullable(&ty) {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_VALUE_UNSUPPORTED,
                        format!("`{}` may be `none`, so `{field}` can't be read", shown(expr.syntax())),
                    )
                    .at(Self::span(scope.path, &expr), "inside a value that may be `none`")
                    .reason("A query reads a parameter's fields only through values that are always there.")
                    .next("Pass the inner value as a parameter of its own"),
                );
                return None;
            }
            // A caller's field (area 8): `caller.id`.
            if let Ty::Opaque(name, _) = &ty
                && let Some(kind) = [
                    jadpo_std::http::PrincipalKind::User,
                    jadpo_std::http::PrincipalKind::Service,
                ]
                .into_iter()
                .find(|kind| kind.type_name() == name)
                && let Some(found) = self.lookup.identity().field_type(kind, field)
            {
                ty = found;
                continue;
            }
            // The bounds `temporal` gives (area 10, "Zones and local times").
            if let Some(found) = range_field(&ty, field) {
                ty = found;
                continue;
            }
            let Some(host) = self.object_of(&ty) else {
                self.value_unsupported(scope, &expr);
                return None;
            };
            let Some(found) = self
                .lookup
                .field(&host, std::slice::from_ref(field))
                .cloned()
            else {
                let fields = self.lookup.fields(&host).unwrap_or(&[]);
                let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
                let near = nearest(field, names.iter().copied());
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_UNKNOWN_FIELD,
                        format!("`{host}` has no field `{field}`"),
                    )
                    .at(Self::span(scope.path, &expr), "not declared")
                    .reason(format!(
                        "`{host}`'s fields are {}.{}",
                        crate::and_list(
                            &names.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>()
                        ),
                        near.map(|n| format!(" Did you mean `{n}`?"))
                            .unwrap_or_default()
                    ))
                    .next(match near {
                        Some(near) => format!("Use `{near}` instead of `{field}`"),
                        None => format!("Name a field of `{host}`"),
                    }),
                );
                return None;
            };
            let next = self.lookup.field_type(&host, std::slice::from_ref(field));
            ty = if found.optional {
                next.nullable()
            } else {
                next
            };
        }
        Some((
            Value::Param {
                index: index as u32,
                path: rest,
            },
            ty,
        ))
    }

    /// The type of a value expression, without reporting anything.
    fn value_type(&self, scope: &Scope<'_>, expr: &Expr) -> Option<Ty> {
        let (first, rest) = dotted(&unparen(expr))?;
        let head = tree::name_ref_text(&first)?;
        let mut ty = scope.params.iter().find(|(n, _)| *n == head)?.1.clone();
        for field in rest.iter().filter_map(tree::name_ref_text) {
            if let Some(found) = range_field(&ty, &field) {
                ty = found;
                continue;
            }
            let host = self.object_of(&ty)?;
            let found = self.lookup.field(&host, std::slice::from_ref(&field))?;
            let next = self.lookup.field_type(&host, &[field]);
            ty = if found.optional {
                next.nullable()
            } else {
                next
            };
        }
        Some(ty)
    }

    /// The object type a value's fields come from.
    fn object_of(&self, ty: &Ty) -> Option<String> {
        let mut current = ty.strip_nullable().clone();
        for _ in 0..64 {
            match &current {
                Ty::Named(name) => return self.lookup.fields(name).map(|_| name.clone()),
                Ty::Field(..) => current = self.lookup.parent(&current)?.strip_nullable().clone(),
                _ => return None,
            }
        }
        None
    }

    /// A constant (area 3), which a query takes wherever it takes a
    /// literal: `longest_manual`, or `pricing.vat_rate` from a library.
    /// `None` when `head` and `rest` name no constant; `Some(None)` when
    /// one was found and reported.
    fn constant(
        &mut self,
        scope: &Scope<'_>,
        expr: &Expr,
        head: &str,
        rest: &[String],
        expected: &Ty,
    ) -> Option<Option<(Value, Ty)>> {
        let (library, name) = match rest {
            [] => (None, head),
            [name] if self.lookup.library(head).is_some() => (Some(head), name.as_str()),
            _ => return None,
        };
        let ty = self.lookup.constant_type(library, name)?.clone();
        let evaluated = self.lookup.constant(library, name)?;
        // A constant whose value is wrong is reported where it is declared.
        let value = evaluated.value.as_ref()?;
        let Some(value) = query_value(value) else {
            self.value_unsupported(scope, expr);
            return Some(None);
        };
        // Compared as the value of its own type, as a parameter is: a
        // list's items are compared by the `in` that takes it.
        if !matches!(value, Value::List(_)) && !self.comparable(&ty, expected.strip_nullable()) {
            self.mismatch(scope, expr, &ty, expected);
            return Some(None);
        }
        Some(Some((value, ty)))
    }

    fn value_unsupported(&mut self, scope: &Scope<'_>, expr: &Expr) {
        let written = shown(expr.syntax());
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_VALUE_UNSUPPORTED,
                format!("`{written}` is worked out, and a query computes nothing"),
            )
            .at(Self::span(scope.path, expr), "computed")
            .reason("A query's values are its parameters and their fields, literals, constants, enum values, lists of those and `clock.now`; anything worked out from them is computed before the call.")
            .next("Work the value out in the action, and pass it as a parameter"),
        );
    }

    // --- one record ------------------------------------------------------------

    /// A query for at most one record finds it by a unique field.
    fn unique(
        &mut self,
        path: &str,
        entity: &EntityModel,
        sections: &Sections,
        filter: &[Condition],
        span: &Span,
        result: &Ty,
    ) {
        let mut pinned: BTreeSet<&str> = BTreeSet::new();
        let mut lines = filter.iter();
        for (present, _) in &sections.filters {
            let Some(condition) = lines.next() else { break };
            if *present {
                continue;
            }
            let mut equal = |c: &Condition| {
                if let Condition::Compare {
                    field,
                    op: Compare::Equal,
                    ..
                } = c
                {
                    entity
                        .fields
                        .iter()
                        .find(|f| f.name == *field)
                        .map(|f| f.name.as_str())
                } else {
                    None
                }
            };
            match condition {
                Condition::And(parts) => pinned.extend(parts.iter().filter_map(&mut equal)),
                other => pinned.extend(equal(other)),
            }
        }
        if filter.len() < sections.filters.len() {
            // A line was wrong, and has been reported.
            return;
        }
        let unique = pinned.contains("id")
            || entity
                .fields
                .iter()
                .any(|f| f.unique && pinned.contains(f.name.as_str()))
            || entity
                .unique
                .iter()
                .any(|rule| rule.fields.iter().all(|f| pinned.contains(f.as_str())));
        if unique {
            return;
        }
        let at = sections
            .filters
            .first()
            .and_then(|(_, m)| m.member_key())
            .map_or_else(|| span.clone(), |k| Self::span(path, &k));
        let mut keys: Vec<String> = vec![format!("`{}.id`", entity.name)];
        keys.extend(
            entity
                .fields
                .iter()
                .filter(|f| f.unique && f.name != "id")
                .map(|f| format!("`{}.{}`", entity.name, f.name)),
        );
        keys.extend(
            entity
                .unique
                .iter()
                .map(|rule| format!("`{}`'s fields", rule.name)),
        );
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_NOT_UNIQUE,
                format!("A query returning `{result}` could match several records"),
            )
            .at(at, "not a unique field")
            .reason(format!(
                "`{result}` is at most one record, so a `where:` line must test a unique field with `==`: {}.",
                crate::and_list(&keys)
            ))
            .next("Test a unique field with `==`, or return `List<T>`"),
        );
    }

    // --- order, limit and pages ------------------------------------------------

    /// `order: [Entity.field asc, ...]`, with `id` added as the tie-break
    /// when the last field could tie (area 6, question 3).
    fn order(
        &mut self,
        path: &str,
        entity: &EntityModel,
        member: &ast::Member,
        page: bool,
    ) -> Vec<OrderField> {
        let Some(value) = member.value() else {
            return Vec::new();
        };
        let Expr::ListExpr(list) = &value else {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_ORDER_FIELD,
                    "`order:` is a list of fields".to_string(),
                )
                .at(Self::span(path, &value), "not a list")
                .reason("`order:` lists the fields to sort by in brackets, each with `asc` or `desc`: `order: [<Entity>.<field> desc]`.")
                .next("Write `order: [<Entity>.<field> asc]`"),
            );
            return Vec::new();
        };
        let mut out: Vec<OrderField> = Vec::new();
        let mut spans: Vec<Span> = Vec::new();
        for item in list.exprs() {
            let item_span = Self::span(path, &item);
            let Expr::OrderedExpr(ordered) = &item else {
                let written = shown(item.syntax());
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_ORDER_DIRECTION,
                        format!("`{written}` says no direction"),
                    )
                    .at(item_span.clone(), "`asc` or `desc`?")
                    .reason("Each field of `order:` says which way it sorts: `asc` from the smallest, `desc` from the largest.")
                    .next(format!("Write `{written} asc` or `{written} desc`"))
                    .edit(Span::new(path, item_span.end, item_span.end), " asc"),
                );
                continue;
            };
            let Some(inner) = ordered.expr() else {
                continue;
            };
            let descending = ordered.direction().is_some_and(|d| d.text() == "desc");
            let Some(field) = self.field_ref(path, entity, &inner) else {
                continue;
            };
            let written = format!("{}.{}", entity.name, field.name);
            if let Some(at) = out.iter().position(|o| o.field == field.name) {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_ORDER_FIELD,
                        format!("`{written}` is in `order:` twice"),
                    )
                    .at(item_span.clone(), "again")
                    .also(spans[at].clone(), "first here")
                    .reason("Records equal on a field are sorted by the fields after it, so a field listed again never changes the order.")
                    .next(format!("Remove the second `{written}`")),
                );
                continue;
            }
            let ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
            let sortable = field.sql_type.as_deref() != Some("jsonb")
                && matches!(
                    self.lookup.representation(&ty),
                    Some(
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
                );
            if !sortable {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_ORDER_FIELD,
                        format!("Records can't be sorted by `{written}`"),
                    )
                    .at(item_span.clone(), format!("`{}`", field_type_name(&field.ty)))
                    .reason(format!(
                        "A query sorts by numbers, times, dates, durations, quantities, percentages, text and identities, which the database and Jadpo order the same way; `{written}` is `{}`.",
                        field.ty
                    ))
                    .next("Sort by another field"),
                );
                continue;
            }
            if page && field.nullable {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_ORDER_FIELD,
                        format!("A page can't be ordered by `{written}`, which may be `none`"),
                    )
                    .at(item_span.clone(), "may be `none`")
                    .reason("The next page starts after the last record's values of the order fields, and a record without a value has no place to start after.")
                    .next("Order the page by a required field"),
                );
                continue;
            }
            spans.push(item_span);
            out.push(OrderField {
                field: field.name.clone(),
                descending,
                added: false,
            });
        }
        if out.is_empty() && list.exprs().next().is_none() {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_ORDER_FIELD,
                    "`order:` lists no fields".to_string(),
                )
                .at(Self::span(path, &value), "empty")
                .reason("`order:` names at least one field to sort by; the compiler adds `id` when records could tie.")
                .next("Write `order: [<Entity>.<field> asc]`"),
            );
        }
        tie_break(entity, &mut out);
        out
    }

    /// `limit:` or a page's `size:`: a whole number from 1, or a value
    /// whose type has a `max:`.
    fn bound(
        &mut self,
        scope: &Scope<'_>,
        value: Option<Expr>,
        member: &ast::Member,
        what: &str,
    ) -> Option<Bound> {
        let Some(value) = value else {
            let at = member.member_key().map_or_else(
                || Span::new(scope.path, 0, 0),
                |k| Self::span(scope.path, &k),
            );
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_LIMIT_UNBOUNDED,
                    format!("`{what}:` has no value"),
                )
                .at(at, "no value")
                .reason("Every list is bounded: a number, or a value whose type has a `max:`.")
                .next(format!("Give `{what}:` a number")),
            );
            return None;
        };
        let span = Self::span(scope.path, &value);
        match Const::from_expr(&value) {
            Const::Number {
                value: n,
                decimal: false,
            } => {
                if n.compare(jadpo_std::Num::whole(1)).is_lt() {
                    self.out.push(
                        Diagnostic::new(
                            &rule::QUERY_LIMIT_RANGE,
                            format!("`{what}: {n}` would read nothing"),
                        )
                        .at(span, "below 1")
                        .reason(format!(
                            "`{what}:` is how many records come back at most, so it is at least 1."
                        ))
                        .next(format!("Give `{what}:` a number from 1")),
                    );
                    return None;
                }
                let whole = u64::try_from(n.mantissa).unwrap_or(u64::MAX);
                return Some(Bound::Literal(whole));
            }
            Const::Number { .. }
            | Const::Str(_)
            | Const::Bool(_)
            | Const::None
            | Const::List(_) => {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_MISMATCH,
                        format!(
                            "`{}` can't be used where a whole number is required",
                            shown(value.syntax())
                        ),
                    )
                    .at(span, "not a whole number")
                    .reason(format!(
                        "`{what}:` is how many records come back at most: a whole number."
                    ))
                    .next(format!("Give `{what}:` a whole number")),
                );
                return None;
            }
            _ => {}
        }
        let (bound, ty) = self.value(scope, &value, &Ty::Prim(Prim::Int))?;
        if self.lookup.is_nullable(&ty) {
            self.optional_in_where(scope, &value);
            return None;
        }
        if self.lookup.representation(&ty) != Some(Prim::Int) {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!(
                        "`{}` can't be used where a whole number is required",
                        shown(value.syntax())
                    ),
                )
                .at(span, format!("a `{ty}`"))
                .reason(format!(
                    "`{what}:` is how many records come back at most: a whole number."
                ))
                .next(format!("Give `{what}:` a whole number")),
            );
            return None;
        }
        let max = self
            .lookup
            .rule_chain(&ty)
            .iter()
            .find_map(|(_, rules)| rules.max);
        if max.is_none() {
            let written = shown(value.syntax());
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_LIMIT_UNBOUNDED,
                    format!("`{written}` has no maximum, so the {what} has no bound"),
                )
                .at(span, format!("`{ty}` has no `max:`"))
                .reason(format!(
                    "Every list is bounded: `{what}:` is a number, or a value whose type has a `max:`, such as the built-in `PageSize` (1 to 100)."
                ))
                .next("Type the value as `PageSize`, or as an `Int` type of your own with a `max:`"),
            );
            return None;
        }
        // The least it can be must still read something: PostgreSQL
        // refuses a negative `LIMIT`, and 0 reads nothing.
        let min = self
            .lookup
            .rule_chain(&ty)
            .iter()
            .find_map(|(_, rules)| rules.min);
        if min.is_none_or(|min| min.compare(jadpo_std::Num::whole(1)).is_lt()) {
            let written = shown(value.syntax());
            let least = min.map_or_else(|| "no `min:`".to_string(), |n| format!("`min: {n}`"));
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_LIMIT_RANGE,
                    format!("`{written}` can be below 1, so the {what} could read nothing"),
                )
                .at(span, format!("`{ty}` has {least}"))
                .reason(format!(
                    "`{what}:` is how many records come back at most, so it is at least 1, and a value's type must say so with `min: 1` or more, as the built-in `PageSize` (1 to 100) does."
                ))
                .next(format!("Give `{ty}` `min: 1`, or type the value as `PageSize`")),
            );
            return None;
        }
        Some(Bound::Value(bound))
    }

    /// `page: { size: ... after: ... }`.
    fn page(&mut self, scope: &Scope<'_>, member: &ast::Member) -> (Option<Bound>, Option<Value>) {
        let key_span = member.member_key().map_or_else(
            || Span::new(scope.path, 0, 0),
            |k| Self::span(scope.path, &k),
        );
        let Some(block) = member.block() else {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_PAGE_SETTING,
                    "`page:` is a block".to_string(),
                )
                .at(key_span, "not a block")
                .reason("`page:` gives the page's `size:` and the cursor to continue `after:` in a block.")
                .next("Write `page: { size: ... after: ... }`"),
            );
            return (None, None);
        };
        let mut size = None;
        let mut after = None;
        let mut seen = BTreeSet::new();
        for (name, setting) in tree::members(Some(block.clone())) {
            let Some(word) = tree::name_text(&name) else {
                continue;
            };
            let span = Self::span(scope.path, &name);
            seen.insert(word.clone());
            match word.as_str() {
                "size" => size = self.bound(scope, setting.value(), &setting, "size"),
                "after" => {
                    let Some(value) = setting.value() else {
                        continue;
                    };
                    let wanted = Ty::Prim(Prim::PageCursor).nullable();
                    let Some((value_ref, ty)) = self.value(scope, &value, &wanted) else {
                        continue;
                    };
                    if self.lookup.representation(&ty) != Some(Prim::Text)
                        || !matches!(ty.strip_nullable(), Ty::Prim(Prim::PageCursor))
                            && self.lookup.base_prim(&ty) != Some(Prim::PageCursor)
                    {
                        self.out.push(
                            Diagnostic::new(
                                &rule::QUERY_PAGE_SETTING,
                                format!("`after:` takes a `PageCursor`, and `{}` is `{ty}`", shown(value.syntax())),
                            )
                            .at(Self::span(scope.path, &value), format!("a `{ty}`"))
                            .reason("`after:` is the cursor a previous page gave in its `next`, which a client sends back: a `PageCursor`, usually an optional field of the request's `Input`.")
                            .next("Pass a `PageCursor` here"),
                        );
                        continue;
                    }
                    after = Some(value_ref);
                }
                _ => {
                    let near = nearest(&word, ["size", "after"]);
                    let problem = Diagnostic::new(
                        &rule::QUERY_PAGE_SETTING,
                        format!("`{word}` isn't a page setting"),
                    )
                    .at(span.clone(), "not a setting")
                    .reason(format!(
                        "A page has a `size:` and an `after:` cursor; there is no offset, because a large offset is slow and skips or repeats records when rows are added.{}",
                        near.map(|n| format!(" Did you mean `{n}`?")).unwrap_or_default()
                    ));
                    self.out.push(match near {
                        Some(near) => problem.next(format!("Use `{near}:`")).edit(span, near),
                        None => problem.next(format!("Remove `{word}:`")),
                    });
                }
            }
        }
        for wanted in ["size", "after"] {
            if !seen.contains(wanted) {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_PAGE_SETTING,
                        format!("`page:` has no `{wanted}:`"),
                    )
                    .at(key_span.clone(), format!("no `{wanted}:`"))
                    .reason(if wanted == "size" {
                        "A page's `size:` is how many records it holds: a number, or a value whose type has a `max:`, such as `PageSize`."
                    } else {
                        "A page continues `after:` the cursor the previous page gave in its `next`; on the first call that cursor is absent."
                    })
                    .next(format!("Add `{wanted}: ...` to the block")),
                );
            }
        }
        (size, after)
    }

    // --- includes --------------------------------------------------------------

    /// The entries of an `include:` block, at nesting `level` (1 for the
    /// query's own).
    fn includes(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        member: &ast::Member,
        level: usize,
        query: &Symbol,
    ) -> Vec<Include> {
        let key_span = member.member_key().map_or_else(
            || Span::new(scope.path, 0, 0),
            |k| Self::span(scope.path, &k),
        );
        if level > INCLUDE_LEVELS {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_INCLUDE_DEPTH,
                    "Includes nest at most two levels".to_string(),
                )
                .at(key_span, "a third level")
                .reason("Each include is one more database query, and a query's includes nest up to two levels so its cost stays easy to see; a deeper shape is a second query called from the action.")
                .next("Read the deeper records with a query of their own"),
            );
            return Vec::new();
        }
        let Some(block) = member.block() else {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_INCLUDE_UNKNOWN,
                    "`include:` is a block of named includes".to_string(),
                )
                .at(key_span, "not a block")
                .reason("Each include puts related records under a field of the result: `include: { <field>: <Entity>.<reference> }`.")
                .next("Write `include: { <field>: <Entity>.<reference> }`"),
            );
            return Vec::new();
        };
        let mut out: Vec<Include> = Vec::new();
        let mut names: BTreeMap<String, Span> = BTreeMap::new();
        for item in block.member_items() {
            let MemberItem::Member(entry) = item else {
                continue;
            };
            let Some(key) = entry.member_key() else {
                continue;
            };
            let Some(name) = key.names().next().and_then(|n| tree::name_text(&n)) else {
                continue;
            };
            let name_span = Self::span(scope.path, &key);
            if let Some(earlier) = names.get(&name) {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_REPEATED_SECTION,
                        format!("`{name}` is included twice"),
                    )
                    .at(name_span, "again")
                    .also(earlier.clone(), "first here")
                    .reason(
                        "Each include fills one field of the result, so a field is included once.",
                    )
                    .next(format!("Remove one `{name}`")),
                );
                continue;
            }
            names.insert(name.clone(), name_span.clone());
            if let Some(include) =
                self.include(scope, entity, &name, &name_span, &entry, level, query)
            {
                out.push(include);
            }
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn include(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        name: &str,
        name_span: &Span,
        entry: &ast::Member,
        level: usize,
        query: &Symbol,
    ) -> Option<Include> {
        let sections = match entry.block() {
            Some(block) => {
                self.sections(scope.path, Some(block), &INCLUDE_SETTINGS, "include", name)
            }
            None => Sections::default(),
        };
        let via_expr = match (entry.value(), sections.first.get("via")) {
            (Some(value), _) => value,
            (None, Some(via)) => via.value()?,
            (None, None) => {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_SECTION_REQUIRED,
                        format!("The include `{name}` doesn't say which reference it follows"),
                    )
                    .at(name_span.clone(), "no `via:`")
                    .reason("An include follows a reference: one of this entity's own gives the record it points at, and another entity's reference to this one gives the records that point here.")
                    .next("Add `via: <Entity>.<reference>` to the block"),
                );
                return None;
            }
        };
        let via_span = Self::span(scope.path, &via_expr);
        let (qualifier, field) = match dotted(&via_expr) {
            Some((first, rest)) if rest.len() == 1 => (
                tree::name_ref_text(&first).unwrap_or_default(),
                tree::name_ref_text(&rest[0]).unwrap_or_default(),
            ),
            _ => (String::new(), String::new()),
        };
        let entities = self.entities;
        let mut found: Option<(IncludeKind, &EntityModel, &FieldModel)> = None;
        if qualifier == entity.name {
            if let Some(f) = entity.fields.iter().find(|f| f.name == field)
                && let Some(reference) = &f.reference
                && let Some(target) = entities.get(&reference.target)
            {
                found = Some((IncludeKind::Parent, target, f));
            }
        } else if let Some(other) = entities.get(&qualifier)
            && let Some(f) = other.fields.iter().find(|f| f.name == field)
            && f.reference
                .as_ref()
                .is_some_and(|r| r.target == entity.name)
        {
            let kind = if f.unique {
                IncludeKind::Child
            } else {
                IncludeKind::Children
            };
            found = Some((kind, other, f));
        }
        let Some((kind, target, via)) = found else {
            // A reference whose type names nothing is resolution's error.
            if entities
                .get(&qualifier)
                .and_then(|e| e.fields.iter().find(|f| f.name == field))
                .is_some_and(|f| f.ty.has_error())
            {
                return None;
            }
            let written = shown(via_expr.syntax());
            let mut options: Vec<String> = entity
                .fields
                .iter()
                .filter(|f| f.reference.is_some())
                .map(|f| format!("`{}.{}`", entity.name, f.name))
                .collect();
            for other in &entities.all().entities {
                for f in &other.fields {
                    if f.reference
                        .as_ref()
                        .is_some_and(|r| r.target == entity.name)
                    {
                        options.push(format!("`{}.{}`", other.name, f.name));
                    }
                }
            }
            options.sort();
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_INCLUDE_UNKNOWN,
                    format!("`{written}` isn't a reference to or from `{}`", entity.name),
                )
                .at(via_span, "not a reference")
                .reason(format!(
                    "An include follows a reference: one of `{e}`'s own gives the record it points at, and another entity's reference to `{e}` gives the records that point here.{}",
                    if options.is_empty() {
                        format!(" No reference leads to or from `{e}`.", e = entity.name)
                    } else {
                        format!(" Those here are {}.", crate::and_list(&options))
                    },
                    e = entity.name
                ))
                .next(if options.is_empty() {
                    "Add a reference field, or read the records with a query of their own".to_string()
                } else {
                    "Name one of those references".to_string()
                }),
            );
            return None;
        };
        // An include stays inside the domain.
        let theirs = self
            .lookup
            .declared(&target.name)
            .iter()
            .find(|s| s.kind == ItemKind::Entity)
            .map(|s| s.ownership.clone());
        if let (Some(Ownership::Domain(theirs)), Ownership::Domain(ours)) =
            (&theirs, &query.ownership)
            && theirs != ours
        {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_OTHER_DOMAIN,
                    format!("The include `{name}` reads `{}`, an entity of domain `{theirs}`", target.name),
                )
                .at(via_span, format!("domain `{theirs}`"))
                .reason(format!(
                    "Includes follow references inside the domain; another domain's data comes from its public queries, so `{theirs}` decides what it shares."
                ))
                .next(format!("Call a public query of `{theirs}` for these records")),
            );
            return None;
        }
        // What the block may hold.
        let allowed: &[&str] = match kind {
            IncludeKind::Parent => &["via", "include"],
            IncludeKind::Child => &["via", "where", "where_present", "include"],
            IncludeKind::Children => &INCLUDE_SETTINGS,
        };
        for (section, member) in &sections.first {
            if !allowed.contains(&section.as_str()) {
                self.include_unexpected(scope.path, member, section, kind, target);
            }
        }
        if kind == IncludeKind::Parent {
            for (_, member) in &sections.filters {
                self.include_unexpected(
                    scope.path,
                    member,
                    if member
                        .member_key()
                        .is_some_and(|k| tree::written(k.syntax()) == "where_present")
                    {
                        "where_present"
                    } else {
                        "where"
                    },
                    kind,
                    target,
                );
            }
        }
        let mut filter = Vec::new();
        if kind != IncludeKind::Parent {
            for (present, member) in &sections.filters {
                if let Some(condition) = self.filter_line(scope, target, member, *present) {
                    filter.push(condition);
                }
            }
        }
        let mut order = Vec::new();
        let mut limit = None;
        if kind == IncludeKind::Children {
            match sections.first.get("order") {
                Some(member) => order = self.order(scope.path, target, member, false),
                None => self.include_needs(name, name_span, target, "order"),
            }
            match sections.first.get("limit") {
                Some(member) => limit = self.bound(scope, member.value(), member, "limit"),
                None => self.include_needs(name, name_span, target, "limit"),
            }
        }
        let includes = match sections.first.get("include") {
            Some(member) => self.includes(scope, target, member, level + 1, query),
            None => Vec::new(),
        };
        let written = sections
            .first
            .get("include")
            .and_then(|m| m.block())
            .map(|b| {
                tree::members(Some(b))
                    .iter()
                    .filter_map(|(n, _)| tree::name_text(n))
                    .collect()
            })
            .unwrap_or_default();
        Some(Include {
            written,
            name: name.to_string(),
            kind,
            entity: target.name.clone(),
            via: via.name.clone(),
            optional: kind == IncludeKind::Parent && via.nullable,
            // Filled in when the result type's field is read.
            element: Element {
                ty: String::new(),
                fields: Vec::new(),
            },
            filter,
            order,
            limit,
            includes,
        })
    }

    fn include_needs(&mut self, name: &str, span: &Span, target: &EntityModel, section: &str) {
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_SECTION_REQUIRED,
                format!("The include `{name}` has no `{section}:`"),
            )
            .at(span.clone(), format!("no `{section}:`"))
            .reason(format!(
                "`{name}` is a list of `{}` records, which comes in a fixed order and is bounded, as every list is: its block gives `order:` and `limit:`.",
                target.name
            ))
            .next(format!(
                "Write `{name}: {{ via: ... order: [...] limit: ... }}`"
            )),
        );
    }

    fn include_unexpected(
        &mut self,
        path: &str,
        member: &ast::Member,
        section: &str,
        kind: IncludeKind,
        target: &EntityModel,
    ) {
        let Some(key) = member.member_key() else {
            return;
        };
        let reason = match kind {
            IncludeKind::Parent => format!(
                "This include is the one `{}` record a reference points at, which is there whenever the reference is, so it takes no conditions, order or limit.",
                target.name
            ),
            _ => format!(
                "This include is at most one `{}` record, found through a unique reference, so it has nothing to order or limit.",
                target.name
            ),
        };
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_SECTION_UNEXPECTED,
                format!("This include takes no `{section}:`"),
            )
            .at(Self::span(path, &key), "not for this include")
            .reason(reason)
            .next(format!("Remove `{section}:`"))
            .edit(Self::span(path, member), ""),
        );
    }

    // --- the result's fields -----------------------------------------------------

    /// The fields of the records a query or include returns: each a field
    /// of `entity` or one of `includes`, in the type's declared order.
    fn element(
        &mut self,
        path: &str,
        ty: &str,
        entity: &EntityModel,
        includes: &[Include],
        include_member: Option<&ast::Member>,
        span: &Span,
    ) -> Option<(Element, Vec<Include>)> {
        let written: Vec<String> = include_member
            .and_then(|m| m.block())
            .map(|b| {
                tree::members(Some(b))
                    .iter()
                    .filter_map(|(n, _)| tree::name_text(n))
                    .collect()
            })
            .unwrap_or_default();
        self.element_with(path, ty, entity, includes, &written, include_member, span)
    }

    #[allow(clippy::too_many_arguments)]
    fn element_with(
        &mut self,
        path: &str,
        ty: &str,
        entity: &EntityModel,
        includes: &[Include],
        written: &[String],
        include_member: Option<&ast::Member>,
        span: &Span,
    ) -> Option<(Element, Vec<Include>)> {
        let include_span = |name: &str| -> Span {
            include_member
                .and_then(|m| m.block())
                .into_iter()
                .flat_map(|b| tree::members(Some(b)))
                .find(|(n, _)| tree::name_text(n).as_deref() == Some(name))
                .map_or_else(|| span.clone(), |(n, _)| Self::span(path, &n))
        };
        if ty == entity.name {
            if let Some(first) = includes.first() {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_INCLUDE_RESULT,
                        format!("`{ty}` has no field `{}` to hold the include", first.name),
                    )
                    .at(include_span(&first.name), "nowhere to put it")
                    .reason(format!(
                        "The result is the entity `{ty}` itself, which holds only its own fields; included records go under fields of a result type made for them."
                    ))
                    .next(format!(
                        "Return a type with `{}` and the fields you need, such as `type {ty}View = Output {{ pick: [...] {}: ... }}`",
                        first.name, first.name
                    )),
                );
                return None;
            }
            let fields = entity
                .fields
                .iter()
                .map(|f| (f.name.clone(), Source::Column(f.name.clone())))
                .collect();
            self.structured(
                span,
                ty,
                entity,
                entity.fields.iter().map(|f| f.name.as_str()),
            );
            return Some((
                Element {
                    ty: ty.to_string(),
                    fields,
                },
                Vec::new(),
            ));
        }
        let declared = self
            .lookup
            .fields(ty)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let mut ok = true;
        let mut fields = Vec::new();
        let mut filled: Vec<Include> = includes.to_vec();
        for include in includes {
            if !declared.iter().any(|f| f.name == include.name) {
                ok = false;
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_INCLUDE_RESULT,
                        format!("`{ty}` has no field `{}` to hold the include", include.name),
                    )
                    .at(include_span(&include.name), "nowhere to put it")
                    .reason(format!(
                        "Each include fills the result's field of the same name, and `{ty}` declares none called `{}`.",
                        include.name
                    ))
                    .next(format!(
                        "Add `{}: ...` to `{ty}`, or rename the include",
                        include.name
                    )),
                );
            }
        }
        let mut columns = Vec::new();
        for field in &declared {
            if let Some(at) = includes.iter().position(|i| i.name == field.name) {
                let include = &includes[at];
                match self.include_field(path, ty, field, include, &include_span(&include.name)) {
                    Some(filled_include) => filled[at] = filled_include,
                    None => ok = false,
                }
                fields.push((field.name.clone(), Source::Include(at)));
                continue;
            }
            let Some(column) = entity.fields.iter().find(|f| f.name == field.name) else {
                ok = false;
                // An include written for it that was wrong has been
                // reported.
                if written.contains(&field.name) {
                    continue;
                }
                let names: Vec<String> = entity
                    .fields
                    .iter()
                    .map(|f| format!("`{}`", f.name))
                    .collect();
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_RESULT_FIELD,
                        format!("`{ty}.{}` isn't a field of `{}`, or an include", field.name, entity.name),
                    )
                    .at(span.clone(), format!("`{}` can't be filled", field.name))
                    .reason(format!(
                        "A query fills its result's fields from `{}`'s fields of the same name ({}) and from its includes; nothing gives `{}`.",
                        entity.name,
                        crate::and_list(&names),
                        field.name
                    ))
                    .next(format!(
                        "Remove `{}` from `{ty}`, or include records under it",
                        field.name
                    )),
                );
                continue;
            };
            let from = Ty::Field(entity.name.clone(), vec![column.name.clone()]);
            let from = if column.nullable {
                from.nullable()
            } else {
                from
            };
            if !field.declared.has_error() && !self.lookup.assignable(&from, &field.declared) {
                ok = false;
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_RESULT_FIELD,
                        format!("`{ty}.{}` is `{}`, which `{}.{}` can't fill", field.name, field.declared, entity.name, column.name),
                    )
                    .at(span.clone(), format!("`{}` doesn't fit", field.name))
                    .reason(format!(
                        "`{}.{}` is `{}`, and a stored value goes into the result as it is, so the result's field takes that type or one it widens to.",
                        entity.name,
                        column.name,
                        field_type_name(&column.ty)
                    ))
                    .next(format!(
                        "Declare `{}` as `{}.{}`, or pick it: `pick: [{}.{}]`",
                        field.name, entity.name, column.name, entity.name, column.name
                    )),
                );
                continue;
            }
            columns.push(column.name.as_str());
            fields.push((field.name.clone(), Source::Column(column.name.clone())));
        }
        self.structured(span, ty, entity, columns.into_iter());
        ok.then(|| {
            (
                Element {
                    ty: ty.to_string(),
                    fields,
                },
                filled,
            )
        })
    }

    /// A stored field kept as `jsonb` can't be read yet: its codec is the
    /// API's JSON, which comes with area 7.
    fn structured<'f>(
        &mut self,
        span: &Span,
        ty: &str,
        entity: &EntityModel,
        columns: impl Iterator<Item = &'f str>,
    ) {
        let structured: Vec<String> = columns
            .filter(|c| {
                entity
                    .fields
                    .iter()
                    .any(|f| f.name == *c && f.sql_type.as_deref() == Some("jsonb"))
            })
            .map(|c| format!("`{}.{c}`", entity.name))
            .collect();
        if structured.is_empty() {
            return;
        }
        self.out.push(
            Diagnostic::new(
                &rule::TOOLCHAIN_NOT_BUILT_YET,
                format!("Reading {} isn't built yet", crate::and_list(&structured)),
            )
            .at(span.clone(), format!("`{ty}` reads a structured field"))
            .reason("A structured field (an object, a list, a set, a map or an enum with data) is stored whole as `jsonb` in the API's JSON (area 5), and that codec comes with routes (area 7).")
            .next(format!("Leave the structured fields out of `{ty}` for now")),
        );
    }

    /// The result field an include fills: the shape its kind gives, of the
    /// included entity or a type derived from it.
    fn include_field(
        &mut self,
        path: &str,
        host: &str,
        field: &crate::types::FieldDef,
        include: &Include,
        span: &Span,
    ) -> Option<Include> {
        let target = self.entities.get(&include.entity)?;
        let declared = &field.declared;
        if declared.has_error() {
            return None;
        }
        let inner = match (include.kind, declared) {
            (IncludeKind::Children, Ty::List(inner)) => Some(inner.as_ref()),
            (IncludeKind::Child, Ty::Nullable(inner)) => Some(inner.as_ref()),
            (IncludeKind::Parent, Ty::Nullable(inner)) if include.optional => Some(inner.as_ref()),
            (IncludeKind::Parent, other)
                if !include.optional && !matches!(other, Ty::Nullable(_) | Ty::List(_)) =>
            {
                Some(other)
            }
            _ => None,
        };
        let element = match inner {
            Some(Ty::Named(name))
                if *name == target.name
                    || matches!(
                        self.lookup.def(name),
                        Some(Def::Fields {
                            base: FieldsBase::Object | FieldsBase::Output,
                            ..
                        })
                    ) =>
            {
                Some(name.clone())
            }
            _ => None,
        };
        let Some(element) = element else {
            let wanted = match include.kind {
                IncludeKind::Children => format!("List<{}>", target.name),
                IncludeKind::Child => format!("{}?", target.name),
                IncludeKind::Parent if include.optional => format!("{}?", target.name),
                IncludeKind::Parent => target.name.clone(),
            };
            let why = match include.kind {
                IncludeKind::Children => format!(
                    "`{}` follows `{}.{}`, which many `{}` records can share, so it is a list.",
                    include.name, target.name, include.via, target.name
                ),
                IncludeKind::Child => format!(
                    "`{}` follows `{}.{}`, which is unique, so at most one `{}` record points here and it may be absent.",
                    include.name, target.name, include.via, target.name
                ),
                IncludeKind::Parent if include.optional => format!(
                    "`{}` follows a reference that may be `none`, so the record it points at may be absent.",
                    include.name
                ),
                IncludeKind::Parent => format!(
                    "`{}` follows a required reference, so the one record it points at is always there.",
                    include.name
                ),
            };
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_INCLUDE_RESULT,
                    format!("`{host}.{}` is `{declared}`, which the include `{}` can't fill", field.name, include.name),
                )
                .at(span.clone(), format!("needs `{wanted}`"))
                .reason(format!(
                    "{why} Its field is `{wanted}`, where `{}` may also be an object type derived from it.",
                    target.name
                ))
                .next(format!("Declare `{}: {wanted}` in `{host}`", field.name)),
            );
            return None;
        };
        let (element, includes) = self.element_with(
            path,
            &element,
            target,
            &include.includes,
            &include.written,
            None,
            span,
        )?;
        Some(Include {
            element,
            includes,
            ..include.clone()
        })
    }

    // --- missing and fails -------------------------------------------------------

    /// `missing: Failure` or `missing: Failure { field: value }`.
    fn missing(&mut self, scope: &Scope<'_>, member: &ast::Member) -> Option<Missing> {
        let value = member.value()?;
        let (head, record) = match &value {
            Expr::RecordExpr(record) => (record.expr()?, Some(record.clone())),
            other => (other.clone(), None),
        };
        let span = Self::span(scope.path, &head);
        let Expr::NameRef(name_ref) = &head else {
            self.not_failure(span, &tree::written(head.syntax()));
            return None;
        };
        let failure = tree::name_ref_text(name_ref)?;
        let Some(info) = self.lookup.failure_info(&failure).cloned() else {
            self.not_failure(span, &failure);
            return None;
        };
        if let Some(kind) = &info.kind
            && kind != "not_found"
        {
            self.out.push(
                Diagnostic::new(
                    &rule::FAILURE_MAPPING_KIND,
                    format!("`missing:` needs a `not_found` failure, and `{failure}` is `{kind}`"),
                )
                .at(span, format!("kind `{kind}`"))
                .reason("Each mapping's condition fixes the kind of failure it becomes: a read that finds nothing is `not_found`, so the client gets the status that means.")
                .next(format!(
                    "Name a `not_found` failure here, or make `{failure}`'s kind `FailureKind.not_found`"
                )),
            );
            return None;
        }
        let declared = self
            .lookup
            .fields(&failure)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let mut fields = Vec::new();
        let mut given = BTreeSet::new();
        let mut ok = true;
        for (name, field_member) in tree::members(record.as_ref().and_then(|r| r.member_list())) {
            let Some(field) = tree::name_text(&name) else {
                continue;
            };
            let field_span = Self::span(scope.path, &name);
            let Some(found) = declared.iter().find(|f| f.name == field) else {
                ok = false;
                let names: Vec<String> = declared.iter().map(|f| format!("`{}`", f.name)).collect();
                self.out.push(
                    Diagnostic::new(
                        &rule::FAILURE_UNKNOWN_CONTEXT_FIELD,
                        format!("`{failure}` has no field `{field}`"),
                    )
                    .at(field_span, "not declared")
                    .reason(if names.is_empty() {
                        format!("`{failure}` has no fields. A failure carries only the fields its `client` and `internal` blocks declare.")
                    } else {
                        format!("`{failure}`'s fields are {}. A failure carries only the fields its `client` and `internal` blocks declare.", crate::and_list(&names))
                    })
                    .next(format!("Remove `{field}`")),
                );
                continue;
            };
            given.insert(field.clone());
            let Some(value) = field_member.value() else {
                continue;
            };
            let expected = self
                .lookup
                .field_type(&failure, std::slice::from_ref(&field));
            let Some((value_ref, ty)) = self.value(scope, &value, &expected) else {
                ok = false;
                continue;
            };
            if !matches!(value_ref, Value::Literal(_))
                && !self.lookup.assignable(&ty, &found.declared)
            {
                ok = false;
                let written = shown(value.syntax());
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_MISMATCH,
                        format!(
                            "`{written}` can't be used where `{}` is required",
                            found.declared
                        ),
                    )
                    .at(Self::span(scope.path, &value), format!("a `{ty}`"))
                    .reason(format!(
                        "`{failure}.{field}` is `{}`, and `{written}` is `{ty}`.",
                        found.declared
                    ))
                    .next(format!("Give `{field}` a `{}` value", found.declared)),
                );
                continue;
            }
            fields.push((field, value_ref));
        }
        let missing: Vec<String> = declared
            .iter()
            .filter(|f| !given.contains(&f.name))
            .map(|f| format!("`{}`", f.name))
            .collect();
        if !missing.is_empty() {
            ok = false;
            let first = missing[0].trim_matches('`').to_string();
            self.out.push(
                Diagnostic::new(
                    &rule::FAILURE_MISSING_CONTEXT_FIELD,
                    format!("`{failure}` is missing {}", crate::and_list(&missing)),
                )
                .at(span, "fields missing")
                .reason("A failure is raised with every field its `client` and `internal` blocks declare, so the client and the log always get them.")
                .next(match record {
                    Some(_) => format!("Add `{first}: ...` to the block"),
                    None => format!("Give its fields in a block: `missing: {failure} {{ {first}: ... }}`"),
                }),
            );
        }
        // Fields in the failure's declared order.
        fields.sort_by_key(|(name, _)| declared.iter().position(|f| f.name == *name));
        ok.then_some(Missing { failure, fields })
    }

    fn not_failure(&mut self, span: Span, written: &str) {
        self.out.push(
            Diagnostic::new(
                &rule::SEMANTIC_UNKNOWN_NAME,
                format!("`{written}` isn't a declared failure"),
            )
            .at(span, "not a failure")
            .reason("`missing:` names the `not_found` failure a caller gets when no record matches, declared in the domain's `failures/` folder.")
            .next(format!(
                "Declare `type {written} = Failure {{ kind: FailureKind.not_found }}`, or name another failure"
            )),
        );
    }

    /// The `fails` list says exactly what can leave the query: its
    /// `missing:` failure, when it has one.
    fn fails(
        &mut self,
        path: &str,
        decl: &ast::QueryDecl,
        name: &str,
        missing: Option<&Missing>,
        member: Option<&ast::Member>,
    ) {
        let listed: Vec<(String, Span)> = decl
            .fails_clause()
            .into_iter()
            .flat_map(|clause| clause.types())
            .map(|ty| (tree::written(ty.syntax()), Self::span(path, &ty)))
            .collect();
        if let Some(missing) = missing
            && !listed
                .iter()
                .any(|(written, _)| *written == missing.failure)
        {
            let at = member
                .and_then(|m| m.value())
                .map_or_else(|| Span::new(path, 0, 0), |v| Self::span(path, &v));
            let failure = &missing.failure;
            self.out.push(
                Diagnostic::new(
                    &rule::FAILURE_UNDECLARED_PROPAGATION,
                    format!("`{failure}` can leave `{name}`, but its `fails` list doesn't say so"),
                )
                .at(at, "raised when nothing matches")
                .reason("`fails` lists exactly the failures that can leave a query, so every caller knows what it must handle.")
                .next(format!("Add `fails {failure}` before the result type")),
            );
        }
        for (written, span) in listed {
            if missing.is_some_and(|m| m.failure == written) {
                continue;
            }
            // A `missing:` that was reported keeps its failure listed.
            if missing.is_none() && member.is_some() {
                continue;
            }
            self.out.push(
                Diagnostic::new(
                    &rule::FAILURE_STALE_DECLARATION,
                    format!("`{written}` can never leave `{name}`"),
                )
                .at(span.clone(), "never raised")
                .reason("A query raises only its `missing:` failure, when it returns exactly one record, and `fails` lists exactly the failures that can leave, so callers aren't made to handle one that never happens.")
                .next(format!("Remove `{written}` from `fails`")),
            );
        }
    }
}

// --- helpers -------------------------------------------------------------------

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

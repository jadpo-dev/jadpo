//! Constants (area 3, "Constants", decided 2026-10-16): each constant's
//! value worked out while compiling, from literals, enum values, other
//! constants, constructions, objects, lists and the operators on them,
//! and checked against every rule of the types it builds. A call of any
//! kind, `config`, `clock` and a constant that depends on itself are
//! refused, since their values are only known while the app runs, or
//! never.
//!
//! The body checker types a constant's value as a `return` of its type
//! (`types::body`, `Context::Constant`); this module works out the value
//! itself, once for the whole project, so a constant read from another
//! file is worked out once and a cycle is found wherever it starts. The
//! executors run the constant's typed body, which can't fail: every
//! construction in it was checked here.
//!
//! Two constants of one type holding one value get `constant.similar_exists`,
//! as two alike library functions get `library.similar_exists` (area 1).

use std::cmp::Ordering;
use std::collections::BTreeMap;

use jadpo_db::{Db, File, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_syntax::SyntaxKind;
use jadpo_syntax::ast::{self, AstNode, Decl, Expr, MemberItem};

use crate::case::starts_upper;
use crate::lookups::{Lookup, Name, NameView, file_at};
use crate::tree::{self, dotted, node_span};
use crate::types::literal::{self, Problem};
use crate::types::{Const, Def, Num, Ty, TypeRef, TypeView};

/// A constant's value, as the compiler worked it out.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    /// A number; `decimal` when it is a `Decimal`.
    Number {
        value: Num,
        decimal: bool,
    },
    Text(String),
    Bool(bool),
    /// A duration as written: `15m`.
    Duration(String),
    None,
    /// `Enum.variant`, with the fields of the data it carries.
    Variant(String, String, Vec<(String, Value)>),
    /// An object with its fields, as written.
    Object(String, Vec<(String, Value)>),
    List(Vec<Value>),
}

impl Value {
    /// The value as Jadpo writes it: `5000`, `0.2000`, `"text"`,
    /// `LogLevel.normal`, `[1, 2]`, `VatBand { starts: "2010-01-01", rate: 0.1750 }`.
    pub fn written(&self) -> String {
        match self {
            Value::Number { value, .. } => value.to_string(),
            Value::Text(text) => format!("{text:?}"),
            Value::Bool(value) => value.to_string(),
            Value::Duration(text) => text.clone(),
            Value::None => "none".to_string(),
            Value::Variant(host, variant, fields) if fields.is_empty() => {
                format!("{}.{variant}", crate::types::shown(host))
            }
            Value::Variant(host, variant, fields) => format!(
                "{}.{variant} {}",
                crate::types::shown(host),
                written_fields(fields)
            ),
            Value::Object(host, fields) => {
                format!("{} {}", crate::types::shown(host), written_fields(fields))
            }
            Value::List(items) => {
                let items: Vec<String> = items.iter().map(Value::written).collect();
                format!("[{}]", items.join(", "))
            }
        }
    }

    /// The value as a literal is checked against a type's rules.
    fn to_const(&self) -> Const {
        match self {
            Value::Number { value, decimal } => Const::Number {
                value: *value,
                decimal: *decimal,
            },
            Value::Text(text) => Const::Str(text.clone()),
            Value::Bool(value) => Const::Bool(*value),
            Value::Duration(text) => Const::Duration(text.clone()),
            Value::None => Const::None,
            Value::Variant(host, variant, fields) if fields.is_empty() => {
                Const::Path(vec![crate::types::shown(host).to_string(), variant.clone()])
            }
            Value::List(items) => Const::List(items.iter().map(Value::to_const).collect()),
            Value::Variant(..) | Value::Object(..) => Const::Other,
        }
    }

    /// Whether two values are the same: numbers by value, so `1.5` is
    /// `1.50`.
    pub fn same(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Number { value: a, .. }, Value::Number { value: b, .. }) => {
                a.compare(*b).is_eq()
            }
            (Value::Variant(h1, v1, f1), Value::Variant(h2, v2, f2)) => {
                h1 == h2 && v1 == v2 && same_fields(f1, f2)
            }
            (Value::Object(h1, f1), Value::Object(h2, f2)) => h1 == h2 && same_fields(f1, f2),
            (Value::List(a), Value::List(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.same(y))
            }
            _ => self == other,
        }
    }
}

fn same_fields(a: &[(String, Value)], b: &[(String, Value)]) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(name, value)| b.iter().any(|(n, v)| n == name && v.same(value)))
}

fn written_fields(fields: &[(String, Value)]) -> String {
    let fields: Vec<String> = fields
        .iter()
        .map(|(name, value)| format!("{name}: {}", value.written()))
        .collect();
    format!("{{ {} }}", fields.join(", "))
}

/// One constant: its file, its declaration's position, and its position
/// in its library for a library's.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConstantId {
    pub path: String,
    pub index: u32,
    pub member: Option<u32>,
}

/// What working out one constant found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Evaluated {
    /// `name`, or `library.name`.
    pub name: String,
    /// Its declared type, resolved and written as the checker writes it.
    pub ty: String,
    /// Its value; `None` when it can't be worked out (a mistake reported
    /// here or by the body check).
    pub value: Option<Value>,
    /// Its own problems, in its own file.
    pub problems: Vec<Diagnostic>,
    /// Whether its value is refused as not fixed, which leaves the body
    /// check's knock-on problems unreported.
    pub not_fixed: bool,
}

/// Every constant in the project, worked out.
#[salsa::tracked(returns(ref))]
pub fn constant_table(db: &dyn Db, project: Project) -> BTreeMap<ConstantId, Evaluated> {
    let mut files: Vec<File> = project.files(db).clone();
    files.sort_by(|a, b| a.path(db).cmp(b.path(db)));
    let mut eval = Eval {
        db,
        project,
        done: BTreeMap::new(),
        visiting: Vec::new(),
        cycles: BTreeMap::new(),
    };
    for file in files {
        for id in constants_of(db, file) {
            eval.get(&id);
        }
    }
    let Eval {
        mut done, cycles, ..
    } = eval;
    for (id, problem) in cycles {
        if let Some(evaluated) = done.get_mut(&id) {
            evaluated.problems.push(problem);
            evaluated.value = None;
        }
    }
    done
}

/// The constants declared in `file`, in source order.
pub fn constants_of(db: &dyn Db, file: File) -> Vec<ConstantId> {
    let path = file.path(db);
    let mut out = Vec::new();
    for item in &item_tree(db, file).items {
        match item.kind {
            ItemKind::Constant => out.push(ConstantId {
                path: path.clone(),
                index: item.index,
                member: None,
            }),
            ItemKind::Library => {
                for member in &item.functions {
                    if member.kind == ItemKind::Constant {
                        out.push(ConstantId {
                            path: path.clone(),
                            index: item.index,
                            member: Some(member.index),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// What working out a constant found, if `index` and `member` are one.
pub fn evaluated<'a>(
    db: &'a dyn Db,
    project: Project,
    path: &str,
    index: u32,
    member: Option<u32>,
) -> Option<&'a Evaluated> {
    constant_table(db, project).get(&ConstantId {
        path: path.to_string(),
        index,
        member,
    })
}

/// The problems with the constants in `file`: their values, and those
/// another constant of the same type already holds.
#[salsa::tracked(returns(ref))]
pub fn constant_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let ids = constants_of(db, file);
    if ids.is_empty() {
        return Vec::new();
    }
    let table = constant_table(db, project);
    let mut out = Vec::new();
    for id in &ids {
        let Some(evaluated) = table.get(id) else {
            continue;
        };
        out.extend(evaluated.problems.iter().cloned());
        let Some(value) = &evaluated.value else {
            continue;
        };
        // The first constant of this type holding this value, in file and
        // source order, is the one the others are told to read.
        let first = table.iter().find(|(other, found)| {
            found.ty == evaluated.ty
                && found.problems.is_empty()
                && found.value.as_ref().is_some_and(|v| v.same(value))
                && *other < id
        });
        if evaluated.problems.is_empty()
            && let Some((other, found)) = first
            && let Some(source) = tree::source(db, file)
        {
            let span = tree::name_span(&source, &id.path, id.index, id.member);
            let own = evaluated.name.rsplit('.').next().unwrap_or(&evaluated.name);
            out.push(
                Diagnostic::new(
                    &rule::CONSTANT_SIMILAR_EXISTS,
                    format!("A constant with this value exists: `{}`", found.name),
                )
                .at(span, format!("same as `{}`", found.name))
                .reason(format!(
                    "`{own}` is a `{}` holding `{}`, as `{}` in `{}` is.",
                    evaluated.ty,
                    value.written(),
                    found.name,
                    other.path
                ))
                .next(format!("Read `{}` instead, and remove `{own}`", found.name)),
            );
        }
    }
    out
}

/// Why a value isn't worked out.
enum Stop {
    /// Refused here, and reported.
    NotFixed,
    /// A mistake reported elsewhere, or a value this check can't work out.
    Unknown,
}

type Worked = Result<Value, Stop>;

struct Eval<'db> {
    db: &'db dyn Db,
    project: Project,
    done: BTreeMap<ConstantId, Evaluated>,
    /// The constants being worked out, outermost first.
    visiting: Vec<ConstantId>,
    /// The error for each cycle, on the constant it was found from.
    cycles: BTreeMap<ConstantId, Diagnostic>,
}

/// One constant being worked out: where it is and what it reads through.
struct Cx<'a> {
    path: String,
    lookup: Lookup<'a>,
    problems: Vec<Diagnostic>,
    not_fixed: bool,
}

impl Cx<'_> {
    fn span(&self, node: &jadpo_syntax::SyntaxNode) -> Span {
        node_span(&self.path, node)
    }
}

impl<'db> Eval<'db> {
    /// The value of constant `id`, working it out the first time.
    fn get(&mut self, id: &ConstantId) -> Option<Value> {
        if let Some(done) = self.done.get(id) {
            return done.value.clone();
        }
        if self.visiting.contains(id) {
            return None;
        }
        self.visiting.push(id.clone());
        let evaluated = self.evaluate(id);
        self.visiting.pop();
        let value = evaluated.value.clone();
        self.done.insert(id.clone(), evaluated);
        value
    }

    fn evaluate(&mut self, id: &ConstantId) -> Evaluated {
        let db = self.db;
        let Some(file) = file_at(db, self.project, Name::new(db, id.path.clone())) else {
            return Evaluated::default();
        };
        let Some(source) = tree::source(db, file) else {
            return Evaluated::default();
        };
        let Some(decl) = tree::decl_at(&source, id.index) else {
            return Evaluated::default();
        };
        let (constant, library) = match (&decl, id.member) {
            (Decl::ConstantDecl(constant), None) => (constant.clone(), None),
            (Decl::LibraryDecl(library), Some(member)) => {
                match tree::library_members(library)
                    .into_iter()
                    .nth(member as usize)
                {
                    Some(Decl::ConstantDecl(constant)) => {
                        (constant, library.name().as_ref().and_then(tree::name_text))
                    }
                    _ => return Evaluated::default(),
                }
            }
            _ => return Evaluated::default(),
        };
        let own = constant
            .name()
            .as_ref()
            .and_then(tree::name_text)
            .unwrap_or_default();
        let name = match &library {
            Some(library) => format!("{library}.{own}"),
            None => own.clone(),
        };
        let mut cx = Cx {
            path: id.path.clone(),
            lookup: Lookup::for_file(db, self.project, file),
            problems: Vec::new(),
            not_fixed: false,
        };
        let ty = constant
            .ty()
            .map_or(Ty::Error, |ty| cx.lookup.resolve(&TypeRef::from_type(&ty)));
        let value = constant.value().and_then(|value| {
            let worked = self.expr(&mut cx, &value, library.as_deref());
            worked.ok()
        });
        Evaluated {
            name,
            ty: ty.to_string(),
            value: value.filter(|_| !ty.has_error()),
            not_fixed: cx.not_fixed,
            problems: cx.problems,
        }
    }

    /// Reads constant `target` from the constant `cx` is working out,
    /// reporting a cycle when `target` is already being worked out.
    fn read(&mut self, cx: &mut Cx<'_>, target: ConstantId, node: &Expr) -> Worked {
        if let Some(at) = self.visiting.iter().position(|id| *id == target) {
            // The loop, from the constant whose read closes it.
            let mut ring: Vec<ConstantId> = self.visiting[at..].to_vec();
            let last = ring.len() - 1;
            ring.rotate_left(last);
            let names: Vec<String> = ring
                .iter()
                .chain(ring.first())
                .map(|id| self.name_of(id))
                .collect();
            let first = names[0].clone();
            let span = cx.span(node.syntax());
            let problem = Diagnostic::new(
                &rule::CONSTANT_CYCLE,
                format!("`{first}` depends on itself"),
            )
            .at(span, "leads back to it")
            .reason(format!(
                "A constant's value is worked out from the constants it reads, and {} leads back to `{first}`, so it has no value.",
                names
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(" → ")
            ))
            .next(format!(
                "Write `{first}`'s value without reading a constant that reads it"
            ));
            // Reported once, in the file of the constant that reads back.
            let reader = self.visiting.last().cloned().unwrap_or(target.clone());
            self.cycles.entry(reader).or_insert(problem);
            return Err(Stop::Unknown);
        }
        self.get(&target).ok_or(Stop::Unknown)
    }

    /// `name`, or `library.name`, of a constant, from its declaration.
    fn name_of(&self, id: &ConstantId) -> String {
        let db = self.db;
        let found = file_at(db, self.project, Name::new(db, id.path.clone())).and_then(|file| {
            item_tree(db, file)
                .items
                .iter()
                .find(|item| item.index == id.index)
                .cloned()
        });
        match (found, id.member) {
            (Some(item), None) => item.name,
            (Some(item), Some(member)) => item
                .functions
                .iter()
                .find(|m| m.index == member)
                .map_or(item.name.clone(), |m| format!("{}.{}", item.name, m.name)),
            (None, _) => String::new(),
        }
    }

    /// Refuses `node`, which is only known while the app runs.
    fn not_fixed(&self, cx: &mut Cx<'_>, node: &Expr, what: String, next: &str) -> Worked {
        if !cx.not_fixed {
            let span = cx.span(node.syntax());
            cx.problems.push(
                Diagnostic::new(
                    &rule::CONSTANT_VALUE_NOT_FIXED,
                    format!("A constant can't {what}"),
                )
                .at(span, "only known while the app runs")
                .reason(
                    "A constant is worked out while compiling, from literals, enum values, other constants, constructions, objects, lists and operators; a call, `config` and `clock` give values only while the app runs.",
                )
                .next(next.to_string()),
            );
        }
        cx.not_fixed = true;
        Err(Stop::NotFixed)
    }

    fn expr(&mut self, cx: &mut Cx<'_>, expr: &Expr, library: Option<&str>) -> Worked {
        match expr {
            Expr::Literal(_) => literal(&Const::from_expr(expr)),
            Expr::ParenExpr(paren) => match paren.expr() {
                Some(inner) => self.expr(cx, &inner, library),
                None => Err(Stop::Unknown),
            },
            Expr::PrefixExpr(prefix) => {
                let negative = prefix.syntax().first_token().map(|t| t.kind()) == Some(SyntaxKind::MINUS);
                if negative && matches!(prefix.expr(), Some(Expr::Literal(_))) {
                    return literal(&Const::from_expr(expr));
                }
                let operand = match prefix.expr() {
                    Some(inner) => self.expr(cx, &inner, library)?,
                    None => return Err(Stop::Unknown),
                };
                match (negative, operand) {
                    (true, Value::Number { value, decimal }) => Ok(Value::Number {
                        value: Num {
                            mantissa: value.mantissa.checked_neg().ok_or(Stop::Unknown)?,
                            scale: value.scale,
                        },
                        decimal,
                    }),
                    (false, Value::Bool(value)) => Ok(Value::Bool(!value)),
                    _ => Err(Stop::Unknown),
                }
            }
            Expr::NameRef(name) => {
                let written = tree::name_ref_text(name).unwrap_or_default();
                if starts_upper(&written) {
                    return Err(Stop::Unknown);
                }
                // A library's constants read each other bare, as its
                // functions call each other.
                let target = library
                    .and_then(|library| library_constant(&cx.lookup, library, &written))
                    .or_else(|| bare_constant(&cx.lookup, &written));
                match target {
                    Some(target) => self.read(cx, target, expr),
                    None => Err(Stop::Unknown),
                }
            }
            Expr::FieldExpr(field) => self.field(cx, expr, field, library),
            Expr::CallExpr(call) => self.call(cx, expr, call, library),
            Expr::RecordExpr(record) => self.record(cx, expr, record, library),
            Expr::ListExpr(list) => {
                let mut items = Vec::new();
                let mut stop = None;
                for item in list.exprs() {
                    match self.expr(cx, &item, library) {
                        Ok(value) => items.push(value),
                        Err(why) => stop = stop.or(Some(why)),
                    }
                }
                match stop {
                    Some(why) => Err(why),
                    None => Ok(Value::List(items)),
                }
            }
            Expr::BinExpr(binary) => self.binary(cx, expr, binary, library),
            Expr::MatchExpr(_) => self.not_fixed(
                cx,
                expr,
                "choose its value with `match`".to_string(),
                "Write the value itself, or make this a function that takes what it depends on",
            ),
            Expr::AttemptExpr(_) => self.not_fixed(
                cx,
                expr,
                "use `attempt`".to_string(),
                "Write the value itself; a construction in a constant is checked while compiling and needs no `attempt`",
            ),
            _ => Err(Stop::Unknown),
        }
    }

    fn field(
        &mut self,
        cx: &mut Cx<'_>,
        expr: &Expr,
        field: &ast::FieldExpr,
        library: Option<&str>,
    ) -> Worked {
        let Some((first, rest)) = dotted(expr) else {
            let receiver = match field.expr() {
                Some(inner) => self.expr(cx, &inner, library)?,
                None => return Err(Stop::Unknown),
            };
            let name = field.name_ref().as_ref().and_then(tree::name_ref_text);
            return select(receiver, name.as_deref());
        };
        let head = tree::name_ref_text(&first).unwrap_or_default();
        let names: Vec<String> = rest.iter().filter_map(tree::name_ref_text).collect();
        let written = tree::written(expr.syntax());
        if head == "config" {
            return self.not_fixed(
                cx,
                expr,
                format!("read `{written}`"),
                &format!("Read `{written}` where the value is used: a setting is chosen per deployment, and a value is a constant or a setting, never both"),
            );
        }
        if head == "clock" {
            return self.not_fixed(
                cx,
                expr,
                format!("read `{written}`"),
                "Read the time where it is used, in an action, or pass it in as a parameter",
            );
        }
        if starts_upper(&head) {
            let canonical = TypeView::canonical(&cx.lookup, &head);
            if let [variant] = names.as_slice()
                && cx
                    .lookup
                    .variants(&head)
                    .is_some_and(|variants| variants.iter().any(|v| v.name == *variant))
            {
                return Ok(Value::Variant(canonical, variant.clone(), Vec::new()));
            }
            return Err(Stop::Unknown);
        }
        if let [member, more @ ..] = names.as_slice()
            && let Some(target) = library_constant(&cx.lookup, &head, member)
        {
            let mut value = self.read(cx, target, expr)?;
            for name in more {
                value = select(value, Some(name))?;
            }
            return Ok(value);
        }
        // A field of another constant's object.
        let mut value = match bare_constant(&cx.lookup, &head) {
            Some(target) => self.read(cx, target, expr)?,
            None => return Err(Stop::Unknown),
        };
        for name in &names {
            value = select(value, Some(name))?;
        }
        Ok(value)
    }

    fn call(
        &mut self,
        cx: &mut Cx<'_>,
        expr: &Expr,
        call: &ast::CallExpr,
        library: Option<&str>,
    ) -> Worked {
        let callee = call.expr();
        let written = callee
            .as_ref()
            .map(|callee| tree::written(callee.syntax()))
            .unwrap_or_default();
        // `Type(value)`: a construction, checked here against the type's
        // rules.
        let target = match &callee {
            Some(Expr::NameRef(name)) => {
                let name = tree::name_ref_text(name).unwrap_or_default();
                let scalar = matches!(cx.lookup.def(&name), Some(Def::Scalar { .. }))
                    || crate::types::Prim::from_name(&name).is_some();
                scalar.then(|| cx.lookup.resolve(&TypeRef::parse(&name)))
            }
            _ => None,
        };
        let Some(target) = target else {
            return self.not_fixed(
                cx,
                expr,
                format!("call `{written}`"),
                &format!(
                    "Write the value `{written}(...)` gives, or call `{written}` where the value is used"
                ),
            );
        };
        let args: Vec<Expr> = call
            .arg_list()
            .into_iter()
            .flat_map(|list| list.args())
            .filter_map(|arg| arg.expr())
            .collect();
        let [value] = args.as_slice() else {
            return Err(Stop::Unknown);
        };
        let value = self.expr(cx, value, library)?;
        if target.has_error() {
            return Err(Stop::Unknown);
        }
        match literal::check(&cx.lookup, &target, &value.to_const()) {
            Ok(()) => Ok(value),
            Err(Problem::Kind { .. }) => Err(Stop::Unknown),
            Err(Problem::Builtin(why)) => {
                let span = cx.span(expr.syntax());
                cx.problems.push(
                    Diagnostic::new(
                        &rule::TYPE_INVALID_LITERAL,
                        format!("`{}` isn't a valid `{target}`", value.written()),
                    )
                    .at(span, format!("not a valid `{target}`"))
                    .reason(format!(
                        "The value {why}; a constant is worked out and checked while compiling."
                    ))
                    .next(format!("Use a value that `{target}` allows")),
                );
                Err(Stop::Unknown)
            }
            Err(Problem::Broken(host, broken)) => {
                let span = cx.span(expr.syntax());
                cx.problems.push(
                    Diagnostic::new(
                        &rule::TYPE_INVALID_LITERAL,
                        format!("`{}` isn't a valid `{target}`", value.written()),
                    )
                    .at(span, format!("breaks `{}`", broken.rule))
                    .reason(format!(
                        "`{host}` declares `{}`, and the value {}. A constant is worked out and checked against every rule while compiling.",
                        broken.rule, broken.why
                    ))
                    .next(format!("Use a value that `{target}` allows")),
                );
                Err(Stop::Unknown)
            }
        }
    }

    fn record(
        &mut self,
        cx: &mut Cx<'_>,
        expr: &Expr,
        record: &ast::RecordExpr,
        library: Option<&str>,
    ) -> Worked {
        let Some((first, rest)) = record.expr().as_ref().and_then(dotted) else {
            return Err(Stop::Unknown);
        };
        let head = tree::name_ref_text(&first).unwrap_or_default();
        let names: Vec<String> = rest.iter().filter_map(tree::name_ref_text).collect();
        let canonical = TypeView::canonical(&cx.lookup, &head);
        let shape = match (cx.lookup.def(&head), names.as_slice()) {
            (Some(Def::Fields { base, .. }), [])
                if !matches!(base, crate::types::FieldsBase::Entity) =>
            {
                Shape::Object
            }
            (Some(Def::Enum { variants }), [variant])
                if variants.iter().any(|v| v.name == *variant) =>
            {
                Shape::Variant(variant.clone())
            }
            (Some(Def::Fields { .. }), [operation]) => {
                let written = format!("{head}.{operation}");
                return self.not_fixed(
                    cx,
                    expr,
                    format!("run `{written}`"),
                    &format!("Run `{written}` in an action, where stored data is read and written"),
                );
            }
            _ => return Err(Stop::Unknown),
        };
        let mut fields = Vec::new();
        let mut stop = None;
        for item in record
            .member_list()
            .into_iter()
            .flat_map(|list| list.member_items())
        {
            let MemberItem::Member(member) = item else {
                continue;
            };
            let Some(key) = member
                .member_key()
                .and_then(|key| key.names().next())
                .as_ref()
                .and_then(tree::name_text)
            else {
                continue;
            };
            let Some(value) = member.value() else {
                stop = stop.or(Some(Stop::Unknown));
                continue;
            };
            match self.expr(cx, &value, library) {
                Ok(value) => fields.push((key, value)),
                Err(why) => stop = stop.or(Some(why)),
            }
        }
        if let Some(why) = stop {
            return Err(why);
        }
        Ok(match shape {
            Shape::Object => Value::Object(canonical, fields),
            Shape::Variant(variant) => Value::Variant(canonical, variant, fields),
        })
    }

    fn binary(
        &mut self,
        cx: &mut Cx<'_>,
        expr: &Expr,
        binary: &ast::BinExpr,
        library: Option<&str>,
    ) -> Worked {
        let (Some(lhs), Some(rhs), Some(op)) = (binary.lhs(), binary.rhs(), binary.op()) else {
            return Err(Stop::Unknown);
        };
        let op = op.text().to_string();
        let left = self.expr(cx, &lhs, library);
        let right = self.expr(cx, &rhs, library);
        let (left, right) = match (left, right) {
            (Ok(left), Ok(right)) => (left, right),
            (Err(Stop::NotFixed), _) | (_, Err(Stop::NotFixed)) => return Err(Stop::NotFixed),
            _ => return Err(Stop::Unknown),
        };
        let worked = match (op.as_str(), &left, &right) {
            ("and", Value::Bool(a), Value::Bool(b)) => Some(Value::Bool(*a && *b)),
            ("or", Value::Bool(a), Value::Bool(b)) => Some(Value::Bool(*a || *b)),
            ("==", a, b) => Some(Value::Bool(a.same(b))),
            ("!=", a, b) => Some(Value::Bool(!a.same(b))),
            ("in", item, Value::List(items)) => {
                Some(Value::Bool(items.iter().any(|i| i.same(item))))
            }
            (
                "<" | "<=" | ">" | ">=",
                Value::Number { value: a, .. },
                Value::Number { value: b, .. },
            ) => {
                let ordering = a.compare(*b);
                Some(Value::Bool(match op.as_str() {
                    "<" => ordering == Ordering::Less,
                    "<=" => ordering != Ordering::Greater,
                    ">" => ordering == Ordering::Greater,
                    _ => ordering != Ordering::Less,
                }))
            }
            ("+", Value::Text(a), Value::Text(b)) => Some(Value::Text(format!("{a}{b}"))),
            (
                "+" | "-" | "*" | "/" | "%",
                Value::Number {
                    value: a,
                    decimal: da,
                },
                Value::Number {
                    value: b,
                    decimal: db,
                },
            ) => {
                let decimal = *da || *db;
                match arithmetic(&op, *a, *b, decimal) {
                    Some(value) => Some(Value::Number { value, decimal }),
                    // A quotient past what this check works out exactly is
                    // left to the executors, which divide as wide as they
                    // need.
                    None if decimal && op == "/" && !b.mantissa.eq(&0) => None,
                    None => {
                        let span = cx.span(expr.syntax());
                        let kind = if decimal { "Decimal" } else { "Int" };
                        cx.problems.push(
                            Diagnostic::new(
                                &rule::TYPE_INVALID_LITERAL,
                                format!("`{}` has no `{kind}` value", tree::written(expr.syntax())),
                            )
                            .at(span, format!("past what a `{kind}` holds"))
                            .reason(format!(
                                "A constant is worked out while compiling, and this is past the range of a `{kind}` or divides by zero."
                            ))
                            .next("Use values whose result a number holds"),
                        );
                        return Err(Stop::Unknown);
                    }
                }
            }
            _ => None,
        };
        worked.ok_or(Stop::Unknown)
    }
}

enum Shape {
    Object,
    Variant(String),
}

/// A literal's value.
fn literal(value: &Const) -> Worked {
    Ok(match value {
        Const::Number { value, decimal } => Value::Number {
            value: *value,
            decimal: *decimal,
        },
        Const::Str(text) => Value::Text(text.clone()),
        Const::Bool(value) => Value::Bool(*value),
        Const::Duration(text) => Value::Duration(text.clone()),
        Const::None => Value::None,
        _ => return Err(Stop::Unknown),
    })
}

/// A field of an object's or a variant's value.
fn select(value: Value, name: Option<&str>) -> Worked {
    let name = name.ok_or(Stop::Unknown)?;
    match value {
        Value::Object(_, fields) | Value::Variant(_, _, fields) => fields
            .into_iter()
            .find(|(field, _)| field == name)
            .map(|(_, value)| value)
            .ok_or(Stop::Unknown),
        _ => Err(Stop::Unknown),
    }
}

/// The constant `name` reads, or library `library`'s.
pub(crate) fn find(lookup: &Lookup<'_>, library: Option<&str>, name: &str) -> Option<ConstantId> {
    match library {
        Some(library) => library_constant(lookup, library, name),
        None => bare_constant(lookup, name),
    }
}

/// The constant beside its use that a bare `name` reads, as code in the
/// reader's domain sees it.
fn bare_constant(lookup: &Lookup<'_>, name: &str) -> Option<ConstantId> {
    let symbol = lookup
        .declared(name)
        .iter()
        .find(|symbol| symbol.kind == ItemKind::Constant && symbol.library.is_none())?;
    Some(ConstantId {
        path: symbol.path.clone(),
        index: symbol.index,
        member: None,
    })
}

/// Library `library`'s constant `name`.
fn library_constant(lookup: &Lookup<'_>, library: &str, name: &str) -> Option<ConstantId> {
    let symbol = lookup
        .library_function(library, name)
        .filter(|symbol| symbol.kind == ItemKind::Constant)?;
    let (_, member) = symbol.library.as_ref()?;
    Some(ConstantId {
        path: symbol.path.clone(),
        index: symbol.index,
        member: Some(*member),
    })
}

/// `a op b` on two numbers, as the executors work it out
/// (`jadpo_value::ops`): an `Int` is 64 bits and its division drops the
/// remainder; a `Decimal` quotient is exact to 18 places past its
/// operands, rounded half to even, with trailing zeros past the operands'
/// places dropped. `None` past the range or for a zero divisor.
fn arithmetic(op: &str, a: Num, b: Num, decimal: bool) -> Option<Num> {
    if !decimal {
        let (x, y) = (
            i64::try_from(a.mantissa).ok()?,
            i64::try_from(b.mantissa).ok()?,
        );
        let n = match op {
            "+" => x.checked_add(y)?,
            "-" => x.checked_sub(y)?,
            "*" => x.checked_mul(y)?,
            "/" => x.checked_div(y)?,
            "%" => {
                if y == 0 {
                    return None;
                }
                x.wrapping_rem(y)
            }
            _ => return None,
        };
        return Some(Num::whole(i128::from(n)));
    }
    let limit = 10i128.checked_pow(38)?;
    let bounded = |n: Num| (n.mantissa.abs() < limit && n.scale <= 38).then_some(n);
    match op {
        "+" | "-" => {
            let scale = a.scale.max(b.scale);
            let x = a.at_scale(scale)?.mantissa;
            let y = b.at_scale(scale)?.mantissa;
            let mantissa = if op == "+" {
                x.checked_add(y)?
            } else {
                x.checked_sub(y)?
            };
            bounded(Num { mantissa, scale })
        }
        "*" => bounded(Num {
            mantissa: a.mantissa.checked_mul(b.mantissa)?,
            scale: a.scale + b.scale,
        }),
        "/" => {
            if b.mantissa == 0 {
                return None;
            }
            const PLACES: u32 = 18;
            let base = a.scale.max(b.scale);
            let scale = base + PLACES;
            let shift = scale + b.scale - a.scale;
            let numerator = a.mantissa.checked_mul(10i128.checked_pow(shift)?)?;
            let (quotient, remainder) = (numerator / b.mantissa, numerator % b.mantissa);
            let twice = remainder.unsigned_abs().checked_mul(2)?;
            let away = match twice.cmp(&b.mantissa.unsigned_abs()) {
                Ordering::Greater => true,
                Ordering::Equal => quotient % 2 != 0,
                Ordering::Less => false,
            };
            let negative = (numerator < 0) != (b.mantissa < 0);
            let mut mantissa = match (away, negative) {
                (false, _) => quotient,
                (true, false) => quotient.checked_add(1)?,
                (true, true) => quotient.checked_sub(1)?,
            };
            let mut places = scale;
            while places > base && mantissa % 10 == 0 {
                mantissa /= 10;
                places -= 1;
            }
            bounded(Num {
                mantissa,
                scale: places,
            })
        }
        _ => None,
    }
}

//! Typed holes (area 2, "Typed holes"): `_` where a value goes, always the
//! error `type.hole`. It names the type the place requires, as the code
//! would write it, and up to five values that fit, closest first: the
//! locals and parameters in scope, nearest first; their fields one level
//! down; the constants the code can read; `clock.now` where the code may
//! read the clock; then the standard operations and library functions
//! whose result fits and whose arguments are all values in scope. With a
//! fit, the next step writes the closest; with none, an object is written
//! out with a `_` for each field that can't be `none`.

use jadpo_db::{Db, File, ItemKind, Project};
use jadpo_diag::{Diagnostic, Edit, Fit, Hole, Span, rule};
use jadpo_syntax::SyntaxKind;
use jadpo_syntax::ast::{AstNode, Expr};

use super::super::literal;
use super::super::ty::{Prim, Ty};
use super::Body;
use crate::std_lib::StdNames;
use crate::tree;

/// The most fits a hole names.
const FITS: usize = 5;

/// A value the code could write in the hole's place.
struct Candidate {
    text: String,
    ty: Ty,
}

impl Body<'_> {
    /// `_` where `expected` is required; `None` where nothing around it
    /// says the type.
    pub(super) fn hole(&mut self, expr: &Expr, expected: Option<&Ty>) {
        let span = self.span(expr);
        // The code around a hole isn't finished: what it declares may be
        // what goes in it, so nothing is reported unused.
        self.holed = true;
        let expected = expected.filter(|ty| concrete(ty));
        let Some(expected) = expected else {
            self.out.push(untyped(span));
            return;
        };
        let fits: Vec<Candidate> = self.fits(expected);
        let written = expected.to_string();
        let mut reason = Vec::new();
        let object = literal::object_wanted(self.table, expected);
        match &object {
            Some(object)
                if object.shape.split_once(' ').map(|(name, _)| name) == Some(&written) =>
            {
                reason.push(format!("`{written}` is an object: `{}`.", object.shape));
            }
            Some(object) => reason.push(format!("`{written}` is a `{}`.", object.shape)),
            None if !matches!(expected.strip_nullable(), Ty::Prim(prim) if prim.is_representation()) =>
            {
                reason.push(format!(
                    "`{written}` is {}.",
                    literal::what(self.table, expected)
                ));
            }
            None => {}
        }
        let named: Vec<String> = fits.iter().map(|fit| format!("`{}`", fit.text)).collect();
        reason.push(match named.as_slice() {
            [] => "Nothing in scope fits.".to_string(),
            [one] => format!("In scope, {one} fits."),
            [rest @ .., last] => {
                format!(
                    "In scope, {} and {last} fit, closest first.",
                    rest.join(", ")
                )
            }
        });
        let mut problem =
            Diagnostic::new(&rule::TYPE_HOLE, format!("`_` stands for a `{written}`"))
                .at(span.clone(), format!("a `{written}` goes here"))
                .reason(reason.join(" "));
        problem = match (fits.first(), &object) {
            (Some(first), _) => problem
                .next(format!("Write `{}`", first.text))
                .edit(span.clone(), first.text.clone()),
            (None, Some(object)) => {
                let value = object.with_holes();
                problem
                    .next(format!(
                        "Write `{value}`{}",
                        literal::help_for(object.standard.as_deref())
                    ))
                    .edit(span.clone(), value)
            }
            (None, None) => problem.next(format!(
                "Write a `{written}` value here{}",
                literal::help_for(literal::standard_behind(self.table, expected).as_deref())
            )),
        };
        let path = self.path;
        let hole = Hole {
            expected: Some(written),
            fits: fits
                .into_iter()
                .map(|fit| Fit {
                    edit: Edit {
                        span: Span::new(path, span.start, span.end),
                        replacement: fit.text.clone(),
                    },
                    ty: fit.ty.to_string(),
                    text: fit.text,
                })
                .collect(),
        };
        self.out.push(problem.hole(hole));
    }

    /// What in scope fits `expected`, closest first, at most [`FITS`].
    fn fits(&self, expected: &Ty) -> Vec<Candidate> {
        let fits = |ty: &Ty| {
            concrete(ty) && !matches!(ty, Ty::None) && self.table.assignable(ty, expected)
        };
        // Nearest first; a name a later one shadows can't be read.
        let mut locals: Vec<Candidate> = Vec::new();
        for local in self
            .scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
        {
            if locals.iter().any(|seen| seen.text == local.name) {
                continue;
            }
            locals.push(Candidate {
                text: local.name.clone(),
                ty: local.ty.clone(),
            });
        }
        let identity = self.identity();
        let mut fields = Vec::new();
        for local in &locals {
            for member in super::members_of(self.table, &identity, &local.ty) {
                fields.push(Candidate {
                    text: format!("{}.{}", local.text, member.name),
                    ty: member.ty,
                });
            }
        }
        let mut values: Vec<Candidate> = locals.into_iter().chain(fields).collect();
        let mut out: Vec<Candidate> = Vec::new();
        let offer = |out: &mut Vec<Candidate>, candidate: Candidate| {
            if out.len() < FITS && !out.iter().any(|seen| seen.text == candidate.text) {
                out.push(candidate);
            }
        };
        for value in &values {
            if fits(&value.ty) {
                offer(
                    &mut out,
                    Candidate {
                        text: value.text.clone(),
                        ty: value.ty.clone(),
                    },
                );
            }
        }
        for (name, ty) in self.constants() {
            if fits(&ty) {
                offer(&mut out, Candidate { text: name, ty });
            }
        }
        let instant = Ty::Prim(Prim::Instant);
        if !self.context.is_pure() && fits(&instant) {
            offer(
                &mut out,
                Candidate {
                    text: "clock.now".to_string(),
                    ty: instant,
                },
            );
        }
        if out.len() < FITS {
            values.retain(|value| concrete(&value.ty));
            for call in self.calls_from(&values, &fits) {
                offer(&mut out, call);
            }
        }
        out
    }

    /// The constants the code can read bare, by name.
    fn constants(&self) -> Vec<(String, Ty)> {
        let mut out: Vec<(String, Ty)> = Vec::new();
        for symbols in self.symbols.all().declarations.values() {
            for symbol in symbols {
                if symbol.kind != ItemKind::Constant
                    || symbol.library.is_some()
                    || !symbol.visible_from(self.ownership)
                    || out.iter().any(|(name, _)| *name == symbol.name)
                {
                    continue;
                }
                if let Some(ty) = self.calls.constant_type(None, &symbol.name) {
                    out.push((symbol.name.clone(), ty.clone()));
                }
            }
        }
        if let Some(library) = &self.library {
            let table = crate::types::callables(self.db, self.project);
            for ((host, name), ty) in &table.library_constants {
                if host == library && !out.iter().any(|(seen, _)| seen == name) {
                    out.push((name.clone(), ty.clone()));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// The standard operations and library functions whose result
    /// `fits` and whose every required argument is one of `values`, each
    /// written with the first that fits its parameter; those that can
    /// fail, which need `attempt`, are left out, and so are those that
    /// read the random generator where the code is pure.
    fn calls_from(&self, values: &[Candidate], fits: &dyn Fn(&Ty) -> bool) -> Vec<Candidate> {
        let argument = |param: &Ty| {
            values
                .iter()
                .find(|value| self.table.assignable(&value.ty, param))
                .map(|value| value.text.clone())
        };
        let mut out: Vec<(usize, Candidate)> = Vec::new();
        let std = StdNames::get();
        for (namespace, _) in std.namespaces() {
            if self.context.is_pure() && matches!(namespace, "random" | "ai") {
                continue;
            }
            for name in std.operations(namespace) {
                let Some(operation) = std.operation(namespace, name) else {
                    continue;
                };
                if jadpo_std::operation(namespace, name)
                    .is_some_and(|entry| !entry.conditions.is_empty())
                {
                    continue;
                }
                let result = self.table.resolve_std(&operation.result);
                if !fits(&result) {
                    continue;
                }
                let mut given = Vec::new();
                let mut complete = true;
                for (index, (param, ty)) in operation.params.iter().enumerate() {
                    if matches!(ty, super::super::ty::TypeRef::Nullable(_)) {
                        continue;
                    }
                    let ty = self.table.resolve_std(ty);
                    let Some(value) = concrete(&ty).then(|| argument(&ty)).flatten() else {
                        complete = false;
                        break;
                    };
                    // The first in its place, the rest by name.
                    given.push(if index == 0 {
                        value
                    } else {
                        format!("{param}: {value}")
                    });
                }
                if complete {
                    out.push((
                        given.len(),
                        Candidate {
                            text: format!("{namespace}.{name}({})", given.join(", ")),
                            ty: result,
                        },
                    ));
                }
            }
        }
        let table = crate::types::callables(self.db, self.project);
        for ((library, name), sig) in &table.library {
            if !sig.fails.is_empty() || !fits(&sig.result) {
                continue;
            }
            let reachable = self.library.as_deref() == Some(library.as_str())
                || (self.symbols.library(library).is_some()
                    && self
                        .symbols
                        .library_function(library, name)
                        .is_some_and(|symbol| symbol.visible_from(self.ownership)));
            if !reachable {
                continue;
            }
            let given: Option<Vec<String>> =
                sig.params.iter().map(|(_, ty)| argument(ty)).collect();
            if let Some(given) = given {
                let callee = if self.library.as_deref() == Some(library.as_str()) {
                    name.clone()
                } else {
                    format!("{library}.{name}")
                };
                out.push((
                    given.len(),
                    Candidate {
                        text: format!("{callee}({})", given.join(", ")),
                        ty: sig.result.clone(),
                    },
                ));
            }
        }
        // Fewest arguments first, then by name.
        out.sort_by(|(a, x), (b, y)| a.cmp(b).then_with(|| x.text.cmp(&y.text)));
        out.into_iter().map(|(_, call)| call).collect()
    }
}

/// `_` where nothing says the type: a `var` with no type, an operand.
fn untyped(span: Span) -> Diagnostic {
    Diagnostic::new(&rule::TYPE_HOLE, "`_` has no type here")
        .at(span, "nothing says what goes here")
        .reason("A hole is answered with the type its place requires, and nothing around this one says it, as for an operator's operand or a value whose field is read.")
        .next("Declare the value with its type first, `var name: Type = _`, and use the name here")
        .hole(Hole {
            expected: None,
            fits: Vec::new(),
        })
}

/// Every `_` in `file` the bodies' check didn't answer, each the error
/// `type.hole`: a hole can never ship, wherever it is written.
#[salsa::tracked(returns(ref))]
pub fn hole_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let path = file.path(db);
    let answered: Vec<(u32, u32)> = super::body_problems(db, project, file)
        .iter()
        .filter(|problem| problem.rule.id == rule::TYPE_HOLE.id)
        .filter_map(|problem| problem.primary.as_ref())
        .map(|label| (label.span.start, label.span.end))
        .collect();
    source
        .syntax()
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::HOLE_EXPR)
        .map(|node| tree::node_span(path, &node))
        .filter(|span| !answered.contains(&(span.start, span.end)))
        .map(outside_code)
        .collect()
}

/// `_` outside a callable's body: a declaration's settings, a route's
/// `run:`, a query's clauses (area 2, "Typed holes").
fn outside_code(span: Span) -> Diagnostic {
    Diagnostic::new(&rule::TYPE_HOLE, "`_` stands for a value here")
        .at(span, "a value goes here")
        .reason("Only code in a callable's body is checked with the values in scope, so here the check can't say what fits: a declaration's settings, a route's `run:` and a query's clauses are written out.")
        .next("Write the value here")
        .hole(Hole {
            expected: None,
            fits: Vec::new(),
        })
}

/// Whether `ty` is a type a value can be written for: no error, no type
/// variable left open.
fn concrete(ty: &Ty) -> bool {
    match ty {
        Ty::Error | Ty::Unknown(_) | Ty::Var(_) | Ty::Number => false,
        Ty::List(inner) | Ty::Set(inner) | Ty::Nullable(inner) => concrete(inner),
        Ty::Map(key, value) => concrete(key) && concrete(value),
        Ty::Opaque(_, args) => args.iter().all(concrete),
        Ty::Function(..) => false,
        _ => true,
    }
}

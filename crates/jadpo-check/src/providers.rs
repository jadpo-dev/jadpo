//! A provider package's operations (area 11, "How the API is reached",
//! "Calling" and "Decoding"; chunk 7.2c): each resource's operations in
//! `resources/`, read and checked, for the calls an app makes through a
//! connection (`couriers.bookings.create(request)`) and the plan the
//! runtime sends them with.
//!
//! An operation says how it is sent, `method:` and `path:` (its
//! placeholders, `{id}`, filled from the input's fields of that spelling
//! on the wire), what it takes and gives (`input:` and `output:`, the
//! package's own types), whether it `changes:` anything and is
//! `idempotent:`, the package failures it answers with (`failures:`,
//! read from the provider's `error_code:`), where the real object is in
//! an envelope (`output_at:`), and an answer that is really a failure
//! (`failed_when: { path: equals: }`). `wire_output:` and `decode:` come
//! with chunk 7.2c2. Problems are `package.operation_invalid`.

use std::collections::BTreeMap;

use jadpo_db::{Db, Folder, Ownership, Place, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Decl, Expr};

use crate::tree::{self, name_text, node_span};
use crate::types::{Const, Def, FieldsBase, Ty, TypeRef, TypeView, type_table};

/// The keys an operation may give (area 11). `wire_output:`, `decode:`
/// and `large_output:` are read by later chunks.
pub const OPERATION_KEYS: [&str; 15] = [
    "method",
    "path",
    "input",
    "output",
    "wire_output",
    "decode",
    "output_at",
    "failed_when",
    "changes",
    "idempotent",
    "failures",
    "source",
    "docs",
    "scope",
    "large_output",
];

/// The keys of `failed_when:`.
pub const FAILED_WHEN_KEYS: [&str; 2] = ["path", "equals"];

/// One part of an operation's path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PathPart {
    /// Text sent as it is: `/v1/bookings`.
    Fixed(String),
    /// A placeholder, `{id}`: the input's field of this spelling on the
    /// wire.
    Field(String),
}

/// `failed_when:`: a `200` answer that is really a failure, by the value
/// at `path` (area 11, "Decoding").
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FailedWhen {
    /// Dotted, from the answer's root: `ok`, `result.status`.
    pub path: String,
    /// The value that marks it, as JSON: `false`, `"error"`, `0`.
    pub equals: String,
}

/// One operation of a provider package's resource, read and checked.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Operation {
    /// The package's folder, `shared/providers/acme_couriers/v1`.
    pub folder: String,
    /// The package's name in code, `AcmeCouriers`.
    pub package: String,
    pub resource: String,
    pub name: String,
    /// Whether its resource is `public`, so a connection calls it.
    pub public: bool,
    pub method: jadpo_std::HttpMethod,
    pub path: Vec<PathPart>,
    /// Its input's type as the checker knows it, and its name as the app
    /// writes it (`AcmeCouriers.CreateBooking`).
    pub input: Option<(Ty, String)>,
    /// What it gives: `Unit` when it gives nothing.
    pub output: Ty,
    pub output_at: Option<String>,
    pub failed_when: Option<FailedWhen>,
    pub changes: bool,
    pub idempotent: bool,
    /// Its failures as the checker knows them.
    pub failures: Vec<String>,
    /// Where it is declared.
    pub file: String,
}

impl Operation {
    /// How a call names it after its connection: `bookings.create`.
    pub fn full_name(&self) -> String {
        format!("{}.{}", self.resource, self.name)
    }

    /// Its path as written, placeholders in braces.
    pub fn written_path(&self) -> String {
        self.path
            .iter()
            .map(|part| match part {
                PathPart::Fixed(text) => text.clone(),
                PathPart::Field(name) => format!("{{{name}}}"),
            })
            .collect()
    }
}

/// Every provider package's operations, and the problems with them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Operations {
    pub list: Vec<Operation>,
    pub problems: Vec<Diagnostic>,
}

impl Operations {
    /// The package in `folder`'s operation `resource.name`.
    pub fn find(&self, folder: &str, resource: &str, name: &str) -> Option<&Operation> {
        self.list.iter().find(|operation| {
            operation.folder == folder && operation.resource == resource && operation.name == name
        })
    }

    /// The package in `folder`'s operations.
    pub fn of<'a>(&'a self, folder: &'a str) -> impl Iterator<Item = &'a Operation> + 'a {
        self.list
            .iter()
            .filter(move |operation| operation.folder == folder)
    }
}

/// Every provider package's operations, read from its `resources/`.
#[salsa::tracked(returns(ref))]
pub fn operations(db: &dyn Db, project: Project) -> Operations {
    let mut out = Operations::default();
    let table = type_table(db, project);
    let contested = jadpo_db::contested(db, project);
    for &file in project.files(db) {
        let location = jadpo_db::location(db, project, file);
        let (Ownership::Package(home), Place::Folder(Folder::Resources)) =
            (&location.ownership, &location.place)
        else {
            continue;
        };
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let path = file.path(db).clone();
        let canonical = |written: &str| contested.canonical(written, Some(&home.code));
        for decl in source.decls() {
            let Decl::ResourceDecl(resource) = decl else {
                continue;
            };
            let Some(resource_name) = resource.name().as_ref().and_then(name_text) else {
                continue;
            };
            for (name, member) in tree::members(resource.member_list()) {
                let Some(operation) = name_text(&name) else {
                    continue;
                };
                let reader = Reader {
                    path: &path,
                    table,
                    canonical: &canonical,
                    package: &home.code,
                    operation: format!("{resource_name}.{operation}"),
                };
                if let Some(read) = reader.read(&member, &mut out.problems) {
                    out.list.push(Operation {
                        folder: home.folder.clone(),
                        package: home.code.clone(),
                        resource: resource_name.clone(),
                        name: operation,
                        public: resource.visibility().is_some(),
                        file: path.clone(),
                        ..read
                    });
                }
            }
        }
    }
    out
}

/// The problems with every provider package's operations.
pub fn operation_problems(db: &dyn Db, project: Project) -> &Vec<Diagnostic> {
    &operations(db, project).problems
}

/// Reads one operation.
struct Reader<'a> {
    path: &'a str,
    table: &'a crate::types::TypeTable,
    canonical: &'a dyn Fn(&str) -> String,
    package: &'a str,
    /// `bookings.create`, as problems name it.
    operation: String,
}

impl Reader<'_> {
    fn span(&self, node: &jadpo_syntax::SyntaxNode) -> Span {
        node_span(self.path, node)
    }

    fn problem(&self, message: impl Into<String>, span: Span, label: &str) -> Diagnostic {
        Diagnostic::new(&rule::PACKAGE_OPERATION_INVALID, message).at(span, label)
    }

    /// The operation `member` declares; `None` when it can't be called.
    fn read(&self, member: &ast::Member, out: &mut Vec<Diagnostic>) -> Option<Operation> {
        let before = out.len();
        let operation = &self.operation;
        let Some(block) = member.block() else {
            out.push(
                self.problem(
                    format!("`{operation}` isn't a block"),
                    self.span(member.syntax()),
                    "no `{ ... }`",
                )
                .reason("An operation is a block of keys: how it is sent, `method:` and `path:`, and what it takes and gives.")
                .next(format!("Write `{} {{ method: HttpMethod.get path: \"/...\" ... }}`", operation.rsplit('.').next().unwrap_or(operation))),
            );
            return None;
        };
        let mut method = None;
        let mut path = None;
        let mut input = None;
        let mut output = None;
        let mut output_at = None;
        let mut failed_when = None;
        let mut changes = None;
        let mut idempotent = false;
        let mut failures = Vec::new();
        for (key, inner) in tree::members(Some(block.clone())) {
            let word = name_text(&key).unwrap_or_default();
            let key_span = self.span(key.syntax());
            let value = inner.value();
            let value_span = value
                .as_ref()
                .map_or_else(|| self.span(inner.syntax()), |v| self.span(v.syntax()));
            let constant = value.as_ref().map_or(Const::Other, Const::from_expr);
            match word.as_str() {
                "method" => method = self.method(&constant, value_span, out),
                "path" => path = self.path(&constant, value_span, out),
                "input" => input = self.ty(value.as_ref(), value_span, "input", out),
                "output" => output = self.ty(value.as_ref(), value_span, "output", out),
                "output_at" => output_at = self.dotted(&constant, value_span, "output_at", out),
                "failed_when" => failed_when = self.failed_when(&inner, value_span, out),
                "changes" => changes = self.flag(&constant, value_span, "changes", out),
                "idempotent" => {
                    idempotent = self
                        .flag(&constant, value_span, "idempotent", out)
                        .unwrap_or(false);
                }
                "failures" => failures = self.failures(value.as_ref(), value_span, out),
                // Decoding a whole answer with the package's function comes
                // with chunk 7.2c2.
                "wire_output" | "decode" => out.push(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_NOT_BUILT_YET,
                        format!("An operation's `{word}:` isn't built yet"),
                    )
                    .at(key_span, "declared, not built")
                    .reason(format!(
                        "The language declares an operation's `{word}:`, decoding a whole answer with the package's own function, but this compiler has nothing to run it with yet."
                    ))
                    .next(format!("Do without `{word}:` for now")),
                ),
                "source" | "docs" | "scope" | "large_output" => {}
                other => {
                    let near = nearest(other, OPERATION_KEYS.iter().copied());
                    let mut problem = self
                        .problem(
                            format!("`{operation}` has no key `{other}:`"),
                            key_span.clone(),
                            "not an operation's key",
                        )
                        .reason(format!(
                            "An operation holds {}.{}",
                            crate::and_list(
                                &OPERATION_KEYS
                                    .iter()
                                    .map(|key| format!("`{key}:`"))
                                    .collect::<Vec<_>>()
                            ),
                            near.map_or_else(String::new, |near| format!(" Did you mean `{near}:`?"))
                        ));
                    problem = match near {
                        Some(near) => problem
                            .next(format!("Use `{near}:` instead of `{other}:`"))
                            .edit(key_span, near),
                        None => problem.next(format!("Remove `{other}:`")),
                    };
                    out.push(problem);
                }
            }
        }
        let name_span = member
            .member_key()
            .map_or_else(|| self.span(member.syntax()), |key| self.span(key.syntax()));
        for (missing, given) in [("method", method.is_some()), ("path", path.is_some())] {
            if !given {
                out.push(
                    self.problem(
                        format!("`{operation}` doesn't give `{missing}:`"),
                        name_span.clone(),
                        &format!("no `{missing}:`"),
                    )
                    .reason("An operation says how it is sent: its `method:` and its `path:` on the provider's host.")
                    .next(match missing {
                        "method" => "Add `method: HttpMethod.get`, or the method the API documents".to_string(),
                        _ => "Add `path: \"/...\"` with the path the API documents".to_string(),
                    }),
                );
            }
        }
        let method = method?;
        let path = path?;
        // Each placeholder is one of the input's fields, by its spelling on
        // the wire.
        let fields = input
            .as_ref()
            .and_then(|(ty, _)| match ty {
                Ty::Named(name) => self.table.fields(name),
                _ => None,
            })
            .unwrap_or_default();
        for part in &path {
            let PathPart::Field(placeholder) = part else {
                continue;
            };
            let found = fields.iter().any(|field| {
                field.wire_name.as_deref().unwrap_or(&field.name) == placeholder.as_str()
            });
            if !found {
                let span = block
                    .member_items()
                    .find_map(|item| match item {
                        ast::MemberItem::Member(member)
                            if member
                                .member_key()
                                .and_then(|key| key.names().next())
                                .and_then(|name| name_text(&name))
                                .as_deref()
                                == Some("path") =>
                        {
                            member.value()
                        }
                        _ => None,
                    })
                    .map_or_else(|| name_span.clone(), |value| self.span(value.syntax()));
                let takes = match &input {
                    Some((_, written)) => {
                        format!("its input, `{written}`, has no field sent as `{placeholder}`")
                    }
                    None => "it has no `input:`".to_string(),
                };
                out.push(
                    self.problem(
                        format!("`{operation}`'s path names `{{{placeholder}}}`, which its input doesn't give"),
                        span,
                        "a placeholder with no field",
                    )
                    .reason(format!(
                        "A placeholder in `path:` is filled from the input's field of that spelling on the wire, and {takes}."
                    ))
                    .next(format!(
                        "Add a field `{placeholder}` to the input, or give the field the provider's spelling with `wire_name: \"{placeholder}\"`"
                    )),
                );
            }
        }
        if out.len() != before {
            return None;
        }
        Some(Operation {
            folder: String::new(),
            package: String::new(),
            resource: String::new(),
            name: String::new(),
            public: false,
            changes: changes.unwrap_or(method != jadpo_std::HttpMethod::Get),
            method,
            path,
            input,
            output: output.map_or(Ty::Prim(crate::types::Prim::Unit), |(ty, _)| ty),
            output_at,
            failed_when,
            idempotent,
            failures,
            file: String::new(),
        })
    }

    fn method(
        &self,
        value: &Const,
        span: Span,
        out: &mut Vec<Diagnostic>,
    ) -> Option<jadpo_std::HttpMethod> {
        if let Const::Path(names) = value
            && let [enum_name, variant] = names.as_slice()
            && enum_name == "HttpMethod"
            && let Some(method) = jadpo_std::HttpMethod::from_name(variant)
        {
            return Some(method);
        }
        out.push(
            self.problem(
                format!("`{}`'s `method:` isn't an HTTP method", self.operation),
                span,
                "not a method",
            )
            .reason("`method:` is how the operation is sent: `HttpMethod.get`, `.post`, `.put`, `.patch` or `.delete`.")
            .next("Write `method: HttpMethod.post`, or the method the API documents"),
        );
        None
    }

    fn path(&self, value: &Const, span: Span, out: &mut Vec<Diagnostic>) -> Option<Vec<PathPart>> {
        let refuse = |why: &str, out: &mut Vec<Diagnostic>| {
            out.push(
                self.problem(
                    format!("`{}`'s `path:` isn't a path", self.operation),
                    span.clone(),
                    "not a path",
                )
                .reason(format!(
                    "`path:` is the operation's path on the provider's host, starting with `/`, each placeholder a name in braces: `\"/v1/things/{{id}}\"`; {why}."
                ))
                .next("Write the path the API documents, such as `path: \"/v1/things\"`"),
            );
            None
        };
        let Const::Str(text) = value else {
            return refuse("this isn't text", out);
        };
        if !text.starts_with('/') {
            return refuse("this doesn't start with `/`", out);
        }
        if text.contains(['?', '#']) {
            return refuse(
                "a query is made from the input's other fields, so the path holds none",
                out,
            );
        }
        let mut parts = Vec::new();
        let mut rest = text.as_str();
        while let Some(open) = rest.find('{') {
            let (fixed, after) = rest.split_at(open);
            if fixed.contains('}') {
                return refuse("a `}` closes no placeholder", out);
            }
            if !fixed.is_empty() {
                parts.push(PathPart::Fixed(fixed.to_string()));
            }
            let Some(close) = after.find('}') else {
                return refuse("a `{` is never closed", out);
            };
            let name = &after[1..close];
            if name.is_empty() || name.contains(['{', '/']) {
                return refuse("a placeholder names one field", out);
            }
            parts.push(PathPart::Field(name.to_string()));
            rest = &after[close + 1..];
        }
        if rest.contains('}') {
            return refuse("a `}` closes no placeholder", out);
        }
        if !rest.is_empty() {
            parts.push(PathPart::Fixed(rest.to_string()));
        }
        Some(parts)
    }

    /// `input:` or `output:`: a type of the package's own, as the app
    /// writes it through the package.
    fn ty(
        &self,
        value: Option<&Expr>,
        span: Span,
        key: &str,
        out: &mut Vec<Diagnostic>,
    ) -> Option<(Ty, String)> {
        let value = value?;
        let written = tree::written(value.syntax());
        let resolved = self
            .table
            .resolve(&TypeRef::from_expr(value).scoped(self.canonical));
        if resolved.has_error() {
            // Resolution reports a name that isn't declared.
            return None;
        }
        if key == "input" {
            let object = match &resolved {
                Ty::Named(name) => matches!(
                    self.table.def(name),
                    Some(Def::Fields {
                        base: FieldsBase::Object,
                        ..
                    })
                ),
                _ => false,
            };
            if !object {
                out.push(
                    self.problem(
                        format!("`{}`'s input `{written}` isn't an object type", self.operation),
                        span,
                        "not an object",
                    )
                    .reason("An operation's input is one object type of the package's: its fields fill the path's placeholders, then the query or the body.")
                    .next(format!(
                        "Declare `type {written} = Object {{ ... }}` in the package's `types/`, and name it here"
                    )),
                );
                return None;
            }
        }
        let shown = match &resolved {
            Ty::Named(name) if !name.contains('.') => format!("{}.{name}", self.package),
            _ => format!("{}.{written}", self.package),
        };
        Some((resolved, shown))
    }

    fn dotted(
        &self,
        value: &Const,
        span: Span,
        key: &str,
        out: &mut Vec<Diagnostic>,
    ) -> Option<String> {
        if let Const::Str(text) = value
            && !text.is_empty()
            && text.split('.').all(|part| !part.is_empty())
        {
            return Some(text.clone());
        }
        out.push(
            self.problem(
                format!("`{}`'s `{key}:` isn't a path into the answer", self.operation),
                span,
                "not a path",
            )
            .reason(format!(
                "`{key}:` names a place in the provider's JSON answer, its members' names joined by dots from the root: `\"data\"`, `\"result.items\"`."
            ))
            .next(format!("Write `{key}: \"data\"`, or where the API puts it")),
        );
        None
    }

    fn flag(
        &self,
        value: &Const,
        span: Span,
        key: &str,
        out: &mut Vec<Diagnostic>,
    ) -> Option<bool> {
        if let Const::Bool(flag) = value {
            return Some(*flag);
        }
        out.push(
            self.problem(
                format!("`{}`'s `{key}:` takes `true` or `false`", self.operation),
                span,
                "not `true` or `false`",
            )
            .reason(match key {
                "changes" => "`changes:` says whether the operation changes anything at the provider, which decides whether it may be sent again.".to_string(),
                _ => "`idempotent: true` says the operation is safe to send again: with the package's `idempotency:` key, or because repeating it changes nothing.".to_string(),
            })
            .next(format!("Write `{key}: true` or `{key}: false`")),
        );
        None
    }

    /// `failed_when: { path: equals: }`.
    fn failed_when(
        &self,
        member: &ast::Member,
        span: Span,
        out: &mut Vec<Diagnostic>,
    ) -> Option<FailedWhen> {
        let mut path = None;
        let mut equals = None;
        let before = out.len();
        for (key, inner) in tree::members(member.block()) {
            let word = name_text(&key).unwrap_or_default();
            let value_span = inner
                .value()
                .map_or_else(|| self.span(inner.syntax()), |v| self.span(v.syntax()));
            let value = inner
                .value()
                .as_ref()
                .map_or(Const::Other, Const::from_expr);
            match word.as_str() {
                "path" => path = self.dotted(&value, value_span, "failed_when: { path:", out),
                "equals" => match json_literal(&value) {
                    Some(json) => equals = Some(json),
                    None => out.push(
                        self.problem(
                            format!("`{}`'s `failed_when: {{ equals: }}` isn't a literal", self.operation),
                            value_span,
                            "not a literal",
                        )
                        .reason("`equals:` is the value at `path:` that marks an answer as a failure, written as a literal: `false`, `\"error\"`, `0` or `none` for JSON's `null`.")
                        .next("Write the value the API documents, such as `equals: false`"),
                    ),
                },
                other => out.push(
                    self.problem(
                        format!("`failed_when:` has no key `{other}:`"),
                        self.span(key.syntax()),
                        "not one of its keys",
                    )
                    .reason("`failed_when:` holds `path:`, where in the answer to look, and `equals:`, the value that marks a failure.")
                    .next(format!("Remove `{other}:`")),
                ),
            }
        }
        if out.len() != before {
            return None;
        }
        match (path, equals) {
            (Some(path), Some(equals)) => Some(FailedWhen { path, equals }),
            _ => {
                out.push(
                    self.problem(
                        format!("`{}`'s `failed_when:` needs `path:` and `equals:`", self.operation),
                        span,
                        "incomplete",
                    )
                    .reason("`failed_when:` marks a `200` answer that is really a failure by the value at a place in it: `path:` says where, `equals:` the value.")
                    .next("Write `failed_when: { path: \"ok\" equals: false }`"),
                );
                None
            }
        }
    }

    /// `failures:`: the package's own failures, as the app maps them.
    fn failures(&self, value: Option<&Expr>, span: Span, out: &mut Vec<Diagnostic>) -> Vec<String> {
        let Some(Expr::ListExpr(list)) = value else {
            out.push(
                self.problem(
                    format!("`{}`'s `failures:` isn't a list", self.operation),
                    span,
                    "not a list",
                )
                .reason("`failures:` lists the package's failures the provider answers this operation with, in square brackets.")
                .next("Write the package's failures in brackets: `failures: [<Failure>]`"),
            );
            return Vec::new();
        };
        let mut found = Vec::new();
        for entry in list.exprs() {
            let written = tree::written(entry.syntax());
            let resolved = self
                .table
                .resolve(&TypeRef::from_expr(&entry).scoped(self.canonical));
            let failure = match &resolved {
                Ty::Named(name) => matches!(
                    self.table.def(name),
                    Some(Def::Fields {
                        base: FieldsBase::Failure,
                        ..
                    })
                )
                .then(|| name.clone()),
                _ => None,
            };
            match failure {
                Some(name) => {
                    if !found.contains(&name) {
                        found.push(name);
                    }
                }
                None if resolved.has_error() => {}
                None => out.push(
                    self.problem(
                        format!("`{written}` isn't one of the package's failures"),
                        self.span(entry.syntax()),
                        "not a failure",
                    )
                    .reason(format!(
                        "`failures:` lists the package's own failures, each `type X = Failure {{ ... }}` in its `failures/`, whose codes the provider answers `{}` with.",
                        self.operation
                    ))
                    .next(format!("Declare `{written}` in the package's `failures/`, or remove it")),
                ),
            }
        }
        found
    }
}

/// A literal as the JSON a provider would send it; `None` for anything
/// that isn't one.
fn json_literal(value: &Const) -> Option<String> {
    match value {
        Const::Bool(flag) => Some(flag.to_string()),
        Const::None => Some("null".to_string()),
        Const::Str(text) => Some(
            jadpo_graph::json::Value::Str(text.clone())
                .to_document()
                .trim_end()
                .to_string(),
        ),
        Const::Number { value, .. } => Some(crate::types::show_number(*value)),
        _ => None,
    }
}

/// Whether `operations` holds problems for the package in `folder`.
pub fn has_problems(db: &dyn Db, project: Project, folder: &str) -> bool {
    operation_problems(db, project).iter().any(|problem| {
        problem
            .primary
            .as_ref()
            .is_some_and(|label| label.span.path.starts_with(&format!("{folder}/")))
    })
}

/// The operations of the package in `folder`, by resource then name, for
/// suggestions.
pub fn names_in(db: &dyn Db, project: Project, folder: &str) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for operation in operations(db, project).of(folder) {
        out.entry(operation.resource.clone())
            .or_default()
            .push(operation.name.clone());
    }
    out
}

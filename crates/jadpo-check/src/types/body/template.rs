//! Email templates (area 16, "Email"; chunk 7.3): an email's `.html` and
//! `.txt` and the shared layout, checked by the body checker as a
//! callable is, so a template's expressions are Jadpo's own, typed the
//! same way, and lowered to a typed body both executors run.
//!
//! A template's callable takes its email's fields, `to` and `locale`
//! first, or the layout's `content`, `subject` and `locale`, and gives
//! the text it writes: each piece of text as it is, each `{{ value }}`
//! written out (escaped in HTML unless it is `Html`, a number in its
//! locale's way, an enum in its wording), `{{ if }}` and `{{ for }}` as
//! their statements. Templates are strict: they read their fields,
//! translate and format, and nothing else ([`Body::template_outside`]);
//! what they write out is text, a number or an enum with wording.

use std::collections::BTreeSet;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::body::{self as typed, BinaryOp, Callee, Expr as TypedExpr, Node};
use jadpo_graph::{Key, Kind};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::facts::{LocalFact, node_key};
use super::{Body, Checked, Context, Local, LocalKind};
use crate::emails::TemplateHost;
use crate::lookups::{Lookup, Name};
use crate::tree;
use crate::types::{Def, Prim, Ty};

/// How a value a template writes out is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TemplateWrite {
    /// Text, escaped in an HTML template.
    Text,
    /// An `Html` value, made safe by its construction, written as it is.
    Html,
    /// A number, written in the template's locale's way (`text.format`).
    Number,
    /// An enum's value in its wording, by its translation's key.
    Enum(String),
    /// `{{ app.display_name }}`: the application's `display_name:`.
    AppName(String),
    /// `{{ locale.direction }}`: the locale's text direction in Jadpo's
    /// wording, `ltr` or `rtl`.
    Direction,
}

/// A partial a template includes (chunk 7.3c): the callable its template
/// of the including kind became, and for each of its fields after
/// `locale`, in order, the argument that gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IncludeFact {
    pub(crate) key: Key,
    pub(crate) args: Vec<Option<usize>>,
}

/// The translation key of Jadpo's wording of `TextDirection`.
pub(crate) const DIRECTION_WORDING: &str = "jadpo.text_direction";

/// The checked template at `path`: its problems and its typed body, the
/// callable its email's delivery renders it with.
#[salsa::tracked(returns(ref))]
pub fn template_body<'db>(db: &'db dyn Db, project: Project, path: Name<'db>) -> Checked {
    let path = path.text(db);
    let Some(host) = crate::emails::template_host(db, project, path) else {
        return Checked::default();
    };
    let Some(text) = project
        .records(db)
        .iter()
        .find(|record| &record.path == path)
        .and_then(|record| record.text.as_ref().ok())
    else {
        return Checked::default();
    };
    let parse = jadpo_syntax::parse_template(text);
    let syntax_problems: Vec<Diagnostic> = parse
        .errors()
        .iter()
        .map(|error| error.to_diagnostic(path))
        .collect();
    if !syntax_problems.is_empty() {
        return Checked {
            problems: syntax_problems,
            ..Checked::default()
        };
    }
    let Some(template) = ast::Template::cast(parse.syntax()) else {
        return Checked::default();
    };
    let lookup = Lookup::within(db, project, &host.ownership);
    let mut checker = Body::new(db, project, path, &host.ownership, &lookup);
    checker.context = Context::Template;
    checker.word = "template";
    checker.flow.name = host.label.clone();
    checker.template(&host, &template);
    let body = super::lower::lower_template(&checker, &lookup, text, &template, host.html);
    let mut problems = std::mem::take(&mut checker.out);
    // What the template does besides reading, translating and formatting
    // is the one mistake in it.
    if problems.is_empty() {
        problems = checker.template_outside(&body);
    }
    problems.append(&mut checker.resolved);
    Checked {
        problems,
        key: Some(Key::new(Kind::Function, &host.label)),
        body: std::sync::Arc::new(body),
        ..Checked::default()
    }
}

/// The fields of `SentEmail`, what `email.sent` lists (chunk 7.3c), when
/// `ty` is one.
pub(crate) fn sent_email_fields(ty: &Ty) -> Option<Vec<(&'static str, Ty)>> {
    let Ty::Opaque(name, args) = ty.strip_nullable() else {
        return None;
    };
    if name != jadpo_std::email::SENT_EMAIL || !args.is_empty() {
        return None;
    }
    let text = || Ty::Prim(Prim::Text);
    Some(
        jadpo_std::email::SENT_EMAIL_FIELDS
            .iter()
            .map(|field| {
                let ty = match *field {
                    "to" | "from" => Ty::Prim(Prim::Email),
                    "html" | "text" => text().nullable(),
                    "links" | "attachments" => Ty::List(Box::new(text())),
                    _ => text(),
                };
                (*field, ty)
            })
            .collect(),
    )
}

/// The names a template's body reads, among the ones it is given.
pub fn template_reads(body: &typed::Body) -> BTreeSet<String> {
    body.sites
        .iter()
        .filter_map(|site| match &site.node {
            Node::Expr(TypedExpr::Local(local)) => {
                let local = body.local(*local);
                (local.kind == typed::LocalKind::Param).then(|| local.name.clone())
            }
            _ => None,
        })
        .collect()
}

impl Body<'_> {
    /// `email.sent` (chunk 7.3c): every email a test has sent so far, read
    /// only in a test.
    pub(super) fn sent_emails(&mut self, span: Span, expr: &Expr) -> Ty {
        if !self.test {
            self.out.push(
                Diagnostic::new(&rule::EMAIL_SENT_CONTEXT, "`email.sent` is read outside a test")
                    .at(span, "not in a test")
                    .reason("`email.sent` lists the emails a test sent, which `jadpo test` catches in place of sending them; outside a test an email goes out once its action commits, and nothing keeps it to read.")
                    .next("Read `email.sent` in a test, after the action that sends the email"),
            );
            return Ty::Error;
        }
        self.facts.sent_emails.insert(node_key(expr.syntax()));
        Ty::List(Box::new(Ty::Opaque(
            jadpo_std::email::SENT_EMAIL.to_string(),
            Vec::new(),
        )))
    }

    /// Checks a template against what it is written for.
    pub(super) fn template(&mut self, host: &TemplateHost, template: &ast::Template) {
        self.scopes.push(Vec::new());
        for (name, ty, span) in &host.names {
            let id = self.facts.locals.len();
            self.facts.locals.push(LocalFact {
                name: name.clone(),
                ty: ty.clone(),
                kind: LocalKind::Param,
                span: span.clone(),
            });
            if let Some(scope) = self.scopes.last_mut() {
                scope.push(Local {
                    id,
                    name: name.clone(),
                    ty: ty.clone(),
                    kind: LocalKind::Param,
                    span: span.clone(),
                    binding: None,
                    used: true,
                });
            }
        }
        self.template_parts(template.template_body(), host);
        self.pop_scope();
        if !self.unknown_name {
            let unused = std::mem::take(&mut self.unused);
            for local in unused {
                self.unused_variable(&local);
            }
        }
    }

    fn template_parts(&mut self, body: Option<ast::TemplateBody>, host: &TemplateHost) {
        for part in body.iter().flat_map(ast::TemplateBody::template_parts) {
            match part {
                ast::TemplatePart::TemplateLiteral(_) => {}
                ast::TemplatePart::TemplateValue(value) => {
                    if let Some(expr) = value.expr() {
                        self.template_value(&expr, host);
                    }
                }
                ast::TemplatePart::TemplateIf(branch) => {
                    let conditions = std::iter::once(
                        branch
                            .template_if_tag()
                            .and_then(|tag| tag.condition())
                            .and_then(|c| c.expr()),
                    );
                    if let Some(condition) = conditions.flatten().next() {
                        self.condition(&condition);
                    }
                    self.scopes.push(Vec::new());
                    self.template_parts(branch.template_body(), host);
                    self.pop_scope();
                    for other in branch.template_elses() {
                        if let Some(condition) = other
                            .template_else_tag()
                            .and_then(|tag| tag.condition())
                            .and_then(|c| c.expr())
                        {
                            self.condition(&condition);
                        }
                        self.scopes.push(Vec::new());
                        self.template_parts(other.template_body(), host);
                        self.pop_scope();
                    }
                }
                ast::TemplatePart::TemplateFor(each) => {
                    let tag = each.template_for_tag();
                    let item = match tag.as_ref().and_then(ast::TemplateForTag::expr) {
                        Some(expr) => self.loop_item(&expr),
                        None => Ty::Error,
                    };
                    self.scopes.push(Vec::new());
                    self.declare(
                        tag.as_ref().and_then(ast::TemplateForTag::name),
                        item,
                        LocalKind::Item,
                        None,
                        "variable",
                    );
                    self.loops += 1;
                    self.template_parts(each.template_body(), host);
                    self.loops -= 1;
                    self.pop_scope();
                }
                ast::TemplatePart::TemplateInclude(include) => self.include(&include, host),
            }
        }
    }

    /// `{{ include name(field: value, ...) }}` (chunk 7.3c): a partial
    /// this template may reach, with its template of this one's kind, given
    /// each of its fields once by name; its `locale` goes with it.
    fn include(&mut self, include: &ast::TemplateInclude, host: &TemplateHost) {
        const REASON: &str = "A partial is a `type Name = EmailPartial { ... }` in `shared/emails/` or the domain's own `emails/`, its `.html` and `.txt` beside it; a template includes it as `{{ include name(field: value, ...) }}`, giving each of its fields once by name, and its locale goes with it.";
        let invalid = |message: String, span: Span, label: &str, next: String| {
            Diagnostic::new(&rule::EMAIL_PARTIAL_INVALID, message)
                .at(span, label.to_string())
                .reason(REASON)
                .next(next)
        };
        let Some(Expr::CallExpr(call)) = include.expr() else {
            let span = self.span(include);
            self.out.push(invalid(
                "`{{ include }}` names no partial".to_string(),
                span,
                "not a partial's call",
                "Write `{{ include button(label: ..., url: ...) }}`, naming a partial and giving its fields".to_string(),
            ));
            return;
        };
        let called = match call.expr() {
            Some(Expr::NameRef(name)) => tree::name_ref_text(&name),
            _ => None,
        };
        let Some(called) = called else {
            let span = self.span(&call);
            self.out.push(invalid(
                "`{{ include }}` names no partial".to_string(),
                span,
                "not a partial's name",
                "Name the partial as written in snake case, `{{ include button(...) }}`"
                    .to_string(),
            ));
            self.args_only(&call);
            return;
        };
        let callee_span = call
            .expr()
            .map_or_else(|| self.span(&call), |callee| self.span(&callee));
        let analysis = crate::emails::analysis(self.db, self.project);
        let Some(partial) = analysis.partial_called(&called).cloned() else {
            let near = jadpo_diag::nearest(
                &called,
                analysis
                    .partials
                    .iter()
                    .map(|partial| partial.called.as_str()),
            );
            let type_name = crate::case::to_camel(&called);
            let next = match near {
                Some(near) => format!(
                    "Include `{near}`, or declare `type {type_name} = EmailPartial {{ ... }}` in `shared/emails/` or this domain's `emails/`"
                ),
                None => format!(
                    "Declare `type {type_name} = EmailPartial {{ ... }}` in `shared/emails/` or this domain's `emails/`"
                ),
            };
            self.out.push(invalid(
                format!("`{called}` isn't a partial"),
                callee_span,
                "no such partial",
                next,
            ));
            self.args_only(&call);
            return;
        };
        // A domain's partial is its own emails'; a shared one everyone's.
        if let jadpo_db::Ownership::Domain(owner) = &partial.ownership
            && host.ownership.domain() != Some(owner.as_str())
        {
            self.out.push(invalid(
                format!("`{called}` is domain `{owner}`'s partial"),
                callee_span,
                "another domain's",
                format!(
                    "Move `{}` to `shared/emails/` so every domain's emails can include it, or give this domain a partial of its own",
                    partial.type_name
                ),
            ));
            self.args_only(&call);
            return;
        }
        let template = if host.html {
            &partial.html
        } else {
            &partial.text
        };
        if template.is_none() {
            let (kind, stem) = (
                if host.html { "html" } else { "txt" },
                partial.path.trim_end_matches(".jadpo").to_string(),
            );
            self.out.push(invalid(
                format!("`{called}` has no `.{kind}` template"),
                callee_span,
                "nothing to include here",
                format!("Create `{stem}.{kind}` beside `{}`", partial.path),
            ));
            self.args_only(&call);
            return;
        }
        // Its own fields, after the `locale` that goes with it.
        let fields: Vec<(String, Ty)> = partial
            .fields
            .iter()
            .skip(crate::types::PARTIAL_FIELDS.len())
            .map(|(name, ty, _)| (name.clone(), ty.clone()))
            .collect();
        let args = Self::arguments(&call);
        let mut given: Vec<Option<usize>> = vec![None; fields.len()];
        let mut ok = true;
        for (index, arg) in args.iter().enumerate() {
            let Some(value) = arg.expr() else { continue };
            let Some(name) = Self::arg_name(arg) else {
                ok = false;
                let span = self.span(&value);
                self.out.push(invalid(
                    format!("`{called}` is given a value without its field's name"),
                    span,
                    "no field named",
                    format!(
                        "Give each field by name: `{called}({})`",
                        fields
                            .iter()
                            .map(|(name, _)| format!("{name}: ..."))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
                self.infer(&value);
                continue;
            };
            let name_span = arg
                .name()
                .map_or_else(|| self.span(&value), |n| self.span(&n));
            if crate::types::PARTIAL_FIELDS
                .iter()
                .any(|(own, _)| *own == name)
            {
                ok = false;
                self.out.push(invalid(
                    format!("`{called}` is given `{name}:`, which goes with it"),
                    name_span,
                    "given already",
                    format!("Remove `{name}:`; a partial is written in the locale of the template that includes it"),
                ));
                self.infer(&value);
                continue;
            }
            let Some(slot) = fields.iter().position(|(field, _)| *field == name) else {
                ok = false;
                let next = match jadpo_diag::nearest(&name, fields.iter().map(|(f, _)| f.as_str()))
                {
                    Some(near) => format!("Give `{near}:` instead of `{name}:`"),
                    None => format!("Remove `{name}:`"),
                };
                self.out.push(invalid(
                    format!("`{called}` has no field `{name}`"),
                    name_span,
                    "not one of its fields",
                    next,
                ));
                self.infer(&value);
                continue;
            };
            if given[slot].is_some() {
                ok = false;
                self.out.push(invalid(
                    format!("`{called}` is given `{name}:` twice"),
                    name_span,
                    "given already",
                    format!("Remove the second `{name}:`"),
                ));
                self.infer(&value);
                continue;
            }
            given[slot] = Some(index);
            let ty = fields[slot].1.clone();
            self.check(&value, &ty);
        }
        if ok {
            let missing: Vec<&str> = fields
                .iter()
                .zip(&given)
                .filter(|((_, ty), given)| given.is_none() && !self.table.is_nullable(ty))
                .map(|((name, _), _)| name.as_str())
                .collect();
            if let Some(first) = missing.first() {
                let span = self.span(&call);
                let listed: Vec<String> = missing.iter().map(|name| format!("`{name}:`")).collect();
                self.out.push(invalid(
                    format!("`{called}` is given no `{first}:`"),
                    span,
                    "a field left out",
                    format!("Give {}", crate::and_list(&listed)),
                ));
            }
        }
        self.facts.includes.insert(
            node_key(call.syntax()),
            IncludeFact {
                key: partial.key(host.html),
                args: given,
            },
        );
    }

    /// `{{ value }}`: what it is, and whether it can be written out.
    fn template_value(&mut self, expr: &Expr, host: &TemplateHost) {
        if let Some(write) = self.template_special(expr) {
            self.facts
                .template_writes
                .insert(node_key(expr.syntax()), write);
            return;
        }
        let before = self.out.len();
        let ty = self.infer(expr);
        if self.out.len() > before || ty.has_error() {
            return;
        }
        match self.writable(&ty, host.html) {
            Ok(write) => {
                self.facts
                    .template_writes
                    .insert(node_key(expr.syntax()), write);
            }
            Err((label, next)) => {
                let written = tree::written(expr.syntax());
                self.out.push(
                    Diagnostic::new(
                        &rule::EMAIL_TEMPLATE_VALUE_INVALID,
                        format!("`{{{{ {written} }}}}` can't be written out"),
                    )
                    .at(self.span(expr), label)
                    .reason(format!(
                        "A template writes out text (`Text`, `PresentationText`, or a type built on one), a number (`Int` or `Decimal`), or an enum whose values have wording{}; `{written}` is `{ty}`.",
                        if host.html { ", and `Html` as it is" } else { "" }
                    ))
                    .next(next),
                );
            }
        }
    }

    /// `{{ app.display_name }}` and `{{ locale.direction }}`, which a
    /// template reads beside its fields (area 16, "Level 3: branding").
    fn template_special(&mut self, expr: &Expr) -> Option<TemplateWrite> {
        let Expr::FieldExpr(field) = expr else {
            return None;
        };
        let Some(Expr::NameRef(receiver)) = field.expr() else {
            return None;
        };
        let head = tree::name_ref_text(&receiver)?;
        let member = field
            .name_ref()
            .and_then(|name| tree::name_ref_text(&name))?;
        match (head.as_str(), member.as_str()) {
            ("app", "display_name") if !self.is_local("app") => {
                let settings = crate::emails::settings(self.db, self.project);
                let Some(name) = settings.display_name.clone() else {
                    self.out.push(
                        Diagnostic::new(
                            &rule::EMAIL_SETTINGS_INVALID,
                            "`app.display_name` is written, but the application has no `display_name:`",
                        )
                        .at(self.span(expr), "no name to write")
                        .reason("`{{ app.display_name }}` writes `application { display_name: }`, the application's name as its readers see it, which `app.jadpo` doesn't give.")
                        .next("Add `display_name: \"<the application's name>\"` to `application { }` in `app.jadpo`"),
                    );
                    return Some(TemplateWrite::Text);
                };
                Some(TemplateWrite::AppName(name))
            }
            ("locale", "direction") => {
                let local = self.find("locale")?;
                if local.ty.to_string() != jadpo_std::LOCALE {
                    return None;
                }
                let id = local.id;
                self.facts.reads.insert(node_key(receiver.syntax()), id);
                Some(TemplateWrite::Direction)
            }
            _ => None,
        }
    }

    /// How a value of type `ty` is written out, or the label and next step
    /// saying why it can't be.
    fn writable(&self, ty: &Ty, html: bool) -> Result<TemplateWrite, (String, String)> {
        if self.table.is_nullable(ty) {
            return Err((
                "may be `none`".to_string(),
                "Give the email a field that is never `none`, or write the value inside `{{ if (...) }}` on a field saying it is there".to_string(),
            ));
        }
        let is_html = match ty {
            Ty::Named(name) => {
                name == "Html"
                    || self
                        .table
                        .parent(ty)
                        .is_some_and(|parent| parent.to_string() == "Html")
            }
            _ => false,
        };
        if is_html {
            return if html {
                Ok(TemplateWrite::Html)
            } else {
                Err((
                    "HTML in a text template".to_string(),
                    "Write `Html` in the `.html` template, and give the `.txt` one a `Text` field"
                        .to_string(),
                ))
            };
        }
        match self.table.representation(ty) {
            Some(Prim::Text | Prim::PresentationText) => return Ok(TemplateWrite::Text),
            Some(Prim::Int | Prim::Decimal) => return Ok(TemplateWrite::Number),
            _ => {}
        }
        if let Ty::Named(name) = ty.strip_nullable()
            && let Some(Def::Enum { .. }) = self.table.def(name)
        {
            return match self.enum_wording(name) {
                Some(key) => Ok(TemplateWrite::Enum(key)),
                None => Err((
                    format!("`{name}` has no wording"),
                    format!("Word `{name}`'s values in its domain's language files"),
                )),
            };
        }
        let shown = ty.to_string();
        let next = if self.table.representation(ty) == Some(Prim::Bool) {
            "Use it as a condition: `{{ if (...) }} ... {{ end }}`".to_string()
        } else if matches!(self.table.structure(ty), Ty::List(_) | Ty::Set(_)) {
            "Walk it with `{{ for item in ... }} ... {{ end }}`, writing each item".to_string()
        } else if let Ty::Named(name) = ty
            && let Some(Def::Fields { .. }) = self.table.def(name)
        {
            format!("Write `{name}`'s fields one by one, `{{{{ value.field }}}}`")
        } else {
            "Format it with `text.format(...)` (area 19), which writes it in the email's locale"
                .to_string()
        };
        Err((format!("this is `{shown}`"), next))
    }

    /// The key an enum's wording is called by: `shipping.tone`, or
    /// Jadpo's for `TextDirection`.
    fn enum_wording(&self, name: &str) -> Option<String> {
        if name == "TextDirection" {
            return Some(DIRECTION_WORDING.to_string());
        }
        for scope in crate::translations::scopes(self.db, self.project) {
            let subjects = crate::translations::subjects(
                self.db,
                self.project,
                Name::new(self.db, scope.clone()),
            );
            if let Some(subject) = subjects.list.iter().find(|subject| {
                subject.kind == crate::translations::Kind::Enum
                    && subject.type_name == name
                    && !subject.broken
            }) {
                return Some(format!("{scope}.{}", subject.key));
            }
        }
        None
    }

    /// What a template's typed body does besides reading its names,
    /// translating and formatting: `email.template_not_allowed` at each.
    pub(super) fn template_outside(&self, body: &typed::Body) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        let mut seen: Vec<(u32, u32)> = Vec::new();
        for site in &body.sites {
            let Node::Expr(expr) = &site.node else {
                continue;
            };
            let what = match expr {
                TypedExpr::Literal(_) | TypedExpr::Local(_) | TypedExpr::Field { .. } => None,
                TypedExpr::Unary { .. } => None,
                TypedExpr::Binary { op, lhs, .. } => match op {
                    BinaryOp::Or
                    | BinaryOp::And
                    | BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::In
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual => None,
                    // The template's own text joined: what lowering writes.
                    BinaryOp::Add if writes_out(body, *lhs) => None,
                    _ => Some((
                        "arithmetic",
                        "Work the value out where the email is emitted, and give it to the email as a field",
                    )),
                },
                TypedExpr::Call { callee, .. } => match callee {
                    Callee::Translation(_) | Callee::Escape => None,
                    // `{{ include ... }}`: a partial's template (chunk 7.3c).
                    Callee::Function(key)
                        if key.name().starts_with(crate::emails::PARTIAL_LABEL) =>
                    {
                        None
                    }
                    Callee::Std(jadpo_std::Op::TextFormat | jadpo_std::Op::LocaleDirection) => None,
                    Callee::Variant(_) => None,
                    _ => Some((
                        "a call",
                        "Call only `translations.<domain>.<message>(...)` and `text.format(...)`; work anything else out where the email is emitted, and give it to the email as a field",
                    )),
                },
                _ => Some((
                    "not a read",
                    "Read the email's fields, translate and format; work anything else out where the email is emitted, and give it to the email as a field",
                )),
            };
            let Some((label, next)) = what else { continue };
            // The innermost site is reported once, not each around it.
            if seen
                .iter()
                .any(|(start, end)| *start <= site.start && site.end <= *end)
            {
                continue;
            }
            seen.push((site.start, site.end));
            out.push(
                Diagnostic::new(
                    &rule::EMAIL_TEMPLATE_NOT_ALLOWED,
                    "A template only reads its fields, translates and formats",
                )
                .at(Span::new(self.path, site.start, site.end), label)
                .reason("Templates are strict: they read their email's fields, call translations and `text.format`, compare values in `{{ if }}` and walk lists with `{{ for }}`, so what an email says is decided where it is emitted, and a reviewer reads its words in one place.")
                .next(next),
            );
        }
        out
    }
}

/// Whether `site` reads the local a template's text is written into.
fn writes_out(body: &typed::Body, site: typed::SiteId) -> bool {
    matches!(&body.site(site).node, Node::Expr(TypedExpr::Local(local))
        if body.local(*local).name == super::lower::TEMPLATE_OUT
            && body.local(*local).kind == typed::LocalKind::VarMut)
}

//! Batches in a body (area 12, "Batches: work over many items"; chunk
//! 6.4): `batches.each(from:, each:, finished:, key:)`, `batches.status`
//! and `batches.cancel`, which only an action calls.
//!
//! `from:` is a call of a named query giving a `Page`, which the run walks
//! a page at a time; its arguments are worked out as the action runs and
//! saved with the run. `each:` and `finished:` are inline functions whose
//! body is one call of an action in the same domain (area 3, "The batch
//! helpers take the same form"): an argument that reads the item (or the
//! summary) is the item itself or a field inside it, and one that doesn't
//! is worked out as the action runs and saved with the run, so it must be
//! a value an entity field could hold. `key:` is saved the same way.

use std::collections::BTreeSet;

use jadpo_db::ItemKind;
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_std::batch::{BATCH_PROGRESS, BATCH_RUN, BatchOp, EACH_PARAMS, IMPORT_SUMMARY};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::json_form::{JsonUse, no_json_form};
use super::super::ty::{Prim, Ty};
use super::facts::Reach;
use super::{Body, CallKind, Context, LocalKind, node_key, text};
use crate::tree;

/// A batch type as the checker holds it.
fn batch_type(name: &str) -> Ty {
    Ty::Opaque(name.to_string(), Vec::new())
}

/// A batch type's field's type (area 2, "The batch types' fields").
pub(crate) fn part_ty(part: jadpo_std::batch::Part) -> Ty {
    use jadpo_std::batch::Part;
    match part {
        Part::Of(builtin) => Ty::Prim(builtin),
        Part::Optional(builtin) => Ty::Prim(builtin).nullable(),
        Part::Enum(name) => Ty::Named(name.to_string()),
        Part::List(name) => Ty::List(Box::new(batch_type(name))),
    }
}

/// The fields of the batch type `ty` is, with their types, when it is
/// one: what a program reads of a run's progress and its summary.
pub(crate) fn batch_fields(ty: &Ty) -> Option<Vec<(&'static str, Ty)>> {
    let Ty::Opaque(name, args) = ty.strip_nullable() else {
        return None;
    };
    if !args.is_empty() {
        return None;
    }
    let fields = jadpo_std::batch::object_fields(name)?;
    Some(
        fields
            .iter()
            .map(|(field, part)| (*field, part_ty(*part)))
            .collect(),
    )
}

/// Whether `expr` is `item` or a field inside it: `row`, `row.sku`.
fn item_path(expr: &Expr, item: &str) -> bool {
    match expr {
        Expr::NameRef(name) => text(name) == item,
        Expr::FieldExpr(field) => field
            .expr()
            .is_some_and(|receiver| item_path(&receiver, item)),
        _ => false,
    }
}

/// Which of a batch's values is saved, for a message.
#[derive(Clone, Copy)]
enum Saved {
    /// One of `from:`'s query's arguments.
    From,
    /// A value `each:` or `finished:` captures.
    Captured(&'static str),
    Key,
}

impl Body<'_> {
    /// `batches.each(...)`, `batches.status(run)` or `batches.cancel(run)`:
    /// only in an action, whose transaction keeps the run it starts or the
    /// cancel it asks for (area 12).
    pub(super) fn batch_call(
        &mut self,
        op: BatchOp,
        call: &ast::CallExpr,
        callee: &ast::Expr,
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        let failed = |body: &mut Self| {
            body.args_only(call);
            (
                Ty::Error,
                CallKind::Effect(written.to_string()),
                Reach::Unknown,
            )
        };
        if self.context != Context::Action {
            let span = self.span(callee);
            let who = super::capital(self.context.with_a());
            let does = match op {
                BatchOp::Each => {
                    "starts a run once the action calling it commits, keeping it in that action's transaction"
                }
                BatchOp::Status => "reads a run the runtime keeps in the store an action writes",
                BatchOp::Cancel => {
                    "asks a run to stop, keeping that in the transaction of the action calling it"
                }
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_BATCH_CONTEXT,
                    format!("{who} can't call `{written}`"),
                )
                .at(span, "outside an action")
                .reason(format!("`{written}` {does}, so only an action calls it."))
                .next(format!("Call `{written}` in an action")),
            );
            return failed(self);
        }
        match op {
            BatchOp::Each => self.batch_each(call, callee, written),
            BatchOp::Status | BatchOp::Cancel => {
                let args = Self::arguments(call);
                if args.len() != 1 || args.iter().any(|arg| arg.name().is_some()) {
                    let at = self.arg_list_span(call);
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_ARGUMENT_COUNT,
                            format!("`{written}` takes a `BatchRun`"),
                        )
                        .at(at, format!("{} given", args.len()))
                        .reason(format!(
                            "`{written}` takes the run `batches.each` gave, in order, without a name."
                        ))
                        .next("Pass the `BatchRun` that `batches.each` gave"),
                    );
                    return failed(self);
                }
                if let Some(value) = args.first().and_then(ast::Arg::expr) {
                    self.check(&value, &batch_type(BATCH_RUN));
                }
                let result = match op {
                    BatchOp::Status => batch_type(BATCH_PROGRESS),
                    _ => Ty::Prim(Prim::Unit),
                };
                (
                    result,
                    CallKind::Effect(written.to_string()),
                    Reach::Batch(op),
                )
            }
        }
    }

    /// `batches.each(from:, each:, finished:, key:)`.
    fn batch_each(
        &mut self,
        call: &ast::CallExpr,
        callee: &ast::Expr,
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        let span = self.span(callee);
        // Which tenant a run is limited to isn't decided for a run an
        // action starts (area 9 limits a subscriber to its event's), so an
        // application with tenants waits for that (chosen in chunk 6.4).
        if crate::policy::root_policy(self.db, self.project)
            .tenant
            .is_some()
        {
            self.out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("`{written}` in an application with tenants isn't built yet"),
                )
                .at(span.clone(), "declared, not built")
                .reason(format!(
                    "A run works as the system, and which tenant a run an action starts is limited to isn't decided yet, so this compiler doesn't run `{written}` where `policy {{ }}` names a `tenant:`."
                ))
                .next(format!("Do without `{written}` for now"))
                .help_at("12-jobs-and-events.html#chosen-in-chunk-6-4"),
            );
        }
        let args = Self::arguments(call);
        let mut slots: Vec<Option<usize>> = Vec::new();
        let mut given: [Option<Expr>; 4] = [None, None, None, None];
        for (index, arg) in args.iter().enumerate() {
            let slot = match Self::arg_name(arg) {
                Some(name) => {
                    let found = EACH_PARAMS.iter().position(|param| *param == name);
                    if found.is_none()
                        && let Some(node) = arg.name()
                    {
                        let params: Vec<(String, Ty)> = EACH_PARAMS
                            .iter()
                            .map(|param| (param.to_string(), Ty::Error))
                            .collect();
                        let at = self.span(&node);
                        self.unknown_argument(written, &name, &params, at);
                    }
                    found
                }
                None => (index < EACH_PARAMS.len()).then_some(index),
            };
            match slot.and_then(|slot| given.get_mut(slot)) {
                Some(place) if place.is_none() => *place = arg.expr(),
                _ => {
                    if let Some(value) = arg.expr() {
                        self.infer(&value);
                    }
                }
            }
            slots.push(slot);
        }
        self.facts.slots.insert(node_key(call.syntax()), slots);
        if given[0].is_none() || given[1].is_none() {
            let at = self.arg_list_span(call);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes `from:` and `each:`"),
                )
                .at(at, "missing")
                .reason(format!(
                    "`{written}` walks the items `from:` gives, a call of a named query giving a `Page`, and runs `each:`, an action called per item; `finished:` and `key:` may follow."
                ))
                .next(format!(
                    "Write `{written}(from: query(...), each: item -> action(item))`"
                )),
            );
        }
        let item = match &given[0] {
            Some(from) => self.batch_from(from, written),
            None => Ty::Error,
        };
        if let Some(each) = &given[1] {
            self.batch_function(each, &item, written, "each");
        }
        if let Some(finished) = &given[2] {
            self.batch_function(finished, &batch_type(IMPORT_SUMMARY), written, "finished");
        }
        if let Some(key) = &given[3] {
            let ty = self.infer(key);
            self.batch_saved(key, &ty, Saved::Key, written);
        }
        (
            batch_type(BATCH_RUN),
            CallKind::Effect(written.to_string()),
            Reach::Batch(BatchOp::Each),
        )
    }

    /// `from:`: a call of a named query giving a `Page<T>`, whose
    /// arguments are saved with the run; the item type `T`.
    fn batch_from(&mut self, from: &Expr, written: &str) -> Ty {
        let refused = |body: &mut Self, at: Span, label: &str| {
            body.out.push(
                Diagnostic::new(
                    &rule::TYPE_BATCH_FROM_INVALID,
                    format!("`{written}`'s `from:` isn't a call of a named query giving a `Page`"),
                )
                .at(at, label.to_string())
                .reason(format!(
                    "A run walks `from:` a page at a time, after the action that starts it commits: it calls a named query that gives a `Page<T>`, and each item is a `T`; the query's arguments are worked out now and saved with the run."
                ))
                .next(format!(
                    "Write `{written}(from: <query>(...), ...)`, with a query whose result is `Page<T>`"
                )),
            );
            Ty::Error
        };
        let Expr::CallExpr(query) = from else {
            self.infer(from);
            let at = self.span(from);
            return refused(self, at, "not a query's call");
        };
        let ty = self.infer(from);
        let reach = self.facts.reaches.get(&node_key(query.syntax())).cloned();
        let is_query = matches!(
            &reach,
            Some(Reach::Callable {
                kind: ItemKind::Query,
                ..
            })
        );
        if matches!(reach, Some(Reach::Unknown) | None) || ty.has_error() {
            // What it calls is reported already.
            return Ty::Error;
        }
        let item = match self.table.structure(&ty) {
            Ty::Opaque(name, args) if name == "Page" && args.len() == 1 => args.into_iter().next(),
            _ => None,
        };
        let Some(item) = item.filter(|_| is_query) else {
            let at = self.span(from);
            let label = if is_query {
                format!("gives `{ty}`")
            } else {
                "not a query's call".to_string()
            };
            return refused(self, at, &label);
        };
        for arg in Self::arguments(query) {
            let Some(value) = arg.expr() else { continue };
            if let Some(saved) = self.facts.types.get(&node_key(value.syntax())).cloned() {
                self.batch_saved(&value, &saved, Saved::From, written);
            }
        }
        item
    }

    /// `each:` or `finished:`: `item -> action(...)`, one call of an action
    /// in this domain, given `param`; what it captures is saved with the
    /// run.
    fn batch_function(&mut self, value: &Expr, param: &Ty, written: &str, which: &'static str) {
        let refused = |body: &mut Self, at: Span, label: &str, reason: String| {
            body.out.push(
                Diagnostic::new(
                    &rule::TYPE_BATCH_EACH_INVALID,
                    format!("`{written}`'s `{which}:` isn't one call of an action"),
                )
                .at(at, label.to_string())
                .reason(reason)
                .next(format!(
                    "Write `{which}: {} -> <action>(...)`, calling an action in this domain",
                    if which == "each" { "item" } else { "summary" }
                )),
            );
        };
        let shape = format!(
            "`{which}:` runs later, after the action that starts the run commits: it is written `{} -> <action>(...)`, one call of an action in this domain, as a subscriber's `run:` is.",
            if which == "each" { "item" } else { "summary" }
        );
        let Expr::LambdaExpr(lambda) = value else {
            self.infer(value);
            let at = self.span(value);
            refused(self, at, "not an inline function", shape);
            return;
        };
        let names: Vec<ast::Name> = lambda.names().collect();
        self.scopes.push(Vec::new());
        let item = self.facts.locals.len();
        let item_name = names.first().and_then(tree::name_text).unwrap_or_default();
        if let [name] = names.as_slice() {
            self.declare(
                Some(name.clone()),
                param.clone(),
                LocalKind::Inline,
                None,
                "parameter",
            );
        } else {
            for name in &names {
                self.declare(
                    Some(name.clone()),
                    Ty::Error,
                    LocalKind::Inline,
                    None,
                    "parameter",
                );
            }
            let at = self.span(lambda);
            refused(
                self,
                at,
                &format!("{} parameters", names.len()),
                format!(
                    "`{which}:` is given one value at a time, so it is written with one parameter. {shape}"
                ),
            );
        }
        let Some(body) = lambda.expr() else {
            self.pop_scope();
            return;
        };
        let action = match &body {
            Expr::CallExpr(inner) => match inner.expr() {
                Some(Expr::NameRef(name)) => {
                    let callee = text(&name);
                    let declared = self
                        .symbols
                        .declared(&callee)
                        .iter()
                        .find(|symbol| symbol.library.is_none() && symbol.kind == ItemKind::Action)
                        .map(|symbol| symbol.ownership == *self.ownership);
                    match declared {
                        Some(true) => Some(inner.clone()),
                        Some(false) => {
                            let at = self.span(&name);
                            refused(
                                self,
                                at,
                                "another domain's action",
                                format!("`{callee}` is declared in another domain. {shape}"),
                            );
                            None
                        }
                        None if self.callable_kind(&callee).is_some() => {
                            let at = self.span(&name);
                            refused(self, at, "not an action", shape.clone());
                            None
                        }
                        // An unknown name is reported where it is checked.
                        None => None,
                    }
                }
                _ => {
                    let at = self.span(&body);
                    refused(self, at, "not a call of an action", shape.clone());
                    None
                }
            },
            _ => {
                let at = self.span(&body);
                refused(self, at, "not a call of an action", shape.clone());
                None
            }
        };
        let Some(inner) = action else {
            self.infer(&body);
            self.pop_scope();
            return;
        };
        // Its failures are the run's problems, which nothing here handles.
        self.handled.push(node_key(inner.syntax()));
        self.infer(&body);
        for arg in Self::arguments(&inner) {
            let Some(value) = arg.expr() else { continue };
            let reads = value
                .syntax()
                .descendants()
                .any(|node| self.facts.reads.get(&node_key(&node)) == Some(&item));
            if reads {
                if !item_path(&value, &item_name) {
                    let at = self.span(&value);
                    refused(
                        self,
                        at,
                        "the item and more",
                        format!(
                            "An argument reading `{item_name}` is `{item_name}` itself or a field inside it, which the run reads from each {}; one that reads nothing of it is worked out now and saved with the run. Work the rest out in the action it calls.",
                            if which == "each" { "item" } else { "summary" }
                        ),
                    );
                }
                continue;
            }
            if let Some(saved) = self.facts.types.get(&node_key(value.syntax())).cloned() {
                self.batch_saved(&value, &saved, Saved::Captured(which), written);
            }
        }
        self.pop_scope();
    }

    /// A value saved with the run: one with no stored form is refused,
    /// naming the value (area 3, "The batch helpers take the same form").
    fn batch_saved(&mut self, value: &Expr, ty: &Ty, saved: Saved, written: &str) {
        if ty.has_error() {
            return;
        }
        // `from:`'s query takes what a request could carry, an `Input`
        // among them; the rest is held as an entity field holds it.
        let used = match saved {
            Saved::From => JsonUse::Write,
            Saved::Captured(_) | Saved::Key => JsonUse::Store,
        };
        let Some((offending, what)) = no_json_form(self.table, ty, &mut BTreeSet::new(), used)
        else {
            return;
        };
        let shown = tree::written(value.syntax());
        let place = match saved {
            Saved::From => "`from:`'s query is given".to_string(),
            Saved::Captured(which) => format!("`{which}:` captures"),
            Saved::Key => "`key:` is".to_string(),
        };
        let held = if offending == ty.strip_nullable().to_string() {
            format!("`{shown}` is `{offending}`")
        } else {
            format!("`{shown}` holds `{offending}`")
        };
        let at = self.span(value);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_BATCH_CAPTURE_NOT_STORABLE,
                format!("`{written}` can't save `{shown}`"),
            )
            .at(at, format!("`{offending}` has no stored form"))
            .reason(format!(
                "What {place} is worked out when the run starts and saved with it, as its type's JSON, so the run can resume after a crash or a deploy; {held}, {what}."
            ))
            .next(format!(
                "Pass something an entity field could hold instead, such as the `id` that finds `{shown}`"
            )),
        );
    }
}

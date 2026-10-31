//! Batches in a body (area 12, "Batches: work over many items"; chunk
//! 6.4): `batches.each(from:, each:, finished:, key:)`, `batches.status`
//! and `batches.cancel`, which only an action calls; and the file
//! helpers' reads (chunk 6.4b) and writes (chunk 6.4b2), which start runs
//! the same way.
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
use jadpo_std::batch::{
    BATCH_PROGRESS, BATCH_RUN, BatchOp, EACH_PARAMS, IMPORT_SUMMARY, ITEMS_AT, MAX_UNPACKED_SIZE,
    READ_PARAMS, RowFormat,
};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::json_form::{JsonUse, no_json_form};
use super::super::shape::FieldsBase;
use super::super::table::Def;
use super::super::ty::{Prim, Ty, TypeRef};
use super::facts::Reach;
use super::{Body, CallKind, Context, LocalKind, node_key, text};
use crate::tree;

/// A batch type as the checker holds it.
pub(super) fn batch_type(name: &str) -> Ty {
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
        Part::Kept(name) => batch_type(name),
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
pub(super) enum Saved {
    /// One of `from:`'s query's arguments.
    From,
    /// A value `each:` or `finished:` captures.
    Captured(&'static str),
    Key,
    /// The stored file a file helper reads.
    File,
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
                BatchOp::Read(_) => {
                    "starts a run over a stored file's rows once the action calling it commits, keeping it in that action's transaction"
                }
                BatchOp::Write(_) => {
                    "starts a run writing a query's records into a stored file once the action calling it commits, keeping it in that action's transaction"
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
            BatchOp::Each => self.batch_each(call, written),
            BatchOp::Read(format) => self.batch_read(format, call, written),
            BatchOp::Write(format) => self.batch_write(format, call, written),
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
    fn batch_each(&mut self, call: &ast::CallExpr, written: &str) -> (Ty, CallKind, Reach) {
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

    /// Whether a call of `json.read` is its file form (chunk 6.4b3): given
    /// `each:`, or anything else only the file form takes, by name, where
    /// the in-memory form takes text and `as:`.
    pub(super) fn reads_a_file(call: &ast::CallExpr) -> bool {
        Self::arguments(call).iter().any(|arg| {
            Self::arg_name(arg)
                .is_some_and(|name| name != "as" && READ_PARAMS[1..].contains(&name.as_str()))
        })
    }

    /// `csv.read(file, as:, each:, finished:, key:, max_unpacked_size:)`,
    /// and `tsv`'s, `json_lines`'s (chunk 6.4b) and `json`'s file form,
    /// which also takes `items_at:` (chunk 6.4b3): the stored file is saved
    /// with the run,
    /// each row read as the `Input` `as:` names and handed to `each:`.
    fn batch_read(
        &mut self,
        format: RowFormat,
        call: &ast::CallExpr,
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        // Only `json.read` takes `items_at:`.
        let takes: Vec<&str> = READ_PARAMS
            .iter()
            .copied()
            .filter(|param| format == RowFormat::Json || *param != READ_PARAMS[ITEMS_AT])
            .collect();
        let args = Self::arguments(call);
        let mut slots: Vec<Option<usize>> = Vec::new();
        let mut given: [Option<Expr>; 7] = Default::default();
        for (index, arg) in args.iter().enumerate() {
            let slot = match Self::arg_name(arg) {
                Some(name) => {
                    let found = READ_PARAMS
                        .iter()
                        .position(|param| *param == name)
                        .filter(|_| takes.contains(&name.as_str()));
                    if found.is_none()
                        && let Some(node) = arg.name()
                    {
                        let params: Vec<(String, Ty)> = takes
                            .iter()
                            .map(|param| (param.to_string(), Ty::Error))
                            .collect();
                        let at = self.span(&node);
                        self.unknown_argument(written, &name, &params, at);
                    }
                    found
                }
                // What came before `items_at:` may be written in order.
                None => (index < ITEMS_AT).then_some(index),
            };
            match slot.and_then(|slot| given.get_mut(slot)) {
                // `as:` is a type, read below rather than as a value.
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
        if given[0].is_none() || given[1].is_none() || given[2].is_none() {
            let at = self.arg_list_span(call);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes a file, `as:` and `each:`"),
                )
                .at(at, "missing")
                .reason(format!(
                    "`{written}` reads the rows of a stored `File`, each as the `Input` `as:` names, and runs `each:`, an action called per row; {} may follow.",
                    if format == RowFormat::Json {
                        "`items_at:`, `finished:`, `key:` and `max_unpacked_size:`"
                    } else {
                        "`finished:`, `key:` and `max_unpacked_size:`"
                    }
                ))
                .next(format!(
                    "Write `{written}(file, as: RowType, each: row -> action(row))`"
                )),
            );
        }
        if let Some(at) = &given[ITEMS_AT] {
            self.items_at(at, written);
        }
        if let Some(size) = &given[MAX_UNPACKED_SIZE] {
            self.unpacked_size(size, written);
        }
        if let Some(file) = &given[0] {
            let ty = self.infer(file);
            let stored = Ty::Opaque(jadpo_std::files::FILE.to_string(), Vec::new());
            if ty.has_error() {
            } else if self.table.assignable(&ty, &stored) && !self.table.is_nullable(&ty) {
                self.batch_saved(file, &ty, Saved::File, written);
            } else {
                let contents = matches!(
                    self.table.structure(ty.strip_nullable()),
                    Ty::Opaque(name, _) if name == jadpo_std::files::FILE_CONTENTS
                );
                let reason = if contents {
                    format!(
                        "`{written}` reads a stored `File` a page at a time, in the background; `FileContents` is what `store.read` gives, a whole file's bytes for passing on, which the format helpers don't take."
                    )
                } else {
                    format!(
                        "`{written}` reads the rows of a stored `File`, such as one an entity's `File` field keeps or `store.put` gave."
                    )
                };
                let span = self.span(file);
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_MISMATCH,
                        format!("`{ty}` can't be used where `File` is required"),
                    )
                    .at(span, format!("this is `{ty}`"))
                    .reason(reason)
                    .next("Pass the stored `File`"),
                );
            }
        }
        let row = match &given[1] {
            Some(value) => self.row_type(format, value, written),
            None => None,
        }
        .unwrap_or(Ty::Error);
        if let Some(each) = &given[2] {
            self.batch_function(each, &row, written, "each");
        }
        if let Some(finished) = &given[3] {
            self.batch_function(finished, &batch_type(IMPORT_SUMMARY), written, "finished");
        }
        if let Some(key) = &given[4] {
            let ty = self.infer(key);
            self.batch_saved(key, &ty, Saved::Key, written);
        }
        (
            batch_type(BATCH_RUN),
            CallKind::Effect(written.to_string()),
            Reach::Batch(BatchOp::Read(format)),
        )
    }

    /// `json.read`'s `items_at:` (chunk 6.4b3): text written out, the names
    /// of the members leading from the document's top object to its list
    /// of items, between dots.
    fn items_at(&mut self, value: &Expr, written: &str) {
        let text = match super::super::rules::Const::from_expr(value) {
            super::super::rules::Const::Str(text) => Some(text),
            _ => None,
        };
        let label = match &text {
            None => {
                if !super::is_literal(value) {
                    self.infer(value);
                }
                "not text written out"
            }
            Some(text) if text.is_empty() => "names no member",
            Some(text) if text.split('.').any(str::is_empty) => "a member with no name",
            Some(_) => return,
        };
        let span = self.span(value);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_BATCH_ROW_INVALID,
                format!("`{written}`'s `items_at:` doesn't say where its list is"),
            )
            .at(span, label)
            .reason(format!(
                "`items_at:` names where the list of items sits in the document, written out as the members leading to it from its top object, between dots: `items_at: \"data.items\"`. When the document is the list, `{written}` is given no `items_at:`."
            ))
            .next(
                "Write the members' names, such as `items_at: \"data.items\"`, or leave `items_at:` out if the document is the list",
            ),
        );
    }

    /// A file helper's `max_unpacked_size:` (chunk 6.4b3): a size written
    /// out, more than nothing, as a `File` field's `max_size:` is.
    fn unpacked_size(&mut self, value: &Expr, written: &str) {
        let text = tree::written(value.syntax());
        if jadpo_std::files::size(&text).is_some_and(|bytes| bytes > 0) {
            return;
        }
        let span = self.span(value);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_BATCH_ROW_INVALID,
                format!("`{written}`'s `max_unpacked_size:` isn't a size"),
            )
            .at(span, "not a size")
            .reason(
                "`max_unpacked_size:` is the most a gzip file may unpack to, so a small file can't unpack to gigabytes: a whole number of bytes, kilobytes, megabytes or gigabytes, more than none, written `700b`, `512kb`, `200mb` or `2gb`, each a thousand times the last.",
            )
            .next("Write the size as a whole number and its unit, such as `max_unpacked_size: 200mb`"),
        );
    }

    /// `from:`'s query, when its order puts a field an update can change
    /// before its `id` (area 12, "No other way"; chunk 6.4b3): a run walks
    /// live records a page at a time, after the last one the page before
    /// ended on, so a record whose place in that order moves mid-run is
    /// missed or met twice. `id`, a field marked `immutable: true` and one
    /// generated as its record is created never change. A judgement call,
    /// declined on the action with a reason when the field can't change in
    /// practice.
    fn batch_order(&mut self, query: &str, from: &Expr, written: &str) {
        let Some(model) = crate::data::queries::query_model(
            self.db,
            self.project,
            crate::lookups::Name::new(self.db, query.to_string()),
        )
        .clone() else {
            return;
        };
        let Some(entity) = crate::data::entity_model(
            self.db,
            self.project,
            crate::lookups::Name::new(self.db, model.entity.clone()),
        )
        .clone() else {
            return;
        };
        let mut order: Vec<String> = model
            .order
            .iter()
            .map(|field| field.field.clone())
            .collect();
        if let Some(distance) = &model.distance {
            order.insert(0, distance.field.clone());
        }
        for name in order {
            let Some(field) = entity.fields.iter().find(|field| field.name == name) else {
                continue;
            };
            let fixed = field.name == "id"
                || field.immutable
                || field.generated == Some(jadpo_std::Generated::OnCreate);
            if !fixed {
                let span = self.span(from);
                let host = &entity.name;
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_BATCH_ORDER_UPDATABLE,
                        format!("`{written}` walks `{query}` in an order an update can change"),
                    )
                    .at(span, format!("ordered by `{host}.{name}`, which an update can change"))
                    .reason(format!(
                        "A run walks live records a page at a time, each page after the record the one before ended on, so a record whose `{host}.{name}` changes mid-run can move behind that place and be missed, or ahead of it and be met twice. An order no update changes meets every record once: `id`, or a time generated as the record is created, then `id`."
                    ))
                    .next(format!(
                        "Order `{query}` by `{host}.id`, or by a field marked `immutable: true` or generated with `Generated.on_create` and then `{host}.id`"
                    )),
                );
                return;
            }
            // Nothing after a unique field that never changes moves a
            // record, unless its key holds the tenancy too (area 9).
            if field.name == "id"
                || crate::data::Entities::new(self.db, self.project).unique_alone(&entity, field)
            {
                return;
            }
        }
    }

    /// A file helper's `as:`: an `Input` written out, each of whose fields
    /// a row of `format` can hold; what is wrong is reported.
    fn row_type(&mut self, format: RowFormat, value: &Expr, written: &str) -> Option<Ty> {
        let target = match TypeRef::from_expr(value) {
            TypeRef::Path(names, args)
                if names
                    .first()
                    .is_some_and(|name| name.starts_with(|c: char| c.is_ascii_uppercase())) =>
            {
                if !self.resolved_by(|names, out| {
                    names.type_expr(value, crate::resolve::Want::Type, out)
                }) {
                    return None;
                }
                Some(self.table.resolve(&TypeRef::Path(names, args)))
            }
            _ => None,
        };
        let refused = |body: &mut Self, label: String, reason: String, next: String| {
            let span = body.span(value);
            body.out.push(
                Diagnostic::new(
                    &rule::TYPE_BATCH_ROW_INVALID,
                    format!(
                        "`{written}` can't read its rows as `{}`",
                        tree::written(value.syntax())
                    ),
                )
                .at(span, label)
                .reason(reason)
                .next(next),
            );
        };
        let Some(target) = target else {
            if !super::is_literal(value) {
                self.infer(value);
            }
            refused(
                self,
                "not a type".to_string(),
                format!(
                    "`{written}` reads each row as the type `as:` names, which is written out, such as `as: ProductRow`: a value can't say which type to read."
                ),
                "Write the row type itself, such as `as: ProductRow`".to_string(),
            );
            return None;
        };
        if target.has_error() {
            return None;
        }
        let fields = match &target {
            Ty::Named(name) => match self.table.def(name) {
                Some(Def::Fields {
                    base: FieldsBase::Input,
                    fields,
                    ..
                }) => Some(fields.clone()),
                _ => None,
            },
            _ => None,
        };
        let Some(fields) = fields else {
            refused(
                self,
                "not an `Input`".to_string(),
                format!(
                    "A row read from a file is an `Input`, checked with an `Input`'s rules (area 7): its fields are its columns, unknown ones refused, and `optional` and `default` apply; `{target}` isn't one."
                ),
                "Declare the row as `type <Name> = Input { ... }`, and read the file as it"
                    .to_string(),
            );
            return None;
        };
        if let Some((offending, what)) =
            no_json_form(self.table, &target, &mut BTreeSet::new(), JsonUse::Read)
        {
            refused(
                self,
                format!("`{offending}` has no form a row holds"),
                format!("A row is read as an `Input` is from a request; `{offending}` is {what}."),
                "Read the row as a type made of values a request carries".to_string(),
            );
            return None;
        }
        if format.delimiter().is_some() {
            for field in &fields {
                if let Some(what) = self.not_one_value(&field.declared) {
                    let name = &field.name;
                    refused(
                        self,
                        format!("`{target}.{name}` holds {what}"),
                        format!(
                            "A {} row holds one value a cell, and `{target}.{name}` holds {what}; a JSON Lines row holds lists and objects, each line one JSON value.",
                            format.namespace().to_uppercase()
                        ),
                        format!(
                            "Read the file with `json_lines.read`, or give `{name}` a type one cell holds"
                        ),
                    );
                    return None;
                }
            }
            let mut columns: Vec<(String, String)> = Vec::new();
            for field in &fields {
                let column = field.column.clone().unwrap_or_else(|| field.name.clone());
                if let Some((_, first)) = columns.iter().find(|(seen, _)| *seen == column) {
                    let (first, name) = (first.clone(), field.name.clone());
                    refused(
                        self,
                        format!("two fields read column `{column}`"),
                        format!(
                            "Each of a row's fields is read from a column of its own, by its name or its `column:`, and `{target}.{first}` and `{target}.{name}` both read `{column}`."
                        ),
                        format!("Give `{name}` a `column:` of its own"),
                    );
                    return None;
                }
                columns.push((column, field.name.clone()));
            }
        }
        Some(target)
    }

    /// What a CSV cell can't hold that `ty` is, after "holds": a list, a
    /// map, an object, an enum with data, or a standard type with no one
    /// text form (area 19, "Data formats").
    pub(super) fn not_one_value(&self, ty: &Ty) -> Option<&'static str> {
        let structure = self.table.structure(ty.strip_nullable());
        match &structure {
            Ty::List(_) | Ty::Set(_) => Some("a list"),
            Ty::Map(..) => Some("a map"),
            Ty::Named(name) | Ty::Opaque(name, _)
                if let Some(entry) = jadpo_std::standard_type(name) =>
            {
                entry
                    .text
                    .is_none()
                    .then_some("a standard type with no one text form")
            }
            Ty::Named(name) => match self.table.def(name) {
                Some(Def::Enum { variants }) if variants.iter().any(|v| v.fields.is_some()) => {
                    Some("an enum with data")
                }
                Some(Def::Enum { .. }) | Some(Def::Scalar { .. }) => None,
                Some(_) => Some("an object"),
                None => None,
            },
            Ty::Opaque(..) => Some("an object"),
            _ => None,
        }
    }

    /// `from:`: a call of a named query giving a `Page<T>`, whose
    /// arguments are saved with the run; the item type `T`.
    pub(super) fn batch_from(&mut self, from: &Expr, written: &str) -> Ty {
        let refused = |body: &mut Self, at: Span, label: &str| {
            body.out.push(
                Diagnostic::new(
                    &rule::TYPE_BATCH_FROM_INVALID,
                    format!("`{written}`'s `from:` isn't a call of a named query giving a `Page`"),
                )
                .at(at, label.to_string())
                .reason(
                    "A run walks `from:` a page at a time, after the action that starts it commits: it calls a named query that gives a `Page<T>`, and each item is a `T`; the query's arguments are worked out now and saved with the run.",
                )
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
        // Area 12 also lets `batches.each`'s `from:` be a provider's list
        // operation, walked by `page.next`; a run is kept only over a
        // named query's pages so far.
        if written == "batches.each"
            && item.is_some()
            && matches!(reach, Some(Reach::Provider { .. }))
        {
            let at = self.span(from);
            self.out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("`{written}` over a provider's list isn't built yet"),
                )
                .at(at, "declared, not built")
                .reason(format!(
                    "The language lets `{written}` walk a provider's list operation by `page.next`, but this compiler keeps a run only over the pages of a named query, so this run couldn't keep its place."
                ))
                .next(format!(
                    "Do without `{written}` over a provider's list for now: keep what it lists in an entity, and walk that with a named query giving a `Page`"
                )),
            );
            return Ty::Error;
        }
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
        if let Some(Reach::Callable { name, .. }) = &reach {
            self.batch_order(name, from, written);
        }
        item
    }

    /// `each:` or `finished:`: `item -> action(...)`, one call of an action
    /// in this domain, given `param`; what it captures is saved with the
    /// run.
    pub(super) fn batch_function(
        &mut self,
        value: &Expr,
        param: &Ty,
        written: &str,
        which: &'static str,
    ) {
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
        let inline_name = names.first().and_then(tree::name_text).unwrap_or_default();
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
                if !item_path(&value, &inline_name) {
                    let at = self.span(&value);
                    refused(
                        self,
                        at,
                        "the item and more",
                        format!(
                            "An argument reading `{inline_name}` is `{inline_name}` itself or a field inside it, which the run reads from each {}; one that reads nothing of it is worked out now and saved with the run. Work the rest out in the action it calls.",
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
    pub(super) fn batch_saved(&mut self, value: &Expr, ty: &Ty, saved: Saved, written: &str) {
        if ty.has_error() {
            return;
        }
        // `from:`'s query takes what a request could carry, an `Input`
        // among them; the rest is held as an entity field holds it.
        let used = match saved {
            Saved::From => JsonUse::Write,
            Saved::Captured(_) | Saved::Key | Saved::File => JsonUse::Store,
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
            Saved::File => format!("`{written}` reads"),
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

//! The file helpers' writes in a body (area 19, "Data formats"; area 12,
//! "Batches"; chunk 6.4b2): `csv.write(store, name:, format:, from:,
//! each:, finished:, key:)`, and `tsv`'s, `json_lines`'s and `json`'s file
//! form, which only an action calls.
//!
//! `store` names a store of files, a connection; `name:` is the file's
//! name, text; `format:` is CSV's and TSV's `CsvFormat`, required, and
//! JSON's and JSON Lines' have none. `from:` is a call of a named query
//! giving a `Page`, as for `batches.each`, and `each:` an inline function
//! giving each item's row, an `Output`, one value a cell for CSV and TSV.
//! It is pure, and what it reads of the action (its parameters and
//! variables) is worked out as the action runs and saved with the run, so
//! it must be a value an entity field could hold. `finished:` receives an
//! `ExportSummary`.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::batch::{BATCH_RUN, BatchOp, EXPORT_SUMMARY, RowFormat, WRITE_PARAMS};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::json_form::{JsonUse, no_json_form};
use super::super::shape::FieldsBase;
use super::super::table::Def;
use super::super::ty::{Prim, Ty};
use super::batches::{Saved, batch_type};
use super::facts::Reach;
use super::{Body, CallKind, node_key, text};
use crate::tree;

/// Where each argument sits among [`WRITE_PARAMS`].
const STORE: usize = 0;
const NAME: usize = 1;
const FORMAT: usize = 2;
const FROM: usize = 3;
const EACH: usize = 4;
const FINISHED: usize = 5;
const KEY: usize = 6;

/// The type each item's row is, as the inline function `each:` gives it.
const ROW: &str = "Row";

impl Body<'_> {
    /// Whether a call of `json.write` is its file form: given more than
    /// the one value the in-memory form takes, or any argument by name.
    pub(super) fn writes_a_file(call: &ast::CallExpr) -> bool {
        let args = Self::arguments(call);
        args.len() > 1 || args.iter().any(|arg| arg.name().is_some())
    }

    /// `csv.write(store, name:, format:, from:, each:, finished:, key:)`,
    /// and the other writers.
    pub(super) fn batch_write(
        &mut self,
        format: RowFormat,
        call: &ast::CallExpr,
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        let cells = format.delimiter().is_some();
        let takes: Vec<&str> = WRITE_PARAMS
            .iter()
            .copied()
            .filter(|param| cells || *param != WRITE_PARAMS[FORMAT])
            .collect();
        let args = Self::arguments(call);
        let mut slots: Vec<Option<usize>> = Vec::new();
        let mut given: [Option<Expr>; 7] = Default::default();
        for (index, arg) in args.iter().enumerate() {
            let slot = match Self::arg_name(arg) {
                Some(name) => {
                    let found = WRITE_PARAMS
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
                // Only the store is written without its name.
                None => (index == STORE).then_some(STORE),
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
        let needed: Vec<usize> = [STORE, NAME, FORMAT, FROM, EACH]
            .into_iter()
            .filter(|at| cells || *at != FORMAT)
            .collect();
        if needed.iter().any(|at| given[*at].is_none()) {
            let at = self.arg_list_span(call);
            let (list, shape) = if cells {
                (
                    "a store, `name:`, `format:`, `from:` and `each:`",
                    format!(
                        "`{written}(store, name: \"<file>\", format: CsvFormat.spreadsheet, from: query(...), each: item -> Row {{ ... }})`"
                    ),
                )
            } else {
                (
                    "a store, `name:`, `from:` and `each:`",
                    format!(
                        "`{written}(store, name: \"<file>\", from: query(...), each: item -> Row {{ ... }})`"
                    ),
                )
            };
            let format_reason = if cells {
                " `format:` says who reads it: `CsvFormat.spreadsheet` writes a byte-order mark and escapes text a spreadsheet would read as a formula, `CsvFormat.standard` neither."
            } else {
                ""
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes {list}"),
                )
                .at(at, "missing")
                .reason(format!(
                    "`{written}` writes the items `from:` gives, a call of a named query giving a `Page`, into a file called `name:` in a store of files, each as the row `each:` gives;{format_reason} `finished:` and `key:` may follow."
                ))
                .next(format!("Write {shape}")),
            );
        }
        if let Some(store) = &given[STORE] {
            self.write_store(store, written);
        }
        // Text, saved with the run as an entity field holds it.
        if let Some(name) = &given[NAME] {
            self.built_on_argument(name, None, &[Prim::Text], written, "name");
        }
        if let Some(value) = &given[FORMAT] {
            self.check(value, &Ty::Named(jadpo_std::CsvFormat::NAME.to_string()));
        }
        let item = match &given[FROM] {
            Some(from) => self.batch_from(from, written),
            None => Ty::Error,
        };
        if let Some(each) = &given[EACH] {
            self.write_row(format, each, &item, written);
        }
        if let Some(finished) = &given[FINISHED] {
            self.batch_function(finished, &batch_type(EXPORT_SUMMARY), written, "finished");
        }
        if let Some(key) = &given[KEY] {
            let ty = self.infer(key);
            self.batch_saved(key, &ty, Saved::Key, written);
        }
        (
            batch_type(BATCH_RUN),
            CallKind::Effect(written.to_string()),
            Reach::Batch(BatchOp::Write(format)),
        )
    }

    /// A writer's store: the name of a connection that keeps files.
    fn write_store(&mut self, store: &Expr, written: &str) {
        let connections = crate::data::connections(self.db, self.project);
        let (shown, what) = match store {
            Expr::NameRef(name) => {
                let held = text(name);
                match connections.kind(&held) {
                    Some(crate::data::ConnectionKind::Files) => return,
                    Some(_) => (held, "a connection that keeps no files".to_string()),
                    None => {
                        let ty = self.infer(store);
                        if ty.has_error() {
                            return;
                        }
                        (held, format!("`{ty}`"))
                    }
                }
            }
            other => {
                let ty = self.infer(other);
                if ty.has_error() {
                    return;
                }
                (tree::written(other.syntax()), format!("`{ty}`"))
            }
        };
        let stores = connections.files();
        let span = self.span(store);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{shown}` isn't a store of files"),
            )
            .at(span, format!("this is {what}"))
            .reason(format!(
                "`{written}` writes its file into a store of files: a connection whose driver keeps files, named as `app.jadpo` declares it{}.",
                match stores.as_slice() {
                    [] => String::new(),
                    listed => format!(
                        ", here {}",
                        crate::and_list(
                            &listed.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>()
                        )
                    ),
                }
            ))
            .next(match stores.first() {
                Some(first) => format!("Write `{written}({first}, ...)`"),
                None => "Declare a connection with `driver: Driver.disk` in `app.jadpo`, and name it here".to_string(),
            }),
        );
    }

    /// A writer's `each:`: `item -> <expression>`, one inline function
    /// giving each item's row, an `Output` a row of `format` can hold;
    /// each local of the action it reads is saved with the run.
    fn write_row(&mut self, format: RowFormat, each: &Expr, item: &Ty, written: &str) {
        let Expr::LambdaExpr(lambda) = each else {
            self.infer(each);
            let span = self.span(each);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_BATCH_EACH_INVALID,
                    format!("`{written}`'s `each:` isn't an inline function giving the row"),
                )
                .at(span, "not an inline function")
                .reason(format!(
                    "`{written}` writes a row for each item, worked out by `each:` after the action that starts the run commits: it is written `item -> Row {{ ... }}`, one pure expression giving the item's row, an `Output`."
                ))
                .next("Write `each: item -> Row { ... }`, building the row from the item"),
            );
            return;
        };
        let first = self.facts.locals.len();
        let mut bound: BTreeMap<String, Ty> = BTreeMap::new();
        if !self.inline(
            lambda,
            std::slice::from_ref(item),
            &Ty::Var(ROW.to_string()),
            &mut bound,
            (written, "each"),
        ) {
            return;
        }
        // What it reads of the action, saved with the run: each local
        // declared before its parameter, once.
        if let Some(body) = lambda.expr() {
            let mut seen: BTreeSet<usize> = BTreeSet::new();
            for node in body.syntax().descendants() {
                let Some(local) = self.facts.reads.get(&node_key(&node)).copied() else {
                    continue;
                };
                if local >= first || !seen.insert(local) {
                    continue;
                }
                let (Some(value), Some(ty)) = (
                    Expr::cast(node.clone()),
                    self.facts.locals.get(local).map(|fact| fact.ty.clone()),
                ) else {
                    continue;
                };
                self.batch_saved(&value, &ty, Saved::Captured("each"), written);
            }
        }
        let Some(row) = bound.remove(ROW) else {
            return;
        };
        if row.has_error() {
            return;
        }
        self.row_written(format, lambda, &row, written);
    }

    /// The row type a writer's `each:` gives: an `Output`, made of values
    /// the wire carries, one value a cell and a column written once for
    /// CSV and TSV.
    fn row_written(
        &mut self,
        format: RowFormat,
        lambda: &ast::LambdaExpr,
        row: &Ty,
        written: &str,
    ) {
        let at = lambda
            .expr()
            .map_or_else(|| self.span(lambda), |body| self.span(&body));
        let refused = |body: &mut Self, label: String, reason: String, next: String| {
            body.out.push(
                Diagnostic::new(
                    &rule::TYPE_BATCH_ROW_INVALID,
                    format!("`{written}` can't write its rows as `{row}`"),
                )
                .at(at.clone(), label)
                .reason(reason)
                .next(next),
            );
        };
        let fields = match row.strip_nullable() {
            Ty::Named(name) => match self.table.def(name) {
                Some(Def::Fields {
                    base: FieldsBase::Output,
                    fields,
                    ..
                }) if !self.table.is_nullable(row) => Some(fields.clone()),
                _ => None,
            },
            _ => None,
        };
        let Some(fields) = fields else {
            refused(
                self,
                "not an `Output`".to_string(),
                format!(
                    "A row written to a file is an `Output`, written as a response's body is (area 7): its fields are its columns, in the order it declares them; `{row}` isn't one."
                ),
                "Declare the row as `type <Name> = Output { ... }`, and build it in `each:`"
                    .to_string(),
            );
            return;
        };
        if let Some((offending, what)) =
            no_json_form(self.table, row, &mut BTreeSet::new(), JsonUse::Write)
        {
            refused(
                self,
                format!("`{offending}` has no form a row holds"),
                format!(
                    "A row is written as an `Output` is to a response; `{offending}` is {what}."
                ),
                "Write the row as a type made of values a response carries".to_string(),
            );
            return;
        }
        if format.delimiter().is_none() {
            return;
        }
        for field in &fields {
            if let Some(what) = self.not_one_value(&field.declared) {
                let name = &field.name;
                refused(
                    self,
                    format!("`{row}.{name}` holds {what}"),
                    format!(
                        "A {} row holds one value a cell, and `{row}.{name}` holds {what}; a JSON Lines row holds lists and objects, each line one JSON value.",
                        format.namespace().to_uppercase()
                    ),
                    format!(
                        "Write the file with `json_lines.write`, or give `{name}` a type one cell holds"
                    ),
                );
                return;
            }
        }
        let mut columns: Vec<(String, String)> = Vec::new();
        for field in &fields {
            let column = field.column.clone().unwrap_or_else(|| field.name.clone());
            if let Some((_, first)) = columns.iter().find(|(seen, _)| *seen == column) {
                let (first, name) = (first.clone(), field.name.clone());
                refused(
                    self,
                    format!("two fields write column `{column}`"),
                    format!(
                        "Each of a row's fields is written to a column of its own, by its name or its `column:`, and `{row}.{first}` and `{row}.{name}` both write `{column}`."
                    ),
                    format!("Give `{name}` a `column:` of its own"),
                );
                return;
            }
            columns.push((column, field.name.clone()));
        }
    }
}

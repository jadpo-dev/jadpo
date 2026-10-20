//! A store's operations in a body (area 16, "Files"; chunk 6.4b):
//! `store.put(bytes, type)` keeps bytes in the `files` connection `store`
//! names and gives the `File`, failing `storage_full` when the store has no
//! room; `store.read(file)` gives a stored file's bytes whole, as
//! `FileContents`, for passing on. Both leave the process, so only an
//! action calls them, before its first read or write (area 6, "Outside
//! calls hold no transaction"), which the runtime holds it to as it runs.
//!
//! `store.read` reads the whole file into memory, so a field whose
//! `max_size:` is over 10 MB is refused where it is read
//! (`type.file_read_too_large`), pointing at the format helpers and
//! `batches.each`; a `File` whose field isn't known is held to the same
//! limit by the runtime.

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::files::{FILE, FILE_CONTENTS, FilesOp, WHOLE_READ_LIMIT, shown_size};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::ty::{Prim, Ty};
use super::facts::Reach;
use super::{Body, CallKind, Context, Fallible, node_key};

fn opaque(name: &str) -> Ty {
    Ty::Opaque(name.to_string(), Vec::new())
}

impl Body<'_> {
    /// `store.put(bytes, type)` or `store.read(file)`, through the `files`
    /// connection `store`.
    pub(super) fn files_call(
        &mut self,
        op: FilesOp,
        store: &str,
        call: &ast::CallExpr,
        callee: &Expr,
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
        let does = match op {
            FilesOp::Put => "keeps bytes in a store of files",
            FilesOp::Read => "reads a file's bytes from a store of files",
        };
        if self.context.is_pure() {
            let span = self.span(callee);
            let who = super::capital(self.context.with_a());
            self.out.push(
                Diagnostic::new(
                    &rule::EFFECT_FUNCTION_PERSISTENCE,
                    format!("{who} can't call `{written}`"),
                )
                .at(span, "a store of files")
                .reason(format!(
                    "{who} is pure, and `{written}` {does}, outside the program; only an action calls a store."
                ))
                .next(format!("Call `{written}` in an action, and pass in what it gives")),
            );
            return failed(self);
        }
        if self.context == Context::Test {
            let span = self.span(callee);
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_NOT_CALLABLE,
                    format!("A test can't call `{written}`"),
                )
                .at(span, "a store of files")
                .reason(format!(
                    "`{written}` {does}; a test reaches stored files through the application's actions, as it writes records through them (area 13)."
                ))
                .next("Call an action that does this"),
            );
            return failed(self);
        }
        let args = Self::arguments(call);
        let expected: &[(&str, Ty)] = match op {
            FilesOp::Put => &[
                ("bytes", Ty::Prim(Prim::Bytes)),
                ("type", Ty::Named(jadpo_std::FILE_TYPE.to_string())),
            ],
            FilesOp::Read => &[("file", Ty::Opaque(FILE.to_string(), Vec::new()))],
        };
        if args.len() != expected.len() || args.iter().any(|arg| arg.name().is_some()) {
            let at = self.arg_list_span(call);
            let takes = match op {
                FilesOp::Put => "the bytes and their `FileType`",
                FilesOp::Read => "the stored `File`",
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes {takes}"),
                )
                .at(at, format!("{} given", args.len()))
                .reason(format!(
                    "`{written}` takes {takes}, in order, without names."
                ))
                .next(match op {
                    FilesOp::Put => format!("Write `{written}(bytes, FileType.pdf)`"),
                    FilesOp::Read => format!("Write `{written}(file)`"),
                }),
            );
            return failed(self);
        }
        let mut file_ty = None;
        for (arg, (_, ty)) in args.iter().zip(expected) {
            let Some(value) = arg.expr() else { continue };
            match op {
                FilesOp::Read => {
                    let given = self.infer(&value);
                    self.require(&value, &given, ty);
                    file_ty = Some(given);
                }
                FilesOp::Put => self.check(&value, ty),
            }
        }
        let key = node_key(call.syntax());
        let result = match op {
            FilesOp::Put => {
                // A store with no room fails as the `attempt` maps it.
                self.facts.constructions.insert(
                    key,
                    Fallible::Construction(vec![
                        jadpo_std::RuleKey::StorageFull.word().to_string(),
                    ]),
                );
                opaque(FILE)
            }
            FilesOp::Read => {
                if let (Some(given), Some(value)) = (file_ty, args.first().and_then(ast::Arg::expr))
                {
                    self.whole_read(&given, &value, store, written);
                }
                opaque(FILE_CONTENTS)
            }
        };
        (
            result,
            CallKind::Effect(written.to_string()),
            Reach::Files(store.to_string(), op),
        )
    }

    /// `store.read` of a field whose `max_size:` is over what is read into
    /// memory whole: `type.file_read_too_large`.
    fn whole_read(&mut self, given: &Ty, value: &Expr, store: &str, written: &str) {
        let Ty::Field(host, path) = given.strip_nullable() else {
            return;
        };
        let [field] = path.as_slice() else {
            return;
        };
        let held = crate::data::entity_model(
            self.db,
            self.project,
            crate::lookups::Name::new(self.db, host.clone()),
        )
        .as_ref()
        .and_then(|entity| entity.fields.iter().find(|f| f.name == *field))
        .and_then(|f| f.file.clone());
        let Some(file) = held else {
            return;
        };
        if file.max_size <= WHOLE_READ_LIMIT {
            return;
        }
        let span = self.span(value);
        let size = shown_size(file.max_size);
        let limit = shown_size(WHOLE_READ_LIMIT);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_FILE_READ_TOO_LARGE,
                format!("`{written}` can't read `{host}.{field}` whole"),
            )
            .at(span, format!("holds up to {size}"))
            .reason(format!(
                "`{written}` reads a whole file into memory, for passing its bytes on, so it reads at most {limit}; `{host}.{field}` holds up to {size}. A file's rows of any size are read a page at a time by the format helpers (`csv.read`, `tsv.read`, `json_lines.read`) or walked with `batches.each`."
            ))
            .next(format!(
                "Read `{host}.{field}`'s rows with a format helper, such as `csv.read`, or keep its `max_size:` at {limit} or less if `{store}.read` is what it needs"
            )),
        );
    }
}

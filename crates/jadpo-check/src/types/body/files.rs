//! A store's operations in a body (area 16, "Files"; chunk 6.4b):
//! `store.put(bytes, type)` keeps bytes in the `files` connection `store`
//! names and gives the `File`, failing `storage_full` when the store has no
//! room; `store.read(file)` gives a stored file's bytes whole, as
//! `FileContents`, for passing on. Both leave the process, so only an
//! action calls them, before its first write (area 6, "Outside calls hold
//! no transaction"), which the runtime holds it to as it runs.
//!
//! `store.read` reads the whole file into memory, so a field whose
//! `max_size:` is over 10 MB is refused where it is read
//! (`type.file_read_too_large`), pointing at the format helpers and
//! `batches.each`; a `File` whose field isn't known is held to the same
//! limit by the runtime.

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::files::{
    FILE, FILE_CONTENTS, FIXTURE_FILE, FIXTURE_FOLDER, FilesOp, UPLOAD_SLOT, WHOLE_READ_LIMIT,
    fixture_type, is_fixture_name, shown_size,
};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::ty::{Prim, Ty};
use super::facts::Reach;
use super::{Body, CallKind, Context, Fallible, node_key};

fn opaque(name: &str) -> Ty {
    Ty::Opaque(name.to_string(), Vec::new())
}

impl Body<'_> {
    /// `Entity.field_upload()` (area 16, "Uploading"): a slot for one
    /// `File` field. It takes nothing, and who may ask is the entity's
    /// `create` and `update` grants, read where the routes are checked. A
    /// field over 100 MB is given a slot in parts, which the same type
    /// holds. Its type.
    pub(super) fn upload_slot(
        &mut self,
        entity: &str,
        field: &str,
        call: &ast::CallExpr,
        block: Option<ast::MemberList>,
        span: jadpo_diag::Span,
    ) -> Ty {
        let written = format!("{entity}.{field}_upload");
        let model = crate::data::data_model(self.db, self.project)
            .entity(entity)
            .and_then(|model| {
                model
                    .fields
                    .iter()
                    .find(|candidate| candidate.name == field)
            })
            .filter(|candidate| candidate.file.is_some());
        if model.is_none() {
            self.args_only(call);
            self.walk(|walk| walk.member_values(block));
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_UNKNOWN_CALLEE,
                    format!("`{entity}` has no operation `{field}_upload`"),
                )
                .at(span, "not an operation")
                .reason(format!(
                    "`<field>_upload` is the operation generated for each `File` field of an entity. `{entity}` has no `File` field `{field}`."
                ))
                .next(format!("Name a `File` field of `{entity}`")),
            );
            return Ty::Error;
        }
        self.args_only(call);
        self.walk(|walk| walk.member_values(block.clone()));
        let args = Self::arguments(call);
        if !args.is_empty() || block.is_some() {
            let at = self.arg_list_span(call);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes nothing"),
                )
                .at(at, "takes nothing")
                .reason(format!(
                    "`{written}` gives a slot for the field `{field}`: its settings say what the slot accepts."
                ))
                .next(format!("Write `{written}()`")),
            );
            return Ty::Error;
        }
        opaque(UPLOAD_SLOT)
    }

    /// `fixture_file("customers.csv", store: imports)` in a test (area 13,
    /// "Files"): the file of that name in the test's domain's
    /// `fixtures/files/`, kept in the test's in-memory store for the
    /// `files` connection `store:` names, as a `File`. Its type is what its
    /// name ends in.
    pub(super) fn fixture_file(
        &mut self,
        call: &ast::CallExpr,
        callee: &Expr,
    ) -> (Ty, CallKind, Reach) {
        let failed = (
            Ty::Error,
            CallKind::Effect(FIXTURE_FILE.to_string()),
            Reach::Unknown,
        );
        if !self.test {
            let span = self.span(callee);
            let who = super::capital(self.context.with_a());
            self.out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_NOT_CALLABLE,
                    format!("{who} can't call `{FIXTURE_FILE}`"),
                )
                .at(span, "only in a test")
                .reason(format!(
                    "`{FIXTURE_FILE}` keeps a file from a domain's `{FIXTURE_FOLDER}/` in a test's in-memory store of files; only a test has one, and a running app reads its files from its stores."
                ))
                .next(format!("Call `{FIXTURE_FILE}` in a test, and pass the `File` it gives to an action")),
            );
            return failed;
        }
        let args = Self::arguments(call);
        let (named, store) = match args.as_slice() {
            [first, second]
                if first.name().is_none() && Self::arg_name(second).as_deref() == Some("store") =>
            {
                (first.expr(), second.expr())
            }
            _ => {
                let at = self.arg_list_span(call);
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_ARGUMENT_COUNT,
                        format!("`{FIXTURE_FILE}` takes a file's name and `store:`"),
                    )
                    .at(at, format!("{} given", args.len()))
                    .reason(format!(
                        "`{FIXTURE_FILE}(\"<file>\", store: <store>)` keeps the file of that name in the test's domain's `{FIXTURE_FOLDER}/` in the store `store:` names."
                    ))
                    .next(format!("Write `{FIXTURE_FILE}(\"<file>\", store: <store>)`")),
                );
                return failed;
            }
        };
        let (Some(named), Some(store)) = (named, store) else {
            return failed;
        };
        self.write_store(&store, FIXTURE_FILE);
        let store_name = match &store {
            Expr::NameRef(name) => super::text(name),
            _ => return failed,
        };
        if crate::data::connections(self.db, self.project).kind(&store_name)
            != Some(crate::data::ConnectionKind::Files)
        {
            return failed;
        }
        let span = self.span(&named);
        let Some(name) = crate::emails::literal_text(&named) else {
            self.infer(&named);
            self.out.push(
                fixture_invalid(
                    format!("`{FIXTURE_FILE}` takes the file's name written out"),
                    span,
                    "not written out",
                )
                .reason(format!(
                    "The file is found as the test is checked, so its name is text written in the call: `{FIXTURE_FILE}(\"<file>\", store: <store>)`."
                ))
                .next("Write the file's name in quotes"),
            );
            return failed;
        };
        let Some(domain) = self.ownership.domain().map(str::to_string) else {
            self.out.push(
                fixture_invalid(
                    format!("A test in `tests/` has no `{FIXTURE_FOLDER}/`"),
                    span,
                    "no domain",
                )
                .reason(format!(
                    "Fixture files are kept beside the tests that use them, in a domain's `{FIXTURE_FOLDER}/`, and a test in the root `tests/` belongs to no domain."
                ))
                .next("Move the test beside its subject in a domain, and the file into that domain's `fixtures/files/`"),
            );
            return failed;
        };
        let folder = format!("domains/{domain}/{FIXTURE_FOLDER}");
        if !is_fixture_name(&name) {
            self.out.push(
                fixture_invalid(
                    format!("`{name}` can't name a fixture file"),
                    span,
                    "not a file's name",
                )
                .reason(format!(
                    "A fixture file is named by its path below `{folder}/`, each part written out, none of them `.`, `..` or starting with a dot."
                ))
                .next(format!("Name a file in `{folder}/`, such as `\"<file>.csv\"`")),
            );
            return failed;
        }
        let path = format!("{folder}/{name}");
        let held: Vec<String> = self
            .project
            .other_files(self.db)
            .iter()
            .filter_map(|other| other.strip_prefix(&format!("{folder}/")))
            .map(str::to_string)
            .collect();
        if !held.contains(&name) {
            let near = jadpo_diag::nearest(&name, held.iter().map(String::as_str));
            let mut problem = fixture_invalid(
                format!("`{folder}/` holds no `{name}`"),
                span.clone(),
                "no such file",
            )
            .reason(format!(
                "`{FIXTURE_FILE}` reads a file from the test's own domain's `{FIXTURE_FOLDER}/`, which {}.",
                match held.as_slice() {
                    [] => "holds no file".to_string(),
                    listed => format!(
                        "holds {}",
                        crate::and_list(
                            &listed.iter().map(|file| format!("`{file}`")).collect::<Vec<_>>()
                        )
                    ),
                }
            ));
            problem = match near {
                Some(near) => problem
                    .next(format!("Use `\"{near}\"` instead"))
                    .edit(span, format!("\"{near}\"")),
                None => problem.next(format!("Put the file at `{path}`")),
            };
            self.out.push(problem);
            return failed;
        }
        let Some(file_type) = fixture_type(&name) else {
            self.out.push(
                fixture_invalid(
                    format!("`{name}` isn't a file of a type a `File` holds"),
                    span,
                    "no `FileType`",
                )
                .reason(
                    "A fixture file's `FileType` is what its name ends in: `.csv`, `.tsv`, `.json`, `.jsonl`, `.pdf`, `.png`, `.jpeg` or `.jpg`, or `.webp`.",
                )
                .next("Rename the file to end in its type"),
            );
            return failed;
        };
        (
            opaque(FILE),
            CallKind::Effect(FIXTURE_FILE.to_string()),
            Reach::FixtureFile {
                store: store_name,
                path,
                file_type: file_type.to_string(),
                // The file's own name, without a folder it is in.
                name: name.rsplit('/').next().unwrap_or(&name).to_string(),
            },
        )
    }

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
                "`{written}` reads a whole file into memory, for passing its bytes on, so it reads at most {limit}; `{host}.{field}` holds up to {size}. A file's rows, of any size, are read a batch at a time by the format helpers (`csv.read`, `tsv.read`, `json_lines.read`)."
            ))
            .next(format!(
                "Read `{host}.{field}`'s rows with a format helper, such as `csv.read`, or keep its `max_size:` at {limit} or less if `{store}.read` is what it needs"
            )),
        );
    }
}

fn fixture_invalid(headline: String, span: jadpo_diag::Span, label: &str) -> Diagnostic {
    Diagnostic::new(&rule::SEMANTIC_FIXTURE_FILE_INVALID, headline).at(span, label.to_string())
}

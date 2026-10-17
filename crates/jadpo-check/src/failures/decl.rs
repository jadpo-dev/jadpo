//! Area 4's rules on `type X = Failure { ... }`: the block holds `kind`,
//! `code`, `message`, `client` and `internal`; the kind is one an
//! application may declare; the code is lower_snake_case and belongs to
//! this failure alone; and a field is either sent to the client or kept
//! for the log, never both. The fields themselves follow area 2's rules,
//! checked with the other type declarations.

use std::collections::BTreeMap;

use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Decl, MemberItem};

use super::table::is_failure_decl;
use super::{DECLARABLE_KINDS, MEMBERS, RUNTIME_CODES};
use crate::case::{is_lower_snake, to_snake};
use crate::lookups::{FailureView, Lookup};
use crate::std_lib::StdNames;
use crate::tree::{self, node_span};
use crate::types::Const;

/// The problems with one file's failure declarations.
#[salsa::tracked(returns(ref))]
pub fn failure_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let lookup = Lookup::for_file(db, project, file);
    let mut checker = Checker {
        path: file.path(db),
        table: &lookup,
        out: Vec::new(),
    };
    for (index, decl) in source.decls().enumerate() {
        if let Decl::TypeDecl(decl) = decl
            && is_failure_decl(&decl)
        {
            checker.failure(&decl, index as u32);
        }
    }
    checker.out
}

struct Checker<'a> {
    path: &'a str,
    /// The declared failures, read name by name and code by code.
    table: &'a Lookup<'a>,
    out: Vec<Diagnostic>,
}

fn did_you_mean(near: Option<&str>) -> String {
    near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
}

fn quoted(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| format!("`{item}`")).collect()
}

/// A code suggested for one that isn't lower_snake_case.
fn code_suggestion(code: &str) -> String {
    let spaced: String = code
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    to_snake(&spaced)
}

impl Checker<'_> {
    fn span(&self, node: &impl AstNode) -> Span {
        node_span(self.path, node.syntax())
    }

    fn failure(&mut self, decl: &ast::TypeDecl, index: u32) {
        let Some(name_node) = decl.name() else { return };
        let Some(name) = tree::name_text(&name_node) else {
            return;
        };
        let name_span = self.span(&name_node);
        let mut seen: BTreeMap<String, Span> = BTreeMap::new();
        let mut sections: Vec<(String, ast::Member)> = Vec::new();
        let mut code_span: Option<Span> = None;
        for item in decl
            .member_list()
            .iter()
            .flat_map(|list| list.member_items())
        {
            let (key_node, member) = match item {
                MemberItem::Member(member) => {
                    let mut names = member.member_key().into_iter().flat_map(|k| k.names());
                    let (Some(first), None) = (names.next(), names.next()) else {
                        continue;
                    };
                    (first, Some(member))
                }
                MemberItem::BareMember(bare) => match bare.name() {
                    Some(name) => (name, None),
                    None => continue,
                },
                _ => continue,
            };
            let Some(key) = tree::name_text(&key_node) else {
                continue;
            };
            let key_span = self.span(&key_node);
            if !MEMBERS.contains(&key.as_str()) {
                self.unknown_member(&key, key_span);
                continue;
            }
            if let Some(first) = seen.get(&key) {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_CONSTRAINT_REPEATED,
                        format!("`{key}` is given twice"),
                    )
                    .at(key_span, "given again")
                    .also(first.clone(), "first given here")
                    .reason("Each setting is given once, so there is never a question of which one counts.")
                    .next(format!("Remove one `{key}`")),
                );
                continue;
            }
            seen.insert(key.clone(), key_span.clone());
            let Some(member) = member else { continue };
            match key.as_str() {
                "kind" => self.kind(&name, &member),
                "code" => {
                    if self.code(&name, &member) {
                        code_span = member.value().map(|v| self.span(&v));
                    }
                }
                "message" => self.message(&member),
                _ => {
                    if member.block().is_some() && member.value().is_none() {
                        sections.push((key, member));
                    } else {
                        self.not_a_block(&key, &member);
                    }
                }
            }
        }
        if !seen.contains_key("kind") {
            self.out.push(
                Diagnostic::new(&rule::FAILURE_KIND_REQUIRED, format!("`{name}` has no `kind`"))
                    .at(name_span.clone(), "a failure without a kind")
                    .reason("A failure's kind decides how it reaches a client, its HTTP status included, and how background work treats it, so every failure names one.")
                    .next("Add `kind:` with the `FailureKind` that fits, such as `kind: FailureKind.not_found`"),
            );
        }
        self.overlap(&sections);
        self.duplicate_code(&name, index, code_span.unwrap_or(name_span));
    }

    fn unknown_member(&mut self, key: &str, span: Span) {
        let near = nearest(key, MEMBERS.iter().copied());
        let problem = Diagnostic::new(
            &rule::TYPE_UNKNOWN_SETTING,
            format!("`{key}` isn't a setting Jadpo knows"),
        )
        .at(span.clone(), "unknown setting")
        .reason(format!(
            "A failure's block holds {}; its kind decides the HTTP status.{}",
            crate::and_list(&quoted(&MEMBERS)),
            did_you_mean(near)
        ));
        self.out.push(match near {
            Some(near) => problem
                .next(format!("Use `{near}` instead of `{key}`"))
                .edit(span, near),
            None => problem.next(format!("Remove `{key}`")),
        });
    }

    fn kind(&mut self, name: &str, member: &ast::Member) {
        let Some(value) = member.value() else {
            self.not_a_value("kind", member);
            return;
        };
        let span = self.span(&value);
        let variants = StdNames::get()
            .enums()
            .get("FailureKind")
            .cloned()
            .unwrap_or_default();
        let written = Const::from_expr(&value);
        let path = match &written {
            Const::Path(path) => path.clone(),
            _ => Vec::new(),
        };
        if let [host, variant] = path.as_slice()
            && host == "FailureKind"
        {
            if !variants.contains(variant) {
                let near =
                    nearest(variant, variants.iter().map(String::as_str)).map(str::to_string);
                let problem = Diagnostic::new(
                    &rule::TYPE_UNKNOWN_ENUM_VARIANT,
                    format!("`FailureKind` has no variant `{variant}`"),
                )
                .at(span.clone(), "not a variant")
                .reason(format!(
                    "The kinds an application declares are {}.{}",
                    crate::and_list(&quoted(&DECLARABLE_KINDS)),
                    did_you_mean(near.as_deref())
                ));
                self.out.push(match near {
                    Some(near) => problem
                        .next(format!("Use `FailureKind.{near}`"))
                        .edit(span, format!("FailureKind.{near}")),
                    None => problem.next("Use one of the kinds an application declares"),
                });
                return;
            }
            if !DECLARABLE_KINDS.contains(&variant.as_str()) {
                self.out.push(
                    Diagnostic::new(
                        &rule::FAILURE_KIND_RUNTIME_ONLY,
                        format!("`{name}` can't have the kind `{variant}`"),
                    )
                    .at(span, "raised only by the runtime")
                    .reason(format!(
                        "The runtime raises `{variant}` itself; an application declares {}, so a client's status always means the same thing.",
                        crate::and_list(&quoted(&DECLARABLE_KINDS))
                    ))
                    .next("Use the kind an application declares that fits this outcome"),
                );
            }
            return;
        }
        // `kind: NotFound` or `kind: not_found`: a kind written without
        // its enum.
        let guess = match path.as_slice() {
            [only] => Some(to_snake(only)).filter(|v| variants.contains(v)),
            _ => None,
        };
        let problem = Diagnostic::new(&rule::TYPE_SETTING_VALUE, "`kind` takes a `FailureKind` value")
            .at(span.clone(), format!("{} here", written.describe()))
            .reason("A failure's kind is one of `FailureKind`'s values, written with the enum's name, such as `FailureKind.not_found`.");
        self.out.push(match guess {
            Some(variant) => problem
                .next(format!("Write `FailureKind.{variant}`"))
                .edit(span, format!("FailureKind.{variant}")),
            None => problem.next("Write a `FailureKind` value, such as `FailureKind.not_found`"),
        });
    }

    /// `code:`; whether it is a usable code.
    fn code(&mut self, name: &str, member: &ast::Member) -> bool {
        let Some(value) = member.value() else {
            self.not_a_value("code", member);
            return false;
        };
        let span = self.span(&value);
        let Const::Str(code) = Const::from_expr(&value) else {
            self.out.push(
                Diagnostic::new(&rule::TYPE_SETTING_VALUE, "`code` takes text in quotes")
                    .at(span, "not text")
                    .reason(format!(
                        "A failure's code is the text clients branch on; without `code:` it is the failure's name in lower_snake_case, `{}`.",
                        to_snake(name)
                    ))
                    .next(format!("Remove `code:` to use `{}`", to_snake(name))),
            );
            return false;
        };
        if is_lower_snake(&code) {
            return true;
        }
        let problem = Diagnostic::new(
            &rule::FAILURE_CODE_FORMAT,
            if code.is_empty() {
                "This failure's code is empty".to_string()
            } else {
                format!("The code `{code}` isn't lower_snake_case")
            },
        )
        .at(span.clone(), "not lower_snake_case")
        .reason("A failure's code is what clients branch on and translate by, so it is lower_snake_case, like a failure's name turned into one.");
        let suggestion = code_suggestion(&code);
        self.out.push(if suggestion.is_empty() {
            problem.next(format!("Remove `code:` to use `{}`", to_snake(name)))
        } else {
            problem
                .next(format!("Write `\"{suggestion}\"`"))
                .edit(span, format!("\"{suggestion}\""))
        });
        false
    }

    fn message(&mut self, member: &ast::Member) {
        let Some(value) = member.value() else {
            self.not_a_value("message", member);
            return;
        };
        if matches!(Const::from_expr(&value), Const::Str(text) if !text.is_empty()) {
            return;
        }
        let span = self.span(&value);
        self.out.push(
            Diagnostic::new(&rule::TYPE_SETTING_VALUE, "`message` takes fixed text in quotes")
                .at(span, "not fixed text")
                .reason("A failure's message is fixed, safe text for developers and logs; it can't include values, and when it is left out the kind's generic message is used.")
                .next("Write the message as text in quotes, or remove `message:`"),
        );
    }

    fn not_a_value(&mut self, key: &str, member: &ast::Member) {
        let span = self.span(member);
        self.out.push(
            Diagnostic::new(&rule::TYPE_SETTING_VALUE, format!("`{key}` takes one value"))
                .at(span, "a block")
                .reason(format!(
                    "`{key}:` is written with one value; only `client:` and `internal:` hold blocks of fields."
                ))
                .next(format!("Write one value after `{key}:`")),
        );
    }

    fn not_a_block(&mut self, key: &str, member: &ast::Member) {
        let span = member
            .value()
            .map_or_else(|| self.span(member), |v| self.span(&v));
        self.out.push(
            Diagnostic::new(&rule::TYPE_SETTING_VALUE, format!("`{key}` takes a block of fields"))
                .at(span, "not a block")
                .reason(format!(
                    "A failure's fields sit in `client: {{ }}`, sent to the caller, and `internal: {{ }}`, kept for the log; `{key}:` holds one field per line."
                ))
                .next(format!("Write `{key}: {{ name: Type }}`")),
        );
    }

    /// A field in both `client` and `internal`.
    fn overlap(&mut self, sections: &[(String, ast::Member)]) {
        let mut first: BTreeMap<String, (String, Span)> = BTreeMap::new();
        for (section, member) in sections {
            for (field, _) in tree::members(member.block()) {
                let Some(text) = tree::name_text(&field) else {
                    continue;
                };
                let span = self.span(&field);
                match first.get(&text) {
                    Some((other, earlier)) if other != section => {
                        self.out.push(
                            Diagnostic::new(
                                &rule::FAILURE_CONTEXT_FIELD_OVERLAP,
                                format!("`{text}` is in both `{other}` and `{section}`"),
                            )
                            .at(span, "declared again")
                            .also(earlier.clone(), "first declared here")
                            .reason("`client` fields are sent to the caller and `internal` fields never leave the server, so a field is one or the other.")
                            .next(format!("Keep `{text}` in one of the two blocks")),
                        );
                    }
                    Some(_) => {}
                    None => {
                        first.insert(text, (section.clone(), span));
                    }
                }
            }
        }
    }

    fn duplicate_code(&mut self, name: &str, index: u32, span: Span) {
        let Some(info) = self.table.failure_info(name) else {
            return;
        };
        if info.path != self.path || info.index != index {
            // A second declaration of the name: resolution reports it.
            return;
        }
        let code = info.code.clone();
        if RUNTIME_CODES.contains(&code.as_str()) {
            self.out.push(
                Diagnostic::new(
                    &rule::FAILURE_DUPLICATE_CODE,
                    format!("The code `{code}` belongs to the runtime"),
                )
                .at(span, "a runtime code")
                .reason(format!(
                    "The runtime answers with `{code}` itself, so a client seeing it could not tell which failure it got; every code belongs to one failure.{}",
                    if info.explicit_code {
                        String::new()
                    } else {
                        format!(" `{name}`'s code comes from its name.")
                    }
                ))
                .next(if info.explicit_code {
                    "Give this failure a code of its own".to_string()
                } else {
                    "Rename the failure, or give it a code of its own with `code:`".to_string()
                }),
            );
            return;
        }
        let Some(first) = self.table.failures_with_code(&code).first() else {
            return;
        };
        if first.name == name {
            return;
        }
        let place = if first.path == self.path {
            String::new()
        } else {
            format!(" in `{}`", first.path)
        };
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_DUPLICATE_CODE,
                format!("The code `{code}` is already used by `{}`", first.name),
            )
            .at(span, "the same code")
            .reason(format!(
                "`{}`{place} has the code `{code}` too. A client relies on a code always meaning the same failure, with the same status, message and fields, so every code belongs to one failure.{}",
                first.name,
                if info.explicit_code {
                    String::new()
                } else {
                    format!(" `{name}`'s code comes from its name.")
                }
            ))
            .next("Give one of them a code of its own, or use one failure for both"),
        );
    }
}

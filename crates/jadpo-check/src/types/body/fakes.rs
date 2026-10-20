//! A test's `fakes:` (area 13, "Fakes for integrations"; area 11, "Tests
//! and fakes"; chunk 7.2d): the answers a provider operation gives the
//! test, in order, each a value of the operation's output type, one of its
//! package failures by its code, or a condition the runtime raises in the
//! provider's place. Every call the test reaches answers through the
//! package's fake: with the next scripted answer, or with the operation's
//! generated default when the test scripts none.
//!
//! The compiler checks every scripted answer against the package, so a
//! fake can't give an answer the provider never would: a value is checked
//! as the operation's output type, and a name must be one of its failures'
//! codes or a condition.

use jadpo_diag::{Diagnostic, nearest, rule};
use jadpo_graph::body::{FakeAnswer, FakeCondition};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::ty::{Prim, Ty};
use super::Body;
use super::facts::node_key;

/// What one scripted answer is, for the typed body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FakeFact {
    pub connection: String,
    /// `pickups.create`.
    pub operation: String,
    pub answer: FakeAnswer,
}

impl Body<'_> {
    /// A test's `fakes: { connection.resource.operation: [answers] }`.
    pub(super) fn fakes(&mut self, member: &ast::Member) {
        let key_span = member
            .member_key()
            .map_or_else(|| self.span(member), |key| self.span(&key));
        let Some(list) = member.block() else {
            self.out.push(
                invalid(
                    "`fakes:` lists each provider operation's answers in a block".to_string(),
                    key_span,
                    "not a block",
                )
                .reason("A test scripts the answers of the provider operations it reaches, each written `<connection>.<resource>.<operation>: [<answer>, ...]`.")
                .next("Write `fakes: { couriers.pickups.create: [slot_taken] }`"),
            );
            return;
        };
        let mut seen: Vec<String> = Vec::new();
        for item in list.member_items() {
            let ast::MemberItem::Member(entry) = item else {
                continue;
            };
            let Some(key) = entry.member_key() else {
                continue;
            };
            let names: Vec<String> = key
                .names()
                .filter_map(|name| crate::tree::name_text(&name))
                .collect();
            let written = names.join(".");
            let span = self.span(&key);
            let [connection, resource, name] = names.as_slice() else {
                self.out.push(
                    invalid(
                        format!("`{written}` isn't a provider operation"),
                        span,
                        "not `connection.resource.operation`",
                    )
                    .reason("`fakes:` scripts a provider operation by its connection, resource and operation, as the code calls it: `couriers.pickups.create`.")
                    .next("Name the operation as the action calls it"),
                );
                continue;
            };
            let connections = crate::connections::package_connections(self.db, self.project);
            let Some(found) = connections
                .connections
                .iter()
                .find(|known| known.name == *connection)
            else {
                let known: Vec<String> = connections
                    .connections
                    .iter()
                    .map(|known| known.name.clone())
                    .collect();
                let near = nearest(connection, known.iter().map(String::as_str));
                let mut problem = invalid(
                    format!("`{connection}` isn't a connection to a provider"),
                    span,
                    "no such connection",
                )
                .reason(format!(
                    "`fakes:` scripts the operations of the app's connections to provider packages{}.",
                    if known.is_empty() {
                        ", and the app has none".to_string()
                    } else {
                        format!(
                            ": {}",
                            crate::and_list(
                                &known
                                    .iter()
                                    .map(|name| format!("`{name}`"))
                                    .collect::<Vec<_>>()
                            )
                        )
                    }
                ));
                problem = match near {
                    Some(near) => problem.next(format!("Use `{near}` instead of `{connection}`")),
                    None => problem.next(format!("Remove `{written}:`")),
                };
                self.out.push(problem);
                continue;
            };
            let operations = crate::providers::operations(self.db, self.project);
            let Some(operation) = operations
                .find(&found.folder, resource, name)
                .filter(|operation| operation.public)
                .cloned()
            else {
                if crate::providers::has_problems(self.db, self.project, &found.folder) {
                    continue;
                }
                let known: Vec<String> = operations
                    .of(&found.folder)
                    .filter(|operation| operation.public)
                    .map(crate::providers::Operation::full_name)
                    .collect();
                let asked = format!("{resource}.{name}");
                let near = nearest(&asked, known.iter().map(String::as_str));
                let mut problem = invalid(
                    format!("`{connection}` has no operation `{asked}`"),
                    span,
                    "no such operation",
                )
                .reason(format!(
                    "`fakes:` scripts the public operations of `{}`{}.",
                    found.package,
                    if known.is_empty() {
                        ", and it has none".to_string()
                    } else {
                        format!(
                            ": {}",
                            crate::and_list(
                                &known
                                    .iter()
                                    .map(|name| format!("`{name}`"))
                                    .collect::<Vec<_>>()
                            )
                        )
                    }
                ));
                problem = match near {
                    Some(near) => problem.next(format!("Use `{connection}.{near}` instead")),
                    None => problem.next(format!("Remove `{written}:`")),
                };
                self.out.push(problem);
                continue;
            };
            if seen.contains(&written) {
                self.out.push(
                    invalid(format!("`{written}` is scripted twice"), span, "scripted again")
                        .reason("A test lists each operation's answers once, in the order the calls take them.")
                        .next(format!("Put every answer of `{written}` in one list")),
                );
                continue;
            }
            seen.push(written.clone());
            let Some(Expr::ListExpr(answers)) = entry.value() else {
                let at = entry
                    .value()
                    .map_or_else(|| span.clone(), |value| self.span(&value));
                self.out.push(
                    invalid(
                        format!("`{written}`'s answers aren't a list"),
                        at,
                        "not a list",
                    )
                    .reason(
                        "A fake's answers are a list, taken in order by the calls the test makes.",
                    )
                    .next(format!("Write `{written}: [<answer>]`")),
                );
                continue;
            };
            let items: Vec<Expr> = answers.exprs().collect();
            if items.is_empty() {
                self.out.push(
                    invalid(
                        format!("`{written}` is scripted with no answers"),
                        self.span(&answers),
                        "no answers",
                    )
                    .reason("An operation the test doesn't script answers with its generated default, so an empty list says nothing.")
                    .next(format!("Remove `{written}:`, or list the answers its calls take")),
                );
                continue;
            }
            // The codes of its failures, and the failure each names.
            let codes: Vec<String> = operation
                .failures
                .iter()
                .filter_map(|failure| {
                    self.failures
                        .failure_info(failure)
                        .map(|info| info.code.clone())
                })
                .collect();
            let gives = !matches!(operation.output, Ty::Prim(Prim::Unit));
            for answer in items {
                let fact = |answer: FakeAnswer| FakeFact {
                    connection: connection.clone(),
                    operation: operation.full_name(),
                    answer,
                };
                if let Expr::NameRef(named) = &answer {
                    let word = super::text(named);
                    let given = if codes.contains(&word) {
                        Some(FakeAnswer::Failed(word.clone()))
                    } else {
                        FakeCondition::named(&word).map(FakeAnswer::Condition)
                    };
                    match given {
                        Some(given) => {
                            self.facts
                                .fakes
                                .insert(node_key(answer.syntax()), fact(given));
                        }
                        None => {
                            let mut allowed: Vec<String> = codes.clone();
                            allowed.extend(
                                FakeCondition::ALL
                                    .iter()
                                    .map(|condition| condition.word().to_string()),
                            );
                            let near = nearest(&word, allowed.iter().map(String::as_str));
                            let mut problem = invalid(
                                format!("`{word}` isn't an answer `{written}` gives"),
                                self.span(&answer),
                                "not a failure or a condition",
                            )
                            .reason(format!(
                                "A scripted answer is {}one of `{written}`'s failures by its code{}, or a condition: `unavailable`, `lost`, `in_use` or `misconfigured`.",
                                if gives {
                                    format!("a `{}`, ", operation.output)
                                } else {
                                    String::new()
                                },
                                if codes.is_empty() {
                                    ", and it declares none".to_string()
                                } else {
                                    format!(
                                        " ({})",
                                        crate::and_list(
                                            &codes
                                                .iter()
                                                .map(|code| format!("`{code}`"))
                                                .collect::<Vec<_>>()
                                        )
                                    )
                                }
                            ));
                            problem = match near {
                                Some(near) => problem
                                    .next(format!("Use `{near}` instead of `{word}`"))
                                    .edit(self.span(&answer), near),
                                None => problem.next(format!("Remove `{word}`")),
                            };
                            self.out.push(problem);
                        }
                    }
                    continue;
                }
                if !gives {
                    self.out.push(
                        invalid(
                            format!("`{written}` gives nothing, and this answer is a value"),
                            self.span(&answer),
                            "a value",
                        )
                        .reason("An operation without `output:` gives nothing, so its fake answers only with one of its failures or a condition.")
                        .next("Script one of its failures by its code, or a condition"),
                    );
                    continue;
                }
                self.check(&answer, &operation.output);
                self.facts
                    .fakes
                    .insert(node_key(answer.syntax()), fact(FakeAnswer::Value));
            }
        }
    }
}

fn invalid(headline: String, span: jadpo_diag::Span, label: &str) -> Diagnostic {
    Diagnostic::new(&rule::SEMANTIC_FAKE_INVALID, headline).at(span, label.to_string())
}

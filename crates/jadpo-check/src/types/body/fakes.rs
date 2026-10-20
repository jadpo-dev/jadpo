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
use jadpo_graph::body::{AiFakeAnswer, FakeAnswer, FakeCondition};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::ty::{Prim, Ty};
use super::Body;
use super::facts::{NodeKey, node_key};

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
                .next("Write `fakes: { <connection>.<resource>.<operation>: [<answer>] }`"),
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
            // A model's answers to a prompt (area 21, "Tests"; chunk
            // 5.5v3).
            if let [head, prompt] = names.as_slice()
                && head == jadpo_std::standard::ai::AI
            {
                if seen.contains(&written) {
                    self.out.push(
                        invalid(format!("`{written}` is scripted twice"), span, "scripted again")
                            .reason("A test lists each prompt's answers once, in the order the model is asked.")
                            .next(format!("Put every answer of `{written}` in one list")),
                    );
                    continue;
                }
                seen.push(written.clone());
                self.ai_fakes(&entry, prompt, &written, span);
                continue;
            }
            let [connection, resource, name] = names.as_slice() else {
                self.out.push(
                    invalid(
                        format!("`{written}` isn't a provider operation"),
                        span,
                        "not `connection.resource.operation`",
                    )
                    .reason("`fakes:` scripts a provider operation by its connection, resource and operation, as the code calls it: `<connection>.<resource>.<operation>`.")
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
            // A package's actions call its operations whether public or
            // not, so a test scripts those too (chunk 7.6).
            let acts = crate::package_code::actions(self.db, self.project)
                .iter()
                .any(|action| action.folder == found.folder);
            let Some(operation) = operations
                .find(&found.folder, resource, name)
                .filter(|operation| operation.public || acts)
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
                                    format!("a value of `{}`, ", operation.output)
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

impl Body<'_> {
    /// Resolves the names a test's `fakes:` uses: a model's scripted
    /// answers' `raw(...)` and `recorded(...)` are the script's own words,
    /// so only what they are given is read.
    pub(super) fn walk_fakes(&mut self, member: &ast::Member) {
        for item in member
            .block()
            .iter()
            .flat_map(ast::MemberList::member_items)
        {
            let ast::MemberItem::Member(entry) = item else {
                continue;
            };
            let model = entry.member_key().is_some_and(|key| {
                key.names()
                    .next()
                    .and_then(|name| crate::tree::name_text(&name))
                    .as_deref()
                    == Some(jadpo_std::standard::ai::AI)
            });
            let (true, Some(Expr::ListExpr(answers))) = (model, entry.value()) else {
                self.walk(|walk| walk.member(&entry));
                continue;
            };
            for answer in answers.exprs() {
                let own = match &answer {
                    Expr::CallExpr(call) => matches!(
                        call.expr(),
                        Some(Expr::NameRef(name)) if matches!(super::text(&name).as_str(), "raw" | "recorded" | "partial")
                    ),
                    _ => false,
                };
                if let (true, Expr::CallExpr(call)) = (own, &answer) {
                    // `partial(...)`'s second is a condition's word.
                    for arg in Self::arguments(call).into_iter().take(1) {
                        if let Some(value) = arg.expr() {
                            self.walk(|walk| walk.expr(&value));
                        }
                    }
                } else {
                    self.walk(|walk| walk.expr(&answer));
                }
            }
        }
    }

    /// `fakes: { ai.<prompt>: [answers] }` (area 21, "Tests"; chunk
    /// 5.5v3): each answer one attempt of an `ai.generate` call of the
    /// prompt takes, in order: a value of the type the call's `as:` names,
    /// checked as one; `raw("...")`, text the model answers with, run
    /// through the real decoding and repair; or one of the call's five
    /// failures, `malformed` an answer that doesn't decode.
    fn ai_fakes(
        &mut self,
        entry: &ast::Member,
        prompt: &str,
        written: &str,
        span: jadpo_diag::Span,
    ) {
        if self.prompt_type(prompt).is_none() {
            let known: Vec<String> = crate::prompts::analysis(self.db, self.project)
                .prompts
                .iter()
                .map(|found| found.key.clone())
                .collect();
            let near = nearest(prompt, known.iter().map(String::as_str));
            let mut problem = invalid(format!("`{prompt}` isn't a prompt"), span, "no such prompt")
                .reason("`fakes: { ai.<prompt>: [...] }` scripts a model's answers to a prompt, named as its type's name in lower_snake_case.");
            problem = match near {
                Some(near) => problem.next(format!("Use `ai.{near}` instead of `{written}`")),
                None => problem.next(format!("Remove `{written}:`")),
            };
            self.out.push(problem);
            return;
        }
        let Some(Expr::ListExpr(answers)) = entry.value() else {
            let at = entry
                .value()
                .map_or_else(|| span.clone(), |value| self.span(&value));
            self.out.push(
                invalid(format!("`{written}`'s answers aren't a list"), at, "not a list")
                    .reason("A model's answers are a list, taken in order, one by each answer the code asks for.")
                    .next(format!("Write `{written}: [<answer>]`")),
            );
            return;
        };
        let items: Vec<Expr> = answers.exprs().collect();
        if items.is_empty() {
            self.out.push(
                invalid(format!("`{written}` is scripted with no answers"), self.span(&answers), "no answers")
                    .reason("A prompt the test doesn't script is answered with a generated value of its call's type, so an empty list says nothing.")
                    .next(format!("Remove `{written}:`, or list the answers its calls take")),
            );
            return;
        }
        let mut types = crate::ai::answer_types(self.db, self.project)
            .get(prompt)
            .cloned()
            .unwrap_or_default();
        // A streamed prompt's answer is text, and its conditions a
        // stream's (chunk 5.5v6).
        let streamed = crate::ai::streamed_prompts(self.db, self.project).contains(prompt);
        let mut words: Vec<&str> = Vec::new();
        if !streamed || !types.is_empty() {
            words.extend(super::generate::CONDITIONS.iter().map(|c| c.word()));
        }
        if streamed {
            for condition in super::stream::CONDITIONS {
                if !words.contains(&condition.word()) {
                    words.push(condition.word());
                }
            }
            let text = Ty::Prim(Prim::Text);
            if !types.contains(&text) {
                types.push(text);
            }
        }
        let calls = match (streamed, types.len() > 1 || !streamed) {
            (true, false) => "`ai.stream`",
            (true, true) => "`ai.generate` and `ai.stream`",
            _ => "`ai.generate`",
        };
        for answer in items {
            let key = node_key(answer.syntax());
            if let Expr::NameRef(named) = &answer {
                let word = super::text(named);
                if words.contains(&word.as_str()) {
                    self.facts
                        .ai_fakes
                        .insert(key, (prompt.to_string(), AiFakeAnswer::Failed(word)));
                    continue;
                }
                let near = nearest(&word, words.iter().copied());
                let mut problem = invalid(
                    format!("`{word}` isn't an answer `{written}` gives"),
                    self.span(&answer),
                    "not a failure",
                )
                .reason(format!(
                    "A model's scripted answer is a value of the type its call asks for, `raw(\"...\")` with text it answers, or one of {calls}'s failures: {}.",
                    crate::and_list(&words.iter().map(|word| format!("`{word}`")).collect::<Vec<_>>())
                ));
                problem = match near {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{word}`"))
                        .edit(self.span(&answer), near),
                    None => problem.next(format!("Remove `{word}`")),
                };
                self.out.push(problem);
                continue;
            }
            if let Expr::CallExpr(call) = &answer
                && let Some(Expr::NameRef(callee)) = call.expr()
            {
                let called = super::text(&callee);
                if called == "recorded" {
                    let at = self.span(&answer);
                    self.out.push(
                        jadpo_diag::Diagnostic::new(
                            &rule::TOOLCHAIN_NOT_BUILT_YET,
                            "`recorded(...)` isn't built yet".to_string(),
                        )
                        .at(at, "decided, not built")
                        .reason("A recorded answer, kept in the prompt's `recordings/` by `jadpo ai record`, comes with roadmap chunk 5.5v4.")
                        .next("Script the answer as a value, or with `raw(\"...\")`, for now"),
                    );
                    continue;
                }
                if called == "partial" {
                    self.partial_answer(call, prompt, written, streamed, key);
                    continue;
                }
                if called == "raw" {
                    let args = Self::arguments(call);
                    match args.as_slice() {
                        [arg] if arg.name().is_none() => {
                            if let Some(text) = arg.expr() {
                                self.check(&text, &Ty::Prim(Prim::Text));
                            }
                            self.facts
                                .ai_fakes
                                .insert(key, (prompt.to_string(), AiFakeAnswer::Raw));
                        }
                        _ => {
                            let at = self.arg_list_span(call);
                            self.out.push(
                                invalid("`raw` takes the text the model answers with".to_string(), at, "not one text")
                                    .reason("`raw(\"...\")` scripts the text a model answers with, which runs through the call's real decoding and correction, so a test can script an answer that doesn't decode.")
                                    .next("Write `raw(\"{\\\"reply\\\": 1}\")`, with the text the model answers"),
                            );
                        }
                    }
                    continue;
                }
            }
            // A value of the type the prompt's calls ask for.
            match types.as_slice() {
                [ty] => {
                    let ty = ty.clone();
                    self.check(&answer, &ty);
                    self.facts
                        .ai_fakes
                        .insert(key, (prompt.to_string(), AiFakeAnswer::Value));
                }
                [] => {
                    self.infer(&answer);
                    self.out.push(
                        invalid(
                            format!("No call asks `{prompt}` for a value"),
                            self.span(&answer),
                            "a value",
                        )
                        .reason(format!(
                            "A scripted value is checked against the type an `ai.generate` call of `{prompt}` decodes its answer into, and no action makes one."
                        ))
                        .next("Script one of `ai.generate`'s failures, or `raw(\"...\")`"),
                    );
                }
                several => {
                    self.infer(&answer);
                    let listed: Vec<String> = several.iter().map(|ty| format!("`{ty}`")).collect();
                    self.out.push(
                        invalid(
                            format!("`{prompt}` is asked for {}, so a value can't be scripted", crate::and_list(&listed)),
                            self.span(&answer),
                            "which type",
                        )
                        .reason(format!(
                            "Calls of `{prompt}` decode its answer as {}, so a scripted value would fit some of them and not others.",
                            crate::and_list(&listed)
                        ))
                        .next("Script the answer as `raw(\"...\")`, which each call decodes as its own type"),
                    );
                }
            }
        }
    }

    /// `partial("...", <condition>)` (area 21, "Tests"; chunk 5.5v6): a
    /// stream that sends the text's pieces and then stops with the
    /// condition, which reaches the client as an `error` event: only for a
    /// prompt an action streams, and only a condition a stream can stop
    /// with once it has started.
    fn partial_answer(
        &mut self,
        call: &ast::CallExpr,
        prompt: &str,
        written: &str,
        streamed: bool,
        key: NodeKey,
    ) {
        let at = self.span(call);
        if !streamed {
            for arg in Self::arguments(call).into_iter().take(1) {
                if let Some(value) = arg.expr() {
                    self.infer(&value);
                }
            }
            self.out.push(
                invalid(format!("`{prompt}` isn't streamed, so `{written}` can't stop part-way"), at, "a partial stream")
                    .reason("`partial(\"...\", <condition>)` scripts a stream that sends its text and then stops with the condition, and only an `ai.stream` call of the prompt streams; an `ai.generate` call's answer comes whole or not at all.")
                    .next("Script one of `ai.generate`'s failures, or stream the prompt with `ai.stream`"),
            );
            return;
        }
        let words: Vec<&str> = super::stream::AFTER_FIRST
            .iter()
            .map(|c| c.word())
            .collect();
        let listed = crate::and_list(
            &words
                .iter()
                .map(|word| format!("`{word}`"))
                .collect::<Vec<_>>(),
        );
        let args = Self::arguments(call);
        let text = args
            .first()
            .filter(|arg| arg.name().is_none())
            .and_then(ast::Arg::expr);
        let condition = args
            .get(1)
            .filter(|arg| arg.name().is_none())
            .and_then(ast::Arg::expr);
        if let Some(text) = &text {
            self.check(text, &Ty::Prim(Prim::Text));
        }
        let word = match &condition {
            Some(Expr::NameRef(named)) => Some(super::text(named)),
            _ => None,
        };
        match (text, word, args.len()) {
            (Some(_), Some(word), 2) if words.contains(&word.as_str()) => {
                self.facts
                    .ai_fakes
                    .insert(key, (prompt.to_string(), AiFakeAnswer::Partial(word)));
            }
            (Some(_), Some(word), 2) => {
                let span = condition
                    .as_ref()
                    .map_or_else(|| at.clone(), |c| self.span(c));
                let mut problem = invalid(
                    format!("`{word}` isn't how a stream stops part-way"),
                    span.clone(),
                    "not a condition",
                )
                .reason(format!(
                    "Once its first piece has come, a stream stops part-way only when the model declines to go on, its time runs out or its answer reaches `max_output:`: {listed}; `too_long` is found before anything is sent."
                ));
                problem = match nearest(&word, words.iter().copied()) {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{word}`"))
                        .edit(span, near),
                    None => problem.next(format!("Write one of {listed}")),
                };
                self.out.push(problem);
            }
            _ => {
                let at = self.arg_list_span(call);
                self.out.push(
                    invalid("`partial` takes the text sent and how the stream stops".to_string(), at, "not a text and a condition")
                        .reason(format!(
                            "`partial(\"...\", <condition>)` scripts a stream that sends the text's pieces, then stops with the condition, one of {listed}."
                        ))
                        .next("Write `partial(\"Our sourdough is\", timed_out)`"),
                );
            }
        }
    }
}

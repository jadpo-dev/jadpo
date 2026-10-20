//! `jadpo ai show <prompt>` (area 21, "Prompts as files"): what a prompt
//! sends, for a reviewer to read before a change is approved: its version,
//! the instructions as the system message, the conversation's turns, and
//! the request as the user message, each free-text placeholder in the data
//! block it is sent in. Nothing is filled in: a placeholder is shown as
//! the prompt's file writes it, so the screen reads as the file does.
//!
//! The screen: the prompt's fields (its type, file and version), one
//! section per message in the order they are sent, then a footer counting
//! the placeholders and the data blocks among them. `jadpo ai usage` is
//! [`crate::ai_usage`]; `jadpo ai record` comes with roadmap chunk 5.5v4b.

use std::path::PathBuf;

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::prompt::{Form, Piece, PromptEntry, as_reviewed};
use jadpo_term::{Block, Fact, Json, Outcome, Screen};

use crate::fmt::project_needed;
use crate::migrate::blocked;
use crate::run::{Call, Reply, either, quoted};
use crate::suggest::nearest;
use crate::table::Value;

/// What `jadpo ai` does.
pub const ACTIONS: &[Value] = &[
    Value {
        name: "show",
        description: "Show what a prompt sends, its placeholders in place",
    },
    Value {
        name: "record",
        description: "Keep the model's answer to a prompt for tests",
    },
    Value {
        name: "usage",
        description: "Show the tokens and cost models used today and this month",
    },
];

pub fn ai(call: &Call) -> Reply {
    let names: Vec<&str> = ACTIONS.iter().map(|action| action.name).collect();
    match call.arguments.first().map(String::as_str) {
        Some("show") => {}
        Some("record") => return crate::ai_record::record(call),
        Some("usage") => return crate::ai_usage::usage(call),
        Some(other) => {
            let diagnostic = Diagnostic::new(
                &rule::CLI_AI_ARGUMENTS,
                format!("`{other}` isn't something `jadpo ai` does"),
            );
            let reason = format!("`jadpo ai` can {}.", either(&quoted(&names)));
            return Reply::failed(match nearest(other, names.iter().copied()) {
                Some(near) => diagnostic
                    .reason(format!("{reason} Did you mean `{near}`?"))
                    .next(format!("Use `jadpo ai {near}`")),
                None => diagnostic
                    .reason(reason)
                    .next("Run `jadpo ai show <prompt>` to see what a prompt sends"),
            });
        }
        None => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::CLI_AI_ARGUMENTS,
                    "`jadpo ai` needs to know what to do",
                )
                .reason("`show` shows what a prompt sends: its instructions, the conversation's turns and its request. `record` keeps the model's answer to it for tests.")
                .next("Run `jadpo ai show <prompt>` with one of the project's prompts"),
            );
        }
    }
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_needed("ai show");
    };
    let loaded = jadpo_db::load(&root);
    let (db, project) = (&loaded.db, loaded.project);
    let analysis = jadpo_check::prompts::analysis(db, project);
    let keys: Vec<&str> = analysis
        .prompts
        .iter()
        .map(|facts| facts.key.as_str())
        .collect();
    let Some(written) = call.arguments.get(1) else {
        return Reply::failed(
            Diagnostic::new(&rule::CLI_AI_ARGUMENTS, "`jadpo ai show` needs a prompt's name")
                .reason(if keys.is_empty() {
                    "This project declares no prompt: a `type X = Prompt { ... }` in a domain's `prompts/` folder, with its text in `<name>.prompt` beside it.".to_string()
                } else {
                    format!("The project's prompts are {}.", crate::run::both(&quoted(&keys)))
                })
                .next(match keys.first() {
                    Some(first) => format!("Run `jadpo ai show {first}`"),
                    None => "Declare a prompt in a domain's `prompts/` folder".to_string(),
                }),
        );
    };
    if let Some(extra) = call.arguments.get(2) {
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_AI_ARGUMENTS,
                format!("`jadpo ai show` takes one prompt, not `{extra}` as well"),
            )
            .reason("It shows one prompt's messages.")
            .next(format!("Run `jadpo ai show {written}`")),
        );
    }
    // Its name as the manifest writes it, `answer_question`, or its type's,
    // `AnswerQuestion`.
    let key = jadpo_db::to_snake(written);
    let Some(facts) = analysis.prompt(&key) else {
        let diagnostic = Diagnostic::new(
            &rule::CLI_AI_ARGUMENTS,
            format!("`{written}` isn't a prompt in this project"),
        );
        let listed = if keys.is_empty() {
            "This project declares no prompt.".to_string()
        } else {
            format!("Its prompts are {}.", crate::run::both(&quoted(&keys)))
        };
        return Reply::failed(match nearest(&key, keys.iter().copied()) {
            Some(near) => diagnostic
                .reason(format!(
                    "No prompt is called `{key}`; the closest is `{near}`."
                ))
                .next(format!("Run `jadpo ai show {near}`")),
            None => diagnostic.reason(listed).next(match keys.first() {
                Some(first) => format!("Run `jadpo ai show {first}`"),
                None => "Declare a prompt in a domain's `prompts/` folder".to_string(),
            }),
        });
    };
    let Some(entry) = &facts.entry else {
        let problems: Vec<Diagnostic> = analysis
            .problems
            .iter()
            .filter(|problem| {
                problem.primary.as_ref().is_some_and(|label| {
                    label.span.path == facts.path || label.span.path == facts.text_path
                })
            })
            .cloned()
            .collect();
        return blocked(&loaded, &cwd, problems, "ai show stopped");
    };
    show(entry, &facts.type_name, &facts.text_path)
}

/// The screen and data for one prompt that checks.
fn show(entry: &PromptEntry, type_name: &str, file: &str) -> Reply {
    let version = entry.version();
    let mut screen = Screen::new();
    screen.push(Block::Fields(vec![
        ("prompt".to_string(), type_name.to_string()),
        ("file".to_string(), file.to_string()),
        ("version".to_string(), version.clone()),
    ]));
    let instructions = as_reviewed(&entry.instructions, &entry.fields);
    let request = as_reviewed(&entry.request, &entry.fields);
    screen.push(Block::Section {
        heading: "system  # Instructions".to_string(),
        lines: instructions.lines().map(str::to_string).collect(),
    });
    if let Some(conversation) = &entry.conversation {
        screen.push(Block::Section {
            heading: "turns".to_string(),
            lines: vec![format!("each message of `{conversation}`, in order")],
        });
    }
    screen.push(Block::Section {
        heading: "user  # Request".to_string(),
        lines: request.lines().map(str::to_string).collect(),
    });
    // Each field written, and how many of its placeholders are data blocks.
    let written: Vec<&str> =
        entry
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .filter(|name| {
                entry.instructions.iter().chain(&entry.request).any(
                    |piece| matches!(piece, Piece::Field { name: written, .. } if written == name),
                )
            })
            .collect();
    let blocks = entry
        .request
        .iter()
        .filter(|piece| match piece {
            Piece::Field { name, .. } => {
                entry.field(name).is_some_and(|field| field.form.is_free())
            }
            Piece::Text(_) => false,
        })
        .count();
    let count = |n: usize, one: &str, many: &str| {
        if n == 1 {
            format!("1 {one}")
        } else {
            format!("{n} {many}")
        }
    };
    screen.push(Block::summary(
        Outcome::Success,
        version.clone(),
        [
            Fact::plain(count(written.len(), "placeholder", "placeholders")),
            Fact::plain(count(blocks, "data block", "data blocks")),
        ],
    ));
    let mut messages = vec![Json::object([
        ("role", Json::from("system")),
        ("content", Json::from(instructions.as_str())),
    ])];
    if let Some(conversation) = &entry.conversation {
        messages.push(Json::object([
            ("role", Json::from("turns")),
            ("field", Json::from(conversation.as_str())),
        ]));
    }
    messages.push(Json::object([
        ("role", Json::from("user")),
        ("content", Json::from(request.as_str())),
    ]));
    let placeholders: Vec<Json> = entry
        .fields
        .iter()
        .map(|field| {
            Json::object([
                ("name", Json::from(field.name.as_str())),
                (
                    "written",
                    Json::from(match field.form {
                        Form::Closed => "text_form",
                        Form::Text => "data_block",
                        Form::Json => "json_data_block",
                    }),
                ),
            ])
        })
        .collect();
    Reply::screen(
        screen,
        Json::object([
            ("prompt", Json::from(entry.key.as_str())),
            ("type", Json::from(type_name)),
            ("file", Json::from(file)),
            ("version", Json::from(version.as_str())),
            ("messages", Json::array(messages)),
            ("placeholders", Json::array(placeholders)),
        ]),
    )
}

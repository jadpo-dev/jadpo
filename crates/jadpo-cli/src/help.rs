//! Help screens and the JSON command reference, generated from the table.

use jadpo_diag::{CATALOGUE, Decider, Rule};
use jadpo_term::{Block, Json, Row, Screen};

use crate::run::Reply;
use crate::table::{self, Argument, Command, Flag, GLOBAL_FLAGS, GROUPS, TAGLINE, Value, Values};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `jadpo`, `jadpo help` and `jadpo --help`: every command grouped by job.
pub fn reference() -> Reply {
    let mut screen = Screen::new().with(Block::header(format!("jadpo {VERSION}"), [TAGLINE]));
    let commands = table::in_help_order();
    for group in GROUPS {
        let rows: Vec<Row> = commands
            .iter()
            .filter(|command| command.group == *group)
            .map(|command| command_row(command))
            .collect();
        if !rows.is_empty() {
            screen.push(Block::list(*group, rows));
        }
    }
    screen.push(global_flags());
    screen.push(Block::hint(
        "jadpo help completions",
        "shows how to turn on Tab completion",
    ));
    Reply::screen(screen, reference_json())
}

/// `jadpo help <command>` and `jadpo <command> --help`.
pub fn command(command: &Command) -> Reply {
    let facts: Vec<String> = match command.synonyms {
        [] => Vec::new(),
        synonyms => vec![format!("also called {}", synonyms.join(", "))],
    };
    let mut screen = Screen::new()
        .with(
            Block::header(format!("jadpo {}", command.name), facts).with_text(command.description),
        )
        .with(Block::section("Usage", [command.usage()]));
    if !command.arguments.is_empty() {
        screen.push(Block::list(
            "Arguments",
            command.arguments.iter().map(argument_row).collect(),
        ));
    }
    if !command.flags.is_empty() {
        screen.push(Block::list(
            "Flags",
            command.flags.iter().map(flag_row).collect(),
        ));
    }
    screen.push(global_flags());
    screen.push(Block::examples(
        command
            .examples
            .iter()
            .map(|example| (example.description, example.command))
            .collect(),
    ));
    Reply::screen(screen, command_json(command))
}

/// Every rule's page, `docs/errors/<rule>.md`, packed by `build.rs`.
const PAGES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/errors.deflate"));

/// The page of the rule `id`, as its Markdown, when it has one.
pub fn rule_page(id: &str) -> Option<String> {
    let packed = miniz_oxide::inflate::decompress_to_vec(PAGES).ok()?;
    let mut rest = packed.as_slice();
    let take = |rest: &mut &[u8]| -> Option<Vec<u8>> {
        let (len, after) = rest.split_first_chunk::<4>()?;
        let len = u32::from_le_bytes(*len) as usize;
        let bytes = after.get(..len)?.to_vec();
        *rest = &after[len..];
        Some(bytes)
    };
    while !rest.is_empty() {
        let name = take(&mut rest)?;
        let page = take(&mut rest)?;
        if name == id.as_bytes() {
            return String::from_utf8(page).ok();
        }
    }
    None
}

/// `jadpo help <rule>`: the rule's page from `docs/errors/`, read in the
/// terminal with no network. The page's prose, then each example in the
/// order it is written: the file it is in, its code, and what `jadpo
/// check` printed. Supporting files the site hides are left out here too.
pub fn rule(rule: &Rule) -> Reply {
    let page = rule_page(rule.id);
    let mut facts = vec![format!("area {}", rule.area)];
    if rule.declinable {
        facts.push("declinable".to_string());
    }
    facts.push(
        match rule.decides {
            Decider::Agent => "an agent can fix it",
            Decider::Human => "a person decides",
        }
        .to_string(),
    );
    let mut screen =
        Screen::new().with(Block::header(rule.id, facts).with_text(format!("{}.", rule.summary)));
    match &page {
        Some(page) => {
            for part in page_parts(page) {
                screen.push(match part {
                    Part::Prose(paragraphs) => Block::prose(paragraphs),
                    Part::Example { heading, lines } => Block::section(heading, lines),
                });
            }
        }
        None => screen.push(Block::prose(["This rule has no page yet."])),
    }
    let data = Json::object([
        ("id", rule.id.into()),
        ("area", i64::from(rule.area).into()),
        ("declinable", rule.declinable.into()),
        ("decides", rule.decides.word().into()),
        ("summary", rule.summary.into()),
        ("help", rule.help().into()),
        ("page", page.map_or(Json::Null, Json::from)),
    ]);
    Reply::screen(screen, data)
}

/// One piece of a rule's page as the terminal shows it.
#[derive(Debug, PartialEq)]
enum Part {
    /// Paragraphs, each one line of text.
    Prose(Vec<String>),
    /// A fenced block: under the file it is in, or what printed it.
    Example { heading: String, lines: Vec<String> },
}

/// A page's Markdown as prose and examples, in order, after its title:
/// paragraphs joined into lines with links read as their text, each list
/// item a paragraph of its own, and each fence that isn't `hidden`.
fn page_parts(page: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut lines = page.lines().skip_while(|line| line.starts_with("# "));
    let end_paragraph = |current: &mut String, paragraphs: &mut Vec<String>| {
        if !current.is_empty() {
            paragraphs.push(link_text(&std::mem::take(current)));
        }
    };
    while let Some(line) = lines.next() {
        if let Some(info) = line.strip_prefix("```") {
            end_paragraph(&mut current, &mut paragraphs);
            if !paragraphs.is_empty() {
                parts.push(Part::Prose(std::mem::take(&mut paragraphs)));
            }
            let body: Vec<String> = lines
                .by_ref()
                .take_while(|line| line.trim_end() != "```")
                .map(str::to_string)
                .collect();
            let words: Vec<&str> = info.split_whitespace().collect();
            let heading = match words.as_slice() {
                ["jadpo", ..] if words.contains(&"hidden") => continue,
                ["jadpo", ..] => words
                    .iter()
                    .find_map(|word| word.strip_prefix("file="))
                    .unwrap_or("main.jadpo")
                    .to_string(),
                ["sh", ..] => "Shell".to_string(),
                ["sql", ..] => "SQL".to_string(),
                _ => "Output".to_string(),
            };
            parts.push(Part::Example {
                heading,
                lines: body,
            });
            continue;
        }
        let line = line.trim();
        if line.is_empty() {
            end_paragraph(&mut current, &mut paragraphs);
        } else if line.starts_with("- ") || line.starts_with("* ") {
            end_paragraph(&mut current, &mut paragraphs);
            current.push_str(line);
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(line);
        }
    }
    end_paragraph(&mut current, &mut paragraphs);
    if !paragraphs.is_empty() {
        parts.push(Part::Prose(paragraphs));
    }
    parts
}

/// `[text](target)` read as `text`, so a page reads whole offline.
fn link_text(paragraph: &str) -> String {
    let mut out = String::new();
    let mut rest = paragraph;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find("](") else {
            break;
        };
        let Some(end) = after[close + 2..].find(')') else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&after[..close]);
        rest = &after[close + 2 + end + 1..];
    }
    out.push_str(rest);
    out
}

fn command_row(command: &Command) -> Row {
    let row = Row::new(command.name, command.summary);
    match command.arguments.first().map(|argument| &argument.values) {
        Some(Values::Fixed(values)) => row.extra(values.iter().map(|value| value.name)),
        _ => row,
    }
}

fn argument_row(argument: &Argument) -> Row {
    let (open, close) = if argument.required {
        ('<', '>')
    } else {
        ('[', ']')
    };
    let row = Row::new(
        format!("{open}{}{close}", argument.name),
        argument.description,
    );
    match &argument.values {
        Values::Fixed(values) => row.extra(values.iter().map(|value| value.name)),
        Values::Commands
        | Values::Path
        | Values::TestPath
        | Values::Query
        | Values::Schedule
        | Values::Deployable
        | Values::Id => row,
    }
}

fn flag_row(flag: &Flag) -> Row {
    Row::new(flag.name, flag.description).extra(flag.values.iter().map(|value| value.name))
}

fn global_flags() -> Block {
    Block::list(
        "On every command",
        GLOBAL_FLAGS.iter().map(flag_row).collect(),
    )
}

fn values_json(values: &[Value]) -> Json {
    Json::array(values.iter().map(|value| {
        Json::object([
            ("name", value.name.into()),
            ("description", value.description.into()),
        ])
    }))
}

fn flag_json(flag: &Flag) -> Json {
    Json::object([
        ("name", flag.name.into()),
        ("value", flag.value.into()),
        ("description", flag.description.into()),
        ("values", values_json(flag.values)),
    ])
}

fn argument_json(argument: &Argument) -> Json {
    let (kind, values) = match &argument.values {
        Values::Fixed(values) => ("choice", values_json(values)),
        Values::Commands => (
            "command",
            Json::array(table::COMMANDS.iter().map(|command| {
                Json::object([
                    ("name", command.name.into()),
                    ("description", command.summary.into()),
                ])
            })),
        ),
        Values::Path | Values::TestPath => ("path", Json::array(Vec::<Json>::new())),
        Values::Query => ("query", Json::array(Vec::<Json>::new())),
        Values::Schedule => ("schedule", Json::array(Vec::<Json>::new())),
        Values::Deployable => ("deployable", Json::array(Vec::<Json>::new())),
        Values::Id => ("id", Json::array(Vec::<Json>::new())),
    };
    Json::object([
        ("name", argument.name.into()),
        ("required", argument.required.into()),
        ("description", argument.description.into()),
        ("kind", kind.into()),
        ("values", values),
    ])
}

/// One command as data: everything its help screen shows, plus the rules it
/// can raise of its own, its argument errors first (every command can also
/// raise the `cli.unknown_*` and `cli.presentation_arguments` rules).
pub fn command_json(command: &Command) -> Json {
    Json::object([
        ("name", command.name.into()),
        ("synonyms", Json::array(command.synonyms.iter().copied())),
        ("group", command.group.into()),
        ("summary", command.summary.into()),
        ("description", command.description.into()),
        ("usage", command.usage().into()),
        (
            "arguments",
            Json::array(command.arguments.iter().map(argument_json)),
        ),
        ("flags", Json::array(command.flags.iter().map(flag_json))),
        (
            "examples",
            Json::array(command.examples.iter().map(|example| {
                Json::object([
                    ("description", example.description.into()),
                    ("command", example.command.into()),
                ])
            })),
        ),
        (
            "rules",
            Json::array(
                std::iter::once(command.arguments_rule.id)
                    .chain(command.raises.iter().map(|rule| rule.id)),
            ),
        ),
    ])
}

/// A canonical JSON value from the graph crate as a `--json` value.
fn from_graph(value: &jadpo_graph::json::Value) -> Json {
    use jadpo_graph::json::Value;
    match value {
        Value::Null => Json::Null,
        Value::Bool(value) => Json::Bool(*value),
        Value::Int(value) => Json::Int(*value),
        Value::Str(value) => Json::Str(value.clone()),
        Value::Array(items) => Json::Array(items.iter().map(from_graph).collect()),
        Value::Object(entries) => Json::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), from_graph(value)))
                .collect(),
        ),
    }
}

/// The whole command reference, the rule catalogue, and the schemas of
/// the manifest and of `jadpo test --json`, for agents and for the docs
/// check.
pub fn reference_json() -> Json {
    Json::object([
        ("name", "jadpo".into()),
        ("version", VERSION.into()),
        ("summary", TAGLINE.into()),
        ("usage", "jadpo <command> [flags]".into()),
        ("groups", Json::array(GROUPS.iter().copied())),
        (
            "commands",
            Json::array(table::in_help_order().into_iter().map(command_json)),
        ),
        ("flags", Json::array(GLOBAL_FLAGS.iter().map(flag_json))),
        (
            "rules",
            Json::array(CATALOGUE.iter().map(|rule| {
                Json::object([
                    ("id", rule.id.into()),
                    ("area", i64::from(rule.area).into()),
                    ("severity", "error".into()),
                    ("declinable", rule.declinable.into()),
                    ("decides", rule.decides.word().into()),
                    ("summary", rule.summary.into()),
                    ("help", rule.help().into()),
                ])
            })),
        ),
        ("manifest", from_graph(&jadpo_graph::manifest::schema())),
        ("testReport", jadpo_test::schema()),
    ])
}

#[cfg(test)]
mod tests {
    use super::{Part, link_text, page_parts};

    #[test]
    fn links_read_as_their_text() {
        assert_eq!(
            link_text("See [area 1, Imports](../language/01-x.md#imports) and `a[0]`."),
            "See area 1, Imports and `a[0]`."
        );
    }

    #[test]
    fn a_page_is_its_prose_and_visible_examples_in_order() {
        let page = "# `x.y`\n\nFirst line\nsame paragraph.\n\n- one\n- two\n\n```jadpo check project=p file=app.jadpo hidden\napplication {}\n```\n\n```jadpo check project=p file=domains/a/b.jadpo expect=x.y\ntype A = Int {}\n```\n\n```text output\nerror: A\n```\n\nAfter.\n";
        assert_eq!(
            page_parts(page),
            [
                Part::Prose(vec![
                    "First line same paragraph.".to_string(),
                    "- one".to_string(),
                    "- two".to_string(),
                ]),
                Part::Example {
                    heading: "domains/a/b.jadpo".to_string(),
                    lines: vec!["type A = Int {}".to_string()],
                },
                Part::Example {
                    heading: "Output".to_string(),
                    lines: vec!["error: A".to_string()],
                },
                Part::Prose(vec!["After.".to_string()]),
            ]
        );
    }
}

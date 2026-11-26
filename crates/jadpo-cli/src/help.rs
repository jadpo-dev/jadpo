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

/// Every topic of the area pages, packed by `build.rs`.
const TOPICS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/topics.deflate"));

/// One topic of an area page (docs/documentation.md, "Topic IDs").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topic {
    pub id: String,
    /// The IDs it had before a rename, which still reach it.
    pub old: Vec<String>,
    pub area: u32,
    pub title: String,
    /// Its Markdown, from its heading to the next topic's, without its ID
    /// line.
    pub page: String,
}

/// Every topic, in the areas' order.
pub fn topics() -> Vec<Topic> {
    let Ok(packed) = miniz_oxide::inflate::decompress_to_vec(TOPICS) else {
        return Vec::new();
    };
    let mut rest = packed.as_slice();
    let take = |rest: &mut &[u8]| -> Option<String> {
        let (len, after) = rest.split_first_chunk::<4>()?;
        let len = u32::from_le_bytes(*len) as usize;
        let bytes = after.get(..len)?.to_vec();
        *rest = &after[len..];
        String::from_utf8(bytes).ok()
    };
    let mut out = Vec::new();
    while !rest.is_empty() {
        let (Some(names), Some(page)) = (take(&mut rest), take(&mut rest)) else {
            break;
        };
        let mut parts = names.splitn(3, '\t');
        let ids: Vec<String> = parts
            .next()
            .unwrap_or_default()
            .split(' ')
            .map(str::to_string)
            .collect();
        let area = parts.next().and_then(|area| area.parse().ok()).unwrap_or(0);
        let title = parts.next().unwrap_or_default().to_string();
        let Some((id, old)) = ids.split_first() else {
            continue;
        };
        out.push(Topic {
            id: id.clone(),
            old: old.to_vec(),
            area,
            title,
            page,
        });
    }
    out
}

/// The topic `id` names, by its ID or one it had before.
pub fn find_topic(id: &str) -> Option<Topic> {
    topics()
        .into_iter()
        .find(|topic| topic.id == id || topic.old.iter().any(|old| old == id))
}

/// `jadpo help topic <id>`, and `jadpo help <id>` when no command has that
/// name: the topic read in the terminal with no network, as a rule's page
/// is. Its prose, its `###` headings, its tables a row to a line, and each
/// example that isn't hidden, under the file it is in.
pub fn topic(topic: &Topic) -> Reply {
    let mut screen = Screen::new().with(Block::header(
        topic.title.clone(),
        [format!("area {}", topic.area), topic.id.clone()],
    ));
    let body: String = topic
        .page
        .lines()
        .skip(1)
        .map(|line| format!("{line}\n"))
        .collect();
    for part in page_parts(&body, "") {
        screen.push(match part {
            Part::Prose(paragraphs) => Block::prose(paragraphs),
            Part::Heading(heading) => Block::section(heading, Vec::<String>::new()),
            Part::Example { heading, lines } => Block::section(heading, lines),
        });
    }
    let data = Json::object([
        ("id", topic.id.as_str().into()),
        ("area", i64::from(topic.area).into()),
        ("title", topic.title.as_str().into()),
        ("page", topic.page.as_str().into()),
    ]);
    Reply::screen(screen, data)
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
    if rule.security {
        facts.push("a security rule".to_string());
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
            for part in page_parts(page, rule.summary) {
                screen.push(match part {
                    Part::Prose(paragraphs) => Block::prose(paragraphs),
                    Part::Heading(heading) => Block::section(heading, Vec::<String>::new()),
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
        ("security", rule.security.into()),
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
    /// A `##` heading.
    Heading(String),
    /// A fenced block: under the file it is in, or what printed it.
    Example { heading: String, lines: Vec<String> },
}

/// A page's Markdown as prose, headings and examples, in order, after its
/// title: paragraphs joined into lines with links read as their text and
/// bold as plain text, each list item a paragraph of its own, each `##`
/// heading, and each fence that isn't `hidden`. The sentence under the
/// title that is the rule's summary is left out, since the header gives it.
fn page_parts(page: &str, summary: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut lines = page.lines().skip_while(|line| line.starts_with("# "));
    let said = format!("{summary}.");
    let end_paragraph = |current: &mut String, paragraphs: &mut Vec<String>| {
        if !current.is_empty() {
            let text = link_text(&std::mem::take(current)).replace("**", "");
            paragraphs.push(text);
        }
    };
    let mut first = true;
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
                ["jadpo", ..] if words.contains(&"fragment") => words
                    .iter()
                    .find_map(|word| word.strip_prefix("file="))
                    .unwrap_or("Code")
                    .to_string(),
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
        let heading = ["## ", "### ", "#### "]
            .iter()
            .find_map(|marks| line.strip_prefix(marks));
        if line.starts_with("<!--") {
            continue;
        }
        if line.starts_with('|') {
            // A table, a row to a line; the line under its header is left
            // out.
            end_paragraph(&mut current, &mut paragraphs);
            let cells: Vec<String> = line
                .trim_matches('|')
                .split('|')
                .map(|cell| cell.trim().to_string())
                .collect();
            if !cells
                .iter()
                .all(|cell| !cell.is_empty() && cell.chars().all(|c| matches!(c, '-' | ':')))
            {
                paragraphs.push(link_text(&cells.join(" | ")).replace("**", ""));
            }
            continue;
        }
        if let Some(heading) = heading {
            end_paragraph(&mut current, &mut paragraphs);
            if !paragraphs.is_empty() {
                parts.push(Part::Prose(std::mem::take(&mut paragraphs)));
            }
            parts.push(Part::Heading(heading.to_string()));
            first = false;
        } else if line.is_empty() {
            end_paragraph(&mut current, &mut paragraphs);
            if first && paragraphs.len() == 1 && parts.is_empty() {
                first = false;
                if paragraphs[0] == said {
                    paragraphs.clear();
                }
            }
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
pub(crate) fn link_text(paragraph: &str) -> String {
    let mut out = String::new();
    let mut rest = paragraph;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else {
            break;
        };
        // A bracket that opens no link, as in `[Access.public]`, is kept.
        let end = after[close + 1..]
            .strip_prefix('(')
            .and_then(|target| target.find(')'));
        let Some(end) = end else {
            out.push_str(&rest[..open + 1 + close + 1]);
            rest = &after[close + 1..];
            continue;
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
        | Values::Flag
        | Values::Declaration
        | Values::Domain
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
        Values::Flag => ("flag", Json::array(Vec::<Json>::new())),
        Values::Declaration => ("declaration", Json::array(Vec::<Json>::new())),
        Values::Domain => ("domain", Json::array(Vec::<Json>::new())),
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
pub(crate) fn from_graph(value: &jadpo_graph::json::Value) -> Json {
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

/// The whole command reference, the rule catalogue, the standard types
/// and operations the type reference is rendered from, and the schemas of
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
                    ("security", rule.security.into()),
                    ("decides", rule.decides.word().into()),
                    ("summary", rule.summary.into()),
                    ("help", rule.help().into()),
                ])
            })),
        ),
        ("types", crate::type_reference::types_json()),
        ("operations", crate::type_reference::operations_json()),
        ("manifest", from_graph(&jadpo_graph::manifest::schema())),
        ("testReport", jadpo_test::schema()),
    ])
}

#[cfg(test)]
mod tests {
    use super::{Part, find_topic, link_text, page_parts, topics};

    #[test]
    fn each_topic_runs_from_its_heading_to_the_next() {
        let lifecycle = find_topic("lifecycle").expect("area 5's topic");
        assert_eq!((lifecycle.area, lifecycle.title.as_str()), (5, "Lifecycle"));
        assert!(lifecycle.page.starts_with("## Lifecycle\n"));
        assert!(!lifecycle.page.contains("<!-- topic:"));
        assert!(
            !lifecycle.page.contains("\n## "),
            "a topic ends at the next `##`"
        );
        // An area's own topic is its `#` heading, up to its first `##`.
        let data = find_topic("data").expect("area 5's own topic");
        assert!(data.page.starts_with("# Entities, stores and migrations\n"));
        // A renamed topic is found by the ID it had.
        assert_eq!(
            find_topic("background-work").map(|topic| topic.id),
            Some("policy-and-background-work".to_string())
        );
        // `## Errors` declares no topic, and only lists error pages.
        assert!(
            topics()
                .iter()
                .all(|topic| !topic.page.starts_with("## Errors\n"))
        );
    }

    #[test]
    fn a_topic_reads_its_tables_a_row_to_a_line_and_its_smaller_headings() {
        let page = "Intro.\n\n<!-- note -->\n### Smaller\n\n| Command | Does |\n|---|---|\n| `jadpo new` | Starts a project |\n";
        assert_eq!(
            page_parts(page, ""),
            vec![
                Part::Prose(vec!["Intro.".to_string()]),
                Part::Heading("Smaller".to_string()),
                Part::Prose(vec![
                    "Command | Does".to_string(),
                    "`jadpo new` | Starts a project".to_string()
                ]),
            ]
        );
    }

    #[test]
    fn a_bracket_that_opens_no_link_is_kept() {
        assert_eq!(
            link_text("as `read: [CompanyRole.viewer]` ([Grants](#grants))."),
            "as `read: [CompanyRole.viewer]` (Grants)."
        );
    }

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
            page_parts(page, "A"),
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

    #[test]
    fn a_formatted_page_gives_its_headings_and_not_its_summary_again() {
        let page = "# `x.y`\n\nA thing\nhappened.\n\n## Why Jadpo refuses it\n\nIt is **wrong**.\n\n```jadpo fragment file=app.jadpo\nx: 1\n```\n";
        assert_eq!(
            page_parts(page, "A thing happened"),
            [
                Part::Heading("Why Jadpo refuses it".to_string()),
                Part::Prose(vec!["It is wrong.".to_string()]),
                Part::Example {
                    heading: "app.jadpo".to_string(),
                    lines: vec!["x: 1".to_string()],
                },
            ]
        );
    }
}

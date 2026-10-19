//! Help screens and the JSON command reference, generated from the table.

use jadpo_diag::CATALOGUE;
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
        Values::Commands | Values::Path | Values::TestPath | Values::Query | Values::Schedule => {
            row
        }
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

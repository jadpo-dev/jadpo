//! The declarative command table. Help screens, `jadpo help --json`, Tab
//! completion and suggestions are all generated from it, so a command
//! describes itself once. A command appears here only once it is built.

use jadpo_diag::{Rule, rule};

use crate::commands;
use crate::run::{Call, Reply};

/// The jobs commands are grouped by, in the order help lists them (the CLI
/// design language's `jadpo` screen). Groups with no built command are
/// left out.
pub const GROUPS: &[&str] = &[
    "Build and check",
    "Data",
    "Look things up",
    "Change safely",
    "Production",
    "Set up",
    "Help",
];

pub const TAGLINE: &str = "a programming environment for coding agents";

#[derive(Debug)]
pub struct Command {
    pub name: &'static str,
    /// Plain synonyms people reach for (area 14): run the same command; help
    /// shows the canonical name.
    pub synonyms: &'static [&'static str],
    pub group: &'static str,
    /// One line for lists and completion: a verb first, under 45 characters,
    /// no full stop.
    pub summary: &'static str,
    /// What `--help` says under the header.
    pub description: &'static str,
    pub arguments: &'static [Argument],
    /// Flags of this command only; [`GLOBAL_FLAGS`] apply to every command.
    pub flags: &'static [Flag],
    pub examples: &'static [Example],
    /// The rule for this command's own argument errors.
    pub arguments_rule: &'static Rule,
    /// Other rules a run of this command can end in, for `jadpo help
    /// --json`.
    pub raises: &'static [&'static Rule],
    pub run: fn(&Call) -> Reply,
}

impl Command {
    pub fn usage(&self) -> String {
        let mut usage = format!("jadpo {}", self.name);
        for argument in self.arguments {
            let (open, close) = if argument.required {
                ('<', '>')
            } else {
                ('[', ']')
            };
            usage.push_str(&format!(" {open}{}{close}", argument.name));
        }
        usage
    }
}

#[derive(Debug)]
pub struct Argument {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
    pub values: Values,
}

/// What an argument or flag accepts, for completion, help and errors.
#[derive(Debug)]
pub enum Values {
    /// One of a fixed list.
    Fixed(&'static [Value]),
    /// The name of a built command.
    Commands,
    /// A file or folder, completed from the folder the command runs in.
    Path,
    /// A test file or a folder: `.test.jadpo` files, and folders.
    TestPath,
    /// The name of one of the project's queries or actions.
    Query,
}

#[derive(Debug)]
pub struct Value {
    pub name: &'static str,
    pub description: &'static str,
}

#[derive(Debug)]
pub struct Flag {
    pub name: &'static str,
    /// The value's placeholder, for flags that take one (`--color <when>`).
    pub value: Option<&'static str>,
    pub description: &'static str,
    pub values: &'static [Value],
}

#[derive(Debug)]
pub struct Example {
    pub description: &'static str,
    pub command: &'static str,
}

pub const COLORS: &[Value] = &[
    Value {
        name: "auto",
        description: "Decide from the terminal",
    },
    Value {
        name: "always",
        description: "Always use colour",
    },
    Value {
        name: "never",
        description: "Never use colour",
    },
];

/// A flag a language page decides that isn't built yet. It is left out
/// of help and completion, like an unbuilt command, but using it says so
/// with `toolchain.not_built_yet` and the section that decides it, rather
/// than calling it unknown.
#[derive(Debug)]
pub struct Planned {
    pub command: &'static str,
    pub name: &'static str,
    /// What it does, as a plural noun phrase or a sentence subject:
    /// "Interactive tests".
    pub what: &'static str,
    /// Why it isn't here yet, as a sentence.
    pub reason: &'static str,
    /// The deciding section, relative to `docs/language/`.
    pub anchor: &'static str,
}

pub static PLANNED_FLAGS: &[Planned] = &[Planned {
    command: "test",
    name: "--interactive",
    what: "Interactive tests",
    reason: "`--interactive` opens a keyboard view of every test (area 13), which comes with the rest of area 14's tooling.",
    anchor: "13-testing.html#running-tests",
}];

/// The planned flag of `command` called `name`.
pub fn planned(command: &str, name: &str) -> Option<&'static Planned> {
    PLANNED_FLAGS
        .iter()
        .find(|flag| flag.command == command && flag.name == name)
}

pub static GLOBAL_FLAGS: &[Flag] = &[
    Flag {
        name: "--json",
        value: None,
        description: "Print data for tools instead of text",
        values: &[],
    },
    Flag {
        name: "--help",
        value: None,
        description: "Show every flag, with examples",
        values: &[],
    },
    Flag {
        name: "--color",
        value: Some("when"),
        description: "Choose when to use colour",
        values: COLORS,
    },
];

pub const SHELLS: &[Value] = &[
    Value {
        name: "zsh",
        description: "The Z shell, macOS's default",
    },
    Value {
        name: "bash",
        description: "The Bourne-again shell",
    },
    Value {
        name: "fish",
        description: "The friendly interactive shell",
    },
];

/// What `jadpo migrate` does (area 14, question 4).
pub const MIGRATE_ACTIONS: &[Value] = &[
    Value {
        name: "new",
        description: "Write the next migration, or finish the latest",
    },
    Value {
        name: "apply",
        description: "Run a database's pending migrations",
    },
    Value {
        name: "status",
        description: "List the migrations and what a database has applied",
    },
];

/// The kinds of test (area 13, "Kinds of test"); route and schedule tests
/// come with their areas.
pub const KINDS: &[Value] = &[
    Value {
        name: "pure",
        description: "Tests that call functions only",
    },
    Value {
        name: "action",
        description: "Tests that call actions directly",
    },
    Value {
        name: "route",
        description: "Tests that send a request (not built yet)",
    },
    Value {
        name: "schedule",
        description: "Tests that run a schedule (not built yet)",
    },
];

pub static COMMANDS: &[Command] = &[
    Command {
        name: "check",
        synonyms: &[],
        group: "Build and check",
        summary: "Check the project for errors",
        description: "Check every Jadpo file in the project: where each file sits, how things are \
                      named, what each name refers to and who may use it, types, logic and failures. \
                      Then write the manifest and the catalogues to build/, errors or not. Without a \
                      path, the project is found by walking up to app.jadpo. A file or folder outside \
                      any project is checked on its own, with the layout rules off, and writes nothing.",
        arguments: &[Argument {
            name: "path",
            description: "A project, file or folder to check",
            required: false,
            values: Values::Path,
        }],
        flags: &[],
        examples: &[
            Example {
                description: "Check the project you are in",
                command: "jadpo check",
            },
            Example {
                description: "Check one file on its own",
                command: "jadpo check notes.jadpo",
            },
            Example {
                description: "Print the problems as data for tools",
                command: "jadpo check --json",
            },
        ],
        arguments_rule: &rule::CLI_CHECK_ARGUMENTS,
        raises: &[],
        run: commands::check,
    },
    Command {
        name: "test",
        synonyms: &[],
        group: "Build and check",
        summary: "Run tests",
        description: "Run the project's tests in the interpreter, in an order a seed decides, and report \
                      each failure with the values in its assertion, what happened instead and the \
                      command that reruns it. Without a path, the project is found by walking up to \
                      app.jadpo and every test runs; a file runs its tests, a folder the tests under it, \
                      and <file>:<line> the test at that line. The project must check without errors \
                      first. With --watch, it keeps running: each save reruns the tests the change \
                      reaches, through the compiler's graph, within the path, name and kind given. \
                      Pure and action tests run so far.",
        arguments: &[Argument {
            name: "path",
            description: "A test file, <file>:<line>, or a folder",
            required: false,
            values: Values::TestPath,
        }],
        flags: &[
            Flag {
                name: "--name",
                value: Some("text"),
                description: "Run tests whose names contain this text",
                values: &[],
            },
            Flag {
                name: "--kind",
                value: Some("kind"),
                description: "Run one kind: pure, action, route, schedule",
                values: KINDS,
            },
            Flag {
                name: "--failed",
                value: None,
                description: "Rerun the tests that failed last time",
                values: &[],
            },
            Flag {
                name: "--seed",
                value: Some("n"),
                description: "Run in the order this seed gives",
                values: &[],
            },
            Flag {
                name: "--watch",
                value: None,
                description: "Rerun the tests each save reaches",
                values: &[],
            },
            Flag {
                name: "--keep-failed",
                value: None,
                description: "Keep a failed test's database to inspect",
                values: &[],
            },
        ],
        examples: &[
            Example {
                description: "Run every test in the project you are in",
                command: "jadpo test",
            },
            Example {
                description: "Run the test at line 12 of a file, in the order seed 48213 gives",
                command: "jadpo test domains/planning/libraries/dates.test.jadpo:12 --seed 48213",
            },
            Example {
                description: "Rerun the tests that failed last time",
                command: "jadpo test --failed",
            },
            Example {
                description: "Rerun the tests each save reaches, until ctrl-c",
                command: "jadpo test --watch",
            },
            Example {
                description: "Print the results as data for tools",
                command: "jadpo test --json",
            },
        ],
        arguments_rule: &rule::CLI_TEST_ARGUMENTS,
        raises: &[
            &rule::TEST_ASSERTION_FAILED,
            &rule::TEST_UNEXPECTED_FAILURE,
            &rule::TEST_FAULT,
            &rule::TEST_FAILED,
            &rule::TEST_NO_TESTS,
            &rule::TEST_RUNTIME_START_FAILED,
            &rule::TOOLCHAIN_NOT_BUILT_YET,
            &rule::TOOLCHAIN_PROJECT_NOT_FOUND,
        ],
        run: commands::test,
    },
    Command {
        name: "fmt",
        synonyms: &["format"],
        group: "Build and check",
        summary: "Format source files",
        description: "Rewrite Jadpo files in the one layout the formatter owns: indentation, spacing, \
                      blank lines and one member per line. In a project it first fixes each file's \
                      `use` lines: adds a missing import, removes an unused one and sorts them. \
                      With --layout it also moves each declaration another file uses into a file \
                      of its own, as layout.promotion_required asks. With --check, change nothing \
                      and fail if any file would change. Without a path, the project is found by \
                      walking up to app.jadpo.",
        arguments: &[Argument {
            name: "path",
            description: "A file or folder instead of the whole project",
            required: false,
            values: Values::Path,
        }],
        flags: &[
            Flag {
                name: "--check",
                value: None,
                description: "Report unformatted files and change nothing",
                values: &[],
            },
            Flag {
                name: "--layout",
                value: None,
                description: "Also make the moves promotion asks for",
                values: &[],
            },
        ],
        examples: &[
            Example {
                description: "Format the project you are in",
                command: "jadpo fmt",
            },
            Example {
                description: "Make the moves layout.promotion_required asks for, then format",
                command: "jadpo fmt --layout",
            },
            Example {
                description: "Check formatting without changing anything, as CI does",
                command: "jadpo fmt --check",
            },
            Example {
                description: "Format one file",
                command: "jadpo fmt app.jadpo",
            },
        ],
        arguments_rule: &rule::CLI_FMT_ARGUMENTS,
        raises: &[],
        run: commands::fmt,
    },
    Command {
        name: "migrate",
        synonyms: &[],
        group: "Data",
        summary: "Write and apply migrations",
        description: "Keep the database in step with the entities (area 5). `new` compares the \
                      entities with the latest migration's snapshot and writes the next folder in \
                      database/migrations/: the whole schema as snapshot.json, the SQL that gets there \
                      as postgres.sql, and changes.jadpo when a change needs a decision, such as a \
                      value for existing rows; once the decisions are written, `new` again finishes \
                      it. `apply` runs a database's pending migrations, each in a transaction; \
                      `status` lists the migrations and, with --url, which a database has applied. \
                      Until configuration gives each connection its URL, --url names the database.",
        arguments: &[
            Argument {
                name: "action",
                description: "What to do",
                required: true,
                values: Values::Fixed(MIGRATE_ACTIONS),
            },
            Argument {
                name: "purpose",
                description: "What a new migration does, in lower_snake_case",
                required: false,
                values: Values::Fixed(&[]),
            },
        ],
        flags: &[
            Flag {
                name: "--url",
                value: Some("url"),
                description: "The PostgreSQL database to apply to or read",
                values: &[],
            },
            Flag {
                name: "--store",
                value: Some("name"),
                description: "Which store that database is",
                values: &[],
            },
        ],
        examples: &[
            Example {
                description: "Write the migration for a new field",
                command: "jadpo migrate new add_rating",
            },
            Example {
                description: "Finish the latest migration once its decisions are written",
                command: "jadpo migrate new",
            },
            Example {
                description: "Apply the pending migrations to a database",
                command: "jadpo migrate apply --url postgresql://localhost/shop",
            },
            Example {
                description: "List the migrations and which a database has applied",
                command: "jadpo migrate status --url postgresql://localhost/shop",
            },
        ],
        arguments_rule: &rule::CLI_MIGRATE_ARGUMENTS,
        raises: &[
            &rule::MIGRATION_APPLY_FAILED,
            &rule::MIGRATION_STORE_DIVERGED,
            &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
            &rule::TOOLCHAIN_NOT_BUILT_YET,
            &rule::TOOLCHAIN_PROJECT_NOT_FOUND,
        ],
        run: commands::migrate,
    },
    Command {
        name: "sql",
        synonyms: &[],
        group: "Data",
        summary: "Show the SQL a query or action sends",
        description: "Print the PostgreSQL a query compiles to (area 6): its statement, then one more \
                      for each include, however many records it is included in, each with the \
                      parameters that fill it and where their values come from. Given an action, \
                      print the statement each of its writes sends, with what each refusal raises: \
                      the failure a conflict: arm names, the not-found failure or a fault. The \
                      statements are fixed when the project is compiled, so the same query always \
                      sends the same text. The project must check without errors first. --explain, \
                      which runs the query on a disposable database and shows PostgreSQL's plan, \
                      comes later.",
        arguments: &[Argument {
            name: "name",
            description: "The query's or action's name",
            required: false,
            values: Values::Query,
        }],
        flags: &[Flag {
            name: "--explain",
            value: None,
            description: "Show PostgreSQL's plan (not built yet)",
            values: &[],
        }],
        examples: &[
            Example {
                description: "Show the statements a page query sends",
                command: "jadpo sql latest_entries",
            },
            Example {
                description: "Show the statements an action's writes send",
                command: "jadpo sql archive_entry",
            },
        ],
        arguments_rule: &rule::CLI_SQL_ARGUMENTS,
        raises: &[
            &rule::TOOLCHAIN_NOT_BUILT_YET,
            &rule::TOOLCHAIN_PROJECT_NOT_FOUND,
        ],
        run: commands::sql,
    },
    Command {
        name: "completions",
        synonyms: &["completion"],
        group: "Set up",
        summary: "Turn on Tab completion",
        description: "Print the script that makes Tab complete jadpo's commands, flags and values, \
                      each with a one-line description. Load it from your shell's start-up file.",
        arguments: &[Argument {
            name: "shell",
            description: "The shell to complete in",
            required: true,
            values: Values::Fixed(SHELLS),
        }],
        flags: &[],
        examples: &[
            Example {
                description: "Turn on Tab completion in zsh",
                command: "echo 'source <(jadpo completions zsh)' >> ~/.zshrc",
            },
            Example {
                description: "Turn on Tab completion in bash",
                command: "echo 'eval \"$(jadpo completions bash)\"' >> ~/.bashrc",
            },
            Example {
                description: "Turn on Tab completion in fish",
                command: "jadpo completions fish > ~/.config/fish/completions/jadpo.fish",
            },
        ],
        arguments_rule: &rule::CLI_COMPLETIONS_ARGUMENTS,
        raises: &[],
        run: commands::completions,
    },
    Command {
        name: "lsp",
        synonyms: &[],
        group: "Set up",
        summary: "Run the language server for editors",
        description: "Run the language server on standard input and output, for an editor to start: \
                      the same errors as jadpo check while you type, hover with signatures, \
                      descriptions, the failures a call can produce and who calls it, and go to \
                      definition. The project is the one holding the editor's workspace folder.",
        arguments: &[],
        // Editors' language clients pass `--stdio`.
        flags: &[Flag {
            name: "--stdio",
            value: None,
            description: "Talk on standard input and output (default)",
            values: &[],
        }],
        examples: &[Example {
            description: "Start the language server, as an editor does",
            command: "jadpo lsp",
        }],
        arguments_rule: &rule::CLI_LSP_ARGUMENTS,
        raises: &[],
        run: commands::lsp,
    },
    Command {
        name: "help",
        synonyms: &[],
        group: "Help",
        summary: "Show commands, flags and examples",
        description: "Show every command grouped by job, or one command's arguments, flags and examples. \
                      With --json, print the whole command reference and the rule catalogue as data.",
        arguments: &[Argument {
            name: "command",
            description: "The command to explain",
            required: false,
            values: Values::Commands,
        }],
        flags: &[],
        examples: &[
            Example {
                description: "Show every command",
                command: "jadpo help",
            },
            Example {
                description: "Show one command's arguments, flags and examples",
                command: "jadpo help completions",
            },
            Example {
                description: "Print the command reference and rule catalogue for tools",
                command: "jadpo help --json",
            },
        ],
        arguments_rule: &rule::CLI_HELP_ARGUMENTS,
        raises: &[],
        run: commands::help,
    },
    Command {
        name: "version",
        synonyms: &[],
        group: "Help",
        summary: "Print the version",
        description: "Print the version of this jadpo. jadpo --version does the same.",
        arguments: &[],
        flags: &[],
        examples: &[
            Example {
                description: "Print the version",
                command: "jadpo version",
            },
            Example {
                description: "Print the version for tools",
                command: "jadpo version --json",
            },
        ],
        arguments_rule: &rule::CLI_VERSION_ARGUMENTS,
        raises: &[],
        run: commands::version,
    },
];

/// Finds a command by its name or a synonym.
pub fn find(word: &str) -> Option<&'static Command> {
    COMMANDS
        .iter()
        .find(|command| command.name == word || command.synonyms.contains(&word))
}

/// Every name a command answers to, canonical names first.
pub fn all_names() -> impl Iterator<Item = &'static str> {
    COMMANDS.iter().map(|command| command.name).chain(
        COMMANDS
            .iter()
            .flat_map(|command| command.synonyms.iter().copied()),
    )
}

/// Commands in help order: by group, then as listed.
pub fn in_help_order() -> Vec<&'static Command> {
    let mut commands: Vec<&Command> = COMMANDS.iter().collect();
    commands.sort_by_key(|command| {
        GROUPS
            .iter()
            .position(|group| *group == command.group)
            .unwrap_or(GROUPS.len())
    });
    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_synonyms_are_unique_and_plain() {
        let mut names: Vec<&str> = all_names().collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "a name or synonym is used twice");
        for name in names {
            assert!(
                name.len() > 1 && name.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "`{name}`: no single letters"
            );
        }
    }

    #[test]
    fn every_command_describes_itself() {
        for command in COMMANDS {
            assert!(
                GROUPS.contains(&command.group),
                "{}: unknown group",
                command.name
            );
            assert!(
                command.summary.len() < 45 && !command.summary.ends_with('.'),
                "{}: summary",
                command.name
            );
            assert!(
                command.description.ends_with('.'),
                "{}: description is sentences",
                command.name
            );
            assert!(
                !command.examples.is_empty(),
                "{}: needs an example",
                command.name
            );
            for example in command.examples {
                assert!(
                    example.command.contains(&format!("jadpo {}", command.name)),
                    "{}: {}",
                    command.name,
                    example.command
                );
            }
            assert!(
                command
                    .arguments_rule
                    .id
                    .starts_with(&format!("cli.{}_", command.name)),
                "{}",
                command.name
            );
        }
        for flag in GLOBAL_FLAGS
            .iter()
            .chain(COMMANDS.iter().flat_map(|command| command.flags))
        {
            assert!(
                flag.name.starts_with("--") && flag.description.len() < 45,
                "{}",
                flag.name
            );
            assert!(
                flag.values.is_empty() || flag.value.is_some(),
                "{}: values need a placeholder",
                flag.name
            );
        }
    }
}

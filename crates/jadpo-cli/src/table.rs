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
    /// The name of one of the project's schedules.
    Schedule,
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

/// What `jadpo api` does (areas 7 and 18).
pub const API_ACTIONS: &[Value] = &[Value {
    name: "update",
    description: "Rewrite each API's record from the routes",
}];

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
        description: "Tests that send a request through a route",
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
                command: "jadpo check main.jadpo",
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
                      Pure, action and route tests run so far.",
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
        name: "dev",
        synonyms: &[],
        group: "Build and check",
        summary: "Run the app and reload on every save",
        description: "Run the application in the interpreter and serve its routes on a local port \
                      (3000 unless --port says otherwise). Each save is checked again, and once it \
                      checks cleanly the new version serves the next request; while a save has \
                      errors, the last good version keeps serving. Each request is one line, with its \
                      request id. The database is the one --url names, or else a disposable \
                      PostgreSQL started for the session; pending migrations are applied as they \
                      appear. With --no-run it only checks on every save and writes build/.",
        arguments: &[],
        flags: &[
            Flag {
                name: "--port",
                value: Some("port"),
                description: "Listen on this port; 0 picks a free one",
                values: &[],
            },
            Flag {
                name: "--url",
                value: Some("url"),
                description: "Use this database, not a disposable one",
                values: &[],
            },
            Flag {
                name: "--no-run",
                value: None,
                description: "Only check on every save; serve nothing",
                values: &[],
            },
        ],
        examples: &[
            Example {
                description: "Serve the project you are in on http://localhost:3000",
                command: "jadpo dev",
            },
            Example {
                description: "Serve it on another port, with your own database",
                command: "jadpo dev --port 8080 --url postgresql://localhost/app_dev",
            },
            Example {
                description: "Print each request as a line of data for tools",
                command: "jadpo dev --json",
            },
        ],
        arguments_rule: &rule::CLI_DEV_ARGUMENTS,
        raises: &[
            &rule::CLI_DEV_PORT_INVALID,
            &rule::CLI_DEV_START_FAILED,
            &rule::TOOLCHAIN_PROJECT_NOT_FOUND,
        ],
        run: commands::dev,
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
                      it. `apply` runs a database's pending migrations, each in a transaction, and a \
                      computed backfill in batches, each in its own, carrying on where a stopped run \
                      left off; `status` lists the migrations and, with --url, which a database has applied. \
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
        name: "routes",
        synonyms: &[],
        group: "Look things up",
        summary: "List every route",
        description: "List every route the project answers (area 7), grouped by API: its method and \
                      full path, the query or action it runs, its authentication and the file it is \
                      written in. Authentication is required, none (anyone may call it) or fresh \
                      (the caller's record is read now).",
        arguments: &[],
        flags: &[],
        examples: &[
            Example {
                description: "List the routes of the project you are in",
                command: "jadpo routes",
            },
            Example {
                description: "Print them as data for tools",
                command: "jadpo routes --json",
            },
        ],
        arguments_rule: &rule::CLI_ROUTES_ARGUMENTS,
        raises: &[&rule::TOOLCHAIN_PROJECT_NOT_FOUND],
        run: commands::routes,
    },
    Command {
        name: "ai",
        synonyms: &[],
        group: "Look things up",
        summary: "Show what a prompt sends",
        description: "`show` prints what a prompt sends a model (area 21): its type, its file and its \
                      version, the first 8 hex characters of a SHA-256 over its two files, then the \
                      instructions as the system message, the conversation's turns, and the request \
                      as the user message. Placeholders are shown as the prompt's file writes them, \
                      each free-text one inside the tagged data block it is sent in.",
        arguments: &[
            Argument {
                name: "action",
                description: "What to do: show",
                required: true,
                values: Values::Fixed(crate::ai::ACTIONS),
            },
            Argument {
                name: "prompt",
                description: "The prompt's name, such as summarise_ticket",
                required: false,
                values: Values::Fixed(&[]),
            },
        ],
        flags: &[],
        examples: &[
            Example {
                description: "See what the summarise_ticket prompt sends",
                command: "jadpo ai show summarise_ticket",
            },
            Example {
                description: "Print it as data for tools",
                command: "jadpo ai show summarise_ticket --json",
            },
        ],
        arguments_rule: &rule::CLI_AI_ARGUMENTS,
        raises: &[
            &rule::TOOLCHAIN_NOT_BUILT_YET,
            &rule::TOOLCHAIN_PROJECT_NOT_FOUND,
        ],
        run: commands::ai,
    },
    Command {
        name: "api",
        synonyms: &[],
        group: "Change safely",
        summary: "Rewrite the API records",
        description: "`update` rewrites each API's committed record, api/<api>.jadpo_record, from the \
                      routes (areas 7 and 18): every route, the fields it takes and sends, and the \
                      failure codes it can answer with. The OpenAPI document's version is a hash of \
                      the record. jadpo check fails while a record disagrees with the routes, so \
                      every change to an API is a diff of one file to review and commit with the \
                      code. A record under its old name, api/<api>.api, is removed.",
        arguments: &[Argument {
            name: "action",
            description: "What to do: update",
            required: true,
            values: Values::Fixed(API_ACTIONS),
        }],
        flags: &[],
        examples: &[Example {
            description: "Bring every API record up to date with the routes",
            command: "jadpo api update",
        }],
        arguments_rule: &rule::CLI_API_ARGUMENTS,
        raises: &[
            &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
            &rule::TOOLCHAIN_PROJECT_NOT_FOUND,
        ],
        run: commands::api,
    },
    Command {
        name: "records",
        synonyms: &[],
        group: "Change safely",
        summary: "List and update the records",
        description: "Every file Jadpo generates and the project commits is a record (area 18): each \
                      API's api/<api>.jadpo_record, the identity registry, each translations \
                      folder's sources, the suggestions the project ignored or taught, and jadpo.lock. \
                      `jadpo records` lists them by the command that writes them, each with whether \
                      it is up to date, and fails while one is not, naming the command to run. \
                      `update` brings every record up to date at once: each API record written from \
                      the routes, each other record rewritten sorted and in the newest format, and a record \
                      under its old name moved.",
        arguments: &[Argument {
            name: "action",
            description: "What to do: update, or nothing to list them",
            required: false,
            values: Values::Fixed(crate::records::ACTIONS),
        }],
        flags: &[],
        examples: &[
            Example {
                description: "See every record and whether it is up to date",
                command: "jadpo records",
            },
            Example {
                description: "Bring every record up to date",
                command: "jadpo records update",
            },
        ],
        arguments_rule: &rule::CLI_RECORDS_ARGUMENTS,
        raises: &[
            &rule::RECORD_EDITED,
            &rule::RECORD_FORMAT_NEWER,
            &rule::RECORD_OUT_OF_DATE,
            &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
            &rule::TOOLCHAIN_PROJECT_NOT_FOUND,
        ],
        run: commands::records,
    },
    Command {
        name: "schedules",
        synonyms: &[],
        group: "Production",
        summary: "Watch and control schedules",
        description: "Every schedule's state and runs are recorded in the database its state is kept \
                      in (area 12). `jadpo schedules` lists each schedule with its last run, when it \
                      is next due or that it is paused, and the alerts that hold: no successful run \
                      within twice its interval, ticks no longer arriving, a run past 80% of the \
                      15-minute limit, three failures in a row, or a lease held past its expiry. \
                      `history` shows a schedule's runs and skipped ticks, newest first, 20 a page, \
                      and its metrics: runs, failures, skipped ticks, duration and delay. `pause` \
                      and `resume` stop and start its due ticks running it. `run` runs it now in \
                      this process, under its lease, so it never runs twice at once. The database \
                      is the one --url names, or the one its connection's setting gives, or the one \
                      a running jadpo dev started. Times are in UTC.",
        arguments: &[
            Argument {
                name: "action",
                description: "What to do: run, pause, resume or history, or nothing to list them",
                required: false,
                values: Values::Fixed(crate::schedules::ACTIONS),
            },
            Argument {
                name: "name",
                description: "The schedule's name",
                required: false,
                values: Values::Schedule,
            },
        ],
        flags: &[
            Flag {
                name: "--url",
                value: Some("url"),
                description: "The database the schedules' state is in",
                values: &[],
            },
            Flag {
                name: "--page",
                value: Some("n"),
                description: "Which page of history, from 1, newest first",
                values: &[],
            },
        ],
        examples: &[
            Example {
                description: "See every schedule, its last run and what is next",
                command: "jadpo schedules",
            },
            Example {
                description: "See a schedule's past runs and its metrics",
                command: "jadpo schedules history nightly_digest",
            },
            Example {
                description: "Stop a schedule running until it is resumed",
                command: "jadpo schedules pause nightly_digest",
            },
            Example {
                description: "Run a schedule now",
                command: "jadpo schedules run nightly_digest",
            },
        ],
        arguments_rule: &rule::CLI_SCHEDULES_ARGUMENTS,
        raises: &[
            &rule::SCHEDULES_LEASE_HELD,
            &rule::SCHEDULES_RUN_FAILED,
            &rule::SCHEDULES_UNAVAILABLE,
            &rule::TOOLCHAIN_PROJECT_NOT_FOUND,
        ],
        run: commands::schedules,
    },
    Command {
        name: "why",
        synonyms: &[],
        group: "Look things up",
        summary: "Explain why a schedule didn't run",
        description: "Answers why a schedule didn't run, or how its last run went (area 17), from its \
                      state and run records: it is paused, a run still holds its lease, a lost run \
                      held its lease past its expiry, ticks stopped arriving, or its last run failed, \
                      with the failure's code and kind. The answer comes first, then its facts, then \
                      the .jadpo place the schedule is declared in. Requests, events and records \
                      come with debugging (area 17).",
        arguments: &[Argument {
            name: "name",
            description: "The schedule's name",
            required: true,
            values: Values::Schedule,
        }],
        flags: &[Flag {
            name: "--url",
            value: Some("url"),
            description: "The database the schedules' state is in",
            values: &[],
        }],
        examples: &[Example {
            description: "Find out why a schedule hasn't run",
            command: "jadpo why nightly_digest",
        }],
        arguments_rule: &rule::CLI_WHY_ARGUMENTS,
        raises: &[
            &rule::SCHEDULES_UNAVAILABLE,
            &rule::TOOLCHAIN_PROJECT_NOT_FOUND,
        ],
        run: commands::why,
    },
    Command {
        name: "declines",
        synonyms: &[],
        group: "Change safely",
        summary: "List or clear declines",
        description: "A judgement-call error, such as a helper like a library's, can be declined with \
                      a reason: a `decline <rule>: \"<reason>\"` line above the declaration it is \
                      on. `list` shows every decline with its rule, place and reason, so each can \
                      be reviewed. `clear` removes the decline lines, so each finding comes back and \
                      jadpo check fails until it is fixed or declined again: after a compiler \
                      upgrade, a new standard type, or as a periodic sweep. `--rule` and a path \
                      narrow both.",
        arguments: &[
            Argument {
                name: "action",
                description: "What to do: list or clear",
                required: true,
                values: Values::Fixed(crate::declines::ACTIONS),
            },
            Argument {
                name: "path",
                description: "Only the declines in this file or folder",
                required: false,
                values: Values::Path,
            },
        ],
        flags: &[Flag {
            name: "--rule",
            value: Some("id"),
            description: "Only the declines of this rule",
            values: &[],
        }],
        examples: &[
            Example {
                description: "See every decline and its reason",
                command: "jadpo declines list",
            },
            Example {
                description: "Decide again every similar-helper decline",
                command: "jadpo declines clear --rule library.similar_exists",
            },
            Example {
                description: "List the declines in shared/ as data for tools",
                command: "jadpo declines list shared --json",
            },
        ],
        arguments_rule: &rule::CLI_DECLINES_ARGUMENTS,
        raises: &[
            &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
            &rule::TOOLCHAIN_PROJECT_NOT_FOUND,
        ],
        run: commands::declines,
    },
    Command {
        name: "fix",
        synonyms: &[],
        group: "Change safely",
        summary: "Fix a standard type's finding",
        description: "When a Jadpo release adds a standard type the application already names \
                      for its own, jadpo check reports the clash (type.standard_clash), and a \
                      type with a standard type's shape under another name is \
                      type.standard_exists. With no arguments, fix lists each such finding and \
                      the fixes it takes. `swap` makes every reference use the standard type, \
                      reading each matched field by the standard field's name, and deletes the \
                      declaration. `rename` gives the application's declaration a new name, at \
                      the declaration, every reference that can see it and the file named after \
                      it. A fix whose result would have an error the project didn't have \
                      changes nothing and shows that error. The editor's quick fix makes the \
                      same swap.",
        arguments: &[
            Argument {
                name: "name",
                description: "The declaration to fix, as its finding names it",
                required: false,
                values: Values::Fixed(&[]),
            },
            Argument {
                name: "fix",
                description: "How to fix it: swap or rename",
                required: false,
                values: Values::Fixed(crate::fix::FIXES),
            },
            Argument {
                name: "new-name",
                description: "The new name, for rename",
                required: false,
                values: Values::Fixed(&[]),
            },
        ],
        flags: &[],
        examples: &[
            Example {
                description: "See what can be fixed after an upgrade",
                command: "jadpo fix",
            },
            Example {
                description: "Use the standard Uuid instead of the app's own",
                command: "jadpo fix Uuid swap",
            },
            Example {
                description: "Keep the app's own type under another name",
                command: "jadpo fix Uuid rename RoomReference",
            },
        ],
        arguments_rule: &rule::CLI_FIX_ARGUMENTS,
        raises: &[
            &rule::TYPE_STANDARD_CLASH,
            &rule::TYPE_STANDARD_EXISTS,
            &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
            &rule::TOOLCHAIN_PROJECT_NOT_FOUND,
        ],
        run: commands::fix,
    },
    Command {
        name: "suggest",
        synonyms: &[],
        group: "Change safely",
        summary: "Hint where a standard type or operation fits",
        description: "Formats, then scans the project for code that could use a standard type or \
                      operation: hand-rolled money, durations, quantities, points, validation \
                      patterns and loops, found by the signals each standard type's entry holds. \
                      It never edits: each finding is numbered (S1, S2...) for the agent to act on, \
                      and the report ends with the routine to follow. `ignore` keeps a finding \
                      quiet until its code changes, in the committed \
                      ignored_suggestions.jadpo_record. `found` records something the scan \
                      missed, before the code changes, and teaches it so the next scan finds the \
                      same pattern, in the committed taught_suggestions.jadpo_record. `share` \
                      sends the hashes of what was learned to Jadpo HQ at once, shows exactly \
                      what it would send of the snippets HQ asks for, and sends them only with \
                      `--approve`, after a person said yes; `suggestions { share: false }` in \
                      app.jadpo or JADPO_SUGGEST_SHARE=off turns sharing off. `--catalogue` \
                      lists every signal.",
        arguments: &[
            Argument {
                name: "action",
                description: "What to do besides scanning: ignore, found or share",
                required: false,
                values: Values::Fixed(crate::suggestions::ACTIONS),
            },
            Argument {
                name: "subject",
                description: "For ignore, the suggestion's number; for found, <file>:<line>",
                required: false,
                values: Values::Fixed(&[]),
            },
            Argument {
                name: "what",
                description: "For ignore, the reason; for found, the type or operation",
                required: false,
                values: Values::Fixed(&[]),
            },
            Argument {
                name: "why",
                description: "For found, the reason",
                required: false,
                values: Values::Fixed(&[]),
            },
        ],
        flags: &[
            Flag {
                name: "--catalogue",
                value: None,
                description: "List every signal the scan looks for",
                values: &[],
            },
            Flag {
                name: "--approve",
                value: None,
                description: "Send what HQ asked for, after a person's yes",
                values: &[],
            },
            Flag {
                name: "--decline",
                value: None,
                description: "Drop the snippets HQ asked for, sending none",
                values: &[],
            },
            Flag {
                name: "--new-token",
                value: None,
                description: "Replace the project token, then share",
                values: &[],
            },
            Flag {
                name: "--unattended",
                value: None,
                description: "Send only hashes; also when CI is set",
                values: &[],
            },
        ],
        examples: &[
            Example {
                description: "Find where the code could use a standard type",
                command: "jadpo suggest",
            },
            Example {
                description: "Keep the first suggestion quiet, with the reason",
                command: "jadpo suggest ignore S1 \"Basis points, not money\"",
            },
            Example {
                description: "Record a miss before fixing it",
                command: "jadpo suggest found domains/shop/actions/pence_of.jadpo:5 Money \"Pence held as Int\"",
            },
            Example {
                description: "See every signal it looks for",
                command: "jadpo suggest --catalogue",
            },
            Example {
                description: "Send what was learned to Jadpo HQ",
                command: "jadpo suggest share",
            },
            Example {
                description: "Send the snippets HQ asked for, after the person said yes",
                command: "jadpo suggest share --approve",
            },
        ],
        arguments_rule: &rule::CLI_SUGGEST_ARGUMENTS,
        raises: &[
            &rule::SUGGEST_RECORD_INVALID,
            &rule::SUGGEST_SHARE_FAILED,
            &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
            &rule::TOOLCHAIN_PROJECT_NOT_FOUND,
        ],
        run: commands::suggest,
    },
    Command {
        name: "translations",
        synonyms: &[],
        group: "Change safely",
        summary: "Keep translations up to date",
        description: "The text the server writes for its readers is translated in Jadpo itself (area 10). \
                      `missing` lists, by language file, the wording a locale lacks (and what an \
                      in-progress locale falls back on), the translations made from default wording \
                      that has since changed, and, with `translations: { review: \
                      TranslationReview.required }`, the wording waiting for acceptance. `unused` \
                      lists the messages no code calls. `update` records, in each translations \
                      folder's sources.jadpo_record, which default wording each translation was \
                      made from. `accept` records a person's acceptance of a locale's changed \
                      wording, and never of a stale translation. `builtin` lists Jadpo's own \
                      messages the application can show, with their fields and the wording to \
                      translate from. `export` writes every message, its description and its \
                      wording for a locale as XLIFF 2.0 or JSON, for Crowdin, Lokalise or any \
                      service that reads them, and `import` reads the translations back, checks \
                      them as the language files are checked, and writes the language files and \
                      the record, or writes nothing.",
        arguments: &[
            Argument {
                name: "action",
                description: "What to do: missing, unused, update, accept, builtin, export or import",
                required: true,
                values: Values::Fixed(crate::translations::ACTIONS),
            },
            Argument {
                name: "locale",
                description: "The locale to list, accept or export, such as fr_fr; for import, the file to read",
                required: false,
                values: Values::Fixed(&[]),
            },
        ],
        flags: &[
            Flag {
                name: "--format",
                value: Some("format"),
                description: "For export: xliff (default) or json",
                values: crate::exchange::command::FORMATS,
            },
            Flag {
                name: "--out",
                value: Some("folder"),
                description: "For export: the folder to write to",
                values: &[],
            },
            Flag {
                name: "--locale",
                value: Some("locale"),
                description: "For import: the locale of a JSON file",
                values: &[],
            },
        ],
        examples: &[
            Example {
                description: "See what is left to translate",
                command: "jadpo translations missing",
            },
            Example {
                description: "Write every locale's wording for a translation service",
                command: "jadpo translations export",
            },
            Example {
                description: "Read back what the service translated",
                command: "jadpo translations import build/translations/fr_fr.xlf",
            },
            Example {
                description: "Record the translations just written",
                command: "jadpo translations update",
            },
            Example {
                description: "Accept the French wording after reading it",
                command: "jadpo translations accept fr_fr",
            },
            Example {
                description: "List the messages nothing calls as data for tools",
                command: "jadpo translations unused --json",
            },
            Example {
                description: "See Jadpo's own messages to word for a new locale",
                command: "jadpo translations builtin",
            },
        ],
        arguments_rule: &rule::CLI_TRANSLATIONS_ARGUMENTS,
        raises: &[
            &rule::TRANSLATION_EXCHANGE_FILE_INVALID,
            &rule::TRANSLATION_EXCHANGE_UNIT_UNKNOWN,
            &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
            &rule::TOOLCHAIN_PROJECT_NOT_FOUND,
            &rule::TOOLCHAIN_SOURCE_READ_FAILED,
        ],
        run: commands::translations,
    },
    Command {
        name: "config",
        synonyms: &[],
        group: "Set up",
        summary: "Check and set local settings",
        description: "Settings are what `config.jadpo` declares: values a deployment provides, \
                      each read from the environment variable named after it in capitals. `check` \
                      lists every setting as set, default, missing or invalid against this \
                      machine's values, the process's environment over `.env.local`, and shows \
                      each problem with the command that fixes it; it never prints a value. \
                      `set` asks for one setting's value at a prompt, hidden when it is secret, \
                      checks it against the setting's type and stores it in `.env.local`, which \
                      it keeps out of git. A value is never an argument, so it cannot reach a \
                      shell history, a log or a chat. `move` makes a constant a setting: a field \
                      of its name and type in `config { }` with its value as `default:`, the \
                      constant deleted and every read rewritten to `config.<name>`; it refuses, \
                      changing nothing, a type a setting can't hold, a name a setting has, and a \
                      read in a query, an update by `id`, a backfill or another constant.",
        arguments: &[
            Argument {
                name: "action",
                description: "What to do",
                required: true,
                values: Values::Fixed(crate::config::ACTIONS),
            },
            Argument {
                name: "field",
                description: "The setting to store a value for, or the constant to move",
                required: false,
                values: Values::Fixed(&[]),
            },
        ],
        flags: &[],
        examples: &[
            Example {
                description: "See which settings still need a value",
                command: "jadpo config check",
            },
            Example {
                description: "Store a secret without typing it anywhere it is kept",
                command: "jadpo config set session_signing_key",
            },
            Example {
                description: "List the settings as data for tools",
                command: "jadpo config check --json",
            },
            Example {
                description: "Make a constant a value each deployment can set",
                command: "jadpo config move limits.max_retries",
            },
        ],
        arguments_rule: &rule::CLI_CONFIG_ARGUMENTS,
        raises: &[
            &rule::CONFIGURATION_MOVE_TYPE_UNSUPPORTED,
            &rule::CONFIGURATION_MOVE_NAME_TAKEN,
            &rule::CONFIGURATION_MOVE_READ_UNSUPPORTED,
            &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
            &rule::CONFIGURATION_VALUE_MISSING,
            &rule::CONFIGURATION_VALUE_INVALID,
            &rule::CLI_CONFIG_TTY_REQUIRED,
            &rule::CLI_CONFIG_PROMPT_FAILED,
            &rule::CONFIGURATION_LOCAL_READ_FAILED,
            &rule::CONFIGURATION_LOCAL_INVALID_ENCODING,
            &rule::CONFIGURATION_LOCAL_SYNTAX,
            &rule::CONFIGURATION_LOCAL_UNSAFE_TARGET,
            &rule::CONFIGURATION_LOCAL_CHANGED,
            &rule::CONFIGURATION_LOCAL_WRITE_FAILED,
        ],
        run: commands::config,
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

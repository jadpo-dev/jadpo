---
title: Tooling
area: 14
status: decided
checked-with: the CLI's tests in this checkout (archived examples: v0 3ce094d0)
rules:
  - toolchain.no_sources
  - syntax.suggestions_unknown_option
  - syntax.suggestions_duplicate_option
  - type.suggestions_value_invalid
  - semantic.multiple_suggestions
---

# Tooling

The `jadpo` command, the formatter, the language server, how errors are shown,
the generated `build/` folder, and the tools agents use to look things up
before they change code.

> Decided on 2026-10-13. Commands and outputs below apply the decisions from areas 1 to 14.

The rule behind all of it: **the compiler is the only source of truth.** The
editor, the agent skill and every generated file come from the same check, so
they cannot disagree with `jadpo check`.

## The `jadpo` command

One command with a short list of subcommands. Every subcommand finds the
project by walking up from the current folder to `app.jadpo`, so a path is
optional (question 1), and every subcommand takes `--json` for output an agent
reads instead of prose (question 2), except the interactive modes, which need
a terminal. Commands have plain synonyms (`fmt` and `format`), complete on Tab,
and ask for a missing argument with a searchable list (questions 16 and 17).

| Command | Does | Status |
|---|---|---|
| `jadpo new <name>` | Creates a project that passes `check`, `fmt --check` and `test` | Decided (question 9), not built: `jadpo new` is `cli.unknown_command` |
| `jadpo check` | Checks everything and writes the derived files in `build/` | Built (chunks 1.1–1.8): writes `build/manifest.json` and the catalogues in `build/catalogue/`; writing files is question 3 |
| `jadpo fmt` | Formats; `--check` only reports; `--layout` also applies promotion moves | Built (chunk 1.2): formats, and `--check` only reports; since chunk 3.5 it fixes each file's imports first, and `--layout` makes promotion moves; since chunk 5.5h3 it writes each file's declarations before their use, the primary declaration last (chunk 3.9 had put it first) |
| `jadpo test` | Runs every test | Built (chunk 2.2): pure and action tests, in the interpreter; route tests since chunk 4.1; schedule tests, `--kind schedule`, since chunk 6.2c |
| `jadpo test <file>` / `<file>:<line>` | Runs one test file, or the test at that line | Built (chunk 2.2); a folder runs the tests under it |
| `jadpo test --name <text>` / `--kind route` / `--failed` / `--affected` | Runs tests whose names match, one kind, last run's failures, or the tests a change reaches | `--name`, `--kind pure` and `--kind action`, and `--failed` built (chunk 2.2), `--kind route` in chunk 4.1; the schedule kind comes with area 12; `--affected` decided in question 7, not built: a change is measured against the merge-base with `main` (Richard, 2026-10-14) |
| `jadpo test --watch` | Reruns the affected tests on every save | Built (chunk 2.3): each save reruns the tests that reach a node whose digest changed since the last run, within the path, name and kind given; a broken build shows its errors and runs nothing; `--json` writes one envelope per line; ctrl-c quits |
| `jadpo test --interactive` | Keyboard view of every test: search, navigate, run a file, one test or all, rerun failures, toggle watch | Decided in area 13, not built: the flag says `toolchain.not_built_yet` (chunk 2.8) |
| `jadpo test --keep-failed` / `--seed <n>` | Keeps a failed test's database and trace; reruns with a given order seed | `--seed` built (chunk 2.2); `--keep-failed` built with the test database (chunk 3.1): it keeps the database and names it in the report; the trace comes with area 17 |
| `jadpo build` | Writes the runnable app into `build/target/`, and each API's `<api>.failures.<locale>.json` beside its OpenAPI document (area 10) | Decided, not built: comes with compiled deployables (Phase 8) |
| `jadpo dev` | Rebuilds and restarts the app on every change; `--no-run` only rebuilds | Built (chunk 4.1): runs the app in the interpreter on the native host at `http://localhost:3000` (`--port` to change it), one line per request with its request id; each save that checks cleanly serves the next request, and while a save has errors the last good version keeps serving. The settings `config.jadpo` declares are read from the environment over `.env.local` and checked first (chunk 5.1): a missing or invalid value stops the app starting and is shown by setting and variable, never by value, and a save of `.env.local` is checked like a save of source, keeping the last good version serving. Each database opens at the address its connection's `url:` setting holds, or the one `--url` names, or else a disposable PostgreSQL for the session (for a store whose address no variable gives), with pending migrations applied as they appear. `--no-run` only checks on every save and writes `build/`; `--json` writes one envelope per line. Since chunk 6.2b, a line for each schedule run and skipped tick (a paused schedule's once), with its trace's id, outcome, what it emitted or its failure and its time, and a line for each alert when it starts; the address of each database it started is in `build/dev/stores.json` while it runs, for `jadpo schedules` and `jadpo why`. Since chunk 6.7, ctrl-c or SIGTERM stops it cleanly: the server drains, its background work giving back what it holds, and it exits 0; a second signal ends it at once (area 12, "Running background work"). Decided 2026-10-16 (area 11), not built: provider calls faked by default, `--fake` and `--sandbox` changing which calls reach the sandbox for one session only, and the services the app's connections declare started with it |
| `jadpo inspect` | Prints the project map: every declaration, call and failure | Decided, not built: until then `build/manifest.json` and the catalogues hold the map |
| `jadpo routes` | Lists every route with its full method and path, file, API, authentication and the query or action it runs | Built (chunk 4.1); authentication shows `required`, `none` or `fresh` since chunk 4.3 (area 8) |
| `jadpo ai show <prompt>` | Prints what a prompt sends a model: its type, file and version, the instructions as the system message, the conversation's turns and the request as the user message, each placeholder as the file writes it (area 21) | Built (chunk 5.5v2), with `--json`; takes the prompt's name or its type's; a misspelt action or prompt is `cli.ai_arguments` with the nearest name, and a prompt with errors shows them instead; `record` and `usage` say `toolchain.not_built_yet` until the `ai` capability |
| `jadpo sql <query>`, `jadpo sql <action>` | Prints a query's generated SQL for PostgreSQL and SQLite, or the statements an action's writes send; `--explain` runs it on a disposable database and shows the plan | Built for PostgreSQL (chunk 3.2): each statement the query sends, one for each include too, with the parameters that fill it, and `--json`. Given an action (chunk 3.4), each `create`, `update`, `delete` and transition it writes, in order, with its parameters as the action writes them, what each refusal raises (the constraint and the failure its `conflict:` arm names, the not-found failure, or a fault) and, for a transition whose `when:` can refuse a record, the statement that asks when no row changed. Tab completes the project's queries and actions, and without one it fails listing them. SQLite comes with a SQLite store; `--explain` is decided, not built: the flag says `toolchain.not_built_yet` |
| `jadpo impact <name>` | Lists what a change to `name` reaches | Decided in area 1, not built |
| `jadpo rename <old> <new>` | Renames a declaration or field everywhere, keeping stored data | Decided (question 5), not built; refuses a new name already in use (2026-10-16) |
| `jadpo domain rename/split/merge` | Moves a domain's files and updates every call | Decided in area 1, not built |
| `jadpo migrate new <purpose>` / `apply` / `status` | Writes, runs and lists migrations | Built (chunk 3.1): `new` writes the next migration, or finishes the latest once its decisions are written; `apply` and `status` take the database's URL with `--url` (and `--store` when there are several), or read it from the connection's setting in the environment over `.env.local` (chunk 5.1); replaces `schema` (question 4). Prompting for a missing purpose, as the design language draws it, waits for prompts: without one the command names a purpose to use |
| `jadpo seed <name>` | Runs a seed; refuses production | Decided in area 5, not built |
| `jadpo config move <constant>` | Makes a constant a setting: a `config` field of its name and type with its value as `default:`, the constant deleted and every read rewritten to `config.<name>` | Built (chunk 5.3f): names the constant as code reads it (`security.max_login_attempts`), or after its domain; lists what changed file by file, then the setting and its variable; refuses, changing nothing, with `configuration.move_type_unsupported`, `configuration.move_name_taken` and `configuration.move_read_unsupported`, every reason at once; undoes a move whose result wouldn't check; supports `--json` (area 10, "Making a constant a setting") |
| `jadpo config set <field>` / `check` | Sets or checks local configuration in `.env.local` | Built (chunk 5.1): `check` lists each setting as `set`, `default`, `missing` or `invalid` with its variable against the environment over `.env.local`, each problem as `configuration.value_missing` or `configuration.value_invalid`, never printing a value, and supports `--json`; `set` asks for one value at a prompt (hidden for a secret), refuses `--json` and a missing terminal, checks the value against the setting's type, and writes `.env.local` only as an ordinary file, through a private temporary file and only if unchanged since it was read, adding the file to `.gitignore` |
| `jadpo translations missing [<locale>]` / `unused` / `update` / `accept <locale>` | Lists, under the language file each is in, wording a locale lacks (and what an in-progress locale falls back on), translations made from wording that has since changed and wording not yet accepted, and the messages no code calls; records each translation's source in `translations/sources.jadpo_record`; accepts a locale's changed wording | Built (chunk 5.4d), each with `--json`: `missing` and `unused` succeed whatever they list; `update` and `accept` stop on syntax errors, write one status line for each record and say `still stale` of a translation nobody redid; a misspelt action or locale is `cli.translations_arguments` with the nearest name (area 10, "Tooling") |
| `jadpo translations builtin` | Lists Jadpo's own messages the app can show, by feature, with their fields, the wording Jadpo ships and the locales that still need it | Built (chunk 5.4e), with `--json`: succeeds whatever it lists; only the messages of features the app has on; `missing`, `update` and `accept` take the root `translations/` folder's wording of them in with the rest (area 10, "Jadpo's and packages' text") |
| `jadpo translations export [<locale>]` / `import <file>` | Exchanges messages, their descriptions and their wording with translation services as XLIFF 2.0 or JSON | Built (chunk 5.4f), each with `--json`: `export` writes `build/translations/<locale>.xlf` (`--format json`, `--out <folder>`) for each translated locale, or the default locale's source file; `import` checks what comes back as `jadpo check` would, writes the language files and the record or nothing, and refuses with the language files' own rules at the unit of the file, `translation.exchange_unit_unknown` or `translation.exchange_file_invalid` (area 10, "Tooling") |
| `jadpo api update` | Rewrites the committed public API record (`api/main.jadpo_record`) from the source | Built (chunk 4.1): one record per API with routes; a record whose API has no routes is removed. It is `api/<api>.jadpo_record` in the record format (chunk 7.0) (area 18), with no `version:` (the OpenAPI version is a hash of it), and `jadpo check` fails while it disagrees (`record.out_of_date`) or was edited by hand (`record.edited`); a record under its old name, `api/<api>.api`, is removed |
| `jadpo records` | Lists every committed record (`.jadpo_record` files and `jadpo.lock`), what writes each, and flags any out of date with the command that updates it | Built (chunk 7.0): grouped by the command that writes them, each with its state, failing while one needs its command; `jadpo records update` brings every record up to date at once and moves records from their old names (area 18) |
| `jadpo declines list` / `clear` | Lists every decline with its rule, place and reason; `clear` removes the matching decline lines so each finding comes back; `--rule <id>`, a path and `--json` | Built (chunk 5.4b): `list` is one list in file order, a reason too long for its row on the lines under it; `clear` writes each file without its matching lines, stops on syntax errors, and points at `jadpo check`; an unknown action, rule or a path outside the project is `cli.declines_arguments` (area 1, "Declining a judgement call") |
| `jadpo fix` | Applies the fix the author picks for a finding, through the same code as the editor's quick fix; first for a release that adds a standard type an app already names (swap to it, or rename) | Built (chunk 5.5a2), with `--json`: alone it lists each `type.standard_clash` and `type.standard_exists` finding with the fixes it takes; `jadpo fix <name> swap` and `jadpo fix <name> rename <NewName>` list what changed file by file; a fix whose result would add an error changes nothing and shows it; a name with nothing to fix, a fix it doesn't take or a taken name is `cli.fix_arguments`. The rename is not `jadpo rename`: it writes no identity line (area 19, "A release that adds a standard type") |
| `jadpo suggest` / `--catalogue` / `ignore <number> "<why>"` / `found <file>:<line> <Type or operation> "<why>"` / `share` / `share --approve` / `--decline` / `--new-token` | Hints where code could use a standard type or operation, never editing; lists every signal; keeps a suggestion quiet; teaches a miss; sends what was learned to Jadpo HQ | Built (chunk 5.5x), with `--json`: formats, then reports numbered findings from the signals on the standard types' entries and the loop table, and ends with the agent's instruction; `ignore` writes `ignored_suggestions.jadpo_record`, `found` writes `taught_suggestions.jadpo_record` (chunk 5.5y), both add to `.jadpo/suggest_pending`; a wrong number, place, type or operation is `cli.suggest_arguments`. `share` built in chunk 5.5y against a fake of Jadpo HQ: sends the pending hashes, shows the snippets HQ asks for and the exact payload, and sends them only with `--approve`; stops at once when sharing is turned off; an unreachable or refusing HQ is `suggest.share_failed` ("Suggestions" and "Sharing with Jadpo HQ" below) |
| `jadpo approval --before <ref>` | Writes the review report of what changed in behaviour, policy and the API | Decided, not built: comes with records (area 18); sign-off is question 12 |
| `jadpo deliveries` | Lists, retries, dismisses, cancels and resolves schedule runs and event deliveries; `show <id>` lists an unfinished multi-step action's steps, and `retry <id> --not-done` and `abandon <id>` act on it (area 11) | Decided (question 13; `show`, `--not-done` and `abandon` 2026-10-16), not built: roadmap 6.12, and `show`, `--not-done` and `abandon` with multi-step calls (7.5) |
| `jadpo schedules` | Lists schedules; `run`, `pause`, `resume`, `history` | Built (chunk 6.2b), with `--json`: a table of each schedule's last run and next due time (or `‖ paused`), the alerts that hold and a footer; `history <name>` 20 records a page (`--page`), grouped by day, then the metrics; `pause` and `resume` set the state row; `run` runs it now in this process under its lease (`schedules.lease_held`, `schedules.run_failed`), and since chunk 6.2d does so only against a running `jadpo dev`'s own database, asking a deployment's own tick to run it otherwise and waiting for its record (`schedules.no_tick`; area 12, "Running a schedule by hand"); since chunk 6.2c each entity's built-in purge schedule is listed too, as `Member.purge`, its runs showing the records they purged; the database is `--url`'s, the connection's setting's, or a running `jadpo dev`'s; an unknown name is `cli.schedules_arguments` with the nearest (area 12, "Watching schedules") |
| `jadpo package new <Name> --library` / `--provider` | Writes a package's own repository: the package format, `recipes/`, a `sample/` app and `GUIDE.md` | Decided in area 11 (2026-10-16), not built |
| `jadpo package add <name or source>` | Installs a package into `packages/<name>/<version>/` and `jadpo.lock`: from the registry (the fixed order of area 11's question 13), a git repository (`github.com/acme/acme-pricing@v2.1.0`), a path in this repository (another Jadpo app's OpenAPI, `../orders`) or a URL serving an OpenAPI document; regenerates the surface and refuses one that differs from its committed `surface.jadpo_record`; a third party's git package needs an acceptance | Decided in area 11 (2026-10-16), not built: chunk 7.1 (registry and path), git later |
| `jadpo package import <format> <file>` | Turns a published description (`openapi`, `smithy`, `postman`, `har`, `dts`) into the app's own integration in `shared/providers/<name>/<version>/`, merging corrections | Decided in area 11, not built: OpenAPI in chunk 7.2, the rest when first needed |
| `jadpo package record <connection> <operation>` / `conform <package> <capability>` | Confirms an operation against the sandbox and saves the recording; runs a capability's conformance suite | Decided in area 11, not built: `record` in chunk 7.2, `conform` in 7.3 |
| `jadpo package check` / `publish` | Runs the publishing bar, writes `surface.jadpo_record` and compares it with the previous tag's; publishes to the registry | Decided in area 11 (2026-10-16), not built: `check` in chunk 7.1, `publish` with the hosted registry |
| `jadpo package update <name>` / `remove <name>` | Moves a package to a new version, printing area 18's review of the surface the app uses; removes one | Decided in area 11, not built: chunk 7.1 |
| `jadpo package show <name> [<declaration>]` | Prints a package's description, licence, version, kind and public surface, or one declaration with its file; `--guide` prints `GUIDE.md` | Decided in area 11 (2026-10-16), not built: chunk 7.1 |
| `jadpo package link <path>` / `unlink <name>` | Compiles the app against a package's live checkout, recorded in the lock and refused by production builds and CI; restores the installed version | Decided in area 11 (2026-10-16), not built |
| `jadpo package recipe <package> <recipe> --into <domain>` | Copies a package's recipe into a domain, where it becomes the app's code | Decided in area 11 (2026-10-16), not built |
| `jadpo services` / `start` / `stop` / `restart <name>` / `logs <name>` / `shell <name>` | Each container a package runs: package, image, state, health, CPU and memory, restarts, uptime; manages one; its logs (`--follow`, `--since`); a shell inside it, development only. In production they talk to the running app, never to the engine | Decided in area 11 (2026-10-16), not built |
| `jadpo calls` / `calls show <id>` / `calls script <operation> <failure>` | The development call log, faked or real, filtered by `--connection`, `--operation` or `--request`; one call in full; the next answer for an operation | Decided in area 11 (2026-10-16), not built: chunk 7.3 |
| `jadpo dev grants add <connection> --owner <id>` | Makes a fake OAuth grant for local development | Decided in area 11 (2026-10-16), not built |
| `jadpo files copy` | Copies files between two stores (S3, R2, disk) | Decided in area 16 (2026-10-16), not built |
| `jadpo runs unfinished --build <build id>` | Run against production before a deploy: lists the unfinished multi-step runs and batches whose recorded path the new build changes, so an operator knows what will stop as `diverged` (area 12) | Decided 2026-10-16, not built |
| `jadpo batches list` / `show <id>` / `cancel <id>` | Lists batch runs (file reads and writes, `batches.each`) with their state and progress; one run's progress and problems; stops one, as `batches.cancel` does (area 12) | Decided 2026-10-16, not built |
| `jadpo search reindex` | Rebuilds a search index from the database | Decided in area 16 (2026-10-16), not built |
| `jadpo trace <id>` / `jadpo why <name>` / `jadpo call` / `jadpo console` | Debugging (area 17) | Proposed in area 17; `jadpo why <schedule>` built (chunk 6.2b), with `--json`: why a schedule didn't run, or how its last run went, ending with its `.jadpo` place (area 12, "Watching schedules"); another name is `cli.why_arguments` |
| `jadpo incident [file \| -]` | Explains a runtime event: from a file, from standard input (`pbpaste \| jadpo incident -`, or a log stream piped in), or, with no argument in a terminal, from text pasted at a prompt; it finds the Jadpo events inside surrounding log text and groups them by request | Decided 2026-10-13, not built: comes with debugging (area 17) |
| `jadpo lsp` | Runs the language server for editors | Built (chunk 1.7): diagnostics, hover and go to definition, from every use of a name and from a `use` line since chunk 3.5; the VS Code extension in `editors/vscode` starts it (chunk 3.6) |
| `jadpo help [command]` | Help for one command; `--help` works everywhere | Built (chunk 1.1): `jadpo help [command]`, `--help` with examples on every command, and `jadpo help --json` with the rule catalogue and the schemas of the manifest and of `jadpo test --json` |
| `jadpo completions <shell>` / `jadpo version` | Prints the Tab-completion script for zsh, bash or fish; prints the version | Built (chunk 1.1) |

## Running tests

`jadpo test` owns everything a test run needs, so nothing about it appears in
source (proposed in area 13, question 8). It starts a disposable local PostgreSQL, or uses
`JADPO_TEST_DATABASE_URL` in CI; builds one template database from the
migrations and copies it per test, so tests run in parallel; and keeps a
failed test's database with `--keep-failed`. `--kind route` runs one kind and
`--affected` the tests a change reaches (question 7). The report lists each
failed test with the failure it got and the one it expected.

The compiler also generates tests from facts it already owns: every route
rejects bad input with `{ field, rule }` details, every declared failure
reaches the client in the right envelope and status, and no output has a
field its type does not declare. They run with the others and appear in the
report under the route they test.

## Formatting

The formatter owns layout, so equivalent code always looks identical and a
diff only ever shows real changes (decided in area 1). It decides indentation,
spacing, blank lines (one between declarations, none doubled) and the order of
sections inside a declaration. It writes one member per line (question 8).
`jadpo fmt --check` fails when a file differs, and the editor formats on save.

Sections go in one order: a failure's `kind`, `code`, `message`, `client`,
`internal`; a route's `authentication`, `rate_limit`, `input`, `output`,
`status`, `deadline`, `run`; an entity's fields, then `identity`,
`persistence`, `lifecycle`, `membership`, `policy`, `freshness`, `guard`,
`person`. Anything else keeps its written order after these, fields are never
reordered, and a section's comments move with it.

The one exception is `config { }`: its fields are sorted by name, each with
its comments, so two workspaces that each add a setting rarely touch the
same lines (area 10, [Fields in name order](10-configuration-and-time.md#fields-in-name-order);
decided 2026-10-16 by Richard, built in chunk 5.3f).

Inside a project, `jadpo fmt` fixes each file's `use` lines before it lays
the file out (area 1, Imports): it adds a missing import, removes an unused
one and moves a name imported from the wrong folder, by applying the edits
`jadpo check`'s import problems carry, then sorts the lines. That needs the
project's names resolved, so it waits until every file parses. `jadpo fmt
--check` fails on a file whose imports it would change, as on one it would
lay out differently. Built in chunk 3.5.

Inside a project, `jadpo fmt` also writes each file's declarations in
definition-before-use order, below its opening comments and `use` lines:
each before the first that uses it, so the primary declaration comes last,
ties broken by kind and then by the order the primary first reaches them,
and declarations that use each other in the order they were written, each
with the comments written above it and after it on its line (area 1, Files
and names). Test files and loose files have no primary declaration and keep
their order. `jadpo fmt --check` fails on a file out of it. Built in chunk
5.5h3, replacing chunk 3.9's primary-first move.

Promotion is the one layout change that moves code between files: when a
second file uses a type or helper, the compiler reports
`layout.promotion_required` with a one-step fix. That fix is applied through
the editor's quick fix or `jadpo fmt --layout`, never on save, so a move is
always a visible, separate change (decided in area 1). `jadpo fmt --layout`
makes every such move a type needs (out of its file with its comments, into
a file of its own below the `use` lines it needs), then fixes the imports
the moves touched; a helper function's move into a library changes its
calls, so it stays a change to make by hand (built in chunk 3.5).

```jadpo fragment
type Flags = Object {
    done: Bool
    pinned: Bool
}

type Count = Int {
    min: 0
}
```

## Editor support

The language server (`jadpo lsp`) gives any editor the compiler's own view:
errors while you type, go to definition, find references, hover with types and
the failures a call can produce, completion (below), rename and formatting. The VS
Code extension in `editors/vscode` adds highlighting and starts the server. It
contains no parser of its own, and a command to open any generated file comes
with the generated files.

> **Built (chunk 3.6).** The extension starts `jadpo lsp` for `.jadpo` files,
> so the errors, hover and go to definition above work in VS Code, and
> highlights keywords, declarations, types, strings, numbers and quantities,
> comments, operators, route paths and `use` lines. The `jadpo.server.path`
> setting names the binary (`jadpo` on `PATH` by default, or
> `${workspaceFolder}/target/release/jadpo` for a checkout's own build), and a
> missing binary says so. The highlighting grammar is written by hand and
> `tools/editor_grammar.py`, part of `tools/verify.py`, fails when it leaves
> out or adds a word the lexer reserves. Install and settings are in
> `editors/vscode/README.md`.

> **Built (chunk 1.7).** `jadpo lsp` gives the errors `jadpo check` gives
> while you type, from one database kept for the server's life, so an edit
> rechecks only what it changes; hover with a declaration written as Jadpo,
> its `///` description, the failures a callable can produce, a failure's
> code, kind and status, and who calls or uses it; and go to definition.
> Go to definition works from every use of a name, in a body, a signature
> or a `use` line (area 1, Imports; chunk 3.5).

Decided in area 1 and not built: promotion comes with quick fixes; rename goes through the same code as `jadpo rename`, so an
entity field renamed in the editor keeps its stored data (question 5).

Decided in area 10 and built in chunk 5.4d: go to definition from a
`translations.<domain>.<message>(...)` call goes to its message type and to
its wording in each language file.

Decided in area 10 on 2026-10-16 (Richard) and built in chunk 5.3f:
"Make this a setting", a code action on a constant's declaration or any
read of it, goes through the same code as `jadpo config move <constant>`
and refuses what it refuses: a refused move is offered unavailable, with
its reasons. Its edit replaces each changed file, creates `config.jadpo`
when there is none and deletes a file the move empties; an editor that
resolves code actions gets it only once the action is chosen. It is
offered, not a fix for an error, so `jadpo fmt --layout`, which makes the
moves errors ask for, never makes it.

Decided in area 1 (question 11, 2026-10-17) and built in chunk 5.4b:
"Decline with a reason", a quick fix on a declinable finding, writes
`decline <rule>: ""` above the declaration the finding is on, and
`decline.no_reason` asks for the reason until it is written;
"Remove decline", on a `decline` line, removes the line. Both go through
the same code as `jadpo declines clear`.

Decided in area 19 (question 4, 2026-10-17) and built in chunk 5.5a2:
"Swap for the standard ...", a quick fix on `type.standard_clash` and
`type.standard_exists`, goes through the same code as `jadpo fix <name>
swap` and is unavailable, with the error it would add, where the command
stops. The rename needs a name, so it stays the command's until the
editor's rename comes with `jadpo rename`.

Decided in area 6 and not built: hovering over a query shows its generated
SQL, the same as `jadpo sql`. The SQL is never written into the source as a
comment, which would go stale, add noise to diffs and invite edits.

### Completion

Decided 2026-10-17 (Richard): the editor offers completion for everything
as you type, and it is generated from the same declarations and data the
compiler uses, never from a list kept for the editor: the project's name
and type tables, the standard library's stubs and the standard types'
data entries, the operation, unit and enum tables, the rule catalogue, the
settings, the translations' messages and the parser's own keyword sets.
So every type, unit and operation a later chunk adds is offered with no
editor code of its own. Where the position requires a type (an argument,
a field, a variable's declared type, a `return`), what fits comes first,
and a unit or a variant that can't go there isn't offered. The contexts:

- **Names in scope** in a body: locals and parameters, the file's
  constants and its domain's callables, types, libraries, the domains it
  may call into and the standard namespaces. A project name the file
  doesn't import yet comes with its `use` line, the edit `jadpo fmt`
  would make, and another domain's private name isn't offered.
- **Members after a value**: `note.` offers the fields the checker reads
  from its type.
- **Operations after a namespace**: `list.`, `text.`, `temporal.` and the
  rest offer each operation with its signature, the failures it can raise
  and its description, from its stub.
- **Variants after a type**: `Shade.` offers the enum's variants, and a
  standard type's are shown with the name its data entry gives people
  (`Locale.en_gb` is "British English").
- **Units straight after a number**, narrowed to the type it goes into:
  after `2` where a `Duration` is required, `d`, `h`, `m`, `s` and `ms`;
  none where an `Int` is. The units are `jadpo-std`'s table, so quantity
  types offer theirs once they have units.
- **Named arguments** of the standard-library call being written, those
  not given yet, the required first; and the fields not given yet of a
  record being built.
- **Type names**, the application's and the standard ones, wherever a type
  is written.
- **Rule ids after `decline`**: the declinable rules only.
- **`config.`**: the settings; **`translations.<domain>.`**: the messages.
- **Keywords** where a declaration or a statement may start.

Choosing a call with required arguments inserts it as a snippet with a
placeholder for each: the first in its place and the rest by name for a
standard-library operation (`temporal.resolve(${1:date}, at: ${2:at},
zone: ${3:zone})`), every one in its place for the application's own
callables. The server offers what the compiler knows and nothing else:
suggestions written by a model belong to the agent, never to `jadpo lsp`.
VS Code asks for completion as you type a word, a `.`, a `:` or a digit.

> **Built (chunk 5.5a3).** Every context above, in `jadpo_check::completion`
> through the checker's analysis of the text as it is typed, and the
> server's `textDocument/completion`. A test offers every operation each
> stub declares after its namespace, with no operation named in the
> completion code.

## Error messages

Every error has one shape, in the terminal, the editor and JSON: what is
wrong, where, why, the recommended next step, who decides (`agent` or
`human`), the rule ID and a help link. One mistake gives one error; the
compiler does not report knock-on errors. An error reads:

```text
error: `titel` isn't defined in this scope
 --> domains/planning/libraries/titles.jadpo:6:12
  |
6 |     return titel
  |            ^^^^^
  reason: Jadpo could not find this name. Did you mean `title`?
  next: Use `title` instead of `titel` (agent)
  rule: type.unknown_value
  help: docs/language/03-logic.html#type-unknown-value
```

What question 6 decided:

- Paths are relative to the project.
- The two impact lines appear only when they say something.
- An obvious repair carries its edit, so the editor and agents can apply it.
- The help link opens the error's own entry on these language pages, which
  already have an example for every rule.
- JSON gives line and column as well as byte offsets, and uses one naming
  style for its keys.

## The `build/` folder

`build/` holds only generated files. It is never edited, never committed and
never read as input, and the same source always produces the same bytes.
Anything a person reviews and keeps, such as the record of the public API
(area 7) or the identity registry (area 5), lives outside it.

```text
build/
  app.meta.json               project map (also printed by jadpo inspect)
  catalogue/
    libraries.md              every library function and constant: signature or type and value, description, callers or readers
    failures.md               every failure: code, kind, status, client fields
  report/
    database.md               every table, column and index, with the query that needs it
    queries.md                each query call: store or cache, row bound, queries per request
    permissions.md            who can do what to every entity and field, and the actions listed apart
  diagnostics/                the error catalogue
  inventory/                  routes and callables
  audit/                      failures, entities, transactions, configuration, policy
  openapi/openapi.json
  compatibility/              public failure codes
  approval/                   the behaviour review subject
  target/                     the runnable app (jadpo build only)
```

> **Built (chunk 1.7).** For a project, `jadpo check` writes
> `build/manifest.json` (area 18) and the catalogues
> `build/catalogue/libraries.md` and `failures.md` with `.json` twins, errors
> or not, so what an agent looks up is never stale; a file is rewritten only
> when its bytes change, and loose files write nothing. The reports, OpenAPI
> and the monitoring rules come with their areas.

The schema report and the permissions report are decided
(areas 5 and 9) and not built; the query report is area 6's input, kept beside the schema report
because a reviewer reads them together. Each is written as Markdown, which an agent can search, with a `.json`
twin for tools (question 10).

## Tools for agents

Agents look things up through the compiler rather than browsing folders.
These were decided in areas 1, 4 and 5. The library and failure catalogues
and the similar-helper check are built (chunk 1.7); the rest are not yet.

- **Library catalogue** (`build/catalogue/libraries.md`): every library
  function with its signature, `///` description and callers, and every
  library constant with its type, value, description and readers (area 3,
  2026-10-16). An agent reads it before writing a helper or a constant.
- **Similar-helper check**: when a new helper looks like an existing one,
  `jadpo check` reports `library.similar_exists` and names it; two constants
  of the same type holding the same value are `constant.similar_exists`
  (area 3, 2026-10-16, built in chunk 5.3e). *Amended 2026-10-17 (Richard, area 1, question 11):* no warning
  severity; both are declinable errors, built in chunk 5.4b: the fix, or a
  `decline <rule>: "<reason>"` line above the declaration.
- **Failure catalogue** (`build/catalogue/failures.md`): every failure with
  its code, kind, status and client fields.
- **Schema and query reports** (`build/report/`): every table, column and
  index with the query that needed it, and each query call's store, row
  bound and query count.
- **`jadpo impact <name>`**: what a change to a declaration reaches.

```text
$ jadpo impact rounding.half_even
rounding.half_even   shared/libraries/rounding.jadpo:4
  called from   ordering (3 calls), payments (1 call)
  entry points  POST /orders, POST /payments/refunds, schedule nightly_invoices
  tests         ordering/actions/place_order.test.jadpo, payments/actions/refund.test.jadpo
  public        no
```

- **`jadpo domain rename`, `split` and `merge`**: move files and update every
  call. A new or renamed domain appears in the approval review (area 1).
- **The Jadpo agent skill** (`skills/jadpo-agent/SKILL.md`): tells an agent
  to run `jadpo check --json` after edits, read the catalogues before adding
  code, and run `jadpo impact <name>` before changing or removing anything
  shared or public. It points at these language pages for syntax.
  It also gives the routine for agents working in parallel workspaces
  (2026-10-16, Richard): before pushing, merge `main`, run `jadpo check`
  (it builds nothing: 0.2 s on the 66-file library-loans example), fix any
  clash, then push. Private functions and types are unique only within
  their domain (area 1), so two domains' helpers don't clash, but a name
  another agent took in the same domain, or across the project, does.
  It gives the rule for a fixed value (2026-10-16, Richard): if a
  deployment could ever need a different value it is a setting in
  `config.jadpo`, otherwise a `constant` (area 3); never both, so a
  `default:` never names a constant, and a constant that turns out to need
  per-deployment values is moved with `jadpo config move <constant>`
  rather than by hand (area 10).
- **Generated CI runs `jadpo check` first** (2026-10-16, Richard), so a
  name clash or any other check error fails in seconds, before tests or a
  build. No page yet says what generates a project's CI (`jadpo new`
  creates none); whatever does must follow this. Neither the skill nor
  generated CI is built.

## Suggestions

Decided 2026-10-17 (Richard); built in chunk 5.5x. `jadpo suggest` says
where code could use a standard type or a standard operation, and never
edits. The edits stay with the agent, and, for type clashes only, with
`jadpo fix`. It is how an agent finds hand-rolled money, durations, points
and loops after writing code, and how Jadpo learns which of its signals are
right.

It formats first, as `jadpo fmt` does (a file with syntax errors stops it,
as every command that writes source stops), then scans the program graph:
every type, field and entity, and every function, action and constant body.
Tests, fixtures and seeds are left out, since they follow the code they test.

### Signals

What it looks for is data on each standard type's contract entry (area 19,
"Every standard type meets one contract"), never code of its own: a type
adds signals by adding data. Each kind of signal has one engine that reads
every entry.

| Signal | Data on the entry | Fires on | Example |
|---|---|---|---|
| `shape` | The fields `type.standard_exists` matches (area 19, question 5) | A type or entity holding all of a shape's fields among others of its own; exactly the shape is `type.standard_exists`, a `jadpo check` error | `latitude` and `longitude` in `Shop` → `Point` |
| `conversion` | Constants code multiplies or divides by, and the words of names a value of the type goes by; a quantity's factors between its units and its units' names and codes | A multiplication or division by one of the constants whose other side has a name with one of the words, unless it already is the type | `total_price * 100` → `Money` |
| `name` | The same words, and the built-in types a hinted name is built on | A field, parameter or type built on one of those types whose name has one of the words | `timeout_ms: Int` → `Duration` |
| `format` | Text that formats a value by hand: a currency symbol, a unit's symbol | A text literal holding it, joined with `+` to a value | `"£" + text.from(pence)` → `Money` |
| `validation` | Texts the type's validation accepts and texts it refuses | A regular expression, in `matches` or a `pattern:` rule, that accepts every one and refuses every one | `pattern: "[^@]+@[^@]+\\.[a-z]+"` → `Email` |
| `loop` | A table beside the operation table, since operations have no contract entry: the loop each operation replaces | A `for` loop whose only statement is that loop's | `total = total + item.price` → `list.sum` |
| `taught` | A snippet `jadpo suggest found` recorded, in this checkout | The same canonical form anywhere | as recorded |

The first entries with signals: `Money` (× or ÷ 100 and 1000, the minor
units of two- and three-place currencies; `price`, `pence`, `cents`,
`cost`; `£`, `$`, `€`, `¥`), every quantity (its units),
`Duration` (1000, 60, 3600, 86400; `ms`, `millis`, `seconds`, `minutes`,
`hours`, `timeout`), `Instant` (1000; `epoch`, `timestamp`, `unix`),
`Point` (its shape), `Email`, `Url` and `Uuid` (their validation texts,
and `email`, `url`, `uuid` as names on `Text`), and the loops of
`list.sum`, `list.min_by` and `list.max_by`. `list.group_by`'s loop builds
a map through a chain of calls that the graph shows only as calls, so it
has no loop signal yet. A type's validation texts are data of their own,
not its examples, so only the types that list them fire.

A snippet gives one finding. Where signals for two types fire on one
snippet, the type whose words more of its names hold wins, then the first
in the entries' order; where a snippet holds a smaller one that fires the
same way, only the smaller is reported. A type with `decline
type.standard_exists` gives no `shape` finding.

### Snippets and their canonical form

A finding is about a snippet: the smallest expression or declaration
member that holds the signal. Each snippet has two forms.

- **Its canonical form**, which signals, ignores and taught snippets match
  on: the graph fragment with its structure normalised and its names kept.
  Operands of `+`, `*`, `==`, `!=`, `and` and `or` are sorted, and chains of
  one of them flattened, so grouping doesn't matter; `>` and `>=` become
  `<` and `<=` with the sides swapped; numbers lose their spelling (`100`,
  `100.0`, `1_00`) and constants folded together are one; a named constant
  is its value; a variable that is never reassigned is followed to its
  value; and exactly equivalent forms are one, so `x / 0.01` is `x * 100`
  and `x * 0.01` is `x / 100`. `total_price * 100`, `100 * total_price`
  and `var factor = 100` then `total_price * factor` are one pattern; a
  BLAKE3 hash of it (16 hex digits) is its index, and the canonical form
  itself confirms a match.
- **Its text**: the source as `jadpo fmt` writes it, comments removed, and
  a hash of that text (16 hex digits of BLAKE3 too), which is what `jadpo suggest share` sends
  (roadmap 5.5y): it never depends on the canonicaliser's version, and only
  lines written the same way in many projects reach HQ's threshold.

### The report

```text
$ jadpo suggest
Suggestions
  S1  domains/shop/libraries/pricing.jadpo:6  Money     conversion
      total_price * 100
  S2  domains/shop/entities/shop.jadpo:2      Point     shape
      latitude, longitude
  S3  domains/shop/types/delivery.jadpo:3     Duration  name
      timeout_ms: Int

What to do
  Review the rest of the code against `jadpo suggest --catalogue`.
  Before changing code for something it missed, run
  `jadpo suggest found <file>:<line> <Type or operation> "<why>"`.
  To keep one quiet, run `jadpo suggest ignore <number> "<why>"`.
  When done, run `jadpo suggest share`.

✓ 3 suggestions  3 files
```

Findings are numbered S1, S2… in file order, each with its place, what it
suggests, the signal and the snippet. A snippet goes beside its row when
every one fits in 80 columns, and otherwise every snippet goes on the line
under its row, cut with `…` only if it is longer than that line. With none it
says `✓ No suggestions`, and the instruction stays, since the review of the
rest still applies. The last line, to run `jadpo suggest share`, shows only
while sharing is on and an HQ address is configured ("Sharing with Jadpo
HQ" below). `--json` gives each finding's number, place, range,
suggestion, signal, reason, text, canonical form and both hashes, and the
instruction's lines. `jadpo suggest --catalogue` lists every signal,
generated from the entries and the loop table, as text or `--json`.

A finding that is gone at the next scan was taken: the scan keeps a note of
its last findings in `.jadpo/suggest_last` (local, untracked), and each one
missing from the next scan, and not ignored meanwhile, becomes an
`accepted` entry to share. The summary counts them.

### Ignoring a suggestion

`jadpo suggest ignore S3 "<why>"` writes the snippet's canonical hash, its
signal and the reason to `ignored_suggestions.jadpo_record` at the project
root, committed and shared by the team (area 1's root list has it; area
18's record format):

```text
// Generated by Jadpo from the suggestions this project ignores; update it with `jadpo suggest ignore`, never by hand.
record ignored_suggestions 2

ignore 3f9a1c0e2b7d4a51 conversion Money "Basis points, not money"
```

The snippet, and every variant with the same canonical form, stays quiet
until its code changes; each scan drops an entry no snippet has any more.
It also adds an `ignored` entry (the text, both hashes, the signal and the
reason) to `.jadpo/suggest_pending`. It sends nothing.

```text
$ jadpo suggest ignore S1 "Basis points, not money"
✓ Ignored S1, Money conversion
  snippet  total_price * 100
  place    domains/shop/libraries/pricing.jadpo:6
  reason   Basis points, not money
```

### Teaching a miss

`jadpo suggest found <file>:<line> <Type or operation> "<why>"`, run
before the code changes, records something the scan missed: the snippet at
that line, its text, both hashes, its canonical form, the suggestion and
the reason. It adds a `found` entry to `.jadpo/suggest_pending`, so the
text survives the fix, and teaches the canonical form as a project signal
in `taught_suggestions.jadpo_record` at the project root, committed and
shared by the team like the ignores (area 1's root list has it; area 18's
record format), so the next scan, in this checkout or a teammate's, finds
the same pattern anywhere as a `taught` finding (chunk 5.5y; decided
2026-10-18, lead session). The snippet at a line is the value of the first
statement that starts there (a `return`'s, a variable's, an
assignment's), or the statement itself when it has none, or else the
field or parameter declared there; a line with none of them is
`cli.suggest_arguments`.

```text
// Generated by Jadpo from the patterns this project taught `jadpo suggest`; update it with `jadpo suggest found`, never by hand.
record taught_suggestions 2

taught 8c1d0e5a4f3b2a19 Money "Pence held as Int" "(+ delivery subtotal)"
```

Each line holds the canonical form's hash, the suggestion, the reason and
the canonical form, which confirms a match; the snippet's text stays out of
the record. `.jadpo/suggest_signals` is only a cache of it: a pattern taught
there before the record existed moves into the record at the next scan.

```text
$ jadpo suggest found domains/shop/actions/pence_of.jadpo:5 Money "Pence held as Int"
✓ Recorded a missed Money at domains/shop/actions/pence_of.jadpo:5
  snippet  subtotal + delivery_pence
  reason   Pence held as Int

→ jadpo suggest  finds the same pattern elsewhere
```

### What is kept, and where

Every entry to share goes to `.jadpo/suggest_pending` with its kind,
`found`, `accepted` or `ignored`, so HQ can tell a signal that was right
from one that fired wrongly. `jadpo suggest share` sends them ("Sharing with
Jadpo HQ" below). Everything in `.jadpo/` is local, and the first command
that writes there adds `.jadpo/` to `.gitignore`. `.jadpo/suggest_last`,
`.jadpo/suggest_pending` and `.jadpo/suggest_signals` hold one JSON object
per line, since they hold source text; only the committed records are in
Jadpo's readable format. Those are written at format 2, with the shared
header (area 18, "Checking a record"), as shown above; files written at
format 1, before chunk 7.0, are still read, and `jadpo records update`
rewrites them.

| File | Committed | Holds |
|---|---|---|
| `ignored_suggestions.jadpo_record` | yes | The team's ignores: hash, signal, suggestion, reason |
| `taught_suggestions.jadpo_record` | yes | The team's taught patterns: hash, suggestion, reason, canonical form |
| `.jadpo/suggest_last` | no | The last scan's findings, so a vanished one counts as accepted |
| `.jadpo/suggest_pending` | no | The entries waiting for `share` |
| `.jadpo/suggest_wanted` | no | The exact payload `share --approve` sends, until it is approved or declined |
| `.jadpo/suggest_shared` | no | That this checkout has shared once, so the first-share line shows once |
| `.jadpo/suggest_signals` | no | A cache of the taught record |

### Sharing with Jadpo HQ

Decided 2026-10-17 (Richard); built in chunk 5.5y against a local fake of
Jadpo HQ, which speaks the protocol below, since HQ itself comes after the
first build, with the registry. Sharing is how Jadpo learns which signals
are right: which findings people take, which they ignore and why, and what
the scan missed.

**Sharing is on unless it is turned off, and nothing goes unless someone
runs `share`.** Only hashes go without a person's approval; a snippet's
text never does.

- **The project token.** `app.jadpo` holds
  `suggestions { project_token: "…" }`: 32 random hex digits tied to
  nothing, no repository, person or machine. It is committed, so a team
  counts as one project and HQ counts distinct projects without knowing
  which they are. `jadpo new` writes it when it is built; until then, and
  whenever it is missing, `share` writes it before sending, made from the
  operating system's generator; with nothing pending it writes none.
  `share --new-token` replaces it and sends nothing; a fork shares its
  parent's token until it does. Only 32 lower-case hex digits read as a
  token: any other value is `type.suggestions_value_invalid`, so a
  repository's name can't be used as one.
- **Turning it off.** `suggestions { share: false }` turns sharing off for
  the project, and `JADPO_SUGGEST_SHARE=off` in the environment for a
  whole machine or organisation (any value but `on` or empty turns it off,
  so a misspelling never sends). `share: false` counts in whichever file
  the `suggestions { }` is. Either way every form of `share`, `--approve`
  and `--new-token` included, stops at once, sends nothing, writes nothing,
  keeps the pending entries and says where sharing was turned off, and the
  scan's report drops its instruction to share; the scan, `ignore` and
  `found` still record entries. Neither can turn it back on over the
  other. A `suggestions { }` block with a problem stops `share` too.
- **The exchange.** `share` sends every pending entry's text hash with its
  kind (`found`, `accepted` or `ignored`), its signal and its suggestion,
  and the token, at once and with no prompt: the agent runs it itself at the
  end of a suggest session, as the report's instruction says. The signal
  and suggestion go because HQ reassesses a signal by its ignore rate and
  most entries never send their text; nothing else goes, not even Jadpo's
  version. HQ replies with the hashes whose text it wants, those seen from
  enough distinct projects. The command writes the request's address and
  body to `.jadpo/suggest_wanted`, shows exactly what `share --approve`
  would send, and stops. A person approves, directly or through the agent
  asking them in its conversation, and only then does `share --approve`
  send those bytes to that address, whatever `JADPO_SUGGEST_HQ` says by
  then: the snippets, each with its kind, signal and suggestion and, for
  an ignored one, its reason (a `found` entry's reason stays local); HQ
  recomputes their hashes. `share --decline` drops them and sends
  nothing, and the next `share` replaces what waits, unsent.
- **Unattended runs** (`--unattended`, or `CI` set to anything) send only
  hashes and skip the request; HQ asks the next project that sends that
  hash, and the next `share` drops a request left waiting. An unattended
  run can't `--approve` (`cli.suggest_arguments`), and `share`'s flags
  given without `share` are refused rather than ignored.
- **Afterwards** the pending file is wiped, whether HQ wanted anything or
  not; a send that fails keeps it. The first time a checkout sends hashes
  (noted in `.jadpo/suggest_shared`, so nothing is written outside the
  project), `share` says so in one line, with how to turn sharing off.
- **Until HQ runs** no address is configured by default, so a suggest
  session never ends in a failure while HQ doesn't exist. `share` says
  `Sharing isn't available yet: Jadpo HQ isn't running. Nothing was sent;
  your entries are kept.`, sends nothing, keeps the entries and succeeds,
  and the scan's report leaves out its instruction to share, as it does
  when sharing is off ("The protocol" below).

```text
$ jadpo suggest share
✓ Sent 3 hashes to Jadpo HQ
    kinds  1 accepted, 1 found, 1 ignored
    asked  nothing more
    token  new, in app.jadpo: commit it so every copy counts as one project

Sharing is on
  Only hashes went. To stop sharing, add `share: false` to `suggestions { }`.
```

When HQ asks for snippets, the payload is shown exactly as it will be sent,
then the decision, written so an agent can relay it:

```text
$ jadpo suggest share
✓ Sent 3 hashes to Jadpo HQ
    kinds  1 accepted, 1 found, 1 ignored
    asked  1 snippet

What `jadpo suggest share --approve` sends
  POST https://hq.example.test/v1/suggestions/snippets
  {
    "snippets": [
      {
        "kind": "ignored",
        "reason": "Basis points, not money",
        "signal": "conversion",
        "suggestion": "Money",
        "text": "total_price * 100"
      }
    ]
  }

◆ decision  Jadpo HQ asks for 1 snippet of this project's code
  next      Ask the person whether to send it, showing them the payload above.
            Run `jadpo suggest share --approve` only after they say yes;
            otherwise run `jadpo suggest share --decline`. (human)
```

```text
$ jadpo suggest share --approve
✓ Sent 1 snippet to Jadpo HQ

$ JADPO_SUGGEST_SHARE=off jadpo suggest share
✓ Sent nothing: sharing is turned off
    where  JADPO_SUGGEST_SHARE=off in the environment
```

`--json` gives the counts, the hashes HQ asked for, the payload, the
address it goes to, and an `approval` object an agent relays: the question,
the payload, the two commands and the rule that `--approve` is only valid
after the person said yes. The command can't check that they did, so the
instruction to ask is the agent's, written into the decision's `next`.

#### The protocol

The contract the real HQ implements unchanged. HTTPS and JSON, UTF-8, under
a versioned path.

- **Address.** `JADPO_SUGGEST_HQ` in the environment. There is no default
  yet: the release that launches HQ, with the registry after the first
  build, sets the real address as the default (lead decision, 2026-10-18).
  `http://` is accepted only for a loopback host
  (`localhost`, `127.0.0.1`, `[::1]`), which is how the tests reach the
  fake; anything else must be `https://`, through OpenSSL (already linked
  by the passkey ceremonies) with the platform's certificate store and the
  host name checked.
- **Headers.** `Content-Type: application/json`, `Accept:
  application/json`, `User-Agent: jadpo`, without a version, and
  `Connection: close`: the client is a small HTTP/1.1 one of `jadpo`'s
  own. The token
  in the body is the only identifier the client sends. The client logs
  nothing about a request: no repository name, user, machine or address.
- **`POST /v1/suggestions/hashes`** sends at most 500 entries; `share`
  sends more in several requests.

  ```json
  {
    "token": "5f0c2a9e7b1d4c38a6e2f9b0d7c4a1e3",
    "entries": [
      { "hash": "8c1d0e5a4f3b2a19", "kind": "ignored", "signal": "conversion", "suggestion": "Money" }
    ]
  }
  ```

  `hash` is the text hash (16 hex digits of BLAKE3 of the snippet's text
  after `jadpo fmt`, comments removed); `kind` is `found`, `accepted` or
  `ignored`; `signal` is a signal's word (`taught` for a `found` entry);
  `suggestion` a standard type or operation. HQ answers `200` with the
  entries whose text it wants, each a hash and kind it was sent, so an
  ignored entry's reason can be asked for without the accepted ones'
  texts; at most 20, so an approval stays readable:

  ```json
  { "wanted": [ { "hash": "8c1d0e5a4f3b2a19", "kind": "ignored" } ] }
  ```

  HQ counts, for each hash, kind, signal and suggestion, the distinct
  tokens it has seen it from, so sending the same entry twice changes
  nothing and a retry is safe. It wants a hash's text once that count
  reaches its threshold and it doesn't hold the text yet (an ignored
  entry: until it holds a few reasons). A hash below the threshold is
  never asked for.
- **`POST /v1/suggestions/snippets`**, only after approval, carries no
  token: a snippet is never tied to a project.

  ```json
  { "snippets": [ { "kind": "ignored", "reason": "Basis points, not money", "signal": "conversion", "suggestion": "Money", "text": "total_price * 100" } ] }
  ```

  `text` is the snippet's text without comments, whose hash is the one HQ
  asked for; `reason` is sent only for an ignored snippet. HQ recomputes
  each hash, keeps the snippets it asked for and refuses the rest,
  answering `200` with `{ "received": 1, "refused": 0 }`.
- **Errors.** Any other status has the body
  `{ "error": { "code": "…", "message": "…" } }`: `400 invalid_request`
  (with the field), `413 too_large` (a body over 1 MiB), `410
  version_retired` (this Jadpo is too old to share; update it), `429
  rate_limited` with `Retry-After` in seconds, and `500`, `502`, `503` or
  `504` for HQ's own trouble.
- **Timeouts and retries.** 5 seconds to connect and 15 for each answer.
  A failed connection, a timeout, `429` and `500`, `502`, `503` and `504`
  are tried again up to three times, after half a second, 1 and 2 seconds
  (at most three and a half seconds of waiting), or after `Retry-After`
  when HQ gives one; a `Retry-After` over 60 seconds ends
  the attempt. Anything else is not retried. When the last try fails,
  `share` fails with `suggest.share_failed`, sends nothing more and keeps
  the pending entries for the next run.
- **What HQ keeps.** For each hash, the set of tokens it came from and its
  kinds, signals and suggestions; the texts and reasons it asked for,
  without a token. Nothing else: addresses are used only to rate-limit, in
  memory, and never written down.

#### HQ's agent

HQ runs an agent over what is shared. It turns shared patterns into signal
data for the next release: new constants, words and validation texts on a
type's entry, a threshold, the tests each new signal is generated with, and
the release notes saying what changed. It reassesses a signal whose ignore
rate across distinct projects is high: it reads the ignored snippets and
their reasons beside the accepted ones, then narrows the signal or removes
it, tested against the accepted snippets so it keeps catching what it got
right. Where a pattern points at something the language lacks, it
proposes a language feature for a person to decide on, and never builds
one.

## Review and sign-off

Some changes need a person's acceptance before they ship: a change that can
break an API client (area 7), a database drop (area 5), weaker policy
(area 9) and a new or renamed domain (area 1). They share one acceptance
form, designed in area 7. `jadpo check` fails until each such change has its
acceptance, and `jadpo approval --before <ref>` writes the report a reviewer
reads: what changed in routes, policy, failures and data. Who may write an
acceptance is question 12.

## Errors only the running app or a build raises

The running app reports three events of its own, never compile errors:
`runtime.unhandled_fault` (a defect or fault reached the boundary; the client
gets `internal_fault`), `runtime.outcome_unknown` (a change may or may not
have committed) and `runtime.startup_failed`. Area 4 decided what each logs.

`jadpo build` refuses features the compiler checks but cannot generate yet,
with `toolchain.target_*` errors. `jadpo check` reports the same errors, so
a project that passes `check` also builds (question 11). Area 8 asks the
same for a protected route with no authentication set up.

### Command errors

Every error that only a command raises, so `jadpo check` cannot show it with a
source example. The list predates the decisions above and is rewritten as
each command is built.

| Rule | Raised by | What it means |
|---|---|---|
| `cli.ai_arguments` | `ai` | AI command has invalid arguments |
| `cli.api_arguments` | `api` | Api command has invalid arguments |
| `cli.build_arguments` | `build` | Build command has invalid arguments |
| `cli.build_experiment_scope` | `build` | Experimental target only builds qualified fixtures |
| `cli.build_experiment_unavailable` | `build` | Shared-Rust experiment checkout was not found |
| `cli.build_tool_failed` | `build` | Experimental target build failed |
| `cli.check_arguments` | `check` | Check command has invalid arguments |
| `cli.config_arguments` | `config` | Config command has invalid arguments |
| `cli.config_prompt_failed` | `config` | Secure configuration prompt failed |
| `cli.config_tty_required` | `config` | Configuration entry requires an interactive terminal |
| `cli.dev_arguments` | `dev` | Dev command has invalid arguments |
| `cli.dev_port_invalid` | `dev` | Development port is invalid |
| `cli.dev_start_failed` | `dev` | Development server could not start |
| `cli.fmt_arguments` | `fmt` | Format command has invalid arguments |
| `cli.incident_arguments` | `incident` | Incident command has invalid arguments |
| `cli.incident_invalid` | `incident` | Runtime event is not a valid enrichment input |
| `cli.incident_manifest_invalid` | `incident` | Local build manifest is not valid JSON |
| `cli.incident_manifest_missing` | `incident` | Local build manifest is missing |
| `cli.incident_manifest_stale` | `incident` | Local build manifest does not match current sources |
| `cli.incident_operation_unknown` | `incident` | Runtime operation is absent from the local manifest |
| `cli.incident_read_failed` | `incident` | Runtime event file could not be read |
| `cli.incident_revision_mismatch` | `incident` | Runtime event and local source revisions differ |
| `cli.lsp_arguments` | `lsp` | Language-server command has invalid arguments |
| `cli.presentation_arguments` | any command | Terminal presentation option is invalid |
| `cli.project_required` | any command | Command requires a project path |
| `cli.routes_arguments` | `routes` | Routes command has invalid arguments |
| `cli.schedules_arguments` | `schedules` | Schedules command has invalid arguments |
| `cli.schema_command_required` | `schema` | Schema command requires a subcommand |
| `cli.schema_decision_arguments` | `schema` | Schema decision command has invalid arguments |
| `cli.schema_diff_arguments` | `schema` | Schema diff command has invalid arguments |
| `cli.schema_index_accept_arguments` | `schema` | Index acceptance requires a recommendation path |
| `cli.schema_plan_arguments` | `schema` | Schema plan command has invalid arguments |
| `cli.schema_rename_arguments` | `schema` | Schema rename command has invalid arguments |
| `cli.schema_snapshot_arguments` | `schema` | Schema snapshot command requires an output path |
| `cli.schema_sql_arguments` | `schema` | Schema SQL command has invalid arguments |
| `cli.signal_handler_failed` | any command | Shutdown handler could not be installed |
| `cli.suggest_arguments` | `suggest` | Suggest command has invalid arguments |
| `cli.test_arguments` | `test` | Test command has invalid arguments |
| `cli.unknown_command` | any command | Unknown Jadpo command |
| `cli.unknown_schema_command` | `schema` | Unknown schema subcommand |
| `cli.watch_arguments` | `watch` | Watch command has invalid arguments |
| `cli.watch_input_read_failed` | `watch` | Watched project inputs could not be read |
| `cli.watch_output_failed` | `watch` | Watch lifecycle event could not be written |
| `cli.why_arguments` | `why` | Why command has invalid arguments |
| `configuration.local_changed` | `config` | Local configuration changed during the update |
| `configuration.local_invalid_encoding` | `config` | Local configuration is not UTF-8 text |
| `configuration.local_read_failed` | `config` | Local configuration could not be read |
| `configuration.local_syntax` | `config` | Local configuration syntax is invalid |
| `configuration.local_unsafe_target` | `config` | Local configuration target is unsafe |
| `configuration.local_write_failed` | `config` | Local configuration could not be saved |
| `configuration.move_name_taken` | `config move`, the editor | A setting already has the constant's name |
| `configuration.move_read_unsupported` | `config move`, the editor | Constant is read where a setting can't be |
| `configuration.move_type_unsupported` | `config move`, the editor | Constant's type can't be a setting |
| `configuration.value_invalid` | `config`, `dev` | Local configuration value is invalid |
| `configuration.value_missing` | `config`, `dev` | Required local configuration is missing |
| `format.changes_required` | `fmt` | Source files are not canonically formatted |
| `format.write_failed` | `fmt` | Formatted source could not be written |
| `index.accept_check_failed` | `schema index-accept` | Index edit did not pass project checks |
| `index.accept_identity_count` | `schema index-accept` | Index acceptance changed an unexpected number of schema identities |
| `index.accept_range_invalid` | `schema index-accept` | Recommended index insertion point is no longer valid |
| `index.accept_registry_missing` | `schema index-accept` | Schema identity registry is required before accepting an index |
| `index.accept_rollback_failed` | `schema index-accept` | Failed index edit could not be rolled back |
| `index.accept_source_read_failed` | `schema index-accept` | Source file for the recommended index could not be read |
| `index.accept_source_write_failed` | `schema index-accept` | Recommended index could not be written to source |
| `index.recommendation_field_missing` | `schema index-accept` | Recommended field no longer exists |
| `index.recommendation_unknown` | `schema index-accept` | Index recommendation is not current |
| `lsp.content_length_missing` | `lsp` | Language-server message has no Content-Length header |
| `lsp.json_invalid` | `lsp` | Language-server message is not valid JSON |
| `lsp.json_write_failed` | `lsp` | Language-server response could not be encoded |
| `lsp.read_failed` | `lsp` | Language-server input could not be read |
| `lsp.root_missing` | `lsp` | Language server has no workspace root |
| `lsp.write_failed` | `lsp` | Language-server response could not be written |
| `migration.decision_artifact_exists` | `schema` | Migration decision file already exists |
| `migration.decision_artifact_invalid` | `schema` | Migration decision file is invalid |
| `migration.decision_artifact_read_failed` | `schema` | Migration decision file could not be read |
| `migration.decision_artifact_write_failed` | `schema` | Migration decision template could not be written |
| `migration.decision_change_set_stale` | `schema` | Migration decisions refer to a different change set |
| `migration.decision_duplicate` | `schema` | Schema change has more than one migration decision |
| `migration.decision_evidence_duplicate` | `schema` | Migration decision repeats an evidence kind |
| `migration.decision_evidence_empty` | `schema` | Migration decision contains empty evidence |
| `migration.decision_evidence_missing` | `schema` | Migration decision is missing required evidence |
| `migration.decision_evidence_unexpected` | `schema` | Migration decision contains unsupported evidence |
| `migration.decision_missing` | `schema` | Schema change has no migration decision |
| `migration.decision_strategy_invalid` | `schema` | Migration strategy is not valid for this change |
| `migration.decision_unexpected` | `schema` | Decision file contains an unrelated schema change |
| `migration.decision_unresolved` | `schema` | Migration decision still needs a strategy |
| `migration.identity_additions_have_removal` | `schema` | Schema additions cannot be registered while identities are missing |
| `migration.identity_registry_exists` | `schema` | Schema identity registry already exists |
| `migration.identity_registry_missing` | `schema` | Schema identity registry is missing |
| `migration.identity_registry_read_failed` | `schema` | Schema identity file could not be read |
| `migration.identity_registry_write_failed` | `schema` | Schema identity registry could not be written |
| `migration.identity_rename_kind` | `schema` | Schema rename kind is not supported |
| `migration.identity_rename_source_unknown` | `schema` | Schema rename source identity does not exist |
| `migration.identity_rename_target_exists` | `schema` | Schema rename target already has an identity |
| `migration.identity_rename_target_unknown` | `schema` | Schema rename target is absent from checked source |
| `migration.identity_snapshot_exists` | `schema` | Schema snapshot output already exists |
| `migration.identity_snapshot_invalid` | `schema` | Schema snapshot is invalid |
| `migration.identity_snapshot_missing` | `schema` | Previous schema snapshot is missing |
| `migration.identity_snapshot_not_canonical` | `schema` | Previous schema snapshot is not canonical |
| `migration.identity_snapshot_shape_missing` | `schema` | Snapshot entry is missing its saved definition |
| `migration.identity_snapshot_write_failed` | `schema` | Schema snapshot could not be written |
| `migration.plan_adapter_invalid` | `schema` | Migration adapter is not supported |
| `migration.plan_decision_rejects_change` | `schema` | Reviewed decision rejects this schema change |
| `migration.plan_exists` | `schema` | Migration plan output already exists |
| `migration.plan_write_failed` | `schema` | Migration plan could not be written |
| `migration.sql_change_unsupported` | `schema` | Schema change has no SQL review generator |
| `migration.sql_expression_unsupported` | `schema` | Backfill expression cannot be compiled to SQL |
| `migration.sql_field_owner_missing` | `schema` | Migration field has no resolvable owning table |
| `migration.sql_field_shape_invalid` | `schema` | Saved field definition is incomplete |
| `migration.sql_literal_invalid` | `schema` | Backfill literal is invalid for the field type |
| `migration.sql_predicate_unsupported` | `schema` | Validation check cannot be compiled to SQL |
| `migration.sql_review_exists` | `schema` | Migration SQL review output already exists |
| `migration.sql_review_write_failed` | `schema` | Migration SQL review could not be written |
| `migration.sql_sqlite_constraint_missing` | `schema` | SQLite rebuild cannot resolve a constraint identity |
| `migration.sql_sqlite_entity_missing` | `schema` | SQLite rebuild cannot find the current entity |
| `migration.sql_sqlite_field_identity_missing` | `schema` | SQLite rebuild cannot resolve a field identity |
| `migration.sql_sqlite_field_missing` | `schema` | SQLite rebuild cannot find the current field |
| `migration.sql_sqlite_index_missing` | `schema` | SQLite rebuild cannot resolve an index identity |
| `migration.sql_sqlite_rebuild_required` | `schema` | SQLite change requires a table rebuild |
| `migration.sql_sqlite_rebuild_unsupported` | `schema` | This combination of SQLite table changes is not supported |
| `migration.sql_sqlite_reference_missing` | `schema` | SQLite rebuild cannot resolve a reference target |
| `migration.sql_sqlite_renamed_table_unsupported` | `schema` | SQLite rebuild cannot target a logically renamed table |
| `migration.sql_strategy_unsupported` | `schema` | Reviewed migration strategy has no SQL implementation |
| `migration.sql_type_unsupported` | `schema` | Field type has no migration SQL mapping |
| `runtime.outcome_unknown` | the running app | Generated runtime could not confirm whether an effect committed |
| `runtime.startup_failed` | the running app | Generated runtime failed during startup |
| `runtime.unhandled_fault` | the running app | Generated runtime contained an unexpected fault |
| `schedules.lease_held` | `schedules run` | Schedule is already running |
| `schedules.no_tick` | `schedules run` | No tick picked up the run asked for |
| `schedules.run_failed` | `schedules run` | Schedule's run failed |
| `schedules.unavailable` | `schedules`, `why` | Schedules' database couldn't be read |
| `suggest.record_invalid` | `suggest` | Suggestions record can't be read |
| `suggest.share_failed` | `suggest share` | Jadpo HQ couldn't be reached or refused what was shared |
| `test.failed` | `test` | Test run failed |
| `test.no_tests` | `test` | No tests to run |
| `test.runtime_start_failed` | `test` | Test runtime could not start |
| `toolchain.artifact_cleanup_failed` | `artifacts`, `build` | Previous build backup could not be removed |
| `toolchain.artifact_promote_failed` | `artifacts`, `build` | Staged build artifacts could not be promoted |
| `toolchain.artifact_stage_failed` | `artifacts`, `build` | Build staging directory could not be created |
| `toolchain.artifact_write_failed` | `check`, `build` | Generated file could not be written |
| `toolchain.project_not_found` | any command | Project path does not exist |
| `toolchain.project_read_failed` | any command | Project directory could not be read |
| `toolchain.scaffold_destination_exists` | `new` | Scaffold destination is an existing file |
| `toolchain.scaffold_destination_not_empty` | `new` | Scaffold destination directory is not empty |
| `toolchain.scaffold_name_invalid` | `new` | Project destination has no valid name |
| `toolchain.scaffold_read_failed` | `new` | Scaffold destination could not be inspected |
| `toolchain.scaffold_write_failed` | `new` | Project scaffold file could not be written |
| `toolchain.source_read_failed` | any command | Jadpo source file could not be read |
| `toolchain.target_auth_not_implemented` | `build` | Protected route `{route}` cannot be generated yet |
| `toolchain.target_durable_workflow_not_implemented` | `build` | Durable workflow target execution is not implemented |
| `toolchain.target_job_not_implemented` | `build` | Scheduled job `{name}` cannot execute yet |
| `toolchain.target_liveness_not_local` | `build` | Authored liveness route is not dependency-free |
| `toolchain.target_store_not_implemented` | `build` | Authority store target adapter is not implemented |
| `translation.exchange_file_invalid` | `translations export`, `translations import` | Translation exchange file can't be read or written |
| `translation.exchange_unit_unknown` | `translations import` | Translation file holds a unit the project does not have |

Three errors cannot be reached at all: `type.job_delivery_selector_invalid`
(Delivery selector does not match its entity types),
`type.job_delivery_service_invalid` (Delivery service does not match its
closed payload types) and `type.job_delivery_sealed_use` (Compiler-owned
delivery value cannot be authored or exposed). They go with the delivery
descriptor if area 12's question 2 is accepted.

## Errors

Every error this area can produce from source, with an example that
triggers it.

### `toolchain.no_sources`

A project folder with no `.jadpo` files, here only a README:

```jadpo project=e-no-sources file=README.md expect=toolchain.no_sources
A project folder with notes but no .jadpo files.
```

### `syntax.suggestions_unknown_option`

`suggestions { }` takes `project_token:` and `share:`, nothing else.

```jadpo check project=e-suggestions-unknown-option expect=syntax.suggestions_unknown_option
suggestions {
    shared: false
}
```

```text output
error: `suggestions` has no option `shared:`
 --> main.jadpo:2:5
  |
2 |     shared: false
  |     ^^^^^^ no such option
  reason: `suggestions` takes `project_token:` and `share:`. Did you mean `share`?
  next: Use `share:` instead of `shared:` (agent)
  rule: syntax.suggestions_unknown_option
  help: docs/language/14-tooling.html#syntax-suggestions-unknown-option

check failed: 1 error, 1 file
```

### `syntax.suggestions_duplicate_option`

Each option is given once.

```jadpo check project=e-suggestions-duplicate-option expect=syntax.suggestions_duplicate_option
suggestions {
    share: false
    share: true
}
```

```text output
error: `share:` is given twice
 --> main.jadpo:3:5
  |
2 |     share: false
  |     ----- first given here
3 |     share: true
  |     ^^^^^ given again
  reason: `suggestions` says each of `project_token:` and `share:` once.
  next: Keep one `share:` and remove the other (agent)
  rule: syntax.suggestions_duplicate_option
  help: docs/language/14-tooling.html#syntax-suggestions-duplicate-option

check failed: 1 error, 1 file
```

### `type.suggestions_value_invalid`

A token is the 32 random hex digits `jadpo suggest share` writes, so a name can't stand in for one; `share:` is `true` or `false`.

```jadpo check project=e-suggestions-value-invalid expect=type.suggestions_value_invalid
suggestions {
    project_token: "acme-shop"
}
```

```text output
error: `project_token: "acme-shop"` isn't a value `project_token` takes
 --> main.jadpo:2:20
  |
2 |     project_token: "acme-shop"
  |                    ^^^^^^^^^^^ not a token
  reason: A project token is 32 random hex digits, tied to nothing, so Jadpo HQ can tell one project from another without knowing which it is.
  next: Remove `project_token:`; `jadpo suggest share` writes a new one (agent)
  rule: type.suggestions_value_invalid
  help: docs/language/14-tooling.html#type-suggestions-value-invalid

check failed: 1 error, 1 file
```

### `semantic.multiple_suggestions`

A project has one `suggestions { }`.

```jadpo check project=e-suggestions-multiple expect=semantic.multiple_suggestions
suggestions {
    share: false
}

suggestions {
    share: true
}
```

```text output
error: `suggestions { }` is declared twice
 --> main.jadpo:5:1
  |
1 | suggestions {
  | ----------- first declared here
5 | suggestions {
  | ^^^^^^^^^^^ declared again
  reason: A project has one token and one answer to whether it shares, in one `suggestions { }` in `app.jadpo`.
  next: Move what this `suggestions` holds into the first, and remove it (agent)
  rule: semantic.multiple_suggestions
  help: docs/language/14-tooling.html#semantic-multiple-suggestions

check failed: 1 error, 1 file
```

## Review

### Status

Each row says what the new compiler has built, with its chunk, and what
is decided but not built yet.

| Feature | Status |
|---|---|
| Command help | Built (chunk 1.1): every command has `--help` with examples, `jadpo help [command]` explains one, and `jadpo help --json` describes them all. |
| Project path | Built (chunk 1.1): every command finds the project by walking up to `app.jadpo`, so a path is optional; run from outside the project, printed paths start with the project as typed (chunk 2.8). |
| Scaffold | Decided (question 9), not built: `jadpo new` is `cli.unknown_command`. |
| Formatter | Built (chunk 1.2): one member per line, one blank line between declarations, and `fmt --check` counts the files it read; since chunk 3.5 it keeps `use` lines at the top, sorted, fixes imports inside a project, and `--layout` makes promotion moves; since chunk 5.5h3 it writes each file's declarations before their use, the primary declaration last (chunk 3.9 had put it first). |
| Error messages | Built (chunks 1.1 on): one error per mistake in area 14's field order, `impact` only when it says something, edits on the next step where one exists, and help links to the language pages. |
| JSON output | Built (chunk 1.1 on): every command takes `--json` and prints one envelope (`schemaVersion` 2 since chunk 2.8) with project-relative paths and `data.root`, line and column beside byte offsets, and camelCase keys; `jadpo test --json` is described in area 13 and under `testReport` in `jadpo help --json`. |
| `build/` | Built (chunk 1.7): `jadpo check` writes `build/manifest.json` and the catalogues in `build/catalogue/`; `jadpo test` keeps its last run in `build/test/`. |
| Language server | Built (chunk 1.7): diagnostics, hover and go to definition, from a `use` line too since chunk 3.5; completion since chunk 5.5a3. |
| Agent tools | The catalogues and the similar-helper check are built (chunk 1.7), the check a declinable error (chunk 5.4b); `migrate` is built (chunk 3.1); `impact`, `domain`, `rename` and `seed` are decided, not built. |
| Suggestions | Built (chunk 5.5x): `jadpo suggest` with its six signal kinds and taught signals, the catalogue, accepted findings, `ignore` and `found`; `share`, `suggestions { }` and the committed taught record built in chunk 5.5y, against a fake of Jadpo HQ (HQ itself comes after the first build). |
| Build-only errors | Not applicable yet: there is no `jadpo build` until Phase 8. |

### Consistency with the rest of the language

- **Two names for one job.** Area 5 decided `jadpo migrate`; today it is
  `jadpo schema`, and indexes are accepted through it although area 5 removed
  the recommendation command.
- **The scaffold breaks area 1's fixed root** with a hidden `.app/` folder,
  and puts a route at the root instead of in a domain.
- **Two output switches.** `--diagnostic-format=json` on three commands while
  others always print JSON; agents have to know which is which.
- **`check` and `build` disagree** on what is valid, so "check passes" does
  not mean "it builds".
- **The skill duplicates the documentation map.** It names five documents to
  read; area 1 decided that agents learn by querying the compiler, and these
  pages are now the language reference.

### Carried to later areas

Area 14 is the last area, so nothing new is carried on. The table records
what other areas carried here and where this page answers it.

| Item | Area | Status |
|---|---|---|
| Formatter owns layout; promotion through the editor or `fmt --layout`; catalogue, impact, similar-helper check, domain commands | 1 | Formatting, Tools for agents; questions 8 and 10 |
| `runtime.unhandled_fault`, `runtime.outcome_unknown`, `runtime.startup_failed` | 4 | Errors only the running app or a build raises |
| `migrate` and rename commands; `migration.*`, `index.*`, `cli.schema_*` errors; `toolchain.target_store_not_implemented` | 5 | Questions 4 and 5; Command errors |
| The query report (store or cache, row bound, query count, indexes) | 6 | The `build/` folder; question 10 |
| `jadpo api update` and CI's record check; `toolchain.target_liveness_not_local` | 7 | Commands; Review and sign-off; question 11 |
| `toolchain.target_auth_not_implemented`; the "protected routes but no authentication" check moving into `check` | 8 | Question 11 |
| The approval command, its report and the protected sign-off | 9 | Review and sign-off; question 12 |
| `config set` / `check`, `dev` restarts, `configuration.local_*`, `configuration.value_*`, `cli.config_*` | 10 | Built (chunk 5.1): Commands; Command errors |
| The `temporal.format (` bug | 10 | Bug |
| The operator's view and commands for deliveries; alerts; `toolchain.target_job_*` and durable workflow errors; unreachable `type.job_delivery_*` errors | 12 | Question 13; question 11; Command errors |
| `jadpo test` itself (PostgreSQL, template copies, parallel runs, `--kind`, `--keep-failed`, the report), the `test.*` errors, and generated tests | 13 | Running tests; Command errors |

### Open questions

1. **Should the project path be optional everywhere?** Today every command
   needs it except `config`, which refuses it. *Recommendation:* yes: find the
   project by walking up to `app.jadpo` from the current folder, and accept a
   path on every command, `config` included. Cargo, npm and Go all work this
   way, and agents stop guessing which form a command takes. *Decided 2026-10-13:* yes, as recommended.
2. **One `--json` switch on every command?** *Recommendation:* yes: replace
   `--diagnostic-format=human|plain|json` with `--json`, plain text when
   piped (as today) and colour otherwise. Every command, `impact` and
   `migrate` included, then has one machine form, like `gh` and `kubectl`. *Decided 2026-10-13:* yes, as recommended.
3. **Should `check` write the derived files, removing `artifacts` and
   `watch`?** *Recommendation:* yes. `check` writes `build/` except the
   runnable app, so the catalogues an agent reads are never stale; `build`
   adds `target/`; `dev --no-run` replaces `watch`. That leaves one command
   per job. Rust's `cargo check` also writes its metadata. *Decided 2026-10-13:* yes, as recommended.
4. **Replace the twelve `schema` commands with `migrate`?** Area 5 decided
   `migrate new` and `apply`. *Recommendation:* `jadpo migrate new <purpose>`
   writes the migration folder with its SQL for review and keeps
   `database/identities.json` current; `apply` runs it; `status` lists
   pending ones. Snapshots and diffs go (the latest migration is the
   snapshot), decision files go (decisions live in `changes.jadpo`, checked by
   `jadpo check`), and the index commands go (area 5). This is the Rails and
   Prisma shape: generate, review, apply. *Decided 2026-10-13:* yes, as recommended.
5. **How is a field or entity renamed without losing data?** Today
   `jadpo schema rename` must run before any other change. *Recommendation:*
   one `jadpo rename <old> <new>` for any declaration or field, which updates
   the source, every caller, the file name and the identity registry in one
   step; the editor's rename calls it. A hand-edited rename then shows as a
   removal and an addition, which `migrate new` refuses with a pointer to the
   command. Domains keep their own `jadpo domain` commands because they move
   folders. *Decided 2026-10-13:* yes, as recommended.
   *Extended 2026-10-16 (Richard, area 1 "Names per domain"):* `jadpo rename`
   refuses a new name already in use before editing anything, naming the
   declaration that holds it and its file, and lists the names in use that
   contain the same word, as `semantic.duplicate_declaration` does. A name
   is in use where the new name would have to be unique: in the domain for a
   private function or type, across the project for everything else. Built
   with `rename` itself.
6. **Trim error messages and fix help links?** *Recommendation:* yes:
   relative paths; the two impact lines only when they say something;
   obvious repairs carry their edit; JSON gains line and column and one key
   style; help links open the rule's entry on these pages, which already
   exist and are checked. Rust's and Elm's messages show that a short, exact
   message with one suggestion is what people and models act on. *Decided 2026-10-13:* yes, as recommended.
7. **Does `jadpo check` run the tests a change affects?** Area 1 described
   check reporting affected domains "and running their tests".
   *Recommendation:* `check` reports which domains and tests a change reaches
   but runs nothing; `jadpo test --affected` runs exactly those. `check` stays
   fast enough to run after every edit, as with `tsc` versus `vitest
   --changed`. *Decided 2026-10-13:* yes, as recommended.
8. **Should the formatter write one member per line?** Today it keeps
   `Object { done: Bool pinned: Bool }`. *Recommendation:* yes, and add a
   blank line between declarations, so `fmt --check` enforces the colon and
   one-per-line rules from areas 2 and 3. A short `pick: [Todo.id, Todo.title]`
   list stays on one line; `include`, `pick` and `omit` lines keep the
   order written (area 2). *Decided 2026-10-13:* yes, as recommended.
9. **What does `jadpo new` create?** *Recommendation:* the fixed root from
   area 1 (`app.jadpo`, `config.jadpo`, `domains/`, `database/`, `tests/`),
   one small domain with an action, a route and a test, and no hidden
   folders, so a new project passes `check`, `fmt --check` and `test`. The
   compiler version moves into `app.jadpo` if it is needed at all. *Decided 2026-10-13:* yes, as recommended.
10. **What form do the catalogues and schema report take?**
    *Recommendation:* Markdown files in `build/catalogue/` and
    `build/report/`, each with a `.json` twin, plus `jadpo impact` for
    questions about one name. Agents already search files well; an MCP
    server can wrap the same output later without a second source. *Decided 2026-10-13:* yes, as recommended.
11. **Should `check` report what `build` would refuse?** *Recommendation:*
    yes: `toolchain.target_*` errors come from `check`, so an agent learns a
    feature cannot run yet before writing code around it, and "check passes"
    means "it builds".
 *Decided 2026-10-13:* yes, as recommended.
12. **Who may accept a reviewed change, and what stops an unaccepted
    release?** Area 9 found that nothing blocks a release today.
    *Recommendation:* acceptances are files in one folder that the git host
    protects (GitHub's code owners and branch protection, or the equivalent),
    and CI runs `jadpo check`, which fails without them. Jadpo builds no
    signing or user accounts of its own; review already happens on the git
    host. *Decided 2026-10-13:* yes, as recommended.
13. **What does an operator use to deal with failed deliveries and schedule
    runs?** Area 12 proposes the states and what an operator can do.
    *Recommendation:* commands first: `jadpo deliveries list`, `retry`,
    `dismiss`, `cancel` and `resolve <id> --done|--not-done`, run against an
    environment through its configuration, each action logged with who did
    it. Alerts are log events the hosting platform's alerting picks up. A web
    screen can come later on the same operations, as Sidekiq and Temporal
    offer both. *Decided 2026-10-13:* yes, as recommended.
14. **How is a deployable declared and built?** (carried from area 1, where
    no area took it) Area 1 decided that deployables (API, worker,
    back-office) are declarations in one project, each compiled from what it
    reaches, but no area gives their syntax. Which commands build and run
    one, and where is it declared?
    *Decided 2026-10-13:* declared in `app.jadpo` under `deployables: { }`,
    each saying what it serves or runs (`serves: [main, webhooks]`,
    `runs: [Trigger.schedules, Trigger.subscribers]`) and its target
    (`Target.native` or `Target.wasm`, decision register 2026-10-13); each is
    compiled from what it reaches; with none declared, one deployable runs
    everything; `jadpo build <deployable>` builds one and `jadpo dev` runs all
    in one process.
15. **Which commands manage integrations?** (carried from area 11) The
    command table above has no `jadpo integration add` or `update`, no
    contract diff and no adapter conformance run. Which of these does the
    toolchain provide?
    *Decided 2026-10-13:* `jadpo provider add <name>` (the fixed order from
    area 11), `provider import openapi|smithy <file>`, `provider record
    <connection> <operation>` and `provider update <name>`, which shows the
    difference in the operations the app uses; `jadpo packages outdated` and
    upgrade plans come with the registry.
    *Revised 2026-10-16 (Richard, area 11):* `jadpo provider …` becomes
    `jadpo package …`, for both kinds of package: `new`, `add`, `import`,
    `record`, `conform`, `check`, `publish`, `update`, `remove`, `show`,
    `link`, `unlink` and `recipe`, in the commands table above; with
    `jadpo services`, `jadpo calls` and `jadpo dev grants add` beside them.
16. **Should commands have synonyms?** Raised by Richard (2026-10-13).
    *Decided 2026-10-13:* yes, plain synonyms where people reach for either
    word (`fmt` and `format`, `delivery` and `deliveries`, `package` and
    `packages`, which were `provider` and `providers` until 2026-10-16), and an unknown command suggests the nearest one ("did you
    mean `format`?"). No cryptic single letters; help and docs show the
    canonical name.
17. **Should arguments complete and prompt?** Raised by Richard
    (2026-10-13). *Decided 2026-10-13:* yes, in two ways. Tab completion for
    zsh, bash and fish (`jadpo completions zsh`) completes commands, flags
    and the project's own names: queries for `jadpo sql`, routes, schedules,
    test files and test names, connections, and every declaration for
    `jadpo impact`. And a
    command missing a required argument in a terminal opens a searchable
    list (`jadpo sql` alone lists every query); without a terminal it fails
    with the valid values listed, so agents get the choices too.
    *Extended 2026-10-13 (Richard):* completion starts at the top level:
    typing `jadpo` and Tab lists every command with a one-line description,
    and every flag is offered with its description, so nobody has to
    remember them. The CLI describes itself completely: `jadpo` alone and
    `jadpo <command> --help` show every subcommand and flag with what it does
    and an example, and `jadpo help --json` gives the whole command
    reference as data for agents. Nobody needs the documentation to learn
    what a command does; the docs are generated from the same source.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Formatting | Carried from area 1: the formatter owns blank lines and section order within a file | 2026-10-12 |
| Promotion | Carried from area 1: required with a one-step fix, applied through the editor or `jadpo fmt --layout`, never on save; demotion never forced | 2026-10-12 |
| Helpers | Carried from area 1: descriptions written once as `///`; the library catalogue, callers and `jadpo impact` are computed; similar helpers flagged by `jadpo check` | 2026-10-12 |
| Domain commands | Carried from area 1: `jadpo domain rename`, `split` and `merge` move files and update every call; a new or renamed domain appears in the approval review | 2026-10-12 |
| Documentation comments | Carried from area 1: `///` feeds generated docs, the library catalogue and OpenAPI | 2026-10-12 |
| Failure catalogue | Carried from area 4: the compiler generates a catalogue of every failure | 2026-10-12 |
| Migration workflow | Carried from area 5: `jadpo migrate new` / `apply`, one migration per store, check fails on drift, the app refuses a mismatched database | 2026-10-12 |
| Schema report | Carried from area 5: the build includes a report of every table, column and index with the query that needs it | 2026-10-12 |
| Seeds | Carried from area 5: `jadpo seed <name>` runs a seed through the application's actions and refuses production | 2026-10-12 |
| Route listing | Carried from area 7: hover shows a route's full path; `jadpo routes` lists every route | 2026-10-12 |
| Query SQL | Carried from area 6: `jadpo sql <query>` (with `--explain`) and an editor hover show a query's generated SQL; never written into source | 2026-10-12 |
| Write SQL | `jadpo sql <action>` also shows the statements an action's writes send, each with its parameters and the conflict arm each constraint maps to (Richard) | 2026-10-15 |
| Permissions report | Carried from area 9: the build writes `build/report/permissions.md` and a `.json` twin, a matrix of who can do what to every entity and field; changes to it appear in the approval review | 2026-10-12 |
| Optional project path | Found by walking up to `app.jadpo`; a path accepted on every command | 2026-10-13 |
| `--json` | One `--json` on every non-interactive command; plain when piped, colour otherwise | 2026-10-13 |
| `check` writes derived files | `check` writes `build/` except the runnable app; `artifacts` and `watch` go; `dev --no-run` rebuilds only | 2026-10-13 |
| `migrate` commands | `migrate new <purpose>`, `apply`, `status` replace the twelve `schema` commands | 2026-10-13 |
| Renaming | `jadpo rename <old> <new>` updates source, callers, file names and the identity registry; the editor's rename calls it; a hand rename of stored data is refused by `migrate new` | 2026-10-13 |
| Renaming a name in use | `jadpo rename` refuses a new name already in use before editing anything, naming the declaration that holds it, and lists the names in use with the same word (Richard) | 2026-10-16 |
| Parallel agents | The agent skill's routine: merge `main`, run `jadpo check`, fix any clash, then push; generated CI runs `jadpo check` first, so a clash fails in seconds; what generates CI is not yet specified (Richard) | 2026-10-16 |
| Error messages | Relative paths, impact lines only when useful, repairs carry their edit, JSON line and column, help links to these pages | 2026-10-13 |
| Affected tests | `check` reports affected tests; `jadpo test --affected` runs them | 2026-10-13 |
| What `--affected` measures against | The merge-base with `main`: the tests reaching any node whose digest differs from that commit's, read through the same before side as diffs (Richard) | 2026-10-14 |
| One member per line | Formatter writes one member per line and a blank line between declarations; short lists stay on one line | 2026-10-13 |
| Section order | The formatter orders sections: a failure's `kind`, `code`, `message`, `client`, `internal`; a route's `authentication`, `rate_limit`, `input`, `output`, `status`, `deadline`, `run`; an entity's fields (in written order), then `identity`, `persistence`, `lifecycle`, `membership`, `policy`, `freshness`, `guard`, `person`; other sections follow in written order (Richard) | 2026-10-13 |
| Scaffold | `jadpo new` creates the fixed root, one small domain and passing tests of each kind; no hidden folders | 2026-10-13 |
| Catalogue form | Markdown in `build/catalogue/` and `build/report/` with `.json` twins, plus `jadpo impact` | 2026-10-13 |
| Build-only errors in `check` | `toolchain.target_*` errors come from `check` | 2026-10-13 |
| Sign-off | Settled by area 7: acceptance files in a host-protected folder; CI's `jadpo check` fails without them | 2026-10-13 |
| Operator commands | `jadpo deliveries list/retry/dismiss/cancel/resolve` against an environment, each logged with who did it | 2026-10-13 |
| Deployables | `deployables: { }` in `app.jadpo` with what each serves or runs and its target; compiled from what it reaches | 2026-10-13 |
| Integration commands | `jadpo provider add/import/record/update`; `packages outdated` later (Revised 2026-10-16: the `jadpo package` commands) | 2026-10-13 |
| Command synonyms | Plain synonyms (`fmt`/`format`), nearest-command suggestions, no single-letter aliases (Richard) | 2026-10-13 |
| Completion and prompts | Tab completion from the top level of commands, flags (with descriptions) and project names; a missing argument opens a searchable list in a terminal and lists valid values otherwise; the CLI describes itself fully (`--help` with examples, `jadpo help --json`), so nobody needs the docs (Richard) | 2026-10-13 |
| Incident input | `jadpo incident` reads a file, standard input, or text pasted at a prompt in a terminal; Jadpo events are found inside surrounding log text (timestamps, platform wrappers) and grouped by request; the build's manifest is found by the event's build id (area 18) (Richard) | 2026-10-13 |
| CLI design language | Approved as designed in the [CLI design language](research/14-cli-design-language.md): six meaning-only colours, single-width symbols with ASCII fallbacks, one layout grid, calm by default; Jadpo's own colours when the background is known, otherwise the terminal theme's; Unicode symbols, no emoji or Nerd Font glyphs (Richard) | 2026-10-13 |
| Translation commands | `jadpo translations missing`, `unused`, `builtin`, `update`, `accept <locale>`, `export` and `import` (XLIFF or JSON), `--json` wherever they print; go to definition from a translations call to its type and wording (Richard, area 10) | 2026-10-16 |
| Package commands | `jadpo package new`, `add` (registry, git, path or URL), `import`, `record`, `conform`, `check`, `publish`, `update`, `remove`, `show`, `link`, `unlink` and `recipe` replace `jadpo provider …` (Richard, area 11) | 2026-10-16 |
| Service, call and grant commands | `jadpo services` with `start`, `stop`, `restart`, `logs` and `shell`; `jadpo calls`, `calls show` and `calls script`; `jadpo dev grants add` (Richard, area 11) | 2026-10-16 |
| Records command | `jadpo records` lists every committed record and flags any out of date (Richard, area 18) | 2026-10-16 |
| Store commands | `jadpo files copy` between stores; `jadpo search reindex` rebuilds a search index (Richard, area 16) | 2026-10-16 |
| Batch commands | `jadpo batches list`, `show` and `cancel` for operators; `jadpo runs unfinished --build <build id>` before a deploy; no deploy command, since a rerun detects a changed build by its build id (decided for Richard) | 2026-10-16 |
| Constants for agents | The library catalogue lists library constants with their type, value, description and readers; `constant.similar_exists` warns on two constants of one type holding one value; the agent skill says a value a deployment could ever need to change is `config`, otherwise a `constant` (area 3). The warning and the catalogue entries built in chunk 5.3e; the skill itself isn't built yet (Richard) | 2026-10-16 |
| Making a constant a setting | `jadpo config move <constant>`, and the editor's "Make this a setting" code action through the same code, moves a constant into `config.jadpo` and rewrites every read (area 10); offered, never required, so not part of `jadpo fmt --layout`; the agent skill points at it. Built in chunk 5.3f (Richard) | 2026-10-16 |
| Editor completion | Completion for everything as you type, generated from the declarations and data the compiler uses, never a list kept for the editor: names in scope, fields, operations with signature, failures and docs, variants with CLDR names, units after a number narrowed to the type required, named arguments, type names, declinable rules, settings, messages and keywords; snippets for calls with required arguments; no AI suggestions in the server. Built in chunk 5.5a3 (Richard) | 2026-10-17 |
| Suggestions | `jadpo suggest` hints where code could use a standard type or operation and never edits; it formats, then matches signals held as data on the standard types' entries (shape, conversion constants, name hints, format literals, validation patterns) and a loop table for operations through canonical graph fragments, numbers its findings and ends with the agent's instruction; `ignore` writes the canonical hash, signal and reason to the committed `ignored_suggestions.jadpo_record`; `found` teaches a miss; a vanished finding is accepted; everything to share waits in `.jadpo/suggest_pending` for `share` (roadmap 5.5y). Built in chunk 5.5x (Richard) | 2026-10-17 |
| Sharing with Jadpo HQ | Sharing is on unless `suggestions { share: false }` or `JADPO_SUGGEST_SHARE=off` turns it off; `share` sends the pending hashes with their kinds and a committed random project token at once, shows the snippets HQ asks for (those seen from enough distinct projects) as the exact payload, and sends them only with `--approve` after a person said yes; unattended runs send only hashes; the pending file is wiped after; taught patterns move to the committed `taught_suggestions.jadpo_record`; the protocol (HTTPS and JSON under `/v1/`) and HQ's agent are written on this page so the real HQ implements them unchanged. Built in chunk 5.5y against a fake of HQ (Richard; taught record: lead session, 2026-10-18) | 2026-10-17 |
| Settings in name order | The formatter sorts `config { }` fields by name, the one exception to fields keeping their written order, so parallel workspaces adding settings rarely clash (area 10). Built in chunk 5.3f (Richard) | 2026-10-16 |

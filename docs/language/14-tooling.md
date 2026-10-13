---
title: Tooling
area: 14
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - toolchain.no_sources
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
| `jadpo new <name>` | Creates a project that passes `check`, `fmt --check` and `test` | Exists; scaffold changes (question 9) |
| `jadpo check` | Checks everything and writes the derived files in `build/` | Exists; writing files is question 3 |
| `jadpo fmt` | Formats; `--check` only reports; `--layout` also applies promotion moves | Exists; `--layout` decided in area 1 |
| `jadpo test` | Runs every test | Exists |
| `jadpo test <file>` / `<file>:<line>` | Runs one test file, or the test at that line | Decided in area 13, not built |
| `jadpo test --name <text>` / `--kind route` / `--failed` / `--affected` | Runs tests whose names match, one kind, last run's failures, or the tests a change reaches | `--kind` exists; the rest decided in area 13 and question 7 |
| `jadpo test --watch` | Reruns the affected tests on every save | Decided in area 13, not built |
| `jadpo test --interactive` | Keyboard view of every test: search, navigate, run a file, one test or all, rerun failures, toggle watch | Decided in area 13, not built |
| `jadpo test --keep-failed` / `--seed <n>` | Keeps a failed test's database and trace; reruns with a given order seed | Decided in area 13, not built |
| `jadpo build` | Writes the runnable app into `build/target/` | Exists |
| `jadpo dev` | Rebuilds and restarts the app on every change; `--no-run` only rebuilds | Exists; `--no-run` replaces `watch` (question 3) |
| `jadpo inspect` | Prints the project map: every declaration, call and failure | Exists |
| `jadpo routes` | Lists every route with its full method and path, file, API, authentication and the query or action it runs | Decided in area 7, not built |
| `jadpo sql <query>` | Prints a query's generated SQL for PostgreSQL and SQLite; `--explain` runs it on a disposable database and shows the plan | Decided in area 6, not built |
| `jadpo impact <name>` | Lists what a change to `name` reaches | Decided in area 1, not built |
| `jadpo rename <old> <new>` | Renames a declaration or field everywhere, keeping stored data | Proposed (question 5) |
| `jadpo domain rename/split/merge` | Moves a domain's files and updates every call | Decided in area 1, not built |
| `jadpo migrate new <purpose>` / `apply` / `status` | Writes, runs and lists migrations | `new` and `apply` decided in area 5; replaces `schema` (question 4) |
| `jadpo seed <name>` | Runs a seed; refuses production | Decided in area 5, not built |
| `jadpo config set <field>` / `check` | Sets or checks local configuration in `.env.local` | Exists |
| `jadpo api update` | Rewrites the committed public API record (`api/main.api`) from the source | Proposed in area 7 (question 10) |
| `jadpo approval --before <ref>` | Writes the review report of what changed in behaviour, policy and the API | Exists; sign-off is question 12 |
| `jadpo deliveries` | Lists, retries, dismisses, cancels and resolves schedule runs and event deliveries | Proposed (question 13) |
| `jadpo schedules` | Lists schedules; `run`, `pause`, `resume`, `history` | Decided in area 12, not built |
| `jadpo provider add` / `import` / `record` / `update` | Adds, imports, confirms and upgrades provider packages | Decided in area 11; question 15 |
| `jadpo trace <id>` / `jadpo why <name>` / `jadpo call` / `jadpo console` | Debugging (area 17) | Proposed in area 17 |
| `jadpo incident [file \| -]` | Explains a runtime event: from a file, from standard input (`pbpaste \| jadpo incident -`, or a log stream piped in), or, with no argument in a terminal, from text pasted at a prompt; it finds the Jadpo events inside surrounding log text and groups them by request | Exists for a file; the rest decided 2026-10-13 |
| `jadpo lsp` | Runs the language server for editors | Exists |
| `jadpo help [command]` | Help for one command; `--help` works everywhere | Proposed (bug below) |

::: today

Every command except `config` needs the project path (`jadpo check .`), while
`config` refuses a path and only works from inside the project. JSON comes
from `--diagnostic-format=json` on `check`, `watch` and `dev`, while
`inspect` and `approval` print JSON always. There is no per-command help:
`jadpo new --help` creates a project folder called `--help`. `jadpo artifacts`
writes the derived files and `jadpo watch` rebuilds without running.
`jadpo schema` has twelve subcommands (`init`, `check`, `add`, `rename`,
`snapshot`, `diff`, `decision-template`, `decision-check`, `plan`, `sql`,
`index-recommend`, `index-accept`).

:::

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

::: today

`jadpo test` builds the app and runs each test one at a time against a fresh
in-memory SQLite database built from the entities, not the migrations (read
by area 13 from generated code). A project with no tests fails with
`test.no_tests` (run).

:::

## Formatting

The formatter owns layout, so equivalent code always looks identical and a
diff only ever shows real changes (decided in area 1). It decides indentation,
spacing, blank lines (one between declarations, none doubled) and the order of
sections inside a declaration. It writes one member per line (question 8).
`jadpo fmt --check` fails when a file differs, and the editor formats on save.

Promotion is the one layout change that moves code between files: when a
second file uses a type or helper, the compiler reports
`layout.promotion_required` with a one-step fix. That fix is applied through
the editor's quick fix or `jadpo fmt --layout`, never on save, so a move is
always a visible, separate change (decided in area 1).

```jadpo fragment
type Flags = Object {
    done: Bool
    pinned: Bool
}

type Count = Int {
    min: 0
}
```

::: today

Doubled blank lines are reduced to one, but a missing blank line between
declarations is not added and several members on one line are kept. This
compiles and passes `jadpo fmt --check`:

```jadpo project=fmt-today
type Flags = Object { done: Bool pinned: Bool }
type Count = Int {
    min: 0
}
```

`jadpo fmt --check` on that project prints "checked 0 source file(s)".

:::

## Editor support

The language server (`jadpo lsp`) gives any editor the compiler's own view:
errors while you type, go to definition, find references, hover with types and
the failures a call can produce, completion, rename and formatting. The VS
Code extension in `editors/vscode` adds highlighting and a command to open any
generated file. It contains no parser of its own.

Decided in area 1 and not built: hover shows a declaration's `///`
description and its callers; promotion and the similar-helper warning come
with quick fixes; rename goes through the same code as `jadpo rename`, so an
entity field renamed in the editor keeps its stored data (question 5).

Decided in area 6 and not built: hovering over a query shows its generated
SQL, the same as `jadpo sql`. The SQL is never written into the source as a
comment, which would go stale, add noise to diffs and invite edits.

## Error messages

Every error has one shape, in the terminal, the editor and JSON: what is
wrong, where, why, the recommended next step, who decides (`agent` or
`human`), the rule ID and a help link. One mistake gives one error; the
compiler does not report knock-on errors. With question 6's changes an
error reads:

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

What question 6 changes:

- Paths are relative to the project.
- The two impact lines appear only when they say something. Today every error
  prints "Compilation is blocked until this problem is fixed" and "No
  public-contract change has been applied", including `fmt --check`.
- An obvious repair carries its edit, so the editor and agents can apply it.
  Today "Use `title`" comes with no edit.
- The help link opens the error's own entry on these language pages, which
  already have an example for every rule. Today links point to
  `jadpo.dev/docs/diagnostics/…`, which did not resolve when checked.
- JSON gives line and column as well as byte offsets, and uses one naming
  style for its keys.

::: today

`jadpo check . --diagnostic-format=json` prints one `diagnostic_report` with
`schemaVersion: 2`, a `summary` block (`source_files`, `declarations`) and a
`diagnostics` list. Each entry has `ruleId`, `summary`, `reason`,
`recommendedNextStep` (with `edits`), `alternatives`, `location` (absolute
path and byte range), `context`, `impact` and `helpId`. Keys mix camelCase
and snake_case, and `diagnosticId` contains the absolute path.

:::

## The `build/` folder

`build/` holds only generated files. It is never edited, never committed and
never read as input, and the same source always produces the same bytes.
Anything a person reviews and keeps, such as the record of the public API
(area 7) or the identity registry (area 5), lives outside it.

```text
build/
  app.meta.json               project map (also printed by jadpo inspect)
  catalogue/
    libraries.md              every library function: signature, description, callers
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

The catalogues, the schema report and the permissions report are decided
(areas 1, 4, 5 and 9) and not built; the query report is area 6's input, kept beside the schema report
because a reviewer reads them together. Each is written as Markdown, which an agent can search, with a `.json`
twin for tools (question 10).

::: today

`jadpo artifacts .` writes `app.meta.json`, `inventory/`, `audit/`,
`approval/`, `validators/plan.json`, `compatibility/`, `openapi/` and
`diagnostics/` (15 files). `jadpo build .` adds `target/`, `sql/` and
`persistence/`. `jadpo check` writes nothing. `jadpo new` also writes a
hidden `.app/project.json`, which area 1's fixed root does not allow.

:::

## Tools for agents

Agents look things up through the compiler rather than browsing folders.
These were decided in areas 1, 4 and 5 and none is built yet.

- **Library catalogue** (`build/catalogue/libraries.md`): every library
  function with its signature, `///` description and callers. An agent reads
  it before writing a helper.
- **Similar-helper warning**: when a new helper looks like an existing one,
  `jadpo check` warns `library.similar_exists` and names it.
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

::: today

The skill tells agents to read `docs/decision-register.md`, `docs/syntax.md`
and `docs/grammar-v0.1.md` before editing, use `jadpo inspect` and
`jadpo artifacts`, and run `jadpo incident` for runtime events. It does not
mention impact or catalogues.

:::

## Review and sign-off

Some changes need a person's acceptance before they ship: a change that can
break an API client (area 7), a database drop (area 5), weaker policy
(area 9) and a new or renamed domain (area 1). They share one acceptance
form, designed in area 7. `jadpo check` fails until each such change has its
acceptance, and `jadpo approval --before <ref>` writes the report a reviewer
reads: what changed in routes, policy, failures and data. Who may write an
acceptance is question 12.

::: today

`jadpo approval` compares two versions and prints the review subject
(`build/approval/subject.json` and a text copy). Nothing blocks a release.

:::

## Errors only the running app or a build raises

The running app reports three events of its own, never compile errors:
`runtime.unhandled_fault` (a defect or fault reached the boundary; the client
gets `internal_fault`), `runtime.outcome_unknown` (a change may or may not
have committed) and `runtime.startup_failed`. Area 4 decided what each logs.

`jadpo build` refuses features the compiler checks but cannot generate yet,
with `toolchain.target_*` errors: protected routes without a supported
authentication adapter, schedules, durable workflows, non-PostgreSQL stores
and a liveness route that reads data. Today `jadpo check` passes these
projects and only `build` fails; for example a project with one schedule
passes `check` and stops at `build` with `toolchain.target_job_not_implemented`
(question 11). Area 8 asks the same for a protected route with no
authentication set up.

### Command errors

Every error that only a command raises, so `jadpo check` cannot show it with a
source example. Generated from the compiler's catalogue.

| Rule | Raised by | What it means |
|---|---|---|
| `cli.build_arguments` | `build` | Build command has invalid arguments |
| `cli.build_experiment_scope` | `build` | Experimental target only builds qualified fixtures |
| `cli.build_experiment_unavailable` | `build` | Shared-Rust experiment checkout was not found |
| `cli.build_tool_failed` | `build` | Experimental target build failed |
| `cli.check_arguments` | `check` | Check command has invalid arguments |
| `cli.config_arguments` | `config` | Config command has invalid arguments |
| `cli.config_prompt_failed` | `config` | Secure configuration prompt failed |
| `cli.config_tty_required` | `config` | Configuration entry requires an interactive terminal |
| `cli.dev_arguments` | `dev` | Dev command has invalid arguments |
| `cli.dev_bun_start_failed` | `dev` | Bun could not start the generated runtime |
| `cli.dev_port_invalid` | `dev` | Development port is invalid |
| `cli.dev_readiness_timeout` | `dev` | Generated runtime did not become ready |
| `cli.dev_rollback_cleanup_failed` | `dev` | Previous runtime revision could not be cleaned up |
| `cli.dev_rollback_prepare_failed` | `dev` | Last-known-good runtime revision could not be preserved |
| `cli.dev_rollback_restore_failed` | `dev` | Last-known-good runtime revision could not be restored |
| `cli.dev_runtime_exited` | `dev` | Generated runtime exited unexpectedly |
| `cli.dev_runtime_status_failed` | `dev` | Generated runtime status could not be read |
| `cli.dev_target_missing` | `dev` | Generated runtime entrypoint is missing |
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
| `cli.schema_command_required` | `schema` | Schema command requires a subcommand |
| `cli.schema_decision_arguments` | `schema` | Schema decision command has invalid arguments |
| `cli.schema_diff_arguments` | `schema` | Schema diff command has invalid arguments |
| `cli.schema_index_accept_arguments` | `schema` | Index acceptance requires a recommendation path |
| `cli.schema_plan_arguments` | `schema` | Schema plan command has invalid arguments |
| `cli.schema_rename_arguments` | `schema` | Schema rename command has invalid arguments |
| `cli.schema_snapshot_arguments` | `schema` | Schema snapshot command requires an output path |
| `cli.schema_sql_arguments` | `schema` | Schema SQL command has invalid arguments |
| `cli.signal_handler_failed` | any command | Shutdown handler could not be installed |
| `cli.unknown_command` | any command | Unknown Jadpo command |
| `cli.unknown_schema_command` | `schema` | Unknown schema subcommand |
| `cli.watch_arguments` | `watch` | Watch command has invalid arguments |
| `cli.watch_input_read_failed` | `watch` | Watched project inputs could not be read |
| `cli.watch_output_failed` | `watch` | Watch lifecycle event could not be written |
| `configuration.local_changed` | `config` | Local configuration changed during the update |
| `configuration.local_check_failed` | `config` | Local configuration is incomplete or invalid |
| `configuration.local_invalid_encoding` | `config` | Local configuration is not UTF-8 text |
| `configuration.local_read_failed` | `config` | Local configuration could not be read |
| `configuration.local_syntax` | `config` | Local configuration syntax is invalid |
| `configuration.local_unsafe_target` | `config` | Local configuration target is unsafe |
| `configuration.local_write_failed` | `config` | Local configuration could not be saved |
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
| `test.failed` | `test` | Test run failed |
| `test.no_tests` | `test` | Project has no executable tests |
| `test.runtime_start_failed` | `test` | Test runtime could not start |
| `toolchain.artifact_cleanup_failed` | `artifacts`, `build` | Previous build backup could not be removed |
| `toolchain.artifact_promote_failed` | `artifacts`, `build` | Staged build artifacts could not be promoted |
| `toolchain.artifact_stage_failed` | `artifacts`, `build` | Build staging directory could not be created |
| `toolchain.artifact_write_failed` | `artifacts`, `build` | Generated artifact could not be written |
| `toolchain.project_not_found` | any command | Project path does not exist |
| `toolchain.project_read_failed` | any command | Project directory could not be read |
| `toolchain.scaffold_destination_exists` | `new` | Scaffold destination is an existing file |
| `toolchain.scaffold_destination_not_empty` | `new` | Scaffold destination directory is not empty |
| `toolchain.scaffold_name_invalid` | `new` | Project destination has no valid name |
| `toolchain.scaffold_read_failed` | `new` | Scaffold destination could not be inspected |
| `toolchain.scaffold_write_failed` | `new` | Project scaffold file could not be written |
| `toolchain.source_read_failed` | any command | Jadpo source file could not be read |
| `toolchain.target_auth_not_implemented` | `build` | Protected route `{route}` cannot be generated yet |
| `toolchain.target_dependency_manifest` | `build` | Generated target attempted to add a dependency manifest |
| `toolchain.target_durable_workflow_not_implemented` | `build` | Durable workflow target execution is not implemented |
| `toolchain.target_external_module` | `build` | Generated target imports an external module |
| `toolchain.target_job_not_implemented` | `build` | Scheduled job `{name}` cannot execute yet |
| `toolchain.target_liveness_not_local` | `build` | Authored liveness route is not dependency-free |
| `toolchain.target_store_not_implemented` | `build` | Authority store target adapter is not implemented |

Three errors cannot be reached at all: `type.job_delivery_selector_invalid`
(Delivery selector does not match its entity types),
`type.job_delivery_service_invalid` (Delivery service does not match its
closed payload types) and `type.job_delivery_sealed_use` (Compiler-owned
delivery value cannot be authored or exposed). They go with the delivery
descriptor if area 12's question 2 is accepted.

## Errors

Every error this area can produce from source, with an example that
triggers it and the compiler's actual output.

### `toolchain.no_sources`

A project folder with no `.jadpo` files, here only a README:

```jadpo project=e-no-sources file=README.md expect=toolchain.no_sources
A project folder with notes but no .jadpo files.
```

## Review

### Status

Each row says how it was checked: run with `jadpo` 3ce094d0, or read from the
source or documents.

| Feature | Status |
|---|---|
| Command help | **Bug (run):** no command has its own help. `--help` after a command is taken as the project path: `jadpo new --help` created a project folder called `--help`, and `jadpo artifacts --help` then wrote `build/` into it. |
| Project path | **Inconsistent (run):** every command needs a path except `config`, which rejects one (`cli.config_arguments`) and checks whichever folder you are in; run from the repository root it checked the whole repository. |
| Scaffold | **Gap (run):** `jadpo new` writes a hidden `.app/project.json` and a root-level route with an inline `action:`, which the syntax review proposes removing, and `jadpo test` on the new project fails with `test.no_tests`. |
| Formatter | **Gap (run):** keeps `Object { done: Bool pinned: Bool }` on one line and adds no blank line between declarations; doubled blank lines are reduced. `fmt --check` on a canonical project prints "checked 0 source file(s)", and `fmt` prints "formatted 0 source file(s)" when nothing changed. `--layout` does not exist. **Bug (reported by area 10, run there):** `fmt` rewrites `temporal.format(` as `temporal.format (`. |
| Error messages | Works: one error per mistake, with reason, next step, owner, rule and help (run). **Gaps (run):** every error prints the same two impact lines, including "Compilation is blocked" for `fmt --check`; an obvious rename ("Use `title`") carries no edit; help links to `jadpo.dev` did not resolve (`curl`: could not resolve host). **Bug (reported by area 10, compiled there):** a misspelt setting, `config.mail_snder`, names the internal `__jadpo_config`. |
| JSON output | Works for `check` (run). **Gaps (run):** absolute paths, including inside `diagnosticId`; byte offsets with no line or column; camelCase and snake_case keys mixed. |
| `build/` | Works: 15 files from `artifacts`, more from `build`, byte-identical on repeat (run; repeatability read from `docs/generated-artifacts.md`). `check` writes nothing. |
| Language server | Diagnostics, symbols, definition, references, hover, completion, rename, semantic tokens and formatting (read from `editors/vscode/README.md`, not run in an editor). |
| Agent tools | **Not built (run):** no `impact`, `domain`, `rename`, `migrate` or `seed` command, no catalogues, no schema report, no similar-helper warning. The skill points to older documents and does not mention `jadpo impact` (read). |
| Build-only errors | **Gap (run):** a project with a schedule passes `check` and fails `build` with `toolchain.target_job_not_implemented`. |

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
| Formatter owns layout; promotion through the editor or `fmt --layout`; catalogue, impact, similar-helper warning, domain commands | 1 | Formatting, Tools for agents; questions 8 and 10 |
| `runtime.unhandled_fault`, `runtime.outcome_unknown`, `runtime.startup_failed` | 4 | Errors only the running app or a build raises |
| `migrate` and rename commands; `migration.*`, `index.*`, `cli.schema_*` errors; `toolchain.target_store_not_implemented` | 5 | Questions 4 and 5; Command errors |
| The query report (store or cache, row bound, query count, indexes) | 6 | The `build/` folder; question 10 |
| `jadpo api update` and CI's record check; `toolchain.target_liveness_not_local` | 7 | Commands; Review and sign-off; question 11 |
| `toolchain.target_auth_not_implemented`; the "protected routes but no authentication" check moving into `check` | 8 | Question 11 |
| The approval command, its report and the protected sign-off | 9 | Review and sign-off; question 12 |
| `config set` / `check`, `dev` restarts, `configuration.local_*`, `configuration.value_*`, `cli.config_*`; the `__jadpo_config` and `temporal.format (` bugs | 10 | Commands; Command errors; Status |
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
16. **Should commands have synonyms?** Raised by Richard (2026-10-13).
    *Decided 2026-10-13:* yes, plain synonyms where people reach for either
    word (`fmt` and `format`, `delivery` and `deliveries`, `provider` and
    `providers`), and an unknown command suggests the nearest one ("did you
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
| Permissions report | Carried from area 9: the build writes `build/report/permissions.md` and a `.json` twin, a matrix of who can do what to every entity and field; changes to it appear in the approval review | 2026-10-12 |
| Optional project path | Found by walking up to `app.jadpo`; a path accepted on every command | 2026-10-13 |
| `--json` | One `--json` on every non-interactive command; plain when piped, colour otherwise | 2026-10-13 |
| `check` writes derived files | `check` writes `build/` except the runnable app; `artifacts` and `watch` go; `dev --no-run` rebuilds only | 2026-10-13 |
| `migrate` commands | `migrate new <purpose>`, `apply`, `status` replace the twelve `schema` commands | 2026-10-13 |
| Renaming | `jadpo rename <old> <new>` updates source, callers, file names and the identity registry; the editor's rename calls it; a hand rename of stored data is refused by `migrate new` | 2026-10-13 |
| Error messages | Relative paths, impact lines only when useful, repairs carry their edit, JSON line and column, help links to these pages | 2026-10-13 |
| Affected tests | `check` reports affected tests; `jadpo test --affected` runs them | 2026-10-13 |
| One member per line | Formatter writes one member per line and a blank line between declarations; short lists stay on one line | 2026-10-13 |
| Scaffold | `jadpo new` creates the fixed root, one small domain and passing tests of each kind; no hidden folders | 2026-10-13 |
| Catalogue form | Markdown in `build/catalogue/` and `build/report/` with `.json` twins, plus `jadpo impact` | 2026-10-13 |
| Build-only errors in `check` | `toolchain.target_*` errors come from `check` | 2026-10-13 |
| Sign-off | Settled by area 7: acceptance files in a host-protected folder; CI's `jadpo check` fails without them | 2026-10-13 |
| Operator commands | `jadpo deliveries list/retry/dismiss/cancel/resolve` against an environment, each logged with who did it | 2026-10-13 |
| Deployables | `deployables: { }` in `app.jadpo` with what each serves or runs and its target; compiled from what it reaches | 2026-10-13 |
| Integration commands | `jadpo provider add/import/record/update`; `packages outdated` later | 2026-10-13 |
| Command synonyms | Plain synonyms (`fmt`/`format`), nearest-command suggestions, no single-letter aliases (Richard) | 2026-10-13 |
| Completion and prompts | Tab completion from the top level of commands, flags (with descriptions) and project names; a missing argument opens a searchable list in a terminal and lists valid values otherwise; the CLI describes itself fully (`--help` with examples, `jadpo help --json`), so nobody needs the docs (Richard) | 2026-10-13 |
| Incident input | `jadpo incident` reads a file, standard input, or text pasted at a prompt in a terminal; Jadpo events are found inside surrounding log text (timestamps, platform wrappers) and grouped by request; the build's manifest is found by the event's build id (area 18) (Richard) | 2026-10-13 |
| CLI design language | Approved as designed in the [CLI design language](research/14-cli-design-language.md): six meaning-only colours, single-width symbols with ASCII fallbacks, one layout grid, calm by default; Jadpo's own colours when the background is known, otherwise the terminal theme's; Unicode symbols, no emoji or Nerd Font glyphs (Richard) | 2026-10-13 |

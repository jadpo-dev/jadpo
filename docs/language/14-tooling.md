---
title: Tooling
area: 14
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Tooling
<!-- topic: tooling -->

The `jadpo` command and everything around it: the formatter, the editor
support, how errors are shown, the generated `build/` folder, the tools
agents use to look things up before they change code, and how an app is
built into the programs you deploy.

Everything here comes from the compiler. The editor, the agent skill and
every generated file are made by the same check as `jadpo check`, so they
can't disagree with it.

## The `jadpo` command
<!-- topic: the-jadpo-command -->

You work with Jadpo through one command, `jadpo`, and a short list of
subcommands. Run them anywhere inside your project: each finds the project
by walking up from the current folder to `app.jadpo`, so you only give a
path to work on another project. Every subcommand takes `--json`, for
output an agent reads instead of prose, except the interactive modes,
which need a terminal.

Commands have plain synonyms where people reach for either word, such as
`fmt` and `format`, and an unknown command suggests the nearest. Tab
completes commands, flags and your project's own names, and a command
missing an argument asks for it from a searchable list.

The commands you use every day:

| Command | Does |
|---|---|
| `jadpo new <name>` | Starts a project |
| `jadpo check` | Checks everything and writes the derived files in `build/` |
| `jadpo fmt` | Formats your code |
| `jadpo test` | Runs your tests |
| `jadpo build [<deployable>]` | Builds the programs you deploy, into `build/target/` |
| `jadpo dev` | Runs the app locally, rebuilding it on every save |

### `jadpo new`

`jadpo new <name>` starts a project in a folder that doesn't exist yet or
is empty, and the project passes `jadpo check`, `jadpo fmt --check` and
`jadpo test` as soon as it is written. It writes the fixed root
([The root](01-project-layout-and-modules.md#the-root)) and one small
domain, `hello`:

```text
todo-app/
  app.jadpo        application { jadpo: "0.1" } and the project token
  config.jadpo     config {}, with no settings yet
  policy.jadpo     who may run greet: Access.public
  api/
    main.jadpo_record    as jadpo api update writes it
  domains/
    hello/
      types/       visitor.jadpo  greeting.jadpo
      actions/     greet.jadpo
      routes/      hello.jadpo  hello.test.jadpo
  database/        empty
  tests/           empty
  AGENTS.md  .gitignore  .gitattributes
```

- `app.jadpo` names the release line of the `jadpo` that ran `new`, so
  `jadpo new` from 0.1.x writes `jadpo: "0.1"`
  ([Which jadpo runs](#which-jadpo-runs)). It also holds the project's
  `suggestions { project_token: }`, 32 random hex digits from the
  operating system's generator ([Sharing with Jadpo
  HQ](#sharing-with-jadpo-hq)); when the generator fails it leaves the
  block out, and `share` writes it later.
- The `hello` domain is one route, `POST /hello`, with
  `authentication: RouteAuthentication.none`. It takes a `Visitor` and
  runs `greet`, which returns a `Greeting`. Because `greet` reaches no
  entity, root policy grants it with `hello.greet: [Access.public]`. The
  route's test, beside it, posts a visitor and checks the status and the
  greeting. The API record is written by the code `jadpo api update`
  runs, from these routes.
- `AGENTS.md` says what Jadpo is, the release line `app.jadpo` names,
  where the docs are (the primer, the index and the error pages on the
  site, as [How an agent uses it](../documentation.md) describes), and
  which commands to run after a change. `.gitignore` holds `build/`, and
  `.gitattributes` holds the lines that keep records readable in a diff
  ([Records and manifest](18-records-and-manifest.md)). There are no
  hidden folders.

The project is named after the folder: the last part of `<name>`, or the
folder it runs in for `jadpo new .`. A folder that holds anything, hidden
files included, is refused with `toolchain.scaffold_destination_not_empty`
and left as it was; a file of that name is
`toolchain.scaffold_destination_exists`, and a `<name>` that ends in no
folder's name, such as `..`, is `toolchain.scaffold_name_invalid`. If a
write fails partway, `jadpo new` removes what it wrote and reports
`toolchain.scaffold_write_failed`.

The screen is one status line, then the command to run next:

```text
Created todo-app
    jadpo  0.1
    route  POST /hello
    files  12 written

next: cd todo-app && jadpo dev  serves it on http://localhost:3000
```

With `--json`, `data` holds the `path` as typed, the `root`, the `jadpo`
line, the `routes` and the `files` it wrote.

### `jadpo check`

`jadpo check` checks the whole project and writes the files derived from
it: `build/manifest.json` and the catalogues in `build/catalogue/`
([The `build/` folder](#the-build-folder)).

### `jadpo fmt`

`jadpo fmt` formats every file. It fixes each file's imports first, then
lays the file out, writing each file's declarations before their use, the
primary declaration last. `--check` only reports what it would change, and
`--layout` also makes promotion moves ([Formatting](#formatting)).

### `jadpo test`

`jadpo test` runs every test. It runs pure and action tests in the
interpreter, route tests, and schedule tests (`--kind schedule`). It also
reads each recording of the app's own integrations through its
operation's decoders (`--kind recording`), and each recorded answer to a
prompt through each type the prompt is asked for as. A recording made from
an older version of the prompt is an alert on the report and in
`data.alerts` ([Kinds of test](13-testing.md#kinds-of-test)).

To run fewer tests:

- `jadpo test <file>` runs one test file, and `<file>:<line>` the test at
  that line. A folder runs the tests under it.
- `--name <text>` runs the tests whose names match, `--kind route` one
  kind of test, and `--failed` the last run's failures.
- `--seed <n>` reruns the tests in the order a given seed sets.

`jadpo test --watch` reruns the affected tests on every save. Each save
reruns the tests that reach something whose digest, a hash of its code,
changed since the last run, within the path, name and kind you gave. While
the build is broken it shows the errors and runs nothing. With `--json` it
writes one envelope per line. Ctrl-c quits.

`--keep-failed` keeps a failed test's database and names it in the report.

### `jadpo build`

`jadpo build` writes the runnable app into `build/target/`, with each
API's `<api>.failures.<locale>.json` beside its OpenAPI document
([Configuration and time](10-configuration-and-time.md)). It takes
`--json`. It first checks as `jadpo check` does, writing the same files,
and stops at the errors.

- An app that declares no deployables builds its one deployable, `app`,
  natively: the binary `build/target/app/app`, and `build/target/app/openapi/`
  with each API's document and failure files. It is built with the
  toolchain and lock this Jadpo pins ([A native
  deployable](#a-native-deployable)).
- An app that declares deployables builds every one, or the one you name,
  each into `build/target/<deployable>/` beside only its own APIs'
  documents ([Declared deployables](#declared-deployables)).
- A deployable built for `Target.wasm` is a WebAssembly module with
  Jadpo's module shim and a Wrangler file, and the screen gives the
  module's size ([A Workers deployable](#a-workers-deployable)).

### `jadpo dev`

`jadpo dev` runs your app locally and keeps it up to date as you edit. It
runs the app in the interpreter at `http://localhost:3000` (`--port`
changes it) and prints one line per request with its request id.

Each save that checks cleanly serves the next request. While a save has
errors, the last good version keeps serving. `--no-run` only checks on
every save and writes `build/`, and `--json` writes one envelope per line.

An editor that saves a file in place first empties it, then writes its
text. So a save that leaves a file empty that had text isn't taken as
finished yet: the reload waits up to 2 seconds for the file's text. An
empty routes file checks cleanly, with no routes, so this keeps a
half-saved file from replacing the version serving. A file still empty
after that reloads as saved ([design notes](notes/14-tooling.md),
"Watching saves"). `jadpo test --watch` waits the same way.

**Settings.** The settings `config.jadpo` declares are read from the
environment, then `.env.local`, and checked before the app starts. A
missing or invalid value stops it starting, and the screen names the
setting and its variable, never the value. A save of `.env.local` is
checked like a save of source, so the last good version keeps serving.

**Databases.** Each database opens at the address its connection's `url:`
setting holds, or the one `--url` names. A store whose address no
variable gives gets a disposable PostgreSQL for the session. Jadpo starts
that from PostgreSQL's own programs (`initdb`, `pg_ctl` and `postgres`), so
PostgreSQL has to be installed on your machine; you don't create a
database or set a password. Pending migrations are applied as they appear.
While it runs, the address of each database it started is in
`build/dev/stores.json`, for `jadpo schedules` and `jadpo why`.

**External tokens.** An app that signs in with `external_token` gets a
local issuer in place of the configured one, and `jadpo dev token
<user-id>` prints a bearer token for that user from it
([External tokens in tests and `jadpo dev`](08-identity.md#external-tokens-in-tests-and-jadpo-dev)).
Not built yet.

**Background work.** Each schedule run and each skipped tick (a paused
schedule's, once) gets a line with its trace's id, its outcome, what it
emitted or its failure, and its time. Each alert gets a line when it
starts. Each step reached by an action with several changing calls is a
line under its request's line. A delivery whose action reached steps gets
a line of its own, with `jadpo deliveries show <id>` when it needs an
operator ([Jobs and events](12-jobs-and-events.md)).

**Provider calls.** Every call to an outside provider is faked by default,
so you need no credential. A connection's `development: { sandbox: }`
lets calls through to the provider's sandbox, never to its live service.
`--fake <connection>` and `--sandbox <operation>` change which for one
session only. Each request's line lists its calls, and each call is logged
for `jadpo calls` ([Provider calls in
development](11-integrations.md#provider-calls-in-development)).

**Stopping.** Ctrl-c or SIGTERM stops it cleanly: the server finishes the
requests it has, its background work gives back what it holds, and it
exits 0. A second signal ends it at once ([Running background
work](12-jobs-and-events.md#running-background-work)).

Not built yet:

- `jadpo test --affected`, which runs the tests a change reaches,
  measured against the merge-base with `main`.
- `jadpo test --interactive`, a keyboard view of every test: search,
  move between tests, run a file, one test or all, rerun failures and
  turn watching on and off ([Testing](13-testing.md)). The flag says
  `toolchain.not_built_yet`.
- The trace `--keep-failed` keeps, which comes with
  [Debugging and review](17-debugging-and-review.md).
- `jadpo dev` starting the services the app's connections declare
  ([External services](11-integrations.md)).

## Commands for looking into and changing code
<!-- topic: commands-for-looking-into-and-changing-code -->

These commands show you what your code does: its routes, the SQL its
queries send, and what a prompt sends a model.

| Command | Does |
|---|---|
| `jadpo routes` | Lists every route |
| `jadpo sql <query>`, `jadpo sql <action>` | Prints the SQL a query or an action's writes send |
| `jadpo ai show <prompt>` | Prints what a prompt sends a model |
| `jadpo ai record <prompt> --as <name>` | Records the model's answer to a prompt, for tests |
| `jadpo ai usage` | Shows the tokens and cost of the app's model calls |

### `jadpo routes`

Lists every route with its full method and path, its file, its API, its
authentication and the query or action it runs. Authentication shows
`required`, `none` or `fresh` ([Routes: signed in by
default](08-identity.md#routes-signed-in-by-default)). A connection's
generated webhook shows `signature` ([External
services](11-integrations.md)), and is listed under `webhooks  /webhooks`
when the app declares no `webhooks` API.

### `jadpo sql`

Prints the SQL a query sends to PostgreSQL: each statement, one for each
include too, with the parameters that fill it. It takes `--json`.

Given an action, it prints each `create`, `update`, `delete` and
transition the action writes, in order, with its parameters as the action
writes them. With each, it says what a refusal raises: the constraint and
the failure its `conflict:` arm names, the not-found failure, or a fault.
For a transition whose `when:` can refuse a record, it also prints the
statement that asks when no row changed.

Tab completes the project's queries and actions. Run without one, it
fails and lists them.

### `jadpo ai show` and `jadpo ai record`

`jadpo ai show <prompt>` prints what a prompt sends a model: its type,
file and version, the instructions as the system message, the
conversation's turns, and the request as the user message, with each
placeholder as the file writes it ([AI](21-ai.md)). It takes `--json`, and
the prompt's name or its type's. A misspelt action or prompt is
`cli.ai_arguments` with the nearest name. A prompt with errors shows them
instead.

`jadpo ai record <prompt> --as <name>` asks the configured model once, as
the prompt's first `ai.generate` call does. It uses the values of the
prompt's last `jadpo dev` call, or those of `--fixture <fixture>`. It
keeps the answer, with the prompt's version, as
`recordings/<prompt>/<name>.json` beside the prompt, its plain values
replaced by placeholders.

Through a package's connection it never asks the live service and never
uses a stand-in credential. With nothing to ask with, it is refused
(`recording.refused`). A refusal, no answer, or an answer that doesn't
decode keeps nothing (`recording.failed`).

### `jadpo ai usage`

Shows the tokens the app's `ai` calls used today and this month (UTC) and
their estimated cost, by prompt and model, against
`ai: { limits: { per_day: } }`. It reads the counts in the limits' store,
and never a prompt or an answer ([AI](21-ai.md)).

It takes `--json`. It shows the day's tokens, calls and cost, a table of
each prompt and model, and a footer that fails once the day's tokens are
spent. The database is the one its connection's setting names, a running
`jadpo dev`'s, or `--url`'s. One it can't read is `ai.usage_unavailable`.

Not built yet:

- `jadpo inspect`, which prints the project map: every declaration, call
  and failure. Until then `build/manifest.json` and the catalogues hold
  the map.
- `jadpo sql` for SQLite, which comes with a SQLite store, and
  `jadpo sql --explain`, which runs the statement on a disposable
  database and shows the plan. The flag says `toolchain.not_built_yet`.
- `jadpo impact <name>`, which lists what a change to `name` reaches
  ([Project layout and modules](01-project-layout-and-modules.md)).
- `jadpo rename <old> <new>`, which renames a declaration or field
  everywhere and keeps its stored data. It refuses a new name already in
  use.
- `jadpo domain rename`, `split` and `merge`, which move a domain's files
  and update every call
  ([Project layout and modules](01-project-layout-and-modules.md)).

## Commands for the database, settings and translations
<!-- topic: commands-for-the-database-settings-and-translations -->

These commands change what sits beside your code: the database's
migrations, your local settings, and the wording of your app in each
language.

| Command | Does |
|---|---|
| `jadpo migrate new <purpose>` / `apply` / `status` | Writes, runs and lists migrations |
| `jadpo seed <name>` | Fills the development database from a seed |
| `jadpo config move <constant>` | Makes a constant a setting |
| `jadpo config set <field>` / `check` | Sets or checks your local settings in `.env.local` |
| `jadpo translations missing` / `unused` / `update` / `accept` | Shows and records the state of each language's wording |
| `jadpo translations builtin` | Lists Jadpo's own messages your app can show |
| `jadpo translations export` / `import` | Exchanges wording with translation services |

### `jadpo migrate`

`new` writes the next migration, or finishes the latest once its
decisions are written ([Migrations](05-data.md#migrations)). `apply` and
`status` take the database's URL with `--url`, and `--store` when there
are several. Otherwise they read it from the connection's setting, in the
environment and then `.env.local`.

A missing purpose will be asked for at a prompt, as the [CLI design
language](../cli-design-language.md) draws it, once prompts are built.
Until then the command names a purpose to use.

### `jadpo seed`

`jadpo seed <name>` runs `database/seeds/<name>.jadpo`
([Seeds](05-data.md#seeds)) against the development database. Each
store's database is the one `--url` names, for a project with one store,
else the address its connection's setting gives, else the one a running
`jadpo dev` started. Pending migrations are applied first.

```text
$ jadpo seed first_staff
✓ Applied 1 migration to `main`

✓ Seeded `first_staff` in 154 ms
    main  localhost:5432/library_dev
```

The address is shown without its credentials. A seed that stops shows
where, as a test's failure does: the failure or fault, its code, the
calls that led there, and that what its earlier calls committed stays
(`seed.failed`). A database that isn't on this machine is refused before
anything runs (`seed.production_refused`).

### `jadpo config move`

`jadpo config move <constant>` turns a constant into a setting: a
`config` field of its name and type, with its value as `default:`. It
deletes the constant and rewrites every read to `config.<name>`
([Making a constant a setting](10-configuration-and-time.md#making-a-constant-a-setting)).

Name the constant as code reads it (`security.max_login_attempts`), or
after its domain. It lists what changed, file by file, then the setting
and its variable. It refuses with `configuration.move_type_unsupported`,
`configuration.move_name_taken` and `configuration.move_read_unsupported`,
every reason at once, and changes nothing. A move whose result wouldn't
check is undone. It takes `--json`.

### `jadpo config set` and `jadpo config check`

`check` lists each setting as `set`, `default`, `missing` or `invalid`,
with its variable, against the environment and then `.env.local`. It
reports each problem as `configuration.value_missing` or
`configuration.value_invalid` and never prints a value. It takes `--json`.

`set` asks for one value at a prompt, hidden for a secret, so the value
never goes on the command line. It refuses `--json` and a missing
terminal. It checks the value against the setting's type, then writes
`.env.local`: only as an ordinary file, through a private temporary file,
and only if the file hasn't changed since it was read. It adds
`.env.local` to `.gitignore`.

### `jadpo translations`

Each of these takes `--json` ([Tooling for
translations](10-configuration-and-time.md#tooling-for-translations)).

- `missing [<locale>]` lists, under the language file each is in, the
  wording a locale lacks and what a locale still in progress falls back
  on. It also lists translations made from wording that has since changed,
  and wording not yet accepted.
- `unused` lists the messages no code calls, and the app's wording of
  library packages' text that shows nothing.
- `update` records each translation's source in
  `translations/sources.jadpo_record`.
- `accept <locale>` accepts a locale's changed wording.

`missing` and `unused` succeed whatever they list. `update` and `accept`
stop on syntax errors, write one status line for each record, and say
`still stale` of a translation nobody redid. A misspelt action or locale is
`cli.translations_arguments`, with the nearest name.

`jadpo translations builtin` lists Jadpo's own messages your app can
show, by feature: their fields, the wording Jadpo ships and the locales
that still need it. It lists only the messages of features your app has
on, takes `--json` and succeeds whatever it lists. `missing`, `update` and
`accept` take the root `translations/` folder's wording of these messages
in with the rest ([Jadpo's and packages'
text](10-configuration-and-time.md#jadpos-and-packages-text)).

`jadpo translations export [<locale>]` and `import <file>` exchange
messages, their descriptions and their wording with translation services,
as XLIFF 2.0 or JSON. Each takes `--json`.

- `export` writes `build/translations/<locale>.xlf` for each translated
  locale, or the default locale's source file. `--format json` and
  `--out <folder>` change the format and the folder.
- `import` checks what comes back as `jadpo check` would. It writes the
  language files and the record, or nothing. It refuses with the language
  files' own rules, at the unit of the file, or with
  `translation.exchange_unit_unknown` or
  `translation.exchange_file_invalid`.

Library packages' text is exchanged under each package's folder name, and
written back as the app's, in the root file.

## Commands for records, findings and review
<!-- topic: commands-for-records-findings-and-review -->

Some files in your project are records: files Jadpo writes for you to
commit, such as the record of your public API, which a reviewer reads in a
pull request ([Records and manifest](18-records-and-manifest.md)). These
commands keep them current, and deal with the findings `jadpo check`
leaves for a person or an agent to settle.

| Command | Does |
|---|---|
| `jadpo api update` | Rewrites the committed record of each public API |
| `jadpo records` | Lists every committed record and flags any out of date |
| `jadpo declines list` / `clear` | Lists or removes your declines |
| `jadpo fix` | Applies the fix you pick for a finding |
| `jadpo suggest` | Hints where code could use a standard type or operation |

### `jadpo api update`

Rewrites the committed record of the public API, `api/main.jadpo_record`,
from the source. It writes one record per API with routes, as
`api/<api>.jadpo_record` in [the record
format](18-records-and-manifest.md#the-record-format), and removes the
record of an API that has no routes. A record has no `version:`: the
OpenAPI version is a hash of it.

`jadpo check` fails while a record disagrees with the source
(`record.out_of_date`) or was edited by hand (`record.edited`). A record
under its old name, `api/<api>.api`, is removed.

### `jadpo records`

Lists every committed record (the `.jadpo_record` files and `jadpo.lock`)
grouped by the command that writes it, each with its state. It fails while
one needs its command. `jadpo records update` brings every record up to
date at once and moves records from their old names.

### `jadpo declines list` and `clear`

A decline is a line in your code that answers a judgement-call finding
with a reason, `decline <rule>: "<reason>"` ([Declining a judgement
call](01-project-layout-and-modules.md#declining-a-judgement-call)).

`list` shows every decline with its rule, place and reason, as one list in
file order. A reason too long for its row goes on the lines under it.
`clear` removes the matching decline lines, so each finding comes back. It
stops on syntax errors and points at `jadpo check` afterwards. Both take
`--rule <id>`, a path and `--json`. An unknown action or rule, or a path
outside the project, is `cli.declines_arguments`.

### `jadpo fix`

Applies the fix you pick for a finding, through the same code as the
editor's quick fix. Its first use is a release of Jadpo that adds a
standard type your app already names ([A release that adds a standard
type](19-standard-library.md#a-release-that-adds-a-standard-type)).

- Alone, it lists each `type.standard_clash` and `type.standard_exists`
  finding with the fixes it takes.
- `jadpo fix <name> swap` and `jadpo fix <name> rename <NewName>` list
  what changed, file by file.

A fix whose result would add an error changes nothing and shows the
error. A name with nothing to fix, a fix it doesn't take, or a name already
taken is `cli.fix_arguments`. It takes `--json`. Its rename isn't
`jadpo rename`: it writes no identity line.

### `jadpo suggest`

Hints where code could use a standard type or operation, and never edits
([Suggestions](#suggestions)). It takes `--json`.

| Form | Does |
|---|---|
| `jadpo suggest` | Formats, then reports numbered findings and ends with the agent's instruction |
| `--catalogue` | Lists every signal |
| `ignore <number> "<why>"` | Keeps a suggestion quiet, in `ignored_suggestions.jadpo_record` |
| `found <file>:<line> <Type or operation> "<why>"` | Teaches a miss, in `taught_suggestions.jadpo_record` |
| `share`, `share --approve`, `--decline`, `--new-token` | Sends what was learned to Jadpo HQ |

The findings come from the signals on the standard types' entries and the
loop table. `ignore` and `found` also add to `.jadpo/suggest_pending`. A
wrong number, place, type or operation is `cli.suggest_arguments`.

`share` sends the pending hashes and shows the snippets HQ asks for, with
the exact payload. It sends those only with `--approve`. It stops at once
when sharing is turned off, and an HQ that can't be reached or refuses is
`suggest.share_failed` ([Sharing with Jadpo HQ](#sharing-with-jadpo-hq)).

Not built yet:

- `jadpo approval --before <ref>`, which writes the review report of what
  changed in behaviour, policy and the API. It comes with
  [Records and manifest](18-records-and-manifest.md); who signs off is
  under [Review and sign-off](#review-and-sign-off).
- Jadpo HQ itself: `share` is built against a fake of it.

## Commands for background work
<!-- topic: commands-for-background-work -->

These commands are for an operator: the person who looks after a running
app. They show what the app's schedules and event deliveries did, and let
you retry or stop what went wrong ([What an operator
sees](12-jobs-and-events.md#what-an-operator-sees)).

| Command | Does |
|---|---|
| `jadpo deliveries` | Lists, retries, dismisses, cancels and resolves schedule runs and event deliveries |
| `jadpo schedules` | Lists schedules; `run`, `pause`, `resume`, `history` |

### `jadpo deliveries`

`delivery` is a synonym, and it takes `--json`.

**What it lists.** A table of what is `failed` or `outcome_unknown`,
newest first, 20 to a page (`--page`). `--state` lists those `pending`,
`running`, `held`, `dismissed`, `cancelled`, `diverged`, `unfinished` or
`abandoned` instead. A delivery is `diverged` when a deploy changed the
steps of the action it was part way through. Above the table is an alert
line for each connection whose circuit breaker is holding deliveries; a
circuit breaker stops calls to a provider that keeps failing, for a time.
Below it are a footer and one hint.

Diverged deliveries and a route's unfinished steps are listed by
default. Parked queue messages are listed as `failed`, as
`queue <connection> · <Event>` with their reason as the code. A
multi-step run unfinished for a day gets an alert line, and `--json`
gives the metric on unfinished records.

**Acting on one.** `retry`, `dismiss`, `cancel` and `resolve <id> --done`
or `--not-done` take an id's last 8 hex digits. Each works only from the
states it applies to (`deliveries.wrong_state`, `deliveries.not_found`),
and each is logged with the operator's name and database role. A
schedule's failed run is retried through the app's tick, as
`jadpo schedules run` retries it. A parked queue message is retried
through the app's next tick, or dismissed.

**An action with several changing calls.** `show <id>` lists each step an
unfinished multi-step action recorded, never a request, an answer or a
key, and the one thing to do. `retry <id> --not-done` sends again a step
that is in doubt or failed, under its next key generation. `abandon <id>`
gives the run up ([Several changing calls in one
action](11-integrations.md#several-changing-calls-in-one-action)).

### `jadpo schedules`

Takes `--json`. Alone, it shows a table of each schedule's last run and
next due time, or `‖ paused`, the alerts that hold, and a footer. Each
entity's built-in purge schedule is listed too, as `Member.purge`, its
runs showing the records they purged ([Watching
schedules](12-jobs-and-events.md#watching-schedules)).

- `history <name>` shows 20 records a page (`--page`), grouped by day,
  then the metrics.
- `pause` and `resume` set the schedule's state.
- `run` runs it now in this process under its lease (`schedules.lease_held`,
  `schedules.run_failed`), but only against a running `jadpo dev`'s own
  database. Against a deployment it asks the deployment's own tick to run
  it and waits for its record (`schedules.no_tick`; [Running a schedule by
  hand](12-jobs-and-events.md#running-a-schedule-by-hand)).

The database is `--url`'s, the connection's setting's, or a running
`jadpo dev`'s. An unknown name is `cli.schedules_arguments`, with the
nearest.

Not built yet: `jadpo deliveries` doesn't list batch runs.

## Commands for packages
<!-- topic: commands-for-packages -->

A package is code you install rather than write: a library, or a provider
package that calls an outside service such as Stripe ([External
services](11-integrations.md)). These commands install, inspect, change and
publish them. Each built one takes `--json`, and an unknown package,
connection or operation is `cli.package_arguments`, with the nearest.

| Command | Does |
|---|---|
| `jadpo package add <name or source>` | Installs a package |
| `jadpo package import <format> <file>` | Turns a published API description into the app's own integration |
| `jadpo package update <name>` / `remove <name>` | Moves a package to a new version, or removes it |
| `jadpo package show <name> [<declaration>]` | Prints a package's description and public surface |
| `jadpo package link <path>` / `unlink <name>` | Compiles the app against a package's live checkout |
| `jadpo package recipe <package> <recipe> --into <domain>` | Copies a package's recipe into your code |
| `jadpo package record <connection> <operation>` | Confirms an operation against the sandbox and saves the recording |
| `jadpo package conform <package> <capability>` | Runs a capability's conformance suite |
| `jadpo package check` | Runs the publishing bar on a package |

### `jadpo package add`

Installs a package into `packages/<name>/<version>/` and `jadpo.lock`. It
installs from a folder or a git repository's tag, such as
`github.com/acme/acme-pricing@v2.1.0` ([Where packages come
from](11-integrations.md#where-packages-come-from)).

The package is compiled as the app's own integration, and its surface (the
names it makes public) is checked byte for byte against its committed
`surface.jadpo_record` (`install.surface_mismatch`). It is copied without
hidden files and locked with its version, digest and source. A repository
or tag that can't be read is `install.fetch_failed`. An install that adds
a name the app already uses is undone.

### `jadpo package import`

Turns a published description of an API into the app's own integration,
in `shared/providers/<name>/<version>/`, merging your corrections.
`--as <Name.vN>` names it. It reads `openapi`.

### `jadpo package update` and `remove`

`update` moves a folder's package to what the folder holds, and a git
package to its newest release of the same major version. It lists each
surface entry added, changed or removed, marked `used by the app` and
`needs a person`: the review of the surface the app uses that [Records and
manifest](18-records-and-manifest.md) describes. `remove` is refused while
the app's code has `use packages/<name>` (`install.in_use`).

### `jadpo package show`

Prints a package's description, licence, version, kind and public
surface, or one declaration with its file. `packages` is a synonym ([The
surface](11-integrations.md#the-surface)).

The description is in bold. The package's facts follow: its version and
source from its lock line, or "the app's own". Then comes a list per
section and a footer counting them. Given a declaration, it prints that in
full with its file and line and its whole description. `--guide` prints
the package's `GUIDE.md`.

### `jadpo package link` and `unlink`

`link <path>` compiles the app against a package's live checkout: the
lock's line becomes `link <path>`, and the line it replaced is kept in
`.jadpo/links`. Every check reads the checkout in place of the installed
folder and shows its problems there. `unlink <name>` restores the
installed version.

A link is for development only. With `CI` set, `jadpo check` and
`jadpo test` refuse it (`install.link_refused`), and `jadpo build` always
does.

### `jadpo package recipe`

Copies a package's recipe into a domain, where it becomes the app's own
code. Every file under the package's `recipes/<recipe>/` is copied to the
same path under `domains/<domain>/`, all or nothing, and never over a file
the app has. The screen lists each file written, then `jadpo check` as
the hint. `jadpo package check` compiles each recipe in the package's
`sample/` app.

### `jadpo package record`

Confirms an operation against the provider's sandbox and saves the
answer as a recording, for tests ([Provenance and
confirmation](11-integrations.md#provenance-and-confirmation)).

`--input` gives the operation's input as JSON, written as a route's body
is. It calls the sandbox once through the connection, never the live
service and never with a stand-in credential (`recording.refused`). It
checks the answer against the operation's output type and keeps it as
`recordings/<resource>.<operation>.json`, in the app's own integration or
a linked checkout, its plain values replaced by placeholders. A refusal,
no answer, or an answer that doesn't decode keeps nothing
(`recording.failed`). A missing or wrong `--input` is
`cli.package_arguments`.

### `jadpo package conform`

`conform <package> ai` runs the `ai` capability's six cases against a
package: a typed answer, a refusal, a cut-off, a stream, an embedding of
the declared length, and the usage reported ([AI](21-ai.md)). They run in
a scratch app holding the package as installed. By default they use its
recordings and a local fake on the loopback address, so nothing leaves
your machine.

`--live` runs them against the provider's API, through the app's
connection to the package. It is refused without that connection, without
its credential or address set, or without `ai:` through it
(`conform.refused`). A case that fails is `conform.failed`.

### `jadpo package check`

Runs the publishing bar, the checks a package passes before it is
released, in a package's own repository or the folder given ([Developing
and publishing a package](11-integrations.md#developing-and-publishing-a-package)).

- The package is checked as the app's own integration, each problem shown
  at its place in the package.
- The examples in its descriptions run as tests (`publish.example_missing`,
  `publish.example_failed`).
- The new version's section of `CHANGELOG.md` is written, and needs your
  reason for the release (`publish.changelog_missing`).
- The version is worked out from the surface at the last `v<version>` tag,
  and a breaking change is refused (`publish.breaking_change`).
- A `recipes` row compiles each recipe in the one domain of the package's
  `sample/` app (`publish.sample_missing` without one).

The bar is shown a row per item: passed, failed or not checked yet. When
the package compiles and its version is allowed, `surface.jadpo_record` is
written, to be compared with the previous tag's.

Not built yet:

- `jadpo package new <Name> --library` or `--provider`, which writes a
  package's own repository: the package format, `recipes/`, a `sample/`
  app and `GUIDE.md`.
- `jadpo package publish`, which publishes to the registry. It comes with
  the hosted registry.
- `jadpo package add` from the registry (in the fixed order [Where
  packages come from](11-integrations.md#where-packages-come-from)
  gives), from another Jadpo app's folder (its OpenAPI, such as
  `../orders`), or from a URL serving an OpenAPI document. A registry name
  and another app's folder say `toolchain.not_built_yet` until the
  registry and the importer. A third party's git package will need an
  acceptance, and publisher trust waits for `jadpo approval`.
- `jadpo package import` for `smithy`, `postman`, `har` and `dts`. They
  are `toolchain.not_built_yet` until first needed.
- `jadpo package conform` for any capability but `ai`, which is
  `toolchain.not_built_yet`.

## Commands for services, calls, email and stored data
<!-- topic: commands-for-services-calls-email-and-stored-data -->

These commands show what your app sent outside while you develop it, the
provider calls and the email, and check a deploy before it happens.

| Command | Does |
|---|---|
| `jadpo calls` | The development call log |
| `jadpo email` | The dev inbox and the sending domain's records |
| `jadpo runs unfinished --build <build id>` | Lists the multi-step runs a deploy would stop |

### `jadpo calls`

Shows the provider calls `jadpo dev` made, faked or real. It takes
`--json`. `--connection`, `--operation` and `--request` filter it.

- `jadpo calls` is a table of the last 500 calls, oldest first, each with
  its id, operation, source (`generated`, `scripted`, `sandbox`,
  `recording`), how it ended, its time and its request, then a footer and
  one hint.
- `calls show <id>` prints one call in full, laid out: what was sent,
  never its headers, and what came back.
- `calls script <operation> <answer>` sets the next answer for an
  operation: one of its failures' codes, or `unavailable`, `lost`,
  `in_use` or `misconfigured`. It checks the operation and the answer
  against the project first. `calls script ai.<prompt> <condition>` sets
  the next answer of a prompt asked through a package that fills `ai`, as
  one of `ai.generate`'s five conditions ([AI](21-ai.md)).

An unknown action, call, operation or answer is `cli.calls_arguments`,
with the nearest ([Debugging and review](17-debugging-and-review.md)).

### `jadpo email`

`jadpo dev` catches every email your app sends in a dev inbox, so nothing
reaches a real address ([Email](16-built-in-services.md#email)). It takes
`--json`.

- `jadpo email` is a table of the mail caught, oldest first, each with its
  id, email, address, subject and health. `--to` narrows it.
- `email show <id>` prints one with its text (`--html` for its HTML), its
  links numbered, its attachments, and each health problem with its fix.
- `email links <id>` numbers its links, and `email open <id> <link>` opens
  one in `$BROWSER` or the system's browser.
- `email clear` empties the inbox.
- `email check-domain [<domain>]` reads SPF, DKIM (`--selector`) and DMARC
  for the domain given, or for `email: { from: }`'s.

An unknown action, mail or link is `cli.email_arguments`, with the
nearest.

### `jadpo runs unfinished`

Run it against production before a deploy. It lists the unfinished
multi-step runs and batches whose recorded path the new build changes, so
an operator knows what will stop as `diverged`
([Deploys](12-jobs-and-events.md#deploys)). It takes `--json`.

The build must be the one the checkout makes; `cli.runs_arguments` names
it otherwise. It shows a table of each record whose build differs and
whose action's step shape the build changes, so the record stops as
diverged, or no longer keeps, so its steps are sent again. It also lists
each running batch run whose call the build changes, which fails. A footer
and one hint follow. `--url` and `--store` work as for `jadpo deliveries`
(`runs.unavailable`).

Not built yet:

- `jadpo services`, with `start`, `stop`, `restart <name>`,
  `logs <name>` (`--follow`, `--since`) and `shell <name>`. It shows each
  container a package runs: its package, image, state, health, CPU and
  memory, restarts and uptime, and manages one. `shell`, a shell inside
  one, is for development only. In production they talk to the running
  app, never to the container engine ([External
  services](11-integrations.md)).
- `jadpo dev grants add <connection> --owner <id>`, which makes a fake
  OAuth grant for local development ([Acting for each user: OAuth
  grants](11-integrations.md#acting-for-each-user-oauth-grants)).
- `jadpo files copy`, which copies files between two stores (S3, R2,
  disk) ([Files](16-built-in-services.md#files)).
- `jadpo batches list`, `show <id>` and `cancel <id>`, which list batch
  runs (file reads and writes, `batches.each`) with their state and
  progress, show one run's progress and problems, and stop one, as
  `batches.cancel` does ([Jobs and events](12-jobs-and-events.md)).
- `jadpo search reindex`, which rebuilds a search index from the
  database ([Search](16-built-in-services.md#search)).

## Commands for debugging, editors and help
<!-- topic: commands-for-debugging-editors-and-help -->

| Command | Does |
|---|---|
| `jadpo why <schedule>` | Says why a schedule didn't run, or how its last run went |
| `jadpo lsp` | Runs the language server for editors |
| `jadpo help [command]` | Help for one command; `--help` works everywhere |
| `jadpo help <rule>` | A rule's page, offline, by the id on an error's `rule:` line |
| `jadpo completions <shell>` | Prints the Tab-completion script for zsh, bash or fish |
| `jadpo version` | Prints the version |
| `jadpo toolchain` | Lists, removes and pins the Jadpo releases kept for projects |

`jadpo why <schedule>` ends with the schedule's `.jadpo` place, and takes
`--json` ([Watching schedules](12-jobs-and-events.md#watching-schedules)).
Another name is `cli.why_arguments`.

`jadpo lsp` is what your editor runs ([Editor support](#editor-support)).

`jadpo toolchain` lists the releases kept for projects, `remove
<version>` deletes one, and `pin` and `unpin` write the release a project
runs with into `jadpo.lock`. Each takes `--json`
([Which jadpo runs](#which-jadpo-runs)).

`jadpo help [command]` explains one command, every command takes `--help`
with examples, and `jadpo help --json` gives the rule catalogue and the
schemas of the manifest and of `jadpo test --json`.

`jadpo help <rule>`, with the id an error's `rule:` line gives
(`jadpo help import.unused`), prints the rule's page offline, the page its
`help:` link opens: the rule, its area, whether it is declinable and who
decides, then the page's prose and each example in order, under the file
it is in, and what `jadpo check` printed for it. The supporting files the
site hides are left out. `--json` gives the rule's facts and the page's
Markdown. A word with a `.` that is no rule is `cli.unknown_command`,
naming the nearest rule.

Not built yet: the debugging commands of [Debugging and
review](17-debugging-and-review.md), which are proposed there:
`jadpo trace <id>`, `jadpo why` for anything but a schedule,
`jadpo call` and `jadpo console`. Also `jadpo incident [file | -]`,
which explains a runtime event from a file, from standard input
(`pbpaste | jadpo incident -`, or a log stream piped in), or, with no
argument in a terminal, from text pasted at a prompt. It finds the Jadpo
events inside the surrounding log text and groups them by request.

## Running tests
<!-- topic: running-tests-from-the-command-line -->

`jadpo test` owns everything a test run needs, so nothing about it
appears in your source ([Testing](13-testing.md)). It starts a disposable
local PostgreSQL, from the PostgreSQL programs installed on your machine,
or uses `JADPO_TEST_DATABASE_URL` in CI. It builds one template database
from the migrations and copies it for each test, so tests run in
parallel. `--keep-failed` keeps a failed test's database, and `--kind
route` runs one kind of test. The report lists each failed test with the
failure it got and the one it expected.

The compiler also generates tests from facts it already has:

- every route refuses bad input with `{ field, rule }` details;
- every declared failure reaches the client in the right envelope and
  status;
- no output has a field its type doesn't declare.

They run with your tests, and the report shows them under the route they
test.

Not built yet: `--affected`, which runs the tests a change reaches.

## Formatting
<!-- topic: formatting -->

`jadpo fmt` lays out your code, so you never choose a style. Equivalent
code always looks the same, and a diff only ever shows real changes
([Project layout and modules](01-project-layout-and-modules.md)). The
editor formats on save, and `jadpo fmt --check` fails when a file differs
from what the formatter would write.

```jadpo fragment
type Flags = Object {
    done: Bool
    pinned: Bool
}

type Count = Int {
    min: 0
}
```

### Layout

The formatter decides indentation, spacing and blank lines: one between
declarations, never two. It writes one member per line, as above. A short
list such as `pick: [Todo.id, Todo.title]` stays on one line.

### The order of sections

The sections inside a declaration go in one order:

- a failure's `kind`, `code`, `message`, `client`, `internal`;
- a route's `authentication`, `rate_limit`, `input`, `output`, `status`,
  `deadline`, `run`;
- an entity's fields, then `identity`, `persistence`, `lifecycle`,
  `membership`, `policy`, `freshness`, `guard`, `person`.

Any other section keeps its written order after these. Fields are never
reordered, and a section's comments move with it.

The one exception is `config { }`: its fields are sorted by name, each
with its comments, so two workspaces that each add a setting rarely touch
the same lines ([Fields in name
order](10-configuration-and-time.md#fields-in-name-order)).

### Imports

Inside a project, `jadpo fmt` fixes each file's `use` lines before it lays
the file out ([Imports](01-project-layout-and-modules.md#imports)). It
adds a missing import, removes an unused one, and moves a name imported
from the wrong folder, by applying the edits that `jadpo check`'s import
problems carry. Then it sorts the lines. That needs the project's names
resolved, so it waits until every file parses. `jadpo fmt --check` fails
on a file whose imports it would change, as it does on one it would lay
out differently.

### The order of declarations

Inside a project, `jadpo fmt` also writes each file's declarations in
definition-before-use order, below its opening comments and `use` lines
([Files and names](01-project-layout-and-modules.md#files-and-names)):

- each declaration comes before the first that uses it, so the primary
  declaration, the one the file is named after, comes last;
- ties are broken by kind, then by the order the primary first reaches
  them;
- declarations that use each other keep the order they were written in;
- each keeps the comments written above it and after it on its line.

Test files and loose files have no primary declaration and keep their
order. `jadpo fmt --check` fails on a file out of order.

### Promotion

Promotion is the one layout change that moves code between files. When a
second file uses a type or helper, `jadpo check` reports
`layout.promotion_required` with a one-step fix. You apply it with the
editor's quick fix or `jadpo fmt --layout`, never on save, so a move is
always a visible, separate change.

`jadpo fmt --layout` makes every move a type needs: out of its file with
its comments, into a file of its own below the `use` lines it needs. Then
it fixes the imports the moves touched. Moving a helper function into a
library changes its calls, so that stays a change you make by hand.

## Which jadpo runs
<!-- topic: which-jadpo-runs -->

A project is written for one release line of Jadpo, such as 0.1, and
while Jadpo is at 0.x a minor release may break an app. So the `jadpo`
that runs a command is the one the project names, whatever you have
installed. You don't do anything for that.

The `jadpo` you install (Homebrew, the npm package `@jadpo/jadpo`, or the
shell or PowerShell installer) is a whole `jadpo` that can also start
another release, as the `go` command does. Before a command runs, it
finds the project: the folder you run it from, walked up to `app.jadpo`,
or else the first argument that names a file or folder in a project. It
reads the line from `application { jadpo: "0.1" }`
([The root](01-project-layout-and-modules.md#the-root)).

When the project is on the installed `jadpo`'s own line, or names no
line, the installed `jadpo` runs the command. Otherwise it runs the
release the project needs, in the same folder and with the same
arguments, and exits as that release does. It picks:

1. the release `jadpo.lock` pins, if there is one (see "Pin a release"
   below);
2. else the newest release of the line it already keeps;
3. else the newest release of the line published on GitHub, which it
   fetches once.

A project on the installed `jadpo`'s own line runs with that `jadpo`, so
a newer patch of the line is used once you install it, or pin it.

Outside a project there is no line to read, so `jadpo new`,
`jadpo --version`, `jadpo help` and every other command run with the
installed version. The VS Code extension runs `jadpo lsp` from the
project's folder, so it gets the project's release with no setting.

### Fetching a release

A release comes from `github.com/jadpo-dev/jadpo`'s releases, and is kept
in `jadpo/releases/<version>/` in your cache folder: `$XDG_CACHE_HOME`,
else `~/.cache`, else `%LOCALAPPDATA%` on Windows. Nothing is written
anywhere else, and two commands that need the same release at once fetch
it once.

Nothing in a download runs until it passes two checks. Its SHA-256 has to
be the one the release publishes beside it. Its GitHub artifact
attestation has to verify too: that is a signed record, kept by GitHub
and Sigstore's public log, saying this repository's `release.yml` built
that archive at the release's tag. `jadpo` checks it with Sigstore's own
verifier against the trusted root it carries. A download that fails
either check is refused, and nothing is kept.

While it fetches, stderr shows a live line, `Fetching Jadpo 0.2.3 for
aarch64-apple-darwin`, with a bar once the archive's size is known, or
that line once where stderr doesn't animate. Then it says what it got,
and the command's own output follows:

```text
✓ Fetched Jadpo 0.2.3 (24.1 MB) and checked its SHA-256 and attestation in 2.3 s
```

Under `--json`, nothing is written but the command's own document.

### The releases kept

| Command | Does |
|---|---|
| `jadpo toolchain` (or `list`) | Lists the releases kept, newest first, with each one's size |
| `jadpo toolchain remove <version>` | Deletes a kept release |

Both take `--json`, and both always run with the `jadpo` you installed,
since they look after its cache. A release you remove is fetched again
the next time a project needs it.

### Pin a release

Pin the exact release a project runs with when its builds must be
reproducible:

| Command | Does |
|---|---|
| `jadpo toolchain pin` | Writes the release the project runs with into `jadpo.lock` |
| `jadpo toolchain unpin` | Takes the pin out again |

The pin is the lock's first line, `jadpo version 0.1.2`
([What is committed](18-records-and-manifest.md#what-is-committed-and-why)).
`pin` runs as the project's release, so that is the release it writes.
Commit the lock with the code. A pin on another line than `jadpo:` names
is out of date, `record.out_of_date`, until `jadpo toolchain pin` writes
it again.

### Run your own build on every project

People working on the compiler set `JADPO_TOOLCHAIN=local`, so the
`jadpo` they start runs every command itself, whatever line the project
names. A project on another line is then refused by `jadpo check`, with
the errors below.

### When the release can't be run

When `jadpo` can't run the release a project needs, because it is offline
with nothing kept, the line has no release yet, or a download fails its
checks, it stops. It never compiles the project itself. The error names
the release to install, says why it couldn't be fetched, and gives the
next step for that cause. It is `toolchain.version_newer` when the
project needs a later release than this one, and
`toolchain.version_older` when it needs an earlier one.

### What a release must keep, for older launchers

Every `jadpo` ever installed goes on running the releases later projects
name, so it fetches them by what it was built knowing. These are a
contract with every installed `jadpo`, and changing any of them is a
breaking change to all of them. A test pins each
(`crates/jadpo-cli/src/launcher.rs`, `contract`), so a change fails the
gate pointing here.

- **The line.** `application { jadpo: "X.Y" }` in `app.jadpo`, the
  `jadpo:` member of the `application` singleton with its value a string
  of two numbers, read on its own without the rest of the project
  (`jadpo_check::jadpo_version::declared`). Any `app.jadpo` a later
  release accepts must still put the line there in that form.
- **The pin.** A `jadpo version X.Y.Z` line in `jadpo.lock`, a release
  with no pre-release, read without the rest of the lock.
- **Where releases are.** GitHub Releases of `jadpo-dev/jadpo`, tagged
  `vX.Y.Z`, each archive at
  `https://github.com/jadpo-dev/jadpo/releases/download/vX.Y.Z/jadpo-cli-<target>.tar.gz`,
  `.zip` for `x86_64-pc-windows-msvc`, for each target `dist-workspace.toml`
  names; a target dropped from it can never run a later release.
- **The SHA-256 and the archive.** `<archive>.sha256` beside it, its first
  word the archive's SHA-256 in hex (`sha256sum`'s form); inside the
  archive, `jadpo` (`jadpo.exe` on Windows) at its top or in its one
  folder, `jadpo-cli-<target>/`. Nothing else in the archive is used.
- **The attestation.** A GitHub artifact attestation of the archive's
  digest on Sigstore's public-good instance, whose certificate GitHub
  Actions issued to
  `https://github.com/jadpo-dev/jadpo/.github/workflows/release.yml@refs/tags/vX.Y.Z`:
  the repository, the workflow's path and its run at the tag
  (release-please starts it with `--ref "$TAG"`). Renaming the
  repository or the workflow, or running it on a branch, makes every
  installed `jadpo` refuse the release.
- **Finding a line's newest release.** GitHub's API without signing in,
  `https://api.github.com/repos/jadpo-dev/jadpo/releases?per_page=100`,
  pages of 100 up to ten, reading each release's `tag_name`, `draft` and
  `prerelease`; and `/attestations/sha256:<digest>` for the attestation,
  its `attestations[].bundle`. GitHub allows 60 such requests an hour from
  one address. A fetch takes two or three, and a kept release none, so the
  limit is met only on a shared address such as CI's. When it is met, the
  error says GitHub's API refused it for that limit and to run the command
  again in an hour; nothing is fetched or kept.
- **The handshake.** A release a `jadpo` starts gets
  `JADPO_TOOLCHAIN_LAUNCHED` set to its own version and runs the command
  itself when that is its version, so it never hands the command on
  again; `JADPO_TOOLCHAIN=local` runs everything with the `jadpo` started.
  The command line reaches the release whole, before any of it is read,
  so a command or flag a later release adds works through any launcher.
- **The cache.** `jadpo/releases/<X.Y.Z>/jadpo` in the user's cache
  folder, written whole by a rename under `.<X.Y.Z>.lock`; a folder there
  is a release that was checked.
- **The trusted root.** Each `jadpo` checks attestations against the
  Sigstore trusted root it was built with, without refreshing it. After
  Sigstore rotates a key that signs GitHub's attestations, older launchers
  refuse releases signed with the new key: the error says Jadpo X couldn't
  be fetched to run it because its archive's attestation doesn't verify,
  and to run again, then to install the newest `jadpo`, whose trusted
  root may be newer, if it fails the same way. Installing a `jadpo` built
  after the rotation is the fix, and its release notes say so.

## Editor support
<!-- topic: editor-support -->

Your editor gets the compiler's own view of your code through the language
server, `jadpo lsp`: errors while you type, hover, go to definition, find
references, completion, formatting and quick fixes. It has no parser of
its own, so it never disagrees with `jadpo check`.

### The VS Code extension

The VS Code extension starts `jadpo lsp` for `.jadpo` files, so everything
the server gives works in VS Code. It formats `.jadpo` files on save by
default. It highlights keywords, declarations, types, strings, numbers and
quantities, comments, operators, route paths and `use` lines.

The `jadpo.server.path` setting names the binary: `jadpo` on your `PATH`
by default, where the installers put it. A language status item names the
`jadpo` running and its version. When none is found, or it didn't start,
the item says so with the one step to take, and a notification says so
once. Nothing is shown when all is well (the [CLI design
language](../cli-design-language.md)).

"Jadpo: Open Generated File" lists the readable files `jadpo check` wrote
in `build/`, grouped by folder, leaving out `build/rust/` and
`build/target/`. It opens the one you choose, and its refresh button runs
`jadpo check` again. The extension's README covers installing it, its
settings and its commands.

Each release packages the extension at the release's version and
publishes it to the VS Code Marketplace and Open VSX once the release's
installers are out.

### Errors and hover

`jadpo lsp` gives the errors `jadpo check` gives while you type. It keeps
one database of the project for its whole life, so an edit rechecks only
what it changes.

Hovering over a name shows:

- its declaration, written as Jadpo, and its `///` description;
- for anything you can call, the failures it can produce;
- for a failure, its code, kind and status;
- who calls or uses it;
- for a route, its full method and path ([HTTP](07-http.md)).

Hovering over a query, where it is declared or called, shows its
generated SQL once the project has no errors: the statements `jadpo sql`
prints from the same plan, one block for the query and one for each
include ([Reads and writes](06-reads-and-writes.md)). The SQL is never
written into your source as a comment.

### Go to definition and find references

Go to definition works from every use of a name: in a body, a signature or
a `use` line ([Imports](01-project-layout-and-modules.md#imports)). From a
`translations.<domain>.<message>(...)` call it goes to the message's type
and to its wording in each language file ([Configuration and
time](10-configuration-and-time.md)).

Find references lists every name in the project that the checker resolves
to the same declaration, or to the same parameter or variable in its body.
A private name of the same spelling in another domain is never counted.

### Formatting and quick fixes

Formatting in the editor runs `jadpo fmt`'s own code on the text the
editor holds, saved or not. When every file parses it fixes the imports,
then lays the file out, as one edit of the whole document. A file that
doesn't parse gets no edit.

Every error whose next step carries edits is a quick fix, named after that
step without the command line's "or run ..." alternative. It makes the
same edits `jadpo check --json` gives an agent. Promotion is one of them,
made as `jadpo fmt --layout` makes it: the move, every file's imports
fixed, and each file the move touched laid out if it was laid out before.
An editor that resolves code actions only gets a quick fix's edit once you
choose it.

### Code actions

A code action is something the editor offers on a piece of code, which you
choose to apply. Each goes through the same code as a command, so the two
never behave differently.

- **"Make this a setting"**, on a constant's declaration or any read of
  it, works as `jadpo config move <constant>` does and refuses what it
  refuses. A move it can't make is offered as unavailable, with its
  reasons. Its edit replaces each changed file, creates `config.jadpo`
  when there is none, and deletes a file the move empties. It is offered,
  not a fix for an error, so `jadpo fmt --layout`, which makes only the
  moves errors ask for, never makes it ([Making a constant a
  setting](10-configuration-and-time.md#making-a-constant-a-setting)).
- **"Decline with a reason"**, a quick fix on a finding you can decline,
  writes `decline <rule>: ""` above the declaration the finding is on.
  `decline.no_reason` asks for the reason until you write it. **"Remove
  decline"**, on a `decline` line, removes the line. Both work as
  `jadpo declines clear` does ([Declining a judgement
  call](01-project-layout-and-modules.md#declining-a-judgement-call)).
- **"Swap for the standard ..."**, a quick fix on `type.standard_clash` and
  `type.standard_exists`, works as `jadpo fix <name> swap` does. Where the
  command stops, it is unavailable and shows the error it would add.
  Renaming instead needs a new name, so that stays with the command until
  the editor's rename comes with `jadpo rename` ([A release that adds a
  standard type](19-standard-library.md#a-release-that-adds-a-standard-type)).

### Completion

The editor offers completion for everything as you type. It is generated
from the same declarations and data the compiler uses, never from a list
kept for the editor: the project's name and type tables, the standard
library's stubs and the standard types' data entries, the operation, unit
and enum tables, the rule catalogue, the settings, the translations'
messages and the parser's own keyword sets. So every type, unit and
operation added to Jadpo is offered with no editor code of its own.

Where the position needs a value of a known type (an argument, a field, a
variable's declared type, a `return`), what fits comes first, and a unit or
a variant that can't go there isn't offered. What it offers:

- **Names in scope** in a body: locals and parameters, the file's
  constants and its domain's callables, types, libraries, the domains it
  may call into and the standard namespaces. A project name the file
  doesn't import yet comes with its `use` line, the edit `jadpo fmt`
  would make. Another domain's private name isn't offered.
- **Members after a value**: `note.` offers the fields the checker reads
  from its type.
- **Operations after a namespace**: `list.`, `text.`, `temporal.` and the
  rest offer each operation with its signature, the failures it can raise
  and its description, from its stub.
- **Variants after a type**: `Shade.` offers the enum's variants. A
  standard type's are shown with the name its data entry gives people
  (`Locale.en_gb` is "British English").
- **Units straight after a number**, narrowed to the type it goes into:
  after `2` where a `Duration` is required, `d`, `h`, `m`, `s` and `ms`;
  none where an `Int` is. The units come from the standard library's own
  table, so quantity types offer theirs once they have units.
- **Named arguments** of the standard-library call being written, those
  not given yet, the required ones first; and the fields not given yet of
  a record being built.
- **Type names**, the application's and the standard ones, wherever a type
  is written.
- **Rule ids after `decline`**: only the rules you can decline.
- **`config.`**: the settings. **`translations.<domain>.`**: the messages.
- **Keywords** where a declaration or a statement may start.

Choosing a call with required arguments inserts it as a snippet, with a
placeholder for each argument. For a standard-library operation the first
is in its place and the rest are named
(`temporal.resolve(${1:date}, at: ${2:at}, zone: ${3:zone})`). For the
application's own callables every one is in its place.

The server offers what the compiler knows and nothing else: suggestions
written by a model belong to your agent, never to `jadpo lsp`. VS Code
asks for completion as you type a word, a `.`, a `:` or a digit.

Not built yet: publishing the extension, which is switched off until the
Marketplace and Open VSX are set up; and rename in the editor. It goes through the same code as
`jadpo rename`, so an entity field renamed in the editor keeps its
stored data, and it waits for that command and the identity registry it
writes ([Data](05-data.md), [Stable
identity](18-records-and-manifest.md#stable-identity)).

## Error messages
<!-- topic: error-messages -->

Every error has one shape, in the terminal, in the editor and as JSON: what
is wrong, where, why, the recommended next step, who decides, the rule ID
and a help link. One mistake gives one error: the compiler doesn't report
the errors that follow from it. An error reads:

```text
error: `titel` isn't defined in this scope
 --> domains/planning/libraries/titles.jadpo:6:12
  |
6 |     return titel
  |            ^^^^^
  reason: Jadpo could not find this name. Did you mean `title`?
  next: Use `title` instead of `titel` (agent)
  rule: type.unknown_value
  help: https://jadpo.dev/docs/errors/type.unknown_value
```

`(human)` at the end of an error's `next:` line tells your coding agent to
stop and ask you which fix you want, because the choice is yours to make
(who can see data, what a setting holds). `(agent)` means your agent can
make the fix itself.

- Paths are relative to the project.
- The two impact lines appear only when they say something.
- An obvious repair carries its edit, so the editor and agents can apply
  it.
- The help link is the address of the error's own page on the site,
  `https://jadpo.dev/docs/errors/<rule>`, which has an example of the
  rule.
- JSON gives line and column as well as byte offsets, and uses one naming
  style for its keys.

### When Jadpo itself fails

Sometimes the fault is Jadpo's. If `jadpo` panics, including inside the
interpreter that `jadpo dev` and `jadpo test` run, it says so first, so
you don't hunt for a mistake in your own code. This is the screen:

```text
✗ failed  This is a bug in Jadpo, not in your code
  ran       jadpo check
  version   jadpo 0.1.0 · aarch64-macos
  in        crates/jadpo-check/src/lib.rs:12

What to do next
  1. Read the crash file. It can quote your project.
     /Users/ada/.cache/jadpo/crashes/2026-10-30T14-02-31Z.txt
  2. Report it with this link, attaching the file.
     https://github.com/jadpo-dev/jadpo/issues/new?template=bug.yml&title=Internal%20error%20in%20jadpo%20check&version=0.1.0%20aarch64-macos
```

The screen names only the command word, never an argument, and never the
panic message. The crash file holds those, so it can quote your project:
read it before you attach it. Jadpo writes it under `$XDG_CACHE_HOME/jadpo/
crashes/`, or `~/.cache/jadpo/crashes/`, and keeps the newest 20. It holds
the version, the platform, the arguments (`jadpo config` keeps only its
subcommand), the panic message and a backtrace. Jadpo sends nothing
anywhere.

A deployed app is different: the client sees only `500 internal_fault`, and
the log says more ([What it logs](#what-it-logs); [Failures](04-failures.md#a-bug-in-jadpo)).

## The `build/` folder
<!-- topic: the-build-folder -->

`build/` holds only generated files. It is never edited, never committed
and never read as input, and the same source always produces the same
bytes. Anything a person reviews and keeps lives outside it, such as the
record of the public API ([The public API
record](07-http.md#the-public-api-record)) or the identity registry
([Stable identity](18-records-and-manifest.md#stable-identity)).

```text
build/
  manifest.json               the application's manifest (area 18)
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
  rust/                       a deployable's Rust and cargo's builds of it (jadpo build only)
  target/                     the runnable app (jadpo build only)
```

For a project, `jadpo check` writes `build/manifest.json` ([Records and
manifest](18-records-and-manifest.md)) and the catalogues
`build/catalogue/libraries.md` and `failures.md`, each with a `.json`
twin. It writes them whether or not there are errors, so what an agent
looks up is never stale. A file is rewritten only when its bytes change.
Loose files, outside any project, write nothing.

Each report is written as Markdown, which an agent can search, with a
`.json` twin for tools. The query report is specified with
[Reads and writes](06-reads-and-writes.md), and is kept beside the schema
report because a reviewer reads them together.

Not built yet: the reports, OpenAPI and the monitoring rules in
`build/`, which come with the pages that specify them. Among them are
the schema report ([Data](05-data.md)) and the permissions report ([The
permissions report](09-policy.md#the-permissions-report)).

## Tools for agents
<!-- topic: tools-for-agents -->

Coding agents look things up through the compiler rather than by browsing
folders. Before an agent writes a helper, it reads what already exists;
before it changes something shared, it asks what the change reaches.

- **The library catalogue** (`build/catalogue/libraries.md`): every
  library function with its signature, `///` description and callers, and
  every library constant with its type, value, description and readers
  ([Logic](03-logic.md)). An agent reads it before writing a helper or a
  constant.
- **The similar-helper check**: when a new helper looks like an existing
  one, `jadpo check` reports `library.similar_exists` and names it. Two
  constants of the same type holding the same value are
  `constant.similar_exists` ([Two constants holding one
  value](03-logic.md#two-constants-holding-one-value)). Both are errors
  you can decline, not warnings: make the fix, or write a
  `decline <rule>: "<reason>"` line above the declaration ([Declining a
  judgement call](01-project-layout-and-modules.md#declining-a-judgement-call)).
- **The failure catalogue** (`build/catalogue/failures.md`): every failure
  with its code, kind, status and client fields.
- **The schema and query reports** (`build/report/`): every table, column
  and index with the query that needed it, and each query call's store,
  row bound and query count.
- **`jadpo impact <name>`**: what a change to a declaration reaches.

```text
$ jadpo impact rounding.half_even
rounding.half_even   shared/libraries/rounding.jadpo:4
  called from   ordering (3 calls), payments (1 call)
  entry points  POST /orders, POST /payments/refunds, schedule nightly_invoices
  tests         ordering/actions/place_order.test.jadpo, payments/actions/refund.test.jadpo
  public        no
```

- **`jadpo domain rename`, `split` and `merge`**: move files and update
  every call. A new or renamed domain appears in the approval review
  ([Project layout and modules](01-project-layout-and-modules.md)).
- **The Jadpo agent skill** (`skills/jadpo-agent/SKILL.md`): tells an agent
  to run `jadpo check --json` after edits, read the catalogues before
  adding code, and run `jadpo impact <name>` before changing or removing
  anything shared or public. It points at these pages for syntax.
  - It gives the routine for agents working in parallel workspaces: before
    pushing, merge `main`, run `jadpo check` (it builds nothing), fix any
    clash, then push. Private functions and types need only be unique
    within their domain, so two domains' helpers don't clash. A name
    another agent took in the same domain, or across the project, does.
  - It gives the rule for a fixed value: if a deployment could ever need a
    different value it is a setting in `config.jadpo`, otherwise a
    `constant` ([A constant or a setting](03-logic.md#a-constant-or-a-setting)).
    Never both, so a `default:` never names a constant. A constant that
    turns out to need a value per deployment is moved with
    `jadpo config move <constant>`, not by hand.
- **Generated CI runs `jadpo check` first**, so a name clash or any other
  check error fails in seconds, before tests or a build. No page says yet
  what generates a project's CI (`jadpo new` creates none); whatever does
  must follow this.

Not built yet: the schema and query reports, `jadpo impact`, the
`jadpo domain` commands, the agent skill and generated CI. The
catalogues and the similar-helper check are built.

## Suggestions
<!-- topic: suggestions -->

`jadpo suggest` finds code that could use a standard type or a standard
operation, such as money held as a whole number of pence, or a loop that
adds up a list. It never edits: the edits stay with your agent, and, for
type clashes only, with `jadpo fix`. An agent runs it after writing code,
to catch hand-rolled money, durations, points and loops. It is also how
Jadpo learns which of its signals are right.

It formats first, as `jadpo fmt` does; a file with syntax errors stops it,
as it stops every command that writes source. Then it scans the program
graph: every type, field and entity, and every function, action and
constant body. Tests, fixtures and seeds are left out, since they follow
the code they test.

## Signals
<!-- topic: signals -->

A signal is a pattern `jadpo suggest` looks for. Signals are data on each
standard type's contract entry ([Standard
library](19-standard-library.md)), never code of their own, so a type
adds signals by adding data. Each kind of signal has one engine that reads
every entry.

| Signal | Data on the entry | Fires on | Example |
|---|---|---|---|
| `shape` | The fields `type.standard_exists` matches ([Standard library](19-standard-library.md)) | A type or entity holding all of a shape's fields among others of its own; exactly the shape is `type.standard_exists`, a `jadpo check` error | `latitude` and `longitude` in `Shop` → `Point` |
| `conversion` | Constants code multiplies or divides by, and the words of names a value of the type goes by; a quantity's factors between its units and its units' names and codes | A multiplication or division by one of the constants whose other side has a name with one of the words, unless it already is the type | `total_price * 100` → `Money` |
| `name` | The same words, and the built-in types a hinted name is built on | A field, parameter or type built on one of those types whose name has one of the words | `timeout_ms: Int` → `Duration` |
| `format` | Text that formats a value by hand: a currency symbol, a unit's symbol | A text literal holding it, joined with `+` to a value | `"£" + text.from(pence)` → `Money` |
| `validation` | Texts the type's validation accepts and texts it refuses | A regular expression, in `matches` or a `pattern:` rule, that accepts every one and refuses every one | `pattern: "[^@]+@[^@]+\\.[a-z]+"` → `Email` |
| `loop` | A table beside the operation table, since operations have no contract entry: the loop each operation replaces | A `for` loop whose only statement is that loop's | `total = total + item.price` → `list.sum` |
| `taught` | A snippet `jadpo suggest found` recorded, in this checkout | The same canonical form anywhere | as recorded |

The entries with signals so far:

- `Money`: × or ÷ 100 and 1000, the minor units of two- and three-place
  currencies; `price`, `pence`, `cents`, `cost`; `£`, `$`, `€`, `¥`.
- Every quantity: its units.
- `Duration`: 1000, 60, 3600, 86400; `ms`, `millis`, `seconds`,
  `minutes`, `hours`, `timeout`.
- `Instant`: 1000; `epoch`, `timestamp`, `unix`.
- `Point`: its shape.
- `Email`, `Url` and `Uuid`: their validation texts, and `email`, `url`,
  `uuid` as names on `Text`.
- The loops of `list.sum`, `list.min_by` and `list.max_by`.

`list.group_by`'s loop builds a map through a chain of calls that the
graph shows only as calls, so it has no loop signal yet. A type's
validation texts are data of their own, not its examples, so only the
types that list them fire.

A snippet gives one finding. Where signals for two types fire on one
snippet, the type whose words more of its names hold wins, then the first
in the entries' order. Where a snippet holds a smaller one that fires the
same way, only the smaller is reported. A type with
`decline type.standard_exists` gives no `shape` finding.

## Snippets and their canonical form
<!-- topic: snippets-and-their-canonical-form -->

A finding is about a snippet: the smallest expression, or member of a
declaration, that holds the signal. Each snippet has two forms.

### Its canonical form

Signals, ignores and taught snippets match on the canonical form: the
graph fragment with its structure normalised and its names kept. So
`total_price * 100`, `100 * total_price`, and `var factor = 100` then
`total_price * factor` are one pattern. To get there:

- Operands of `+`, `*`, `==`, `!=`, `and` and `or` are sorted, and chains
  of one of them flattened, so grouping doesn't matter.
- `>` and `>=` become `<` and `<=` with the sides swapped.
- Numbers lose their spelling (`100`, `100.0`, `1_00`), and constants
  folded together are one.
- A named constant is its value, and a variable that is never reassigned
  is followed to its value.
- Exactly equivalent forms are one, so `x / 0.01` is `x * 100` and
  `x * 0.01` is `x / 100`.

A BLAKE3 hash of the canonical form, 16 hex digits, is its index, and the
canonical form itself confirms a match.

### Its text

The snippet's text is the source as `jadpo fmt` writes it, comments
removed. Its hash, 16 hex digits of BLAKE3 too, is what `jadpo suggest
share` sends. It never depends on the version of the code that makes the
canonical form, and only lines written the same way in many projects reach
HQ's threshold.

## The report
<!-- topic: the-report -->

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
every one fits in 80 columns. Otherwise every snippet goes on the line
under its row, cut with `…` only if it is longer than that line.

With no findings it says `✓ No suggestions`, and the instruction stays,
since reviewing the rest still applies. The last line, to run
`jadpo suggest share`, shows only while sharing is on and an HQ address is
configured ([Sharing with Jadpo HQ](#sharing-with-jadpo-hq)).

`--json` gives each finding's number, place, range, suggestion, signal,
reason, text, canonical form and both hashes, and the instruction's lines.
`jadpo suggest --catalogue` lists every signal, generated from the entries
and the loop table, as text or `--json`.

A finding that is gone at the next scan was taken. The scan keeps a note
of its last findings in `.jadpo/suggest_last`, which is local and not
committed. Each one missing from the next scan, and not ignored meanwhile,
becomes an `accepted` entry to share. The summary counts them.

## Ignoring a suggestion
<!-- topic: ignoring-a-suggestion -->

When a finding is wrong for your code, keep it quiet with a reason:

```text
$ jadpo suggest ignore S1 "Basis points, not money"
✓ Ignored S1, Money conversion
  snippet  total_price * 100
  place    domains/shop/libraries/pricing.jadpo:6
  reason   Basis points, not money
```

This writes the snippet's canonical hash, its signal and the reason to
`ignored_suggestions.jadpo_record` at the project root. The file is
committed, so the whole team shares the ignores ([The
root](01-project-layout-and-modules.md#the-root) lists it, in [the record
format](18-records-and-manifest.md#the-record-format)):

```text
// Generated by Jadpo from the suggestions this project ignores; update it with `jadpo suggest ignore`, never by hand.
record ignored_suggestions 2

ignore 3f9a1c0e2b7d4a51 conversion Money "Basis points, not money"
```

The snippet, and every variant with the same canonical form, stays quiet
until its code changes. Each scan drops an entry no snippet has any more.
Ignoring also adds an `ignored` entry (the text, both hashes, the signal
and the reason) to `.jadpo/suggest_pending`. It sends nothing.

## Teaching a miss
<!-- topic: teaching-a-miss -->

When the scan missed something, record it before you change the code:

```text
$ jadpo suggest found domains/shop/actions/pence_of.jadpo:5 Money "Pence held as Int"
✓ Recorded a missed Money at domains/shop/actions/pence_of.jadpo:5
  snippet  subtotal + delivery_pence
  reason   Pence held as Int

→ jadpo suggest  finds the same pattern elsewhere
```

`jadpo suggest found <file>:<line> <Type or operation> "<why>"` records the
snippet at that line, its text, both hashes, its canonical form, the
suggestion and the reason. It adds a `found` entry to
`.jadpo/suggest_pending`, so the text survives the fix.

It also teaches the canonical form as a signal of the project's own, in
`taught_suggestions.jadpo_record` at the project root. Like the ignores,
it is committed and shared by the team ([The
root](01-project-layout-and-modules.md#the-root) lists it, in [the record
format](18-records-and-manifest.md#the-record-format)). So the next scan,
in this checkout or a teammate's, finds the same pattern anywhere as a
`taught` finding.

```text
// Generated by Jadpo from the patterns this project taught `jadpo suggest`; update it with `jadpo suggest found`, never by hand.
record taught_suggestions 2

taught 8c1d0e5a4f3b2a19 Money "Pence held as Int" "(+ delivery subtotal)"
```

Each line holds the canonical form's hash, the suggestion, the reason and
the canonical form, which confirms a match. The snippet's text stays out
of the record. `.jadpo/suggest_signals` is only a cache of it: a pattern
found only in the cache moves into the record at the next scan.

The snippet at a line is the value of the first statement that starts
there (a `return`'s, a variable's, an assignment's), or the statement
itself when it has none, or else the field or parameter declared there. A
line with none of them is `cli.suggest_arguments`.

## What is kept, and where
<!-- topic: what-is-kept-and-where -->

Every entry to share goes to `.jadpo/suggest_pending` with its kind:
`found`, `accepted` or `ignored`. The kind lets HQ tell a signal that was
right from one that fired wrongly. `jadpo suggest share` sends them
([Sharing with Jadpo HQ](#sharing-with-jadpo-hq)).

Everything in `.jadpo/` is local, and the first command that writes there
adds `.jadpo/` to `.gitignore`. `.jadpo/suggest_last`,
`.jadpo/suggest_pending` and `.jadpo/suggest_signals` hold one JSON object
per line, since they hold source text. Only the committed records are in
Jadpo's readable format. Those are written at format 2, with the shared
header ([The record format](18-records-and-manifest.md#the-record-format)),
as shown above. Files at format 1 are still read, and
`jadpo records update` rewrites them.

| File | Committed | Holds |
|---|---|---|
| `ignored_suggestions.jadpo_record` | yes | The team's ignores: hash, signal, suggestion, reason |
| `taught_suggestions.jadpo_record` | yes | The team's taught patterns: hash, suggestion, reason, canonical form |
| `.jadpo/suggest_last` | no | The last scan's findings, so a vanished one counts as accepted |
| `.jadpo/suggest_pending` | no | The entries waiting for `share` |
| `.jadpo/suggest_wanted` | no | The exact payload `share --approve` sends, until it is approved or declined |
| `.jadpo/suggest_shared` | no | That this checkout has shared once, so the first-share line shows once |
| `.jadpo/suggest_signals` | no | A cache of the taught record |

## Sharing with Jadpo HQ
<!-- topic: sharing-with-jadpo-hq -->

Sharing is how Jadpo learns which signals are right: which findings people
take, which they ignore and why, and what the scan missed. Jadpo HQ is the
service that collects them.

**Sharing is on unless it is turned off, and nothing goes unless someone
runs `share`.** Only hashes go without a person's approval; a snippet's
text never does.

```text
$ jadpo suggest share
✓ Sent 3 hashes to Jadpo HQ
    kinds  1 accepted, 1 found, 1 ignored
    asked  nothing more
    token  new, in app.jadpo: commit it so every copy counts as one project

Sharing is on
  Only hashes went. To stop sharing, add `share: false` to `suggestions { }`.
```

### The project token

`app.jadpo` holds `suggestions { project_token: "…" }`: 32 random hex
digits tied to nothing, no repository, person or machine. It is committed,
so a team counts as one project, and HQ counts distinct projects without
knowing which they are.

Whenever it is missing, `share` writes it before sending, made from the
operating system's random generator. With nothing pending it writes none.
`share --new-token` replaces it and sends nothing. A fork shares its
parent's token until it does. Only 32 lower-case hex digits read as a
token: any other value is `type.suggestions_value_invalid`, so a
repository's name can't be used as one.

### Turning it off

`suggestions { share: false }` turns sharing off for the project.
`JADPO_SUGGEST_SHARE=off` in the environment turns it off for a whole
machine or organisation; any value but `on` or empty turns it off, so a
misspelling never sends. `share: false` counts in whichever file the
`suggestions { }` is in. Neither can turn sharing back on over the other.

Either way, every form of `share`, `--approve` and `--new-token` included,
stops at once. It sends nothing, writes nothing, keeps the pending
entries and says where sharing was turned off. The scan's report drops its
instruction to share. The scan, `ignore` and `found` still record entries.
A `suggestions { }` block with a problem stops `share` too.

### What `share` sends

`share` sends every pending entry's text hash with its kind (`found`,
`accepted` or `ignored`), its signal and its suggestion, and the token. It
does so at once, with no prompt: the agent runs it itself at the end of a
suggest session, as the report's instruction says. The signal and
suggestion go because HQ reassesses a signal by how often it is ignored,
and most entries never send their text. Nothing else goes, not even
Jadpo's version.

HQ replies with the hashes whose text it wants: those seen from enough
distinct projects. The command writes the request's address and body to
`.jadpo/suggest_wanted`, shows exactly what `share --approve` would send,
and stops.

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

Here `(human)` means the choice is yours: whether a snippet of your code
leaves your machine.

### Approving or declining

A person approves, directly or through the agent asking them in its
conversation. Only then does `share --approve` send those bytes to that
address, whatever `JADPO_SUGGEST_HQ` says by then. It sends the snippets,
each with its kind, signal and suggestion and, for an ignored one, its
reason; a `found` entry's reason stays local. HQ recomputes their hashes.

`share --decline` drops them and sends nothing. The next `share` replaces
what waits, unsent.

```text
$ jadpo suggest share --approve
✓ Sent 1 snippet to Jadpo HQ

$ JADPO_SUGGEST_SHARE=off jadpo suggest share
✓ Sent nothing: sharing is turned off
    where  JADPO_SUGGEST_SHARE=off in the environment
```

`--json` gives the counts, the hashes HQ asked for, the payload, the
address it goes to, and an `approval` object an agent relays: the
question, the payload, the two commands, and the rule that `--approve` is
only valid after the person said yes. The command can't check that they
did, so asking is the agent's job, written into the decision's `next`.

### Unattended runs

An unattended run (`--unattended`, or `CI` set to anything) sends only
hashes and skips HQ's request for text. HQ asks the next project that
sends that hash, and the next `share` drops a request left waiting. An
unattended run can't `--approve` (`cli.suggest_arguments`), and `share`'s
flags given without `share` are refused rather than ignored.

### Afterwards

The pending file is wiped, whether HQ wanted anything or not; a send that
fails keeps it. The first time a checkout sends hashes, `share` says so in
one line, with how to turn sharing off. That first time is noted in
`.jadpo/suggest_shared`, so nothing is written outside the project.

### While HQ isn't running

No HQ address is configured by default, so a suggest session never ends
in a failure while HQ doesn't exist. `share` says
`Sharing isn't available yet: Jadpo HQ isn't running. Nothing was sent;
your entries are kept.`, sends nothing, keeps the entries and succeeds.
The scan's report leaves out its instruction to share, as it does when
sharing is off ([The protocol](#the-protocol)).

### The protocol

This is the contract the real HQ implements unchanged: HTTPS and JSON,
UTF-8, under a versioned path.

- **Address.** `JADPO_SUGGEST_HQ` in the environment. There is no default
  yet: the release that launches HQ, with the registry, sets the real
  address as the default. `http://` is accepted only for a loopback host
  (`localhost`, `127.0.0.1`, `[::1]`), which is how the tests reach the
  fake. Anything else must be `https://`, through OpenSSL, with the
  platform's certificate store and the host name checked. Only the
  `jadpo` tool links OpenSSL; no deployable does.
- **Headers.** `Content-Type: application/json`, `Accept:
  application/json`, `User-Agent: jadpo`, without a version, and
  `Connection: close`: the client is a small HTTP/1.1 one of `jadpo`'s
  own. The token in the body is the only identifier the client sends. The
  client logs nothing about a request: no repository name, user, machine
  or address.
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
  after `jadpo fmt`, comments removed). `kind` is `found`, `accepted` or
  `ignored`. `signal` is a signal's word (`taught` for a `found` entry).
  `suggestion` is a standard type or operation. HQ answers `200` with the
  entries whose text it wants, each a hash and kind it was sent, so an
  ignored entry's reason can be asked for without the accepted ones'
  texts. It names at most 20, so an approval stays readable:

  ```json
  { "wanted": [ { "hash": "8c1d0e5a4f3b2a19", "kind": "ignored" } ] }
  ```

  For each hash, kind, signal and suggestion, HQ counts the distinct
  tokens it has seen it from, so sending the same entry twice changes
  nothing and a retry is safe. It wants a hash's text once that count
  reaches its threshold and it doesn't hold the text yet; for an ignored
  entry, until it holds a few reasons. A hash below the threshold is never
  asked for.
- **`POST /v1/suggestions/snippets`**, only after approval, carries no
  token: a snippet is never tied to a project.

  ```json
  { "snippets": [ { "kind": "ignored", "reason": "Basis points, not money", "signal": "conversion", "suggestion": "Money", "text": "total_price * 100" } ] }
  ```

  `text` is the snippet's text without comments, whose hash is the one HQ
  asked for. `reason` is sent only for an ignored snippet. HQ recomputes
  each hash, keeps the snippets it asked for and refuses the rest,
  answering `200` with `{ "received": 1, "refused": 0 }`.
- **Errors.** Any other status has the body
  `{ "error": { "code": "…", "message": "…" } }`: `400 invalid_request`
  (with the field), `413 too_large` (a body over 1 MiB), `410
  version_retired` (this Jadpo is too old to share; update it), `429
  rate_limited` with `Retry-After` in seconds, and `500`, `502`, `503` or
  `504` for HQ's own trouble.
- **Timeouts and retries.** 5 seconds to connect and 15 for each answer.
  A failed connection, a timeout, `429`, and `500`, `502`, `503` and `504`
  are tried again up to three times: after half a second, 1 and 2 seconds
  (at most three and a half seconds of waiting), or after `Retry-After`
  when HQ gives one. A `Retry-After` over 60 seconds ends the attempt.
  Nothing else is retried. When the last try fails, `share` fails with
  `suggest.share_failed`, sends nothing more and keeps the pending entries
  for the next run.
- **What HQ keeps.** For each hash, the set of tokens it came from and its
  kinds, signals and suggestions; and the texts and reasons it asked for,
  without a token. Nothing else: addresses are used only to rate-limit, in
  memory, and never written down.

### HQ's agent

HQ runs an agent over what is shared. It turns shared patterns into signal
data for the next release: new constants, words and validation texts on a
type's entry, a threshold, the tests each new signal is generated with,
and the release notes saying what changed.

It reassesses a signal that is ignored often across distinct projects. It
reads the ignored snippets and their reasons beside the accepted ones,
then narrows the signal or removes it, tested against the accepted
snippets so it keeps catching what it got right. Where a pattern points at
something the language lacks, it proposes a language feature for a person
to decide on, and never builds one.

Not built yet: Jadpo HQ itself, which comes later, with the registry.
Sharing is built against a local fake of HQ that speaks the protocol
above. `jadpo new` writes the project token; for a project without one,
`share` writes it.

## Review and sign-off
<!-- topic: review-and-sign-off -->

Some changes need a person to accept them before they ship: a change that
can break an API client ([Breaking changes](07-http.md#breaking-changes)),
dropping something from the database ([Data](05-data.md)), a weaker policy
([Policy](09-policy.md)), and a new or renamed domain ([Project layout and
modules](01-project-layout-and-modules.md)). They all use one form of
acceptance, a file a person writes saying they accept the change
([Acceptances](18-records-and-manifest.md#acceptances)).

`jadpo check` fails until each such change has its acceptance. Acceptances
are files in one folder that your git host protects, with GitHub's code
owners and branch protection or the equivalent, and CI runs `jadpo check`,
which fails without them. So review happens where it already does, on the
git host: Jadpo has no signing or user accounts of its own.

`jadpo approval --before <ref>` writes the report a reviewer reads: what
changed in routes, policy, failures and data.

Not built yet: `jadpo approval`, which comes with [Records and
manifest](18-records-and-manifest.md).

## Errors only the running app or a build raises
<!-- topic: errors-only-the-running-app-or-a-build-raises -->

The running app reports three events of its own, never compile errors:

- `runtime.unhandled_fault`: a defect or fault reached the boundary, and
  the client gets `internal_fault`;
- `runtime.outcome_unknown`: a change may or may not have been saved;
- `runtime.startup_failed`: the app couldn't start.

[Logging](04-failures.md#logging) says what each logs.

`jadpo build` refuses features the compiler checks but can't generate
yet, with `toolchain.target_*` errors. `jadpo check` reports the same
errors, so a project that passes `check` also builds. [Identity](08-identity.md)
asks the same of a protected route in an app with no sign-in set up.

## A native deployable
<!-- topic: a-native-deployable -->

A deployable is one program your app is built into. An app that declares
no `deployables:` has exactly one, `app`: a native server binary that
serves every API and runs all the background work. An app that declares
them is built as [Declared deployables](#declared-deployables) says.

```text
$ jadpo build
  binary    build/target/app/app
  build     b_e20fc10f5474
  commit    8d113e6
  settings  DATABASE_URL

✓ built  app · native in 49.1 s
→ jadpo migrate apply  it starts only once its database has every migration
```

`jadpo build` and `jadpo build app` build it. Any other name is
`cli.build_arguments`.

### What `jadpo build` writes

It checks as `jadpo check` does, writing the same files in `build/`. At
the first error it stops with the same screen, ending `✗ build stopped`.

Otherwise it writes the deployable's Rust into `build/rust/app/`: the
program's crate, `jadpo-program-app`, and a binary named after the
deployable, with the toolchain this Jadpo pins in `rust-toolchain.toml`
and the runtime crates' `Cargo.lock`. It compiles that in release,
offline and `--locked`, in the release profile its workspace's
`Cargo.toml` gives: fat link-time optimisation in one codegen unit at
`opt-level = 3`, its symbols kept. The paths it was built from are
remapped and `SOURCE_DATE_EPOCH` is fixed: the variable's value when it is
set, else the commit's time.

The binary is `build/target/app/app`, and each API's OpenAPI document and
`<api>.failures.<locale>.json` are copied into `build/target/app/openapi/`.
It is built without the runtime's `testing` feature.

cargo failing is `toolchain.build_failed`, since the Rust Jadpo writes
always compiles. A file it can't write is `toolchain.artifact_write_failed`.

### What it compiles with

Everything a build needs comes with `jadpo` or is fetched by it, so a
released `jadpo` builds anywhere and you don't install Rust. A native
build still needs your machine's C compiler, which links the binary and
compiles `ring`'s cryptography.

- **The runtime crates' source** is carried in the `jadpo` binary, with
  their `Cargo.lock` and the toolchain file.
- **The Rust toolchain** that file pins (`rustc`, `cargo` and the
  standard library for your machine, and wasm32's once a Worker is built)
  is fetched from Rust's download server the first time a build needs it,
  into `jadpo/toolchains/<version>-<host>/` of your cache folder. Each
  archive is checked against the SHA-256 the release's manifest gives, and
  the manifest against the digest this Jadpo pins.
  `JADPO_RUST_DIST_SERVER` names a mirror, whose manifest is checked
  against the `.sha256` beside it.
- **The crates the lock names** come from crates.io, or whatever cargo's
  own configuration puts in its place. cargo fetches them only when its
  cache can't build offline, into `CARGO_HOME` when that is set, else the
  cache folder's `jadpo/cargo/`, each checked against the lock's
  checksum.

Each fetch says what it is fetching (`Fetching Rust 1.99.0 for
aarch64-apple-darwin`, `Fetching the runtime's crates`). Where stderr
animates, it is on the live line, each archive with a bar; otherwise it is
a line on stderr; under `--json` it says nothing. The screen's `fetched`
line, and `--json`'s `fetched`, say what came (`Rust 1.99.0 (146.9 MB)
and 292 crates`). A fetch that fails is `toolchain.fetch_failed`.

With `CARGO` naming a cargo, that cargo builds instead and nothing is
fetched. cargo sets `CARGO` for what it runs, and the gate does for this
repository's tests.

### The build stamp

Every build carries a stamp saying what it is ([Records and
manifest](18-records-and-manifest.md)):

- the build id: `b_` and the first twelve hex digits of a SHA-256 over the
  deployable's program (what it is compiled from: its Rust and its plan,
  its name and its stores' migrations), the compiler's version, the target
  and `jadpo.lock`;
- the commit `git` says the project is at, and whether its tree holds
  changes the commit doesn't (`build/` and `.jadpo/` aside); a project git
  doesn't track has no commit and counts as changed;
- the compiler, the deployable and the target.

The binary holds the stamp as a constant. The runtime plan it holds
records the build id where `jadpo dev` records the graph's digest
([Several changing calls in one
action](11-integrations.md#several-changing-calls-in-one-action)).

### Starting

The binary reads only the variables its settings name, and refuses to
start when one is missing or invalid. It names each setting and its
variable, never the value ([Configuration and
time](10-configuration-and-time.md)).

It builds each connection's driver. One it can't build is listed as it
starts, and answered `misconfigured` when used. A database whose
connection names no `url:` stops the start, since only `jadpo dev` and
`jadpo test` give one an address of their own.

Each database the project keeps migrations in is compared with the
migrations the build holds. A migration not applied, or one applied that
the build doesn't have or that changed since, stops the start
([Migrations](05-data.md#migrations)). A database it can't reach yet is
waited for, trying again after 1 second and doubling to 10, since a
database that is down isn't a configuration error.

Then it listens on `PORT` (8080 when unset) on `HOST` (every IPv4
interface when unset), with a worker per core.

### Running and stopping

Each worker serves requests and runs the tick, the runtime's regular
pass over background work. The first SIGTERM or ctrl-c stops it: it
accepts nothing more, answers the requests it is reading, lets its
background work give back what it holds, and ends with code 0. A second
ends it at once. A start that fails ends with code 1.

### Running one tick

The binary takes one command. None, or `serve`, serves as above.
`<deployable> tick` runs one tick and ends ([The
tick](12-jobs-and-events.md#the-tick)). Another scheduler runs it when
`schedules: { trigger: ScheduleTrigger.external }` says the server's own
timer never ticks.

`tick` reads its settings, builds its drivers and compares its stores'
migrations as serving does, but waits for no database: one it can't reach
ends it with code 1, `store_unreachable`. It ends with code 0, or with 1
when the tick couldn't work in a store or didn't finish, having panicked
or outlasted the drain. The first SIGTERM or ctrl-c drains it, giving back
what it holds. Any other command ends with code 2, `command_unknown`,
without repeating what was given.

`jadpo build` gives such a deployable a `tick` line, with the command and
how often to run it (`build/target/app/app tick · each minute`), and
`--json` a `tick` (`command`, `every_ms`).

### What it logs

It logs one JSON object a line on standard output, its keys sorted, each
with the stamp's fields (`build`, `commit`, `modified`, `compiler`,
`deployable`, `target`):

- `runtime.startup_failed`, with every problem that stopped the start
  (`problems`, each with a `problem` code, the setting, variable or store
  it is about, and a `message`);
- `runtime.waiting`, while a database can't be reached;
- `runtime.started`, with the address, workers, `trigger` (`native` or
  `external`) and each connection whose driver wasn't built (`unbuilt`);
- `runtime.ticked`, after each tick, the server's own timer's as well as a
  `tick` command's: `ran`, each schedule it ran or skipped by name with
  its outcome, `deliveries`, `received`, `polled`, `deleted`, `alerts`,
  `problems`, each store it couldn't work in, `unbuilt` and `ms`;
- `operation.failed`, for a fault behind a 500, with its classification,
  route, `request_id` and `.jadpo` frames ([Failures](04-failures.md),
  [Debugging and review](17-debugging-and-review.md)). When the fault is a
  bug in Jadpo, `bug_in_jadpo` is `true` and `report` holds the issue
  form's address; otherwise they are `false` and `null`;
- `runtime.stopped`.

The rest of a deployable's log comes with telemetry ([Built-in
services](16-built-in-services.md)).

## Declared deployables
<!-- topic: declared-deployables -->

An app can be built into several programs, for example a Worker that
serves its public API and a server that runs its back office.
`application { deployables: { } }` in `app.jadpo` names each one, what it
serves, what it runs and what it is built for:

```jadpo check project=deployables file=app.jadpo
application {
    apis: {
        main: {
            prefix: /
        }
        backstage: {
            prefix: /backstage
        }
    }
    connections: {
        cache: {
            driver: Driver.redis
        }
    }
    rate_limits: {
        store: cache
    }
    deployables: {
        decline rate_limits.counted_in_memory: "One instance of `web` is enough until Redis's driver comes"
        web: {
            serves: [main]
            runs: [Trigger.subscribers]
            target: Target.wasm
        }
        back_office: {
            serves: [backstage]
            runs: [Trigger.schedules]
            target: Target.native
        }
    }
}
```

```text
$ jadpo build
  worker    build/target/web/wrangler.json
  module    build/target/web/web.wasm
  size      8.1 MB, 2.9 MB gzipped
  build     b_1c5e09d2a7f3
  serves    main
  runs      subscribers
  calls     pdf 2 at once in each instance

  binary    build/target/back_office/back_office
  build     b_e20fc10f5474
  serves    backstage
  runs      schedules

  commit    8d113e6
  settings  DATABASE_URL

✓ built  web and back_office · wasm and native in 61.3 s
→ jadpo migrate apply  they start only once their database has every migration
```

### `serves:`, the APIs it answers

`serves:` lists APIs by name: those `apis:` declares, or `main`, and
`webhooks` once a connection receives webhooks. A deployable answers only
those APIs' routes, each API's `openapi.json` among them, and a webhook
only when it serves `webhooks`. Every deployable answers its health
checks. Two deployables may serve one API.

### `runs:`, the background work it runs

`runs:` lists the background work it runs:

- `Trigger.schedules`: the app's schedules, each entity's purge, each
  poll, and a batch run picked up again;
- `Trigger.subscribers`: each delivery of an event or an email, after its
  action commits and from the outbox, and the queues `receiving:` reads.

An action it serves still writes its events to the outbox, for a
deployable that runs them. A delivery in a deployable that doesn't run
subscribers, or a batch run in one that doesn't run schedules, waits for
one that does. Every deployable's tick deletes what has expired, which
any instance may.

### `target:`, what it is built for

`target:` is `Target.native`, a server, or `Target.wasm`, WebAssembly on
Cloudflare Workers. A deployable with no `target:` has none, like the
`app` of an app that declares no deployables, and is built natively
([design notes](notes/14-tooling.md), "A deployable's target when none is
declared").

### What each reaches

A deployable reaches everything its work can lead to. From the routes of
the APIs it serves, the webhooks' replies, the actions its schedules and
subscribers run and the batch runs it carries on, through every callable
they call, to the connections they use: a store of files, a provider's
operation, a package's action, `ai:`'s model. With them come its polls,
the webhooks it receives and the queues `receiving:` reads. Where it runs
subscribers, it also reaches `email:`'s connection, through which the
app's emails and the sign-in email are delivered, unless
`magic_link: { send: }` names an action that sends the sign-in email.

It also reaches what Jadpo runs by name: the action `on_first_sign_in:`
names, where it serves an API; the action `magic_link: { send: }` names,
and every email's templates, where it runs subscribers; and every provider
package's `decode:` and `encode:`.

### What each holds

A deployable's program holds only what it reaches: those callables, the
routes of the APIs it serves, its schedules and the subscribers' actions
only where it runs them, the connections it reaches with every database,
and the settings those read. So it reads no other setting as it starts. A
subscriber it doesn't run is still listed, so what it emits waits for the
deployable that does.

An app's wording and its stores are kept whole, and so is who signs in.
But a sign-in method's `signing_key:` is kept only where the deployable
serves an API that takes that method, and `origin:` only where it serves
one that takes browser sessions. So a Worker that only runs the
subscribers starts without either.

### Its target, checked

A deployable that declares a target is refused each connection it reaches
whose driver that target can't run, and the error names the alternative
(`deployable.target_unsupported`; [External
services](11-integrations.md)):

| Target | Refused | Use instead |
|---|---|---|
| Workers | The disk | A package over S3's API, such as Cloudflare R2 |
| Workers | A service Jadpo runs without `host:` | `host:`, with where it is hosted |
| Workers | SMTP on port 25 | Port 587 |
| Native | A Cloudflare queue, which only a Worker receives | A package over Cloudflare's HTTP pull API |
| Native | Cloudflare's rate-limiting binding as the store of its rate limits | Redis |

A deployable built for Workers that serves an API needs a store for its
rate limits, `rate_limits: { store: }` ([Rate
limits](16-built-in-services.md#rate-limits); `rate_limits.store_required`).
Until Redis's driver comes, a Redis store there would leave each instance
counting in its own memory. That is a judgement call, which a `decline`
line answers, as `web` above does (`rate_limits.counted_in_memory`).

A deployable built for Workers is refused a `client_address:` source other
than `cloudflare`, at its `source:` ([Client
addresses](10-configuration-and-time.md#client-addresses)).

A deployable built for Workers that reaches a connection with
`max_concurrent:` is listed under "On Workers" once `jadpo check` passes,
and `jadpo build` says so on the Worker's `calls` line. Workers runs as
many instances as traffic needs, and each counts the cap on its own
([External services design notes](notes/11-integrations.md)).

### Nothing left unserved

Once `deployables:` is declared, every API with routes needs a deployable
that serves it, and the app's schedules and subscribers need one that runs
them (`deployable.unserved`). A route with `settle:` is served only where
subscribers run (`deployable.settle_unrun`).

### The build

`jadpo build` builds every deployable, and `jadpo build <deployable>` one.
Each goes into `build/target/<deployable>/` with the documents of the APIs
it serves, and its Rust into `build/rust/<deployable>/`. They are all
compiled into a cargo target in your cache folder, shared by every project
built for the same host and set of locales, so the runtime crates compile
once for them all ([design notes](notes/14-tooling.md), "Where
deployables are compiled").

A name the app doesn't declare is `cli.build_arguments`, with the nearest.
A deployable built for `Target.wasm` is built for Workers ([A Workers
deployable](#a-workers-deployable)).

Each binary's stamp names it, and its build id is its own. The id digests
the deployable's program, so a change it doesn't reach leaves it alone
([Records and manifest](18-records-and-manifest.md)). `jadpo check` writes
each deployable's target and build id in the manifest, with no build id
while the project has errors. `settings` lists the variables the
deployables read, each once.

## A Workers deployable
<!-- topic: a-workers-deployable -->

A deployable with `target: Target.wasm` runs on Cloudflare Workers.
`jadpo build` writes it as a WebAssembly module, Jadpo's JavaScript shim
and a Wrangler file, ready for `wrangler deploy`:

```text
$ jadpo build kiosk
  worker    build/target/kiosk/wrangler.json
  module    build/target/kiosk/kiosk.wasm
  size      8.1 MB, 2.9 MB gzipped
  database  DATABASE_URL through Hyperdrive c020574a5623407bbe0ccd192bab9545
  build     b_1c5e09d2a7f3
  serves    main
  runs      schedules, subscribers
  commit    8d113e6
  settings  DATABASE_URL

✓ built  kiosk · wasm in 42.3 s
→ jadpo migrate apply  it starts only once its database has every migration
```

### What `jadpo build` writes

Its Rust goes in `build/rust/<deployable>/`: the program's crate and a
`cdylib`, `jadpo-worker-<deployable>`. The `cdylib` exports the Worker's
`fetch`, `scheduled` and `queue`, and `hand_on` for work handed on,
through the Workers host. It is given the program, its build stamp and
each store's migrations, as a native binary is.

It links the package driver, which every Worker has; PostgreSQL's when the
project keeps a database, since every deployable's tick reaches each
store; SMTP's when the deployable reaches an SMTP connection; and no
other.

It is built for `wasm32-unknown-unknown` in release, offline and
`--locked`, without the runtime's `testing` feature, into the cargo target
the native deployables share. Its release profile, from its workspace, is
fat link-time optimisation in one codegen unit at `opt-level = "s"`, for
size, since a Cloudflare plan limits what a Worker uploads. It is then
bound by wasm-bindgen's own library, at the release the lock holds, so
nothing is downloaded and no command-line tool has to match. The module's
name section and its `producers` section are left out.

Into `build/target/<deployable>/`, over what an earlier build left there,
go:

- `<deployable>.wasm`, the module;
- `<deployable>.js`, wasm-bindgen's JavaScript for it;
- `worker.mjs`, Jadpo's module shim, which instantiates the module once
  per isolate and hands it each request, the Worker's `env` and the
  invocation's `ctx`. It exports `JadpoHandOn`, a named entrypoint whose
  `handOn` takes on work another invocation handed on;
- `wrangler.json`;
- the documents of the APIs it serves.

### The Wrangler file

Here is the Wrangler file for the kiosk above:

```json
{
  "name": "repair-cafe-kiosk",
  "main": "worker.mjs",
  "compatibility_date": "2026-10-01",
  "no_bundle": true,
  "find_additional_modules": true,
  "rules": [
    { "type": "ESModule", "globs": ["**/*.mjs", "**/*.js"] },
    { "type": "CompiledWasm", "globs": ["**/*.wasm"], "fallthrough": false }
  ],
  "observability": { "enabled": true },
  "hyperdrive": [
    { "binding": "DATABASE_URL", "id": "c020574a5623407bbe0ccd192bab9545" }
  ],
  "triggers": { "crons": ["* * * * *"] }
}
```

It is built as one value and written out once, so no binding is left out
for another. Its fields:

- **`name`**: the app's name and the deployable's, `<app>-<deployable>`,
  so two apps' `kiosk`s are two Workers in one account. The app's name is
  its folder's until `app.jadpo` gives one ([OpenAPI](07-http.md#openapi)).
  Each is lower-cased, and each run of anything but an ASCII letter or
  digit becomes one `-`, since Cloudflare takes letters, digits and
  dashes. It is at most 63 characters, with the deployable's name kept
  whole (`23-repair-cafe-kiosk`).
- **`main`**: the shim.
- **`compatibility_date`**: the date the Workers host is tested at, under
  the workerd Jadpo pins (2026-10-01).
- **`no_bundle` and `find_additional_modules`**, with a rule for each
  kind of file, so the files are uploaded as they are written: `.mjs` and
  `.js` as ES modules, and `.wasm` as compiled WebAssembly. The latter has
  `fallthrough: false`, since Wrangler warns of a rule that hides its own
  default without saying so.
- **`observability: { enabled: true }`**: Workers Logs keeps the lines the
  Worker logs only when it is enabled, and the Wrangler Jadpo pins leaves
  it off when the file doesn't say so.
- **`secrets: { required: [ ] }`**: the variable of each setting without a
  `default:`. A binding on Workers is a variable or a secret, and a
  required secret is Wrangler's one way to name a binding that must be set
  without giving its value. So `wrangler deploy` refuses until each is
  set. A setting with a default is left to it.
- **`hyperdrive: [ ]`**: each database's address setting as a Hyperdrive
  binding by its variable. Hyperdrive is Cloudflare's connection pool in
  front of your database ([PostgreSQL through
  Hyperdrive](05-data.md#postgresql-through-hyperdrive)). The binding has
  its configuration's `id` when the build's environment gives one in
  `JADPO_HYPERDRIVE_<variable>` (`JADPO_HYPERDRIVE_DATABASE_URL`), and
  none otherwise, which `wrangler deploy` refuses.
- **`triggers: { crons: ["* * * * *"] }`**: the minute's cron trigger,
  which runs the tick when the tick has a store to keep or sweep.
- **`ratelimits`**, with rate limits counted in Cloudflare's binding: a
  binding for each limit, with its name, its `namespace_id` and
  `simple: { limit, period }` ([Rate
  limits](16-built-in-services.md#rate-limits)).
- **`queues: { consumers: [ ] }`**: each queue on Cloudflare Queues the
  Worker receives, with its `queue`, `max_batch_size` 10,
  `max_batch_timeout` 5, `max_retries` 30 and its `dead_letter_queue`. The
  dead-letter queue is a consumer too, with `max_retries` 100 and no
  dead-letter queue of its own; the Worker parks its messages for an
  operator ([External services](11-integrations.md)).

No binding names the Worker itself. What one invocation's 30 seconds
didn't reach is handed on to a fresh one through the loopback binding
`ctx.exports` gives its `JadpoHandOn` entrypoint ([Running background
work](12-jobs-and-events.md#running-background-work)). That reaches the
Worker that handed it on, whatever name `wrangler deploy --name` or
`--env` gave it.

Wrangler's own `deploy --dry-run`, at the version Jadpo pins, accepts the
file with no account and no warning, in tests run with every network
connection refused.

### The build stamp

Its target is `wasm`. The module holds the stamp as a constant, as a
binary does, and the manifest gives the deployable's target as `wasm`.

### The screen

For each Worker, `jadpo build` shows:

- `worker`, its Wrangler file;
- `module`, the module;
- `size`, the module's bytes raw and gzipped at zlib's default level, as
  Wrangler reports an upload, in decimal units, as Cloudflare states its
  limits;
- `database`, each database's address binding through Hyperdrive with its
  id, or `no id`, with an alert saying how to give one and that its
  Hyperdrive must keep no cache;
- its build id, and what it serves and runs;
- `calls`, each connection's `max_concurrent:` it reaches, counted in each
  instance (`pdf 2 and ocr 4 at once in each instance`).

Built with no id in `JADPO_HYPERDRIVE_DATABASE_URL`, its database line
says so, and an alert says what to do:

```text
  database  DATABASE_URL through Hyperdrive, no id
  …
▲ alert  `DATABASE_URL` has no Hyperdrive id: create one with caching off and
  build with it in `JADPO_HYPERDRIVE_DATABASE_URL`
```

The `--json` data gives `wrangler`; the module's `path`, `bytes` and
`gzip_bytes`; `hyperdrive`, each binding and its `id` or null; `binary`
null; and every deployable's `per_instance`, each `connection` and its
`max_concurrent`, empty except on a Worker that reaches a cap.

### Starting

A Worker has no start of its own, so an isolate starts at its first
request, as a native binary starts when its process does. It makes its
program once, and reads only the variables its settings name, from the
Worker's bindings. It refuses a `client_address:` source other than
`cloudflare` ([Client
addresses](10-configuration-and-time.md#client-addresses)).

A problem there is logged once, as `runtime.startup_failed` naming each
setting by its variable and never its value. The isolate then answers
every request `503` with `unavailable` and a request id, until a new
version with other bindings replaces it.

### Its database

Each request builds its connections' drivers, over `connect()` and
`fetch()`, since a socket belongs to the request that opened it. A
database's address is its Hyperdrive binding's, reached without TLS inside
Cloudflare, with Hyperdrive using TLS to the database ([PostgreSQL through
Hyperdrive](05-data.md#postgresql-through-hyperdrive)). A text binding of
that name is reached directly, over TLS through `startTls()`.

Until each database the project keeps migrations in has been compared
with the build's migrations and found current, each request compares them
first. A migration not applied, or one applied that the build doesn't have
or that changed since, is logged as `runtime.startup_failed` and the
request answered `503`. A database it can't reach yet is logged as
`runtime.waiting` and answered the same. So a migration applied after the
Worker is deployed is picked up by its next request, with no restart. Once
current, an isolate doesn't compare again, and logs `runtime.started` with
each connection whose driver wasn't built.

### Serving

Each request goes through the runtime as a native one does: the route
table, input, policy, transactions, `settle:`, rate limits and the
failure envelope. The client's address is `CF-Connecting-IP`, which
Cloudflare sets on every request ([Configuration and
time](10-configuration-and-time.md)).

The work a response leaves runs after it in `waitUntil`, within its 30
seconds. What that doesn't reach is handed on to a fresh invocation of the
Worker through the loopback `ctx.exports` gives its `JadpoHandOn`, which
carries it on within what is left of its own 30 seconds, counted from its
start.

Rate limits are counted in the store `rate_limits:` names, Cloudflare's
rate-limiting binding. They are counted in the isolate's memory only while
the binding can't be asked: missing, failing, or not answering within 500
ms. Each binding's fallback is logged once an isolate, as
`rate_limits.fallback` ([Rate limits](16-built-in-services.md#rate-limits)).

A streamed answer (`ai.stream`) is a `ReadableStream`, each event sent as
it is read. A client that goes away ends the request, and with it the
model's call.

An invocation has at most six connections waiting for their answer's head
at once, as Cloudflare allows. The runtime counts them across its
database, its providers, SMTP and its own fetches, the work it leaves in
`waitUntil` included. A seventh waits for one within its own timeout, and
one whose timeout passes first never went out, so it is sent again where
a call may be. `max_concurrent:` caps a connection's calls per isolate,
and a slot one invocation gives back goes to the call of another that has
waited longest.

### The tick

Each run of the minute's cron trigger calls the Worker's `scheduled`. That
starts the isolate as a request does and, once its databases are current,
runs the runtime's tick ([The tick](12-jobs-and-events.md#the-tick)): the
schedules due, the outbox's sweep and the deletions, and the queues
`receiving:` reads through a package. Out of date, it logs why as a
request does and ticks nothing.

While the app receives a queue, it ticks again at each 10-second mark of
the cron's minute, one tick after another, so a queue is read as often as
natively. The invocation ends with the minute, and its ticks share the
cron's 15 minutes.

Each tick is logged as `runtime.ticked`, as a native server logs its own,
with each store it couldn't work in. An `every:` under a minute is refused
for a Worker that runs the schedules
([`deployable.target_unsupported`](../errors/deployable.target_unsupported.md)).

### A queue

Each batch Cloudflare Queues hands the Worker's `queue` starts the isolate
as a request does. Once its databases are current, the batch goes through
the runtime's consumer, each message on its own:

- a message is kept once, as its event;
- a repeat, and a message parked for an operator, are acknowledged
  (`ack()`);
- a message the store can't take now is retried after 10 seconds, doubled
  for each attempt up to an hour (`retry({ delaySeconds })`).

Then the kept events' deliveries run in `waitUntil`, handed on past its 30
seconds as a response's work is.

Out of date, or for a batch from a queue it doesn't receive, every message
is retried the same way, and `queue.retried` is logged with the queue, how
many and why. With 30 retries, a message waits out about 22 hours of a
deploy's migration before Cloudflare moves it to the dead-letter queue.

A batch from that dead-letter queue, its messages' retries spent, has
each message parked whole for an operator as `dead_lettered`, with its id
and event when they can be read, and acknowledged. So `jadpo deliveries`
lists it, and a retry has the tick read it again.

A message whose `retry()` throws would be acknowledged when the handler
ends well. So the handler throws once every message is answered, and
Cloudflare hands again each one not acknowledged.

### What it logs

It logs one JSON object a line on the console, which Workers Logs and
`wrangler tail` read, with the same events and stamp fields as a native
binary's: `runtime.startup_failed`, `runtime.waiting`, `runtime.started`,
`runtime.ticked` after each tick of its cron, `operation.failed` for a
fault behind a 500 with its `.jadpo` frames, and `queue.retried` for a
batch with a message retried. A Worker and a native binary start, compare
their stores and log through the same code, so they say the same things
alike.

## Command errors
<!-- topic: command-errors -->

These are the errors only a command raises, so `jadpo check` can't show
them with a source example. Some rows belong to commands that the design
notes replace (`schema`, `artifacts`, `watch`); each row is rewritten as
its command is built.

| Rule | Raised by | What it means |
|---|---|---|
| `ai.usage_unavailable` | `ai usage` | AI usage's database couldn't be read |
| `cli.ai_arguments` | `ai` | AI command has invalid arguments |
| `cli.api_arguments` | `api` | Api command has invalid arguments |
| `cli.build_arguments` | `build` | Build command has invalid arguments |
| `cli.check_arguments` | `check` | Check command has invalid arguments |
| `cli.config_arguments` | `config` | Config command has invalid arguments |
| `cli.config_prompt_failed` | `config` | Secure configuration prompt failed |
| `cli.config_tty_required` | `config` | Configuration entry requires an interactive terminal |
| `cli.deliveries_arguments` | `deliveries` | Deliveries command has invalid arguments |
| `cli.dev_arguments` | `dev` | Dev command has invalid arguments |
| `cli.dev_port_invalid` | `dev` | Development port is invalid |
| `cli.dev_start_failed` | `dev` | Development server could not start |
| `cli.email_arguments` | `email` | Email command has invalid arguments |
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
| `cli.new_arguments` | `new` | New command has invalid arguments |
| `cli.package_arguments` | `package` | Package command has invalid arguments |
| `cli.package_not_openapi` | `package import` | Description to import is not an OpenAPI 3 document |
| `cli.package_unreadable` | `package import` | Description to import could not be read as JSON |
| `cli.presentation_arguments` | any command | Terminal presentation option is invalid |
| `cli.project_required` | any command | Command requires a project path |
| `cli.routes_arguments` | `routes` | Routes command has invalid arguments |
| `cli.runs_arguments` | `runs` | Runs command has invalid arguments |
| `cli.schedules_arguments` | `schedules` | Schedules command has invalid arguments |
| `cli.schema_command_required` | `schema` | Schema command requires a subcommand |
| `cli.schema_decision_arguments` | `schema` | Schema decision command has invalid arguments |
| `cli.schema_diff_arguments` | `schema` | Schema diff command has invalid arguments |
| `cli.schema_index_accept_arguments` | `schema` | Index acceptance requires a recommendation path |
| `cli.schema_plan_arguments` | `schema` | Schema plan command has invalid arguments |
| `cli.schema_rename_arguments` | `schema` | Schema rename command has invalid arguments |
| `cli.schema_snapshot_arguments` | `schema` | Schema snapshot command requires an output path |
| `cli.schema_sql_arguments` | `schema` | Schema SQL command has invalid arguments |
| `cli.seed_arguments` | `seed` | Seed command has invalid arguments |
| `cli.signal_handler_failed` | any command | Shutdown handler could not be installed |
| `cli.suggest_arguments` | `suggest` | Suggest command has invalid arguments |
| `cli.test_arguments` | `test` | Test command has invalid arguments |
| `cli.toolchain_arguments` | `toolchain` | Toolchain command has invalid arguments |
| `cli.unknown_command` | any command | Unknown Jadpo command |
| `cli.unknown_schema_command` | `schema` | Unknown schema subcommand |
| `cli.watch_arguments` | `watch` | Watch command has invalid arguments |
| `cli.watch_input_read_failed` | `watch` | Watched project inputs could not be read |
| `cli.watch_output_failed` | `watch` | Watch lifecycle event could not be written |
| `cli.why_arguments` | `why` | Why command has invalid arguments |
| `conform.failed` | `package conform` | Package's answers don't conform to its capability |
| `conform.refused` | `package conform` | Conformance suite can't run against the provider's API |
| `recording.failed` | `package record`, `ai record` | Sandbox or model gave no answer to record |
| `recording.refused` | `package record`, `ai record` | Answer can't be recorded through this connection |
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
| `deliveries.not_found` | `deliveries` | No delivery or schedule run has the id given |
| `deliveries.unavailable` | `deliveries` | Deliveries' database couldn't be read |
| `deliveries.wrong_state` | `deliveries` | Delivery or schedule run isn't in a state the action applies to |
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
| `install.fetch_failed` | `package add`, `update` | Package's git repository or tag couldn't be read ([Installing errors](11-integrations.md#installing-errors)) |
| `install.in_use` | `package remove` | Package to remove is still used by the app ([Installing errors](11-integrations.md#installing-errors)) |
| `install.link_refused` | `check`, `test` in CI | Linked package in a CI or production build ([Installing errors](11-integrations.md#installing-errors)) |
| `install.surface_mismatch` | `package add`, `update` | Package's committed surface doesn't match its code ([Installing errors](11-integrations.md#installing-errors)) |
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
| `publish.breaking_change` | `package check` | Package's surface breaks its last release within one major version ([Publishing errors](11-integrations.md#publishing-errors)) |
| `publish.changelog_missing` | `package check` | Package's changelog doesn't say why its new version is released ([Publishing errors](11-integrations.md#publishing-errors)) |
| `publish.description_missing` | `package check` | Package's public declaration has no description ([Publishing errors](11-integrations.md#publishing-errors)) |
| `publish.example_failed` | `package check` | Example in a package's description doesn't compile or doesn't hold ([Publishing errors](11-integrations.md#publishing-errors)) |
| `publish.example_missing` | `package check` | Package's function has no example, or a failure it raises is shown by none ([Publishing errors](11-integrations.md#publishing-errors)) |
| `publish.sample_missing` | `package check` | Package's recipes have no sample app with one domain to compile in ([Publishing errors](11-integrations.md#publishing-errors)) |
| `runtime.outcome_unknown` | the running app | Generated runtime could not confirm whether an effect committed |
| `runtime.startup_failed` | the running app | Generated runtime failed during startup; a deployable's log event, with every problem that stopped it ([A native deployable](#a-native-deployable)) |
| `runtime.unhandled_fault` | the running app | Generated runtime contained an unexpected fault |
| `runs.unavailable` | `runs unfinished` | Unfinished runs' database couldn't be read |
| `schedules.lease_held` | `schedules run`, `deliveries retry` | Schedule is already running |
| `schedules.no_tick` | `schedules run`, `deliveries retry` | No tick picked up the run asked for |
| `schedules.run_failed` | `schedules run`, `deliveries retry` | Schedule's run failed |
| `schedules.unavailable` | `schedules`, `why` | Schedules' database couldn't be read |
| `seed.failed` | `seed` | Seed ended with a failure or a fault |
| `seed.production_refused` | `seed` | Seed refused a database that isn't on this machine |
| `seed.unavailable` | `seed` | Seed's database couldn't be reached |
| `suggest.record_invalid` | `suggest` | Suggestions record can't be read |
| `suggest.share_failed` | `suggest share` | Jadpo HQ couldn't be reached or refused what was shared |
| `test.failed` | `test` | Test run failed |
| `test.no_tests` | `test` | No tests to run |
| `test.runtime_start_failed` | `test` | Test runtime could not start |
| `toolchain.artifact_cleanup_failed` | `artifacts`, `build` | Previous build backup could not be removed |
| `toolchain.artifact_promote_failed` | `artifacts`, `build` | Staged build artifacts could not be promoted |
| `toolchain.artifact_stage_failed` | `artifacts`, `build` | Build staging directory could not be created |
| `toolchain.artifact_write_failed` | `check`, `build` | Generated file could not be written |
| `toolchain.build_failed` | `build` | Rust compiler couldn't build the deployable |
| `toolchain.fetch_failed` | `build` | Rust toolchain or crates couldn't be fetched |
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

Three errors can't be reached at all: `type.job_delivery_selector_invalid`
(Delivery selector does not match its entity types),
`type.job_delivery_service_invalid` (Delivery service does not match its
closed payload types) and `type.job_delivery_sealed_use` (Compiler-owned
delivery value cannot be authored or exposed). They belong to the
delivery descriptor that the [Jobs and events design
notes](notes/12-jobs-and-events.md) leave open, and go with it if it is
accepted.

## What is built
<!-- topic: tooling-built -->

Each row says what is built and what is specified but not built yet.

| Feature | Status |
|---|---|
| Command help | Built: every command has `--help` with examples, `jadpo help [command]` explains one, `jadpo help <rule>` prints a rule's page offline, and `jadpo help --json` describes them all. |
| Project path | Built: every command finds the project by walking up to `app.jadpo`, so a path is optional; run from outside the project, printed paths start with the project as typed. |
| Scaffold | Built: `jadpo new <name>` writes the fixed root on this `jadpo`'s release line with its project token, the `hello` domain (one route, its action, its policy line, its API record and a route test), `AGENTS.md`, `.gitignore` and `.gitattributes`, passing `check`, `fmt --check` and `test` as written, and refuses a folder that holds anything, with `--json`. The line an app declares is checked: `application { jadpo: "0.1" }`, with `toolchain.version_newer`, `toolchain.version_older` and `toolchain.version_invalid` ([The root](01-project-layout-and-modules.md#the-root)). |
| Which jadpo runs | Built in 0.1.0: every `jadpo` runs the release a project's line or `jadpo.lock` pin names, kept in the cache or fetched from GitHub's releases, its archive checked against its published SHA-256 and its GitHub artifact attestation (Sigstore's verifier, with the public-good trusted root carried in the binary) before it runs, and refuses with `toolchain.version_newer` or `toolchain.version_older` when it can't; `jadpo toolchain` lists, removes, pins and unpins, and `JADPO_TOOLCHAIN=local` runs everything with the `jadpo` started. The trusted root is the one this release was built with: no TUF refresh. |
| Formatter | Built: one member per line, one blank line between declarations, and `fmt --check` counts the files it read; it keeps `use` lines at the top, sorted, fixes imports inside a project, `--layout` makes promotion moves, and it writes each file's declarations before their use, the primary declaration last. |
| Error messages | Built: one error per mistake, in the field order [Error messages](#error-messages) gives, `impact` only when it says something, edits on the next step where one exists, and help links to each error's page on the site. |
| JSON output | Built: every command takes `--json` and prints one envelope (`schemaVersion` 2) with project-relative paths and `data.root`, line and column beside byte offsets, and camelCase keys; `jadpo test --json` is described in [Testing](13-testing.md) and under `testReport` in `jadpo help --json`. |
| `build/` | Built: `jadpo check` writes `build/manifest.json` and the catalogues in `build/catalogue/`; `jadpo test` keeps its last run in `build/test/`. Not built yet: the reports, OpenAPI and the monitoring rules in `build/`. |
| Commands | Built: each command [The `jadpo` command](#the-jadpo-command) and the topics after it describe, apart from those in their "Not built yet" notes. |
| Language server | Built: diagnostics; hover, with a query's SQL and a route's full path; go to definition, from a `use` line too; find references; completion; formatting as `jadpo fmt`; quick fixes from each error's edit, promotion among them; and the code actions under [Editor support](#editor-support). Not built yet: rename, which waits for `jadpo rename`. |
| VS Code extension | Built: starts `jadpo lsp`, formats on save, highlights, a status item naming the `jadpo` running, "Open Generated File", and packaging and publishing on each release, switched off until the Marketplace and Open VSX are set up. |
| Agent tools | Partly built: the catalogues and the similar-helper check, a declinable error, are built; `migrate` is built; `impact`, `domain` and `rename` are not built yet, nor are the agent skill and generated CI. |
| Suggestions | Built: `jadpo suggest` with its six signal kinds and taught signals, the catalogue, accepted findings, `ignore`, `found`, `share`, `suggestions { }` and the committed taught record, `share` against a fake of Jadpo HQ (HQ itself comes later). |
| Review and sign-off | Not built yet: `jadpo approval`, which comes with [Records and manifest](18-records-and-manifest.md). |
| Build-only errors | Built: `jadpo build` for the one deployable an app that declares none has and for declared deployables, with their `toolchain.target_*` errors; its `settings` line, and a Worker's `secrets: { required: [ ] }`, name every setting a deployable reads, its connections' included. |

## Errors

Every error this area can produce from source, with an example that
triggers it.

- [`syntax.suggestions_unknown_option`](../errors/syntax.suggestions_unknown_option.md)
- [`syntax.suggestions_duplicate_option`](../errors/syntax.suggestions_duplicate_option.md)
- [`type.suggestions_value_invalid`](../errors/type.suggestions_value_invalid.md)
- [`semantic.multiple_suggestions`](../errors/semantic.multiple_suggestions.md)
- [`deployable.invalid`](../errors/deployable.invalid.md)
- [`deployable.api_unknown`](../errors/deployable.api_unknown.md)
- [`deployable.trigger_unknown`](../errors/deployable.trigger_unknown.md)
- [`deployable.target_unknown`](../errors/deployable.target_unknown.md)
- [`deployable.empty`](../errors/deployable.empty.md)
- [`deployable.unserved`](../errors/deployable.unserved.md)
- [`deployable.settle_unrun`](../errors/deployable.settle_unrun.md)
- [`deployable.target_unsupported`](../errors/deployable.target_unsupported.md)

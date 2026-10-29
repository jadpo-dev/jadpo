---
title: Tooling
area: 14
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Tooling
<!-- topic: tooling -->

The `jadpo` command, the formatter, the language server, how errors are shown,
the generated `build/` folder, and the tools agents use to look things up
before they change code.

> Commands and outputs below apply the decisions from areas 1 to 14.

The rule behind all of it: **the compiler is the only source of truth.** The
editor, the agent skill and every generated file come from the same check, so
they cannot disagree with `jadpo check`.

## The `jadpo` command
<!-- topic: the-jadpo-command -->

One command with a short list of subcommands. Every subcommand finds the
project by walking up from the current folder to `app.jadpo`, so a path is
optional, and every subcommand takes `--json` for output an agent reads
instead of prose, except the interactive modes, which need a terminal.
Commands have plain synonyms (`fmt` and `format`), complete on Tab, and ask
for a missing argument with a searchable list ([design notes](notes/14-tooling.md), "Command synonyms"
and "Completion and prompts").

| Command | Does | Status |
|---|---|---|
| `jadpo new <name>` | Creates a project that passes `check`, `fmt --check` and `test` | Not built yet: `jadpo new` is `cli.unknown_command` |
| `jadpo check` | Checks everything and writes the derived files in `build/` | Built: writes `build/manifest.json` and the catalogues in `build/catalogue/` |
| `jadpo fmt` | Formats; `--check` only reports; `--layout` also applies promotion moves | Built: formats, and `--check` only reports; it fixes each file's imports first, `--layout` makes promotion moves, and it writes each file's declarations before their use, the primary declaration last |
| `jadpo test` | Runs every test | Built: pure and action tests, in the interpreter; route tests; schedule tests, `--kind schedule`; each recording of the app's own integrations read by its operation's decoders, `--kind recording`; and each recording of a prompt's answer read through each type it is asked for as, one made from an older prompt an alert on the report and in `data.alerts` (area 13, "Kinds of test") |
| `jadpo test <file>` / `<file>:<line>` | Runs one test file, or the test at that line | Built; a folder runs the tests under it |
| `jadpo test --name <text>` / `--kind route` / `--failed` / `--affected` | Runs tests whose names match, one kind, last run's failures, or the tests a change reaches | `--name`, `--kind` and `--failed` built; `--affected` not built yet: a change is measured against the merge-base with `main` |
| `jadpo test --watch` | Reruns the affected tests on every save | Built: each save reruns the tests that reach a node whose digest changed since the last run, within the path, name and kind given; a broken build shows its errors and runs nothing; `--json` writes one envelope per line; ctrl-c quits |
| `jadpo test --interactive` | Keyboard view of every test: search, navigate, run a file, one test or all, rerun failures, toggle watch | Specified in area 13, not built yet: the flag says `toolchain.not_built_yet` |
| `jadpo test --keep-failed` / `--seed <n>` | Keeps a failed test's database and trace; reruns with a given order seed | `--seed` built; `--keep-failed` built with the test database: it keeps the database and names it in the report; the trace comes with area 17 |
| `jadpo build [<deployable>]` | Writes the runnable app into `build/target/`, and each API's `<api>.failures.<locale>.json` beside its OpenAPI document (area 10) | Built, with `--json`: it checks as `jadpo check` does, writing the same files, and stops at its errors. For an app that declares no deployables it builds the one it has, `app`, natively, into `build/target/app/app` and `build/target/app/openapi/` with each API's document and failure files, built with the toolchain and lock this Jadpo pins; see [A native deployable](#a-native-deployable). Otherwise every declared deployable, or the one named, each in `build/target/<deployable>/` beside only its own APIs' documents; see [Declared deployables](#declared-deployables). One built for `Target.wasm` is a WebAssembly module with Jadpo's module shim and a Wrangler file, the module's size on the screen; see [A Workers deployable](#a-workers-deployable) |
| `jadpo dev` | Rebuilds and restarts the app on every change; `--no-run` only rebuilds | Built: runs the app in the interpreter on the native host at `http://localhost:3000` (`--port` to change it), one line per request with its request id; each save that checks cleanly serves the next request, and while a save has errors the last good version keeps serving. The settings `config.jadpo` declares are read from the environment over `.env.local` and checked first: a missing or invalid value stops the app starting and is shown by setting and variable, never by value, and a save of `.env.local` is checked like a save of source, keeping the last good version serving. Each database opens at the address its connection's `url:` setting holds, or the one `--url` names, or else a disposable PostgreSQL for the session (for a store whose address no variable gives), with pending migrations applied as they appear. `--no-run` only checks on every save and writes `build/`; `--json` writes one envelope per line. Each schedule run and skipped tick (a paused schedule's once) has a line, with its trace's id, outcome, what it emitted or its failure and its time, and each alert a line when it starts; the address of each database it started is in `build/dev/stores.json` while it runs, for `jadpo schedules` and `jadpo why`. Ctrl-c or SIGTERM stops it cleanly: the server drains, its background work giving back what it holds, and it exits 0; a second signal ends it at once (area 12, "Running background work"). A save that leaves a file empty that had text is taken as an editor's save in place between its truncate and its write, not yet saved: the reload waits for the file's text, up to 2 seconds, so an empty routes file, which checks cleanly with no routes, never replaces the version serving; a file still empty then reloads as saved ([design notes](notes/14-tooling.md), "Watching saves"; `jadpo test --watch` waits the same way). Every provider call is faked by default, with no credential needed, unless the connection's `development: { sandbox: }` lets it through to the provider's sandbox, never live; `--fake <connection>` and `--sandbox <operation>` change which for one session only, each request's line lists its calls, and each call is logged for `jadpo calls` (area 11, "Provider calls in development"). Each step an action with several changing calls reached is a line under its request's line, and a delivery whose action reached steps gets a line of its own, with `jadpo deliveries show <id>` when it needs an operator (area 12). Not built yet: the services the app's connections declare started with it (area 11) |

## Commands for looking into and changing code
<!-- topic: commands-for-looking-into-and-changing-code -->

| Command | Does | Status |
|---|---|---|
| `jadpo inspect` | Prints the project map: every declaration, call and failure | Not built yet: until then `build/manifest.json` and the catalogues hold the map |
| `jadpo routes` | Lists every route with its full method and path, file, API, authentication and the query or action it runs | Built; authentication shows `required`, `none` or `fresh` (area 8), and `signature` for a connection's generated webhook (area 11), listed under `webhooks  /webhooks` when the app declares no `webhooks` API |
| `jadpo ai show <prompt>` / `record <prompt> --as <name>` | Prints what a prompt sends a model: its type, file and version, the instructions as the system message, the conversation's turns and the request as the user message, each placeholder as the file writes it; records the model's answer to it for tests (area 21) | Built, with `--json`; takes the prompt's name or its type's; a misspelt action or prompt is `cli.ai_arguments` with the nearest name, and a prompt with errors shows them instead. `jadpo ai record <prompt> --as <name>` asks the configured model once, as the first `ai.generate` call of the prompt does, with the values of its last `jadpo dev` call or `--fixture <fixture>`'s, and keeps the answer as `recordings/<prompt>/<name>.json` beside the prompt, its plain values placeholders, with the prompt's version; through a package's connection never live and never with a stand-in credential, and with nothing to ask with (`recording.refused`); a refusal, no answer or one that doesn't decode keeps nothing (`recording.failed`) (area 21) |
| `jadpo ai usage` | Shows the tokens the app's `ai` calls used today and this month (UTC) and their estimated cost, by prompt and model, against `ai: { limits: { per_day: } }`, from the counts in the limits' store; never a prompt or an answer (area 21) | Built, with `--json`: the day's tokens, calls and cost, a table of each prompt and model, and a footer that fails once the day's tokens are spent; the database is its connection's setting's, a running `jadpo dev`'s, or `--url`'s (`ai.usage_unavailable`) |
| `jadpo sql <query>`, `jadpo sql <action>` | Prints a query's generated SQL for PostgreSQL and SQLite, or the statements an action's writes send; `--explain` runs it on a disposable database and shows the plan | Built for PostgreSQL: each statement the query sends, one for each include too, with the parameters that fill it, and `--json`. Given an action, each `create`, `update`, `delete` and transition it writes, in order, with its parameters as the action writes them, what each refusal raises (the constraint and the failure its `conflict:` arm names, the not-found failure, or a fault) and, for a transition whose `when:` can refuse a record, the statement that asks when no row changed. Tab completes the project's queries and actions, and without one it fails listing them. SQLite comes with a SQLite store; `--explain` is not built yet: the flag says `toolchain.not_built_yet` |
| `jadpo impact <name>` | Lists what a change to `name` reaches | Specified in area 1, not built yet |
| `jadpo rename <old> <new>` | Renames a declaration or field everywhere, keeping stored data | Not built yet; refuses a new name already in use |
| `jadpo domain rename/split/merge` | Moves a domain's files and updates every call | Specified in area 1, not built yet |

## Commands for the database, settings and translations
<!-- topic: commands-for-the-database-settings-and-translations -->

| Command | Does | Status |
|---|---|---|
| `jadpo migrate new <purpose>` / `apply` / `status` | Writes, runs and lists migrations | Built: `new` writes the next migration, or finishes the latest once its decisions are written; `apply` and `status` take the database's URL with `--url` (and `--store` when there are several), or read it from the connection's setting in the environment over `.env.local`. Prompting for a missing purpose, as the design language draws it, waits for prompts: without one the command names a purpose to use |
| `jadpo seed <name>` | Runs a seed; refuses production | Specified in area 5, not built yet |
| `jadpo config move <constant>` | Makes a constant a setting: a `config` field of its name and type with its value as `default:`, the constant deleted and every read rewritten to `config.<name>` | Built: names the constant as code reads it (`security.max_login_attempts`), or after its domain; lists what changed file by file, then the setting and its variable; refuses, changing nothing, with `configuration.move_type_unsupported`, `configuration.move_name_taken` and `configuration.move_read_unsupported`, every reason at once; undoes a move whose result wouldn't check; supports `--json` (area 10, "Making a constant a setting") |
| `jadpo config set <field>` / `check` | Sets or checks local configuration in `.env.local` | Built: `check` lists each setting as `set`, `default`, `missing` or `invalid` with its variable against the environment over `.env.local`, each problem as `configuration.value_missing` or `configuration.value_invalid`, never printing a value, and supports `--json`; `set` asks for one value at a prompt (hidden for a secret), refuses `--json` and a missing terminal, checks the value against the setting's type, and writes `.env.local` only as an ordinary file, through a private temporary file and only if unchanged since it was read, adding the file to `.gitignore` |
| `jadpo translations missing [<locale>]` / `unused` / `update` / `accept <locale>` | Lists, under the language file each is in, wording a locale lacks (and what an in-progress locale falls back on), translations made from wording that has since changed and wording not yet accepted, and the messages no code calls; records each translation's source in `translations/sources.jadpo_record`; accepts a locale's changed wording | Built, each with `--json`: `missing` and `unused` succeed whatever they list; `update` and `accept` stop on syntax errors, write one status line for each record and say `still stale` of a translation nobody redid; a misspelt action or locale is `cli.translations_arguments` with the nearest name (area 10, "Tooling for translations"); `unused` also lists the app's wording of library packages' text that shows nothing |
| `jadpo translations builtin` | Lists Jadpo's own messages the app can show, by feature, with their fields, the wording Jadpo ships and the locales that still need it | Built, with `--json`: succeeds whatever it lists; only the messages of features the app has on; `missing`, `update` and `accept` take the root `translations/` folder's wording of them in with the rest (area 10, "Jadpo's and packages' text") |
| `jadpo translations export [<locale>]` / `import <file>` | Exchanges messages, their descriptions and their wording with translation services as XLIFF 2.0 or JSON | Built, each with `--json`: `export` writes `build/translations/<locale>.xlf` (`--format json`, `--out <folder>`) for each translated locale, or the default locale's source file; `import` checks what comes back as `jadpo check` would, writes the language files and the record or nothing, and refuses with the language files' own rules at the unit of the file, `translation.exchange_unit_unknown` or `translation.exchange_file_invalid` (area 10, "Tooling for translations"); library packages' text is exchanged under each package's folder name and written back as the app's, in the root file |

## Commands for records, findings and review
<!-- topic: commands-for-records-findings-and-review -->

| Command | Does | Status |
|---|---|---|
| `jadpo api update` | Rewrites the committed public API record (`api/main.jadpo_record`) from the source | Built: one record per API with routes; a record whose API has no routes is removed. It is `api/<api>.jadpo_record` in the record format (area 18), with no `version:` (the OpenAPI version is a hash of it), and `jadpo check` fails while it disagrees (`record.out_of_date`) or was edited by hand (`record.edited`); a record under its old name, `api/<api>.api`, is removed |
| `jadpo records` | Lists every committed record (`.jadpo_record` files and `jadpo.lock`), what writes each, and flags any out of date with the command that updates it | Built: grouped by the command that writes them, each with its state, failing while one needs its command; `jadpo records update` brings every record up to date at once and moves records from their old names (area 18) |
| `jadpo declines list` / `clear` | Lists every decline with its rule, place and reason; `clear` removes the matching decline lines so each finding comes back; `--rule <id>`, a path and `--json` | Built: `list` is one list in file order, a reason too long for its row on the lines under it; `clear` writes each file without its matching lines, stops on syntax errors, and points at `jadpo check`; an unknown action, rule or a path outside the project is `cli.declines_arguments` (area 1, "Declining a judgement call") |
| `jadpo fix` | Applies the fix the author picks for a finding, through the same code as the editor's quick fix; first for a release that adds a standard type an app already names (swap to it, or rename) | Built, with `--json`: alone it lists each `type.standard_clash` and `type.standard_exists` finding with the fixes it takes; `jadpo fix <name> swap` and `jadpo fix <name> rename <NewName>` list what changed file by file; a fix whose result would add an error changes nothing and shows it; a name with nothing to fix, a fix it doesn't take or a taken name is `cli.fix_arguments`. The rename is not `jadpo rename`: it writes no identity line (area 19, "A release that adds a standard type") |
| `jadpo suggest` / `--catalogue` / `ignore <number> "<why>"` / `found <file>:<line> <Type or operation> "<why>"` / `share` / `share --approve` / `--decline` / `--new-token` | Hints where code could use a standard type or operation, never editing; lists every signal; keeps a suggestion quiet; teaches a miss; sends what was learned to Jadpo HQ | Built, with `--json`: formats, then reports numbered findings from the signals on the standard types' entries and the loop table, and ends with the agent's instruction; `ignore` writes `ignored_suggestions.jadpo_record`, `found` writes `taught_suggestions.jadpo_record`, both add to `.jadpo/suggest_pending`; a wrong number, place, type or operation is `cli.suggest_arguments`. `share` is built against a fake of Jadpo HQ: sends the pending hashes, shows the snippets HQ asks for and the exact payload, and sends them only with `--approve`; stops at once when sharing is turned off; an unreachable or refusing HQ is `suggest.share_failed` ("Suggestions" and "Sharing with Jadpo HQ" below) |
| `jadpo approval --before <ref>` | Writes the review report of what changed in behaviour, policy and the API | Not built yet: comes with records (area 18); who signs off is under [Review and sign-off](#review-and-sign-off) |

## Commands for background work
<!-- topic: commands-for-background-work -->

| Command | Does | Status |
|---|---|---|
| `jadpo deliveries` | Lists, retries, dismisses, cancels and resolves schedule runs and event deliveries; `show <id>` lists an unfinished multi-step action's steps, and `retry <id> --not-done` and `abandon <id>` act on it (area 11) | Built, with `--json`, synonym `delivery`: a table of what is `failed` or `outcome_unknown`, newest first, 20 a page (`--page`), `--state` for `pending`, `running`, `held`, `dismissed`, `cancelled`, `diverged`, `unfinished` or `abandoned`, an alert line per connection whose circuit breaker holds deliveries, a footer and one hint; `retry`, `dismiss`, `cancel` and `resolve <id> --done` or `--not-done` by an id's last 8 hex digits, each from its state only (`deliveries.wrong_state`, `deliveries.not_found`) and logged with the operator's name and database role; a schedule's failed run retried through the app's tick, as `jadpo schedules run` (area 12, "What an operator sees"); a diverged delivery and a route's unfinished steps listed by default, `show <id>` listing each recorded step (never a request, an answer or a key) and the one thing to do, `retry <id> --not-done` sending a step in doubt or that failed again under its next key generation, and `abandon <id>` (area 12, "What an operator sees"); parked queue messages listed as `failed` (`queue <connection> · <Event>`, their reason as the code), retried through the app's next tick and dismissed; an alert line once a multi-step run has been unfinished a day, and the metric on unfinished records in `--json` (area 12); batch runs not listed yet |
| `jadpo schedules` | Lists schedules; `run`, `pause`, `resume`, `history` | Built, with `--json`: a table of each schedule's last run and next due time (or `‖ paused`), the alerts that hold and a footer; `history <name>` 20 records a page (`--page`), grouped by day, then the metrics; `pause` and `resume` set the state row; `run` runs it now in this process under its lease (`schedules.lease_held`, `schedules.run_failed`), and does so only against a running `jadpo dev`'s own database, asking a deployment's own tick to run it otherwise and waiting for its record (`schedules.no_tick`; area 12, "Running a schedule by hand"); each entity's built-in purge schedule is listed too, as `Member.purge`, its runs showing the records they purged; the database is `--url`'s, the connection's setting's, or a running `jadpo dev`'s; an unknown name is `cli.schedules_arguments` with the nearest (area 12, "Watching schedules") |

## Commands for packages
<!-- topic: commands-for-packages -->

| Command | Does | Status |
|---|---|---|
| `jadpo package new <Name> --library` / `--provider` | Writes a package's own repository: the package format, `recipes/`, a `sample/` app and `GUIDE.md` | Specified in area 11, not built yet |
| `jadpo package add <name or source>` | Installs a package into `packages/<name>/<version>/` and `jadpo.lock`: from the registry (the fixed order of area 11), a git repository (`github.com/acme/acme-pricing@v2.1.0`), a path in this repository (another Jadpo app's OpenAPI, `../orders`) or a URL serving an OpenAPI document; regenerates the surface and refuses one that differs from its committed `surface.jadpo_record`; a third party's git package needs an acceptance | Built for a folder and a git repository's tag, with `--json`: the package compiled as the app's own integration and its surface checked byte for byte against its committed `surface.jadpo_record` (`install.surface_mismatch`), copied without hidden files and locked with its version, digest and source; a repository or tag that can't be read is `install.fetch_failed`; a name the app uses is undone; another Jadpo app's folder and a registry name say `toolchain.not_built_yet` until the importer and the registry; publisher trust waits for `jadpo approval` (area 11) |
| `jadpo package import <format> <file>` | Turns a published description (`openapi`, `smithy`, `postman`, `har`, `dts`) into the app's own integration in `shared/providers/<name>/<version>/`, merging corrections; `--as <Name.vN>` names it | Built for `openapi` (area 11); the other formats are `toolchain.not_built_yet` until first needed |
| `jadpo package record <connection> <operation>` / `conform <package> <capability>` | Confirms an operation against the sandbox and saves the recording; runs a capability's conformance suite | `record` built, with `--json`: `--input` gives the operation's input as JSON, written as a route's body is; it calls the sandbox once through the connection, never live and never with a stand-in credential (`recording.refused`), checks the answer against the operation's output type and keeps it as `recordings/<resource>.<operation>.json` in the app's own integration or a linked checkout, its plain values placeholders; a refusal, no answer or one that doesn't decode keeps nothing (`recording.failed`); an unknown connection or operation, or a missing or wrong `--input`, is `cli.package_arguments` with the nearest (area 11, "Provenance and confirmation"). `conform <package> ai` built, with `--json`: the `ai` suite's six cases (a typed answer, a refusal, a cut-off, a stream, an embedding of the declared length, the usage reported) in a scratch app holding the package as installed, by default against its recordings and a local fake on the loopback address, so nothing leaves the machine; `--live` against the provider's API through the app's connection to the package, refused without one, without its credential or address set, or without `ai:` through it (`conform.refused`); a case that fails is `conform.failed`; another capability is `toolchain.not_built_yet` (area 21) |
| `jadpo package check` / `publish` | Runs the publishing bar, writes `surface.jadpo_record` and compares it with the previous tag's; publishes to the registry | `check` built, with `--json`: the examples in descriptions run as tests (`publish.example_missing`, `publish.example_failed`) and the new version's section of `CHANGELOG.md` written, needing the author's why (`publish.changelog_missing`); in a package's own repository (or the folder given), the package checked as the app's own integration and each problem shown at its place in the package; the bar a row per item, passed, failed or not checked yet; the version worked out from the surface at the last `v<version>` tag, a breaking change refused (`publish.breaking_change`); `surface.jadpo_record` written when the package compiles and its version is allowed; a `recipes` row, each recipe compiled in the one domain of the package's `sample/` app (`publish.sample_missing` without one) (area 11). `publish` not built yet: comes with the hosted registry |
| `jadpo package update <name>` / `remove <name>` | Moves a package to a new version, printing area 18's review of the surface the app uses; removes one | Built, with `--json`: `update` moves a folder's package to what the folder holds and a git package to its newest release of the same major version, listing each surface entry added, changed or removed, marked `used by the app` and `needs a person`; `remove` is refused while the app's code has `use packages/<name>` (`install.in_use`) |
| `jadpo package show <name> [<declaration>]` | Prints a package's description, licence, version, kind and public surface, or one declaration with its file; `--guide` prints `GUIDE.md` | Built, with `--json`: the description in bold, the package's facts (version and source from its lock line, or "the app's own"), a list per section and a footer counting them; one declaration in full with its file and line and its whole description; a package or declaration it can't find is `cli.package_arguments` with the nearest; `packages` is a synonym (area 11, "The surface") |
| `jadpo package link <path>` / `unlink <name>` | Compiles the app against a package's live checkout, recorded in the lock and refused by production builds and CI; restores the installed version | Built, with `--json`: the lock's line becomes `link <path>` and the line it replaced is kept in `.jadpo/links`; every check reads the checkout in the installed folder's place and shows its problems there; with `CI` set, `jadpo check` and `jadpo test` refuse a link (`install.link_refused`), and `jadpo build` always does |
| `jadpo package recipe <package> <recipe> --into <domain>` | Copies a package's recipe into a domain, where it becomes the app's code | Built, with `--json`: every file under the package's `recipes/<recipe>/` copied to the same path under `domains/<domain>/`, all or nothing and never over a file the app has; the screen lists each file written, then `jadpo check` as the hint. `jadpo package check` compiles each recipe in the package's `sample/` app |

## Commands for services, calls, email and stored data
<!-- topic: commands-for-services-calls-email-and-stored-data -->

| Command | Does | Status |
|---|---|---|
| `jadpo services` / `start` / `stop` / `restart <name>` / `logs <name>` / `shell <name>` | Each container a package runs: package, image, state, health, CPU and memory, restarts, uptime; manages one; its logs (`--follow`, `--since`); a shell inside it, development only. In production they talk to the running app, never to the engine | Specified in area 11, not built yet |
| `jadpo calls` / `calls show <id>` / `calls script <operation> <failure>` | The development call log, faked or real, filtered by `--connection`, `--operation` or `--request`; one call in full; the next answer for an operation | Built, with `--json`: a table of the last 500 provider calls `jadpo dev` made, oldest first, each with its id, operation, source (`generated`, `scripted`, `sandbox`, `recording`), how it ended, its time and its request, a footer and one hint; `show` prints what was sent (never its headers) and what came back, laid out; `script` checks the operation and its answer (one of its failures' codes, or `unavailable`, `lost`, `in_use`, `misconfigured`) against the project and leaves it for the next call; an unknown action, call, operation or answer is `cli.calls_arguments` with the nearest (area 17, "Provider calls in development: `jadpo calls`"); `script ai.<prompt> <condition>` sets the next answer of a prompt asked through a package filling `ai`, one of `ai.generate`'s five conditions (area 21) |
| `jadpo email` / `email show <id>` / `links <id>` / `open <id> <link>` / `clear` / `check-domain [<domain>]` | The dev inbox `jadpo dev` catches every mail in, and the sending domain's records | Built, with `--json`: a table of the mail caught, oldest first, each with its id, email, address, subject and health, `--to` narrowing it; `show` prints one with its text (`--html` for its HTML), its links numbered, its attachments and each health problem with its fix; `links` numbers its links and `open` opens one in `$BROWSER` or the system's browser; `clear` empties the inbox; `check-domain` reads SPF, DKIM (`--selector`) and DMARC for the domain given or `email: { from: }`'s; an unknown action, mail or link is `cli.email_arguments` with the nearest (area 16, "Email") |
| `jadpo dev grants add <connection> --owner <id>` | Makes a fake OAuth grant for local development | Specified in area 11, not built yet |
| `jadpo files copy` | Copies files between two stores (S3, R2, disk) | Specified in area 16, not built yet |
| `jadpo runs unfinished --build <build id>` | Run against production before a deploy: lists the unfinished multi-step runs and batches whose recorded path the new build changes, so an operator knows what will stop as `diverged` (area 12) | Built, with `--json`: the build must be the one the checkout makes (`cli.runs_arguments` names it otherwise); a table of each record whose build differs and whose action's step shape the build changes (stops as diverged) or no longer keeps (sends its steps again), and each running batch run whose call it changes (fails), a footer and one hint; `--url` and `--store` as `jadpo deliveries` (`runs.unavailable`) |
| `jadpo batches list` / `show <id>` / `cancel <id>` | Lists batch runs (file reads and writes, `batches.each`) with their state and progress; one run's progress and problems; stops one, as `batches.cancel` does (area 12) | Not built yet |
| `jadpo search reindex` | Rebuilds a search index from the database | Specified in area 16, not built yet |

## Commands for debugging, editors and help
<!-- topic: commands-for-debugging-editors-and-help -->

| Command | Does | Status |
|---|---|---|
| `jadpo trace <id>` / `jadpo why <name>` / `jadpo call` / `jadpo console` | Debugging (area 17) | Proposed in area 17; `jadpo why <schedule>` built, with `--json`: why a schedule didn't run, or how its last run went, ending with its `.jadpo` place (area 12, "Watching schedules"); another name is `cli.why_arguments` |
| `jadpo incident [file \| -]` | Explains a runtime event: from a file, from standard input (`pbpaste \| jadpo incident -`, or a log stream piped in), or, with no argument in a terminal, from text pasted at a prompt; it finds the Jadpo events inside surrounding log text and groups them by request | Not built yet: comes with debugging (area 17) |
| `jadpo lsp` | Runs the language server for editors | Built: diagnostics, hover, go to definition from every use of a name and from a `use` line, and completion; the VS Code extension in `editors/vscode` starts it |
| `jadpo help [command]` | Help for one command; `--help` works everywhere | Built: `jadpo help [command]`, `--help` with examples on every command, and `jadpo help --json` with the rule catalogue and the schemas of the manifest and of `jadpo test --json` |
| `jadpo completions <shell>` / `jadpo version` | Prints the Tab-completion script for zsh, bash or fish; prints the version | Built |

## Running tests
<!-- topic: running-tests-from-the-command-line -->

`jadpo test` owns everything a test run needs, so nothing about it appears in
source (area 13). It starts a disposable local PostgreSQL, or uses
`JADPO_TEST_DATABASE_URL` in CI; builds one template database from the
migrations and copies it per test, so tests run in parallel; and keeps a
failed test's database with `--keep-failed`. `--kind route` runs one kind and
`--affected` the tests a change reaches. The report lists each
failed test with the failure it got and the one it expected.

The compiler also generates tests from facts it already owns: every route
rejects bad input with `{ field, rule }` details, every declared failure
reaches the client in the right envelope and status, and no output has a
field its type does not declare. They run with the others and appear in the
report under the route they test.

## Formatting
<!-- topic: formatting -->

The formatter owns layout, so equivalent code always looks identical and a
diff only ever shows real changes (area 1). It decides indentation,
spacing, blank lines (one between declarations, none doubled) and the order of
sections inside a declaration. It writes one member per line; a short list
such as `pick: [Todo.id, Todo.title]` stays on one line.
`jadpo fmt --check` fails when a file differs, and the editor formats on save.

Sections go in one order: a failure's `kind`, `code`, `message`, `client`,
`internal`; a route's `authentication`, `rate_limit`, `input`, `output`,
`status`, `deadline`, `run`; an entity's fields, then `identity`,
`persistence`, `lifecycle`, `membership`, `policy`, `freshness`, `guard`,
`person`. Anything else keeps its written order after these, fields are never
reordered, and a section's comments move with it.

The one exception is `config { }`: its fields are sorted by name, each with
its comments, so two workspaces that each add a setting rarely touch the
same lines (area 10, [Fields in name order](10-configuration-and-time.md#fields-in-name-order)).

Inside a project, `jadpo fmt` fixes each file's `use` lines before it lays
the file out (area 1, Imports): it adds a missing import, removes an unused
one and moves a name imported from the wrong folder, by applying the edits
`jadpo check`'s import problems carry, then sorts the lines. That needs the
project's names resolved, so it waits until every file parses. `jadpo fmt
--check` fails on a file whose imports it would change, as on one it would
lay out differently.

Inside a project, `jadpo fmt` also writes each file's declarations in
definition-before-use order, below its opening comments and `use` lines:
each before the first that uses it, so the primary declaration comes last,
ties broken by kind and then by the order the primary first reaches them,
and declarations that use each other in the order they were written, each
with the comments written above it and after it on its line (area 1, Files
and names). Test files and loose files have no primary declaration and keep
their order. `jadpo fmt --check` fails on a file out of it.

Promotion is the one layout change that moves code between files: when a
second file uses a type or helper, the compiler reports
`layout.promotion_required` with a one-step fix. That fix is applied through
the editor's quick fix or `jadpo fmt --layout`, never on save, so a move is
always a visible, separate change (area 1). `jadpo fmt --layout`
makes every such move a type needs (out of its file with its comments, into
a file of its own below the `use` lines it needs), then fixes the imports
the moves touched; a helper function's move into a library changes its
calls, so it stays a change to make by hand.

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
<!-- topic: editor-support -->

The language server (`jadpo lsp`) gives any editor the compiler's own view:
errors while you type, go to definition, find references, hover with types and
the failures a call can produce, completion (below), rename and formatting. The VS
Code extension in `editors/vscode` adds highlighting and starts the server. It
contains no parser of its own, and a command to open any generated file comes
with the generated files.

The extension starts `jadpo lsp` for `.jadpo` files, so the errors, hover
and go to definition work in VS Code, and highlights keywords,
declarations, types, strings, numbers and quantities, comments, operators,
route paths and `use` lines. The `jadpo.server.path` setting names the
binary (`jadpo` on `PATH` by default, or
`${workspaceFolder}/target/release/jadpo` for a checkout's own build), and
a missing binary says so. The highlighting grammar is written by hand and
`tools/editor_grammar.py`, part of `tools/verify.py`, fails when it leaves
out or adds a word the lexer reserves. Install and settings are in
`editors/vscode/README.md`.

`jadpo lsp` gives the errors `jadpo check` gives while you type, from one
database kept for the server's life, so an edit rechecks only what it
changes; hover with a declaration written as Jadpo, its `///` description,
the failures a callable can produce, a failure's code, kind and status,
and who calls or uses it; and go to definition from every use of a name,
in a body, a signature or a `use` line (area 1, Imports).

Not built yet: promotion as a quick fix, and rename, which goes through
the same code as `jadpo rename`, so an entity field renamed in the editor
keeps its stored data.

Go to definition from a `translations.<domain>.<message>(...)` call goes
to its message type and to its wording in each language file (area 10).

"Make this a setting", a code action on a constant's declaration or any
read of it, goes through the same code as `jadpo config move <constant>`
and refuses what it refuses: a refused move is offered unavailable, with
its reasons. Its edit replaces each changed file, creates `config.jadpo`
when there is none and deletes a file the move empties; an editor that
resolves code actions gets it only once the action is chosen. It is
offered, not a fix for an error, so `jadpo fmt --layout`, which makes the
moves errors ask for, never makes it (area 10).

"Decline with a reason", a quick fix on a declinable finding, writes
`decline <rule>: ""` above the declaration the finding is on, and
`decline.no_reason` asks for the reason until it is written;
"Remove decline", on a `decline` line, removes the line. Both go through
the same code as `jadpo declines clear` (area 1).

"Swap for the standard ...", a quick fix on `type.standard_clash` and
`type.standard_exists`, goes through the same code as `jadpo fix <name>
swap` and is unavailable, with the error it would add, where the command
stops. The rename needs a name, so it stays the command's until the
editor's rename comes with `jadpo rename` (area 19).

Not built yet: hovering over a query shows its generated SQL, the same as
`jadpo sql` (area 6). The SQL is never written into the source as a
comment, which would go stale, add noise to diffs and invite edits.

### Completion

The editor offers completion for everything as you type, and it is
generated from the same declarations and data the compiler uses, never
from a list kept for the editor: the project's name and type tables, the
standard library's stubs and the standard types' data entries, the
operation, unit and enum tables, the rule catalogue, the settings, the
translations' messages and the parser's own keyword sets. So every type,
unit and operation added to Jadpo is offered with no editor code of its
own. Where the position requires a type (an argument,
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

Every context above is built, in `jadpo_check::completion` through the
checker's analysis of the text as it is typed, and the server's
`textDocument/completion`. A test offers every operation each stub
declares after its namespace, with no operation named in the completion
code.

## Error messages
<!-- topic: error-messages -->

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

The rules:

- Paths are relative to the project.
- The two impact lines appear only when they say something.
- An obvious repair carries its edit, so the editor and agents can apply it.
- The help link opens the error's own entry on these language pages, which
  already have an example for every rule.
- JSON gives line and column as well as byte offsets, and uses one naming
  style for its keys.

## The `build/` folder
<!-- topic: the-build-folder -->

`build/` holds only generated files. It is never edited, never committed and
never read as input, and the same source always produces the same bytes.
Anything a person reviews and keeps, such as the record of the public API
(area 7) or the identity registry (area 5), lives outside it.

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

For a project, `jadpo check` writes `build/manifest.json` (area 18) and
the catalogues `build/catalogue/libraries.md` and `failures.md` with
`.json` twins, errors or not, so what an agent looks up is never stale; a
file is rewritten only when its bytes change, and loose files write
nothing. The reports, OpenAPI and the monitoring rules come with their
areas.

The schema report and the permissions report are not built yet (areas 5
and 9); the query report is area 6's input, kept beside the schema report
because a reviewer reads them together. Each is written as Markdown, which
an agent can search, with a `.json` twin for tools.

## Tools for agents
<!-- topic: tools-for-agents -->

Agents look things up through the compiler rather than browsing folders.
These come from areas 1, 4 and 5. The library and failure catalogues and
the similar-helper check are built; the rest are not yet.

- **Library catalogue** (`build/catalogue/libraries.md`): every library
  function with its signature, `///` description and callers, and every
  library constant with its type, value, description and readers (area 3).
  An agent reads it before writing a helper or a constant.
- **Similar-helper check**: when a new helper looks like an existing one,
  `jadpo check` reports `library.similar_exists` and names it; two constants
  of the same type holding the same value are `constant.similar_exists`
  (area 3). Both are declinable errors, not warnings (area 1): the fix, or
  a `decline <rule>: "<reason>"` line above the declaration.
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
  It also gives the routine for agents working in parallel workspaces:
  before pushing, merge `main`, run `jadpo check` (it builds nothing), fix
  any clash, then push. Private functions and types are unique only within
  their domain (area 1), so two domains' helpers don't clash, but a name
  another agent took in the same domain, or across the project, does.
  It gives the rule for a fixed value: if a
  deployment could ever need a different value it is a setting in
  `config.jadpo`, otherwise a `constant` (area 3); never both, so a
  `default:` never names a constant, and a constant that turns out to need
  per-deployment values is moved with `jadpo config move <constant>`
  rather than by hand (area 10).
- **Generated CI runs `jadpo check` first**, so a
  name clash or any other check error fails in seconds, before tests or a
  build. No page yet says what generates a project's CI (`jadpo new`
  creates none); whatever does must follow this. Neither the skill nor
  generated CI is built.

## Suggestions
<!-- topic: suggestions -->

`jadpo suggest` says where code could use a standard type or a standard
operation, and never edits. The edits stay with the agent, and, for type
clashes only, with `jadpo fix`. It is how an agent finds hand-rolled money,
durations, points and loops after writing code, and how Jadpo learns which
of its signals are right.

It formats first, as `jadpo fmt` does (a file with syntax errors stops it,
as every command that writes source stops), then scans the program graph:
every type, field and entity, and every function, action and constant body.
Tests, fixtures and seeds are left out, since they follow the code they test.

## Signals
<!-- topic: signals -->

What it looks for is data on each standard type's contract entry (area 19,
"Every standard type meets one contract"), never code of its own: a type
adds signals by adding data. Each kind of signal has one engine that reads
every entry.

| Signal | Data on the entry | Fires on | Example |
|---|---|---|---|
| `shape` | The fields `type.standard_exists` matches (area 19) | A type or entity holding all of a shape's fields among others of its own; exactly the shape is `type.standard_exists`, a `jadpo check` error | `latitude` and `longitude` in `Shop` → `Point` |
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

## Snippets and their canonical form
<!-- topic: snippets-and-their-canonical-form -->

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
  a hash of that text (16 hex digits of BLAKE3 too), which is what `jadpo
  suggest share` sends: it never depends on the canonicaliser's version,
  and only lines written the same way in many projects reach HQ's threshold.

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

## Ignoring a suggestion
<!-- topic: ignoring-a-suggestion -->

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

## Teaching a miss
<!-- topic: teaching-a-miss -->

`jadpo suggest found <file>:<line> <Type or operation> "<why>"`, run
before the code changes, records something the scan missed: the snippet at
that line, its text, both hashes, its canonical form, the suggestion and
the reason. It adds a `found` entry to `.jadpo/suggest_pending`, so the
text survives the fix, and teaches the canonical form as a project signal
in `taught_suggestions.jadpo_record` at the project root, committed and
shared by the team like the ignores (area 1's root list has it; area 18's
record format), so the next scan, in this checkout or a teammate's, finds
the same pattern anywhere as a `taught` finding. The snippet at a line is
the value of the first
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
the record. `.jadpo/suggest_signals` is only a cache of it: a pattern found
only in the cache moves into the record at the next scan.

```text
$ jadpo suggest found domains/shop/actions/pence_of.jadpo:5 Money "Pence held as Int"
✓ Recorded a missed Money at domains/shop/actions/pence_of.jadpo:5
  snippet  subtotal + delivery_pence
  reason   Pence held as Int

→ jadpo suggest  finds the same pattern elsewhere
```

## What is kept, and where
<!-- topic: what-is-kept-and-where -->

Every entry to share goes to `.jadpo/suggest_pending` with its kind,
`found`, `accepted` or `ignored`, so HQ can tell a signal that was right
from one that fired wrongly. `jadpo suggest share` sends them ("Sharing with
Jadpo HQ" below). Everything in `.jadpo/` is local, and the first command
that writes there adds `.jadpo/` to `.gitignore`. `.jadpo/suggest_last`,
`.jadpo/suggest_pending` and `.jadpo/suggest_signals` hold one JSON object
per line, since they hold source text; only the committed records are in
Jadpo's readable format. Those are written at format 2, with the shared
header (area 18, "Checking a record"), as shown above; files at format 1
are still read, and `jadpo records update` rewrites them.

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

Sharing is built against a local fake of Jadpo HQ, which speaks the
protocol below; HQ itself comes later, with the registry. Sharing is how
Jadpo learns which signals are right: which findings people take, which
they ignore and why, and what the scan missed.

**Sharing is on unless it is turned off, and nothing goes unless someone
runs `share`.** Only hashes go without a person's approval; a snippet's
text never does.

- **The project token.** `app.jadpo` holds
  `suggestions { project_token: "…" }`: 32 random hex digits tied to
  nothing, no repository, person or machine. It is committed, so a team
  counts as one project and HQ counts distinct projects without knowing
  which they are. `jadpo new` writes it (not built yet); whenever it is
  missing, `share` writes it before sending, made from the operating
  system's generator; with nothing pending it writes none.
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

### The protocol

The contract the real HQ implements unchanged. HTTPS and JSON, UTF-8, under
a versioned path.

- **Address.** `JADPO_SUGGEST_HQ` in the environment. There is no default
  yet: the release that launches HQ, with the registry, sets the real
  address as the default.
  `http://` is accepted only for a loopback host
  (`localhost`, `127.0.0.1`, `[::1]`), which is how the tests reach the
  fake; anything else must be `https://`, through OpenSSL, which only the
  `jadpo` tool links (no deployable does), with the platform's certificate
  store and the host name checked.
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

### HQ's agent

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
<!-- topic: review-and-sign-off -->

Some changes need a person's acceptance before they ship: a change that can
break an API client (area 7), a database drop (area 5), weaker policy
(area 9) and a new or renamed domain (area 1). They share one acceptance
form, designed in area 7. `jadpo check` fails until each such change has its
acceptance, and `jadpo approval --before <ref>` writes the report a reviewer
reads: what changed in routes, policy, failures and data. Acceptances are
files in one folder the git host protects (GitHub's code owners and branch
protection, or the equivalent), and CI runs `jadpo check`, which fails
without them. Jadpo builds no signing or user accounts of its own, since
review already happens on the git host.

## Errors only the running app or a build raises
<!-- topic: errors-only-the-running-app-or-a-build-raises -->

The running app reports three events of its own, never compile errors:
`runtime.unhandled_fault` (a defect or fault reached the boundary; the client
gets `internal_fault`), `runtime.outcome_unknown` (a change may or may not
have committed) and `runtime.startup_failed`. Area 4 decided what each logs.

`jadpo build` refuses features the compiler checks but cannot generate yet,
with `toolchain.target_*` errors. `jadpo check` reports the same errors, so
a project that passes `check` also builds. Area 8 asks the
same for a protected route with no authentication set up.

## A native deployable
<!-- topic: a-native-deployable -->

How an app with no `deployables:` is built and run; an app that declares
them is built as [Declared deployables](#declared-deployables) says.

- **One deployable, `app`.** An app with no `deployables:` has one
  deployable, called `app`, which serves every API and runs everything,
  natively. `jadpo build` and `jadpo build app` build it; any other name
  is `cli.build_arguments`.
- **What `jadpo build` writes.** It checks as `jadpo check` does, writing
  the same files in `build/`, and stops at the first error with the same
  screen, ending `✗ build stopped`. Otherwise it writes the deployable's
  Rust into `build/rust/app/` (the program's crate, `jadpo-program-app`, and a binary named after
  the deployable, the toolchain this Jadpo pins in `rust-toolchain.toml`,
  the runtime crates' `Cargo.lock`) and compiles it in release, offline,
  `--locked`, in the release profile its workspace's `Cargo.toml` gives
  (fat link-time optimisation in one codegen unit at
  `opt-level = 3`, its symbols kept), with the paths it was built from remapped and
  `SOURCE_DATE_EPOCH` fixed (`SOURCE_DATE_EPOCH` when it is set, else the
  commit's time), into `build/target/app/app`, with each API's OpenAPI
  document and `<api>.failures.<locale>.json` copied into
  `build/target/app/openapi/`. It is built without the runtime's
  `testing` feature. cargo failing is `toolchain.build_failed`, since the
  Rust Jadpo writes always compiles; a file it can't write is
  `toolchain.artifact_write_failed`. This Jadpo compiles a deployable
  against the runtime crates it was built from, in its own checkout.
- **The build stamp** (area 18): the build id, `b_` and the first twelve
  hex digits of a SHA-256 over the deployable's program (what it is
  compiled from: its Rust and its plan, its name and its stores'
  migrations),
  the compiler's version, the target and `jadpo.lock`; the commit `git`
  says the project is at, and whether its tree holds changes the commit
  doesn't (`build/` and `.jadpo/` aside), a project git doesn't track
  having no commit and counting as changed; the compiler; the deployable;
  and the target. The binary holds it as a constant, and the runtime plan
  it holds records the build id where `jadpo dev` records the graph's
  digest (area 11, "Deploys").
- **Starting.** The binary reads only the variables its settings name,
  through the runtime's `config::load`, and refuses to start, naming each
  setting and its variable, never the value, when one is missing or
  invalid (area 10). It builds each connection's driver through the
  factory; one it can't build is listed as it starts and answered
  `misconfigured` when used. A database whose connection names no
  `url:`, which only `jadpo dev` and `jadpo test` give one, stops the
  start. Each database the project keeps migrations in is compared with
  the migrations the build holds: one not applied, or one applied that
  the build doesn't have or that changed since, stops the start (area 5);
  one it can't reach yet is waited for, trying again after 1 second,
  doubling to 10, since a database that is down is not a configuration
  error (area 10). It then listens on `PORT` (8080 when unset) on `HOST`
  (every IPv4 interface when unset), a worker per core.
- **Running and stopping.** Each worker serves and ticks through the
  runtime's `Serving` on its own `NativeHost`. The first SIGTERM or
  ctrl-c stops it: it accepts nothing more, answers the requests it is
  reading, lets its background work give back what it holds, and ends
  with code 0; a second ends it at once. A start that fails ends with
  code 1.
- **One tick** (area 12). The binary takes one
  command: none, or `serve`, serves as above; `<deployable> tick` reads
  its settings, builds its drivers and compares its stores' migrations
  as serving does, but waits for no database (one it can't reach ends
  it with code 1, `store_unreachable`), runs one tick of the runtime's
  and ends: code 0, or 1 when the tick couldn't work in a store, or
  didn't finish, having panicked or outlasted the drain. It is
  what another scheduler runs when `schedules: { trigger:
  ScheduleTrigger.external }` says the server's own timer never ticks
  (area 12, "The tick"); the first SIGTERM or ctrl-c drains it, giving
  back what it holds. Anything else ends with code 2,
  `command_unknown`, without repeating what was given. `jadpo build`
  gives such a deployable a `tick` line, the command and how often to
  run it (`build/target/app/app tick · each minute`), and `--json` its
  `tick` (`command`, `every_ms`).
- **What it logs.** One JSON object a line on standard output, its keys
  sorted, each with the stamp's fields (`build`, `commit`, `modified`,
  `compiler`, `deployable`, `target`): `runtime.startup_failed` with
  every problem that stopped the start (`problems`, each with a
  `problem` code, the setting, variable or store it is about, and a
  `message`), `runtime.waiting` while a database can't be reached,
  `runtime.started` with the address, workers, `trigger` (`native` or
  `external`) and each connection whose driver wasn't built (`unbuilt`),
  `runtime.ticked` after each tick, the server's own timer's
  as a `tick`'s (`ran`, each schedule it ran or skipped by name
  with its outcome, `deliveries`, `received`, `polled`, `deleted`,
  `alerts`, `problems`, each store it couldn't work in, `unbuilt` and
  `ms`), `operation.failed` for a fault
  behind a 500 with its classification, route, `request_id` and `.jadpo`
  frames (areas 4 and 17), and `runtime.stopped`. The rest of a
  deployable's log comes with area 16's telemetry.

```text
$ jadpo build
  binary    build/target/app/app
  build     b_e20fc10f5474
  commit    8d113e6
  settings  DATABASE_URL

✓ built  app · native in 49.1 s
→ jadpo migrate apply  it starts only once its database has every migration
```

## Declared deployables
<!-- topic: declared-deployables -->

`application { deployables: { } }` in `app.jadpo` names each program the
app is built into, what it serves, what it runs and what it is built for:

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

- **What it serves.** `serves:` lists APIs by name: those `apis:`
  declares, or `main`, and `webhooks` once a connection receives
  webhooks. A deployable answers only those APIs' routes, each API's
  `openapi.json` among them, and a webhook only when it serves
  `webhooks`; every one answers its health checks. Two may serve one API.
- **What it runs.** `runs:` lists the background work it runs:
  `Trigger.schedules`, the app's schedules, each entity's purge, each
  poll and a batch run picked up again; and `Trigger.subscribers`, each
  delivery of an event or an email, after its action commits and from
  the outbox, and the queues `receiving:` reads. An action it serves
  still writes its events to the outbox, for one that runs them; a
  delivery in a deployable that doesn't run subscribers, or a batch run
  in one that doesn't run schedules, waits for one that does. Every
  deployable's tick deletes what has expired, which any instance may.
- **What it is built for.** `target:` is `Target.native`, a server, or
  `Target.wasm`, WebAssembly on Cloudflare Workers. A deployable with no
  `target:` has none, as the one an app that declares none has, `app`,
  and is built natively ([design notes](notes/14-tooling.md), "A deployable's target when none is
  declared").
- **What each reaches.** From the routes of the APIs it serves, the
  webhooks' replies, the actions its schedules and subscribers run and
  the batch runs it carries on, through every callable they call, to the
  connections they use: a store of files, a provider's operation, a
  package's action, `ai:`'s model; with its polls, the webhooks it
  receives, the queues `receiving:` reads and `email:`'s connection.
  It also reaches what Jadpo runs by name: the action
  `on_first_sign_in:` names where it serves an API, the action
  `magic_link: { send: }` names and every email's templates where it
  runs subscribers, and every provider package's `decode:` and
  `encode:`.
- **What each holds.** Its program holds only what it
  reaches: those callables, the routes of the APIs it serves, its
  schedules and the subscribers' actions only where it runs them, the
  connections it reaches with every database, and the settings those
  read, so it reads no other setting as it starts. A subscriber it
  doesn't run is still listed, so what it emits waits for the deployable
  that does. An app's wording and its stores are kept whole, and so is
  who signs in, but a method's `signing_key:` only where it serves an API
  that takes that method, and `origin:` only where it serves one that
  takes browser sessions: a Worker that only runs the
  subscribers starts without either.
- **Its target, checked** (area 11). A deployable that
  declares a target is refused each connection it reaches whose driver
  that target can't run, naming the alternative
  (`deployable.target_unsupported`): on Workers, the disk (a package over
  S3's API, such as Cloudflare R2), a service Jadpo runs without `host:`
  (`host:` with where it is hosted), and SMTP on port 25 (port 587);
  natively, a Cloudflare queue, which only a Worker receives (a package
  over Cloudflare's HTTP pull API), and Cloudflare's rate-limiting
  binding as the store of its rate limits (Redis). One built for Workers
  that serves an API needs that store, `rate_limits: { store: }` (area
  16, "Rate limits"; `rate_limits.store_required`); until Redis's driver
  comes, a Redis store there would leave each instance counting in its
  own memory, a judgement call a `decline` line answers, as `web` above
  does (`rate_limits.counted_in_memory`); and it is refused a
  `client_address:` source other than `cloudflare`, at its `source:`
  (area 10, "Client addresses"). A deployable built for
  Workers that reaches a connection with `max_concurrent:` is listed
  under "On Workers" once `jadpo check` passes, and `jadpo build` says it
  on the Worker's `calls` line: Workers runs as many instances as traffic
  needs, and each counts the cap on its own ([area 11's design notes](notes/11-integrations.md)).
- **Nothing left unserved.** Once `deployables:` is declared, every API
  with routes needs a deployable that serves it, and the app's schedules
  and subscribers one that runs them (`deployable.unserved`); a route
  with `settle:` is served only where subscribers run
  (`deployable.settle_unrun`).
- **The build.** `jadpo build` builds every deployable and `jadpo build
  <deployable>` one, each into `build/target/<deployable>/` with the
  documents of the APIs it serves, its Rust in
  `build/rust/<deployable>/`, all compiled into a cargo target in the
  user's cache folder shared by every project built for the same host
  and locale set, so the runtime crates compile once for them all
  (decisions table, "Where deployables are compiled"). A name the app doesn't
  declare is `cli.build_arguments`, with the nearest; a deployable built
  for `Target.wasm` is built for Workers ([A Workers
  deployable](#a-workers-deployable)). Each binary's stamp names it, and
  its build id is its own: it digests the deployable's program, so a
  change it doesn't reach leaves it alone (area 18, "A build id is a
  digest"), and `jadpo check` writes each deployable's target and build
  id in the manifest, with no build id while the project has errors.
  `settings` lists the variables the deployables read, each once.

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

## A Workers deployable
<!-- topic: a-workers-deployable -->

- **What `jadpo build` writes.** For a deployable with `target:
  Target.wasm`, its Rust in `build/rust/<deployable>/`: the program's
  crate and a `cdylib`, `jadpo-worker-<deployable>`, exporting the
  Worker's `fetch`, `scheduled` and `queue`, and `hand_on` for work
  handed on,
  through the Workers host, with the program, its build stamp and each
  store's migrations as a native binary is given them. It
  links the package driver, which every Worker has, PostgreSQL's when the
  project keeps a database (every deployable's tick reaches each store),
  SMTP's when the deployable reaches an SMTP connection, and no other.
  It is built for `wasm32-unknown-unknown` in release, offline and
  `--locked`, without the runtime's `testing` feature, into the cargo
  target the native deployables share, in the release profile its
  workspace gives (fat link-time optimisation in one
  codegen unit at `opt-level = "s"`, for size, since a plan limits what a
  Worker uploads), then bound by wasm-bindgen's own
  library at the release the lock holds, so nothing is downloaded and no
  command-line tool has to match, leaving out the module's name section
  and its `producers` section. Into `build/target/<deployable>/`, after
  what an earlier build left there: `<deployable>.wasm`, the module;
  `<deployable>.js`, wasm-bindgen's JavaScript for it; `worker.mjs`,
  Jadpo's module shim, which instantiates the module once per isolate and
  hands each request, the Worker's `env` and the invocation's `ctx` to it,
  and exports `JadpoHandOn`, a named entrypoint whose `handOn` takes on
  work another invocation handed on; `wrangler.json`; and
  the documents of the APIs it serves.
- **The Wrangler file.** Built as one value and written out once, so no
  binding is left out for another. `name`, the app's name
  and the deployable's, `<app>-<deployable>`, so two apps' `kiosk`s are
  two Workers in one account: the app's name is its folder's until
  `app.jadpo` gives one (area 7, "OpenAPI"), each lower-cased, each run
  of anything but an ASCII letter or digit one `-`, since Cloudflare
  takes letters, digits and dashes, at most 63 of them with the
  deployable's kept whole (`23-repair-cafe-kiosk`); `main`, the
  shim; `compatibility_date`, the date the Workers host is tested at under
  the workerd pinned in `tools/workers/` (2026-10-01); `no_bundle` and
  `find_additional_modules`, with a rule for each kind of file (`.mjs` and
  `.js` as ES modules, `.wasm` as compiled WebAssembly, the latter with
  `fallthrough: false`, since Wrangler warns of a rule that hides its own
  default without saying so), so the files are uploaded as they are
  written; `observability: { enabled: true }`, since Workers Logs keeps
  the lines the Worker logs only when it is enabled and the pinned
  Wrangler sends nothing, leaving it off, when the file doesn't say so;
  `secrets: { required: [ ] }`, the
  variable of each setting without a `default:`; and
  `hyperdrive: [ ]`, each database's address setting as a Hyperdrive
  binding by its variable, with its configuration's `id` when the build's
  environment gives one in `JADPO_HYPERDRIVE_<variable>`
  (`JADPO_HYPERDRIVE_DATABASE_URL`), and none otherwise, which `wrangler
  deploy` refuses (area 5, "PostgreSQL through Hyperdrive"). A binding on
  Workers is a variable or a secret, and a required secret is Wrangler's
  one way to name a binding that must be set without giving its value, so
  `wrangler deploy` refuses until each is set; a setting with a default is
  left to it. `triggers: { crons: ["* * * * *"] }`, the minute's cron
  trigger, runs its tick when the tick has a store to keep or sweep.
  With rate limits counted in Cloudflare's binding,
  `ratelimits`: a binding for each limit, its name, its `namespace_id` and
  `simple: { limit, period }` (area 16, "Rate limits"). No
  binding names the Worker itself: what one invocation's 30 seconds
  didn't reach is handed on to a fresh one through the loopback binding
  `ctx.exports` gives its `JadpoHandOn` entrypoint (area 12, "Running
  background work"), which reaches the Worker that handed it on whatever
  name `wrangler deploy --name` or `--env` gave it. Each queue on
  Cloudflare Queues the Worker receives
  is a consumer under `queues: { consumers: [ ] }`: its `queue`,
  `max_batch_size` 10, `max_batch_timeout` 5, `max_retries` 30
  and its `dead_letter_queue`; and that dead-letter queue is
  a consumer too, `max_retries` 100 and none of its own, whose messages
  the Worker parks for an operator (area 11). Wrangler's own `deploy
  --dry-run`, pinned in `tools/workers/`, accepts the file with no
  account and no warning, run in the tests with every network
  connection refused.
- **The build stamp.** Its target is `wasm`; the module holds the stamp as
  a constant, as a binary does, and the manifest gives the deployable's
  target as `wasm`.
- **The screen.** For each Worker, `worker`, its Wrangler file; `module`,
  the module; `size`, the module's bytes raw and gzipped at zlib's default
  level, as Wrangler reports an upload, in decimal units, as Cloudflare
  states its limits; `database`, each database's address binding through
  Hyperdrive with its id, or `no id` and an alert saying how to give one
  and that its Hyperdrive must keep no cache; then its build id and what
  it serves and runs; and `calls`, each connection's `max_concurrent:` it
  reaches, counted in each instance (`pdf 2 and ocr 4 at once in each
  instance`). The `--json` data gives `wrangler`, the module's
  `path`, `bytes` and `gzip_bytes`, and `hyperdrive`, each binding and its
  `id` or null, with `binary` null, and every deployable's
  `per_instance`, each `connection` and its `max_concurrent`, empty but
  on a Worker that reaches a cap.
- **Starting.** A Worker has no start of its own, so an isolate starts
  at its first request, as a native binary starts as its process does:
  it makes its program once, reads only the variables its settings name
  from the Worker's bindings, through the runtime's `config::load`, and
  refuses a `client_address:` source other than `cloudflare` (area 10,
  "Client addresses"). A problem there is logged once, as
  `runtime.startup_failed` naming each setting by its variable and never
  its value, and the isolate answers every request `503` with
  `unavailable` and a request id, as it would until a new version with
  other bindings replaces it.
- **Its database.** Each request builds its connections' drivers through
  the factory, over `connect()` and `fetch()`, since a socket belongs to
  the request that opened it. A database's address is its Hyperdrive
  binding's, reached without TLS inside Cloudflare, Hyperdrive using TLS
  to the database (area 5, "PostgreSQL through Hyperdrive"); a text
  binding of that name is reached directly, over TLS through
  `startTls()`. Until each database the project keeps
  migrations in has been compared with the build's migrations and found
  current, each request compares them first: one not applied, or one
  applied that the build doesn't have or that changed since, is logged as
  `runtime.startup_failed` and the request answered `503`; one it can't
  reach yet is logged as `runtime.waiting` and answered the same. So a
  migration applied after the Worker is deployed is picked up by its next
  request, with no restart; once current, an isolate doesn't compare
  again, and logs `runtime.started` with each connection whose driver
  wasn't built.
- **Serving.** Each request goes through the runtime as a native one
  does: the route table, input, policy, transactions, `settle:`, rate
  limits and the failure envelope. The client's address is
  `CF-Connecting-IP`, which Cloudflare sets on every request (area 10);
  the work a response leaves runs after it in `waitUntil`, within its 30
  seconds, and what that doesn't reach is handed on to a fresh invocation
  of the Worker through the loopback `ctx.exports` gives its
  `JadpoHandOn`, which carries it on within what is left of its own 30
  seconds, counted from its start; rate limits are
  counted in the store `rate_limits:` names, Cloudflare's rate-limiting
  binding, and in the isolate's memory only while it can't be asked:
  missing, failing, or not answering within 500 ms, each binding's
  fallback logged once an isolate as `rate_limits.fallback` (area 16,
  "Rate limits"). A streamed answer (`ai.stream`) is a `ReadableStream`,
  each event sent as it is read, and a client that goes away ends the
  request and with it the model's call. An invocation has
  at most six connections waiting for their answer's head at once, as
  Cloudflare allows, counted by the runtime across its database, its
  providers, SMTP and its own fetches, the work it leaves in `waitUntil`
  included: a seventh waits for one within its own timeout, and one whose
  timeout passes first never went out, so it is sent again where a call
  may be. `max_concurrent:` caps a connection's calls per isolate, a slot
  one invocation gives back handed to the call of another that has waited
  longest.
- **The tick.** Each run of the minute's cron trigger calls the Worker's
  `scheduled`, which starts the isolate as a request does and, once its
  databases are current, runs the runtime's tick (area 12, "The tick"):
  the schedules due, the outbox's sweep and the deletions, and the
  queues `receiving:` reads through a package. While the app receives a
  queue it ticks again at each 10-second mark of the cron's minute, one
  tick after another, so a queue is read as often as natively; the
  invocation ends with the minute and its ticks share the cron's 15
  minutes. Out of date, it logs why as a request does and ticks nothing.
  Each tick is logged as `runtime.ticked`, as a native server logs its
  own, with each store it couldn't work in.
  An `every:` under a minute is refused for a Worker that runs the
  schedules ([`deployable.target_unsupported`](../errors/deployable.target_unsupported.md)).
- **A queue.** Each batch Cloudflare Queues hands the Worker's `queue`
  starts the isolate as a request does and, once its databases are
  current, is kept through the runtime's consumer: each message kept once
  as its event, a repeat and one parked for an operator acknowledged with
  it (`ack()`), and one the store can't take now retried after 10 seconds
  doubled per attempt, at most an hour (`retry({ delaySeconds })`),
  each on its own; then the kept events'
  deliveries run in `waitUntil`, handed on past its 30 seconds as a
  response's work is. Out of date, or a batch from a queue it doesn't
  receive, every message is retried the same way, and `queue.retried`
  logged with the queue, how many and why; with 30 retries a message
  waits out about 22 hours of a deploy's migration before Cloudflare
  moves it to the dead-letter queue. A batch from that queue, its
  messages' retries spent, has each message parked whole for an operator
  as `dead_lettered`, with its id and event when they can be read, and
  acknowledged, so `jadpo deliveries` lists it and a retry has the tick
  read it again. A message whose `retry()` throws would be
  acknowledged when the handler ends well, so the handler throws once
  every message is answered, and Cloudflare hands again each one not
  acknowledged.
- **What it logs.** One JSON object a line on the console, which Workers
  Logs and `wrangler tail` read, with the same events and stamp fields as
  a native binary's: `runtime.startup_failed`, `runtime.waiting`,
  `runtime.started`, `runtime.ticked` after each tick of its cron, and
  `operation.failed` for a fault behind a 500 with its `.jadpo` frames,
  and `queue.retried` for a batch with a message retried. A Worker and a
  native binary start, compare their stores and log through the same
  code (`jadpo_runtime::deployable`), so they say the same
  things alike.

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

Built with no id in `JADPO_HYPERDRIVE_DATABASE_URL`, its database line
says so, and an alert says what to do:

```text
  database  DATABASE_URL through Hyperdrive, no id
  …
▲ alert  `DATABASE_URL` has no Hyperdrive id: create one with caching off and
  build with it in `JADPO_HYPERDRIVE_DATABASE_URL`
```

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

## Command errors
<!-- topic: command-errors -->

Every error that only a command raises, so `jadpo check` cannot show it with a
source example. Some rows belong to commands the decisions below replace
(`schema`, `artifacts`, `watch`); each row is rewritten as its command is built.

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
| `cli.signal_handler_failed` | any command | Shutdown handler could not be installed |
| `cli.suggest_arguments` | `suggest` | Suggest command has invalid arguments |
| `cli.test_arguments` | `test` | Test command has invalid arguments |
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
| `install.fetch_failed` | `package add`, `update` | Package's git repository or tag couldn't be read (area 11, "Installing errors") |
| `install.in_use` | `package remove` | Package to remove is still used by the app (area 11, "Installing errors") |
| `install.link_refused` | `check`, `test` in CI | Linked package in a CI or production build (area 11, "Installing errors") |
| `install.surface_mismatch` | `package add`, `update` | Package's committed surface doesn't match its code (area 11, "Installing errors") |
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
| `publish.breaking_change` | `package check` | Package's surface breaks its last release within one major version (area 11, "Publishing errors") |
| `publish.changelog_missing` | `package check` | Package's changelog doesn't say why its new version is released (area 11, "Publishing errors") |
| `publish.description_missing` | `package check` | Package's public declaration has no description (area 11, "Publishing errors") |
| `publish.example_failed` | `package check` | Example in a package's description doesn't compile or doesn't hold (area 11, "Publishing errors") |
| `publish.example_missing` | `package check` | Package's function has no example, or a failure it raises is shown by none (area 11, "Publishing errors") |
| `publish.sample_missing` | `package check` | Package's recipes have no sample app with one domain to compile in (area 11, "Publishing errors") |
| `runtime.outcome_unknown` | the running app | Generated runtime could not confirm whether an effect committed |
| `runtime.startup_failed` | the running app | Generated runtime failed during startup; a deployable's log event, with every problem that stopped it ([A native deployable](#a-native-deployable)) |
| `runtime.unhandled_fault` | the running app | Generated runtime contained an unexpected fault |
| `runs.unavailable` | `runs unfinished` | Unfinished runs' database couldn't be read |
| `schedules.lease_held` | `schedules run`, `deliveries retry` | Schedule is already running |
| `schedules.no_tick` | `schedules run`, `deliveries retry` | No tick picked up the run asked for |
| `schedules.run_failed` | `schedules run`, `deliveries retry` | Schedule's run failed |
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
| `toolchain.build_failed` | `build` | Rust compiler couldn't build the deployable |
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

## What is built
<!-- topic: tooling-built -->

Each row says what is built and what is specified but not built yet.

| Feature | Status |
|---|---|
| Command help | Built: every command has `--help` with examples, `jadpo help [command]` explains one, and `jadpo help --json` describes them all. |
| Project path | Built: every command finds the project by walking up to `app.jadpo`, so a path is optional; run from outside the project, printed paths start with the project as typed. |
| Scaffold | Not built yet: `jadpo new` is `cli.unknown_command`. The version an app declares is checked: `application { jadpo: "0.1" }`, with `toolchain.version_newer` and `toolchain.version_invalid` (area 1, "The root"). |
| Formatter | Built: one member per line, one blank line between declarations, and `fmt --check` counts the files it read; it keeps `use` lines at the top, sorted, fixes imports inside a project, `--layout` makes promotion moves, and it writes each file's declarations before their use, the primary declaration last. |
| Error messages | Built: one error per mistake in area 14's field order, `impact` only when it says something, edits on the next step where one exists, and help links to the language pages. |
| JSON output | Built: every command takes `--json` and prints one envelope (`schemaVersion` 2) with project-relative paths and `data.root`, line and column beside byte offsets, and camelCase keys; `jadpo test --json` is described in area 13 and under `testReport` in `jadpo help --json`. |
| `build/` | Built: `jadpo check` writes `build/manifest.json` and the catalogues in `build/catalogue/`; `jadpo test` keeps its last run in `build/test/`. |
| Language server | Built: diagnostics, hover, go to definition, from a `use` line too, and completion. |
| Agent tools | Partly built: the catalogues and the similar-helper check, a declinable error, are built; `migrate` is built; `impact`, `domain`, `rename` and `seed` are not built yet. |
| Suggestions | Built: `jadpo suggest` with its six signal kinds and taught signals, the catalogue, accepted findings, `ignore`, `found`, `share`, `suggestions { }` and the committed taught record, `share` against a fake of Jadpo HQ (HQ itself comes later). |
| Build-only errors | Built: `jadpo build` for the one deployable an app that declares none has and for declared deployables, with their `toolchain.target_*` errors. |

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

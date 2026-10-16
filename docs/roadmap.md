# Jadpo roadmap

**Updated:** 2026-10-16. This page is the only list of outstanding work: the
delivery plan for building Jadpo from the decided language, one chunk per
session. Record decisions in the [decision register](decision-register.md)
and the [language pages](language/README.md), not here.

## The direction (decided 2026-10-13)

- **The language is specified** in [docs/language](language/README.md):
  eighteen areas, all decided. Those pages are the specification and their
  examples become the tests.
- **A new compiler, built around one graph**
  ([compiler-runtime §1.1–1.2](compiler-runtime.md#11-graph-centred-compiler-decided-2026-10-13)).
  The first compiler (v0) was archived, not reworked.
- **Two ways to run the graph:** an interpreter inside `jadpo` for `dev`,
  `test` and `call`, and compiled Rust for deployables, native and
  WebAssembly (Cloudflare Workers), sharing one runtime crate. Every test runs
  both ways.
- **First build scope:** the graph core, both executors and what the golden
  todo uses. Everything else follows in the order below.

## How a session works

Saying **"continue the roadmap"** starts this routine; it runs unattended.

1. Take the first chunk below that is not ticked. Its tag (`opus · high`)
   is the model and effort it needs. Say in one line which chunk is next and
   its tag, then carry on without waiting. The building is done by a
   subagent started with exactly that model and effort, so the tag applies
   whatever the session itself runs on; the session plans, reviews the
   result, runs the gate and merges. Chunks marked **needs
   Richard** stop for him at the point named; every other chunk runs
   unattended.
2. Plan it in the session, not in a document: read its area pages, list the
   examples and errors it must make pass, and write those as failing tests
   first.
3. Build until its "done when" holds and the gate (`tools/verify.py`) passes.
   The full gate runs once, in the building subagent; when merging, the
   session runs `--profile quick` only if merging `main` brought in code,
   and leaves the full run to CI.
   Every chunk keeps the examples in `examples/` passing; each phase ends
   with an examples chunk that brings them up to date with everything the
   phase built (Richard, 2026-10-15).
4. Open a pull request from a branch named `chunk-<number>`, merge it when CI
   passes, and tick it here in the same pull request.
   If the chunk needs a language decision that is not on the pages, stop and
   ask Richard with a recommendation; never special-case the compiler for an
   example.
5. Then take the next chunk the same way. Stop at the end of a phase, at a
   **needs Richard** point, when a language question needs him, or when the
   session's context is getting long; the next session picks up from the
   roadmap. Before starting a chunk, check that no other session is on it:
   skip it if an open pull request or a branch named `chunk-<number>` pushed
   in the last three hours exists.

A chunk is sized for one session. If one turns out larger, split it here
before starting the second half. Tags: `opus` or `sonnet`; effort `high`
or `xhigh`, never lower (Richard, 2026-10-13); the hardest chunks (graph,
interpreter, concurrency, policy, compiled targets, WebAssembly
optimisation) are `xhigh`.

## Phase 0 — Reset

- [x] **0.1 Merge the language review.** Pull request from `language-review`
  to `main`. **Needs Richard** to approve opening it.
- [x] **0.2 Archive the current compiler.** Tag `archive/compiler-v0` on
  `main`, then delete `jadpo/`, the old examples' generated output, the old
  compile tests and tools that only serve the old compiler. Keep the golden
  contract (`examples/golden-todo/acceptance.json`), the experiments' results
  and the diagnostics catalogue as reference. The language pages' "today"
  boxes stop being compiled and are marked as the archived compiler's
  behaviour. Done when `main` builds nothing old and the docs check passes.
- [x] **0.3 Documentation clean-up.** The language pages become the only
  specification: older specifications move to `docs/archive/` with a pointer
  to the area that replaced them; the decision register keeps decisions
  outside the language and links to the area pages for the rest;
  `AGENTS.md` and `docs/README.md` are rewritten for this roadmap. Done when
  every live document is current and linked from `docs/README.md`.
- [x] **0.4 Architecture plan.** One session designing what every later chunk
  builds on: the Rust workspace and crates; the graph's node and edge kinds,
  stable identity and canonical JSON; the incremental query engine; the
  executor interface shared by interpreter and code generator; the runtime
  crate's boundaries and the capability driver interface; how area-page
  examples become tests; the gate, including the check that bans example
  application names in compiler source. Written into
  [compiler-runtime.md](compiler-runtime.md). **Needs Richard** to read it
  once before Phase 1.

## Phase 1 — Graph core (areas 1–4)

- [x] **1.1 Workspace and CLI shell:** `opus · high` `jadpo` with the agreed
  [design language](language/research/14-cli-design-language.md),
  self-describing help, `--json`, completion skeleton.
- [x] **1.2 Parser, syntax tree and formatter** `opus · high` for the decided grammar.
- [x] **1.3 Project layout and domains** `opus · high` (area 1).
- [x] **1.4 Values and types** `opus · high` (area 2).
- [x] **1.5 Logic** `opus · high` (area 3).
- [x] **1.6 Failures** `opus · high` (area 4).
- [x] **1.7 The graph, identity and manifest:** `opus · xhigh` `jadpo check` writes
  `build/manifest.json`; the language server gives diagnostics and hover.
- [x] **1.8 Typed bodies and one resolver** `opus · xhigh`, from the Phase 1
  review: a per-callable query giving typed bodies (site ids, types,
  resolved callees, literal targets, fallibility, failure sets) that
  diagnostics, graph edges, usage, cycles and the language server all derive
  from; per-name lookups so a body depends only on what it reads; typed type
  references in the graph; value validators and a typed standard-library
  operation table in a leaf crate shared with the runtime; transitive
  crate-graph and wasm32 gates; the quadratic naming check.
- [x] **1.9 Phase 1 review fixes** `opus · high`: the review's correctness,
  formatter, parser-depth, docs-gate and LSP findings, and area 1's
  decided exception letting a type fold to the name of the callable in its
  file (`CreateTodo` beside `create_todo`).

Done when areas 1–4's decided examples check, and each area's errors are
listed on its page with the new compiler's real output (the v0 lists are
reference only).

## Phase 2 — Interpreter and tests (area 13)

- [x] **2.1 Interpreter** `opus · xhigh` for pure logic, with errors at `.jadpo` lines.
  It walks 1.8's typed bodies. Also fold `resolve.rs`'s separate body walk
  into the typed-body query (left over from 1.9), so name errors and
  unused-variable errors come from one walk.
- [x] **2.2 Test runner:** `opus · high` file, line, name, seed, failure reports as decided,
  `--json`.
- [x] **2.3 Watch mode** `opus · high` under the 1-second target on a fixture project.
- [x] **2.4 Teaching examples** `opus · high`: commented example projects in
  `examples/` covering every feature of areas 1–4 and 13 built so far, each
  with its own tests run by `jadpo test` in the gate, and the coverage check
  that fails when a built feature appears in no example
  ([decision](decision-register.md#examples-teach-every-feature--2026-10-14)).
  Design the examples' order and commenting style first, as a reader
  learning the language would meet them.
- [x] **2.5 Construction failures by rule** `opus · high`: the block form
  `attempt CakeCount(n) { max: OrderTooLarge }` with `invalid:` as the
  catch-all, required and unreachable arms, object literals that never fail,
  and every page example and teaching example moved to it (area 2,
  "Construction failures by rule", 2026-10-14).
- [x] **2.6 Conditions as proof** `opus · xhigh`: an `if` condition narrows
  the ranges the checker knows, so a construction it makes safe needs no
  `attempt`, and the compiler reports the call sites that flip.
- [x] **2.7 Phase 2 review: soundness** `opus · xhigh`. Proofs that pass
  invalid values: a `Text` built into an `Email`, `Url` or `IpAddress` type
  unchecked (the built-in check becomes a `format` rule, reachable from a
  `format:` arm); decimal comparison and ordering that overflow into a
  wrong answer; decimal `/` keeping fewer than 18 extra places on large
  operands. Wrong results: a failure inside an outcome `match`'s
  arguments caught by that `match`; a set literal in a named set type built
  as a list; `include` copying fields alphabetically instead of in declared
  order; `i64::MIN % -1`. Silent passes: `List<T>(xs)` and three-part calls
  (`a.b.c()`) that check clean and fault at run time. A refinement that
  loosens an inherited rule (`max_length: 50` on a 40) is an error, as
  area 2 says a refinement only adds rules.
- [x] **2.8 Phase 2 review: tooling and docs** `opus · high`. Rerun
  commands and locations that work from where the command was run; a
  callable's undeclared failures no longer give its callers wrong `next`
  steps; `--watch` exits when its output closes and says when errors
  clear, including after a file is removed; location paths never cut; one
  location shape in `--json` and the `jadpo test --json` schema documented;
  `--failed` when the failed tests are gone; decided but unbuilt flags say
  `not_built_yet`; the smaller wording and completion findings; and
  `visibility.public_unused` (area 1, "Unused `public`"). Gate: rich
  snapshots stripped of the run time, the watch latency ceiling near the
  1-second target, README "try next" outcomes checked. Docs: areas 1, 13
  and 14 stop calling archived-compiler behaviour current.
- [x] **2.9 Phase 2 review: teaching examples** `opus · high`. Types that
  carry meaning, not just the right representation; business rules raised
  with `reject` (a stock problem is a `conflict`, not an `invalid_value`),
  with `if` checks as proof beside rule arms; a genuine `invalid:`
  catch-all; failures handled in application code, not turned into zero in
  tests; fixtures taught before public fixtures (08 before 07); the wrong
  comments and README claims the review lists; sets and maps described as
  far as they work.
- [x] **2.10 Executor shape for Phase 3** `opus · xhigh`, from the Phase 2
  review, before any database work. The typed body says which sites and
  callables can pause; the interpreter walks actions, tests and fixtures
  asynchronously and functions synchronously; `Ctx` is passed in, and the
  runtime owns the action boundary (transaction, rerun, commit) as §2.5
  says. `jadpo-plan` begins: type plans with declared field order,
  validator chains, field defaults, failure shapes, the frame table and
  callable numbering, literals decoded once, typed rule keys. Outcomes and
  assertion recording move into the runtime behind an executor trait the
  runner calls; the runner takes a per-test environment, and `Program`
  splits into shared data and per-thread caches. Watch keeps the previous
  graph and classifies every edge kind, so later edges can't be missed.
  Bodies and the graph shared behind `Arc`; the duplicated report code
  merged. Sets use the canonical order area 2 decides ("Set order"), from one shared comparator.
- [x] **2.11 Named collection construction** `opus · high`: `Names(found)`
  builds a named list, set or map type from a collection of its item type,
  checking only the collection's own rules, with arms, `invalid:` and proof
  as for any construction (area 2, "Named collection construction"); page
  examples, a teaching example and its errors.

## Phase 3 — Data (areas 5, 6)

- [x] **3.1 Entities, migrations and PostgreSQL:** `opus · high` snapshots, system tables,
  the test database template.
- [x] **3.2 Queries:** `opus · high` filters, order, pages, includes, selected columns,
  `jadpo sql`.
- [x] **3.3 Writes:** `opus · xhigh` entity operations, conflicts, concurrent-change rerun,
  counters.
- [x] **3.4 Lifecycle and purge.** `opus · high`
- [x] **3.5 Imports** `opus · xhigh` (area 1, "Imports", 2026-10-14):
  `use <folder> { Names }` and `use <domain>`, every name not declared in
  the file imported, built-ins never, a test file seeing its subject's
  declarations; a declaration another file uses moved to a file of its own,
  with the one-step fix; errors for a missing, unknown and unused import;
  `jadpo fmt` adding, removing and sorting imports, then run over every
  example and every page example to migrate them; go to definition from
  every use of a name, including on a `use` line. Done when no example or
  page example checks without its imports.
- [x] **3.6 VS Code extension** `sonnet · high`: `editors/vscode` starts
  `jadpo lsp`, with highlighting, so errors, hover and go to definition work
  in the editor (area 14, "Editor support"). The archived extension is
  reference only.
- [x] **3.7 Isolation by what an action does** `opus · xhigh` (area 6,
  "Counters and other one-step updates", 2026-10-15): an action that loads
  no record, itself or through any action it calls, runs at read committed;
  every other action at repeatable read. Done when two concurrent counters
  that are their actions' first statements both commit with no rerun (today
  the second reruns), and a counter after a read still reruns.
- [x] **3.8 Habits from other languages** `opus · high`
  ([decision](decision-register.md#errors-for-habits-from-other-languages--2026-10-15)):
  a researched catalogue of constructs people and agents carry over from
  other languages, each with its own error, an edit and recovery as if the
  Jadpo form were written; no syntax error followed by a second on the same
  line, checked over a corpus of broken files; each rule with a checked
  example on its area page.
- [x] **3.9 One declaration and what it uses** `opus · high` (area 1,
  "Files and names", 2026-10-15): a supporting type, failure or helper the
  file's primary declaration doesn't use, directly or through one another,
  is `layout.unused_declaration`, tests not counting; `jadpo fmt` puts the
  primary declaration first, after any opening comment, and leaves the
  rest in order. Every example that breaks either rule is rewritten so
  each helper does a real job for its action, starting with
  `examples/05-logic/domains/kitchen/actions/set_oven.jadpo`, and the
  example gate still finds every built feature. Done when the error has a
  checked example on area 1 and no example or page example breaks either
  rule.
- [x] **3.10 Phase 3 review: soundness** `opus · high`. A called action
  that fails rolls back its own writes (a savepoint per called action whose
  failure the caller handles); purge follows self and cyclic cascades so a
  held record never stops the batch; a patch can't send `none` to a
  required field; joining text onto a stored field is `||` or refused;
  structured fields, reading a second store and direct entity writes in
  tests are each built or a check-time `not_built_yet`, never a run-time
  fault; enum variants and validation rules are in migration snapshots so
  changing them needs a migration; a query limit or page size can't be
  below 1; misleading messages (a backfill's reason, faults blamed on the
  database) fixed; text-order indexes `COLLATE "C"`.
- [x] **3.11 Test databases that scale** `opus · high`: several copy
  workers or reused copies; one PostgreSQL server per gate run shared
  through `JADPO_TEST_DATABASE_URL`; the language pages run `jadpo test`
  once; the CI PostgreSQL steps no longer take turns; `--profile quick`
  truly needs no PostgreSQL; one integration-test binary per crate.
- [x] **3.12 The runtime owns what both executors need** `opus · xhigh`,
  before 4.1: failure shapes, refusals, missing plans and field and type
  plans move into `jadpo-std` as plain data; one runtime entry point runs a
  read or write (opening its own boundary outside an action) and returns
  the value, failure or conflict arm, leaving the interpreter only the arm
  dispatch. PostgreSQL text rendered by one leaf crate (conditions,
  statements, table definitions). Production ids from host randomness;
  purge given a time budget; the panic lints cover `indexing_slicing`,
  `unreachable` and `unimplemented`; the duplicated migration and import
  code merged; stale crate docs corrected.
- [x] **3.13 Incremental data checking** `opus · xhigh`, before 4.4:
  per-name entity and query models without positions, diagnostics per
  file, so an edit rechecks only what reads the changed name; the plan
  built incrementally per callable and entity (or §2.4 corrected to what is
  built); entities, queries and writes in the scale test.

## Phase 4 — HTTP, identity and policy (areas 7–9)

- [x] **4.1 Routes:** `opus · high` groups, inputs, outputs, statuses, OpenAPI, the API
  record, default rate limits.
- [x] **4.2 Request guarantees:** `opus · high` idempotency (a lookup ignores
  keys past 24 hours; deleting them comes with 6.2), deadlines, body limits,
  health.
- [x] **4.3 Sign-in:** `opus · high` the principal, browser sessions,
  routes signed in by default, `current_principal`, starting and ending a
  session, signing in from route tests. Split: API tokens and service keys
  are 4.3b, the revocation delay 5.1b.
- [x] **4.3b API tokens and service keys** `opus · high`: `service_key:`
  with `Service.issue_key`, `.keys` and `.revoke_key` and a
  `jadpo_service_keys` table, service callers (`service_disabled`,
  `Principal.service`, a route test signing in as a service), and
  `api_token:` with `sessions.issue_token`, `.tokens` and `.revoke_token`
  (area 8, decided 2026-10-16).
- [x] **4.4 Policy:** `opus · xhigh` grants compiled into queries, tenancy, the permissions
  report, and `sessions.revoke_token` and `Service.revoke_key` gated by
  the owning record's grants (today they revoke any id, area 8). Split:
  field restrictions are 4.4b.
- [x] **4.4b Field restrictions:** `opus · xhigh` a field's own
  `policy:` and `FieldAccess.server_only`, `policy.field_widens_entity`,
  `policy.output_field_unproved` with a field's value followed through
  variables and other types, writes refused for a field the caller may
  not change, `policy.role_field_create_forbidden`, proving every caller
  who may read a record may read the parents a query includes, the
  report's field rows, and policy's grants and scopes in the graph and
  manifest (area 9).
- [x] **4.4c Listing tokens and keys from a `GET`** `sonnet · high`
  (area 8, decided 2026-10-16): a `GET` route runs `sessions.tokens` and
  `Service.keys` as generated queries, and `IssuedToken` and `IssuedKey`
  can be held in an `Output`, so a "signed-in devices" screen is one route.
- [x] **4.6 Teaching examples for Phase 4** `opus · high`: bring
  `examples/` up to date with routes, identity and policy, and pass
  `tools/verify.py --require-examples`.
- [x] **4.7 Phase 4 decisions** `opus · high` (Richard, 2026-10-16): a
  reference between tenant-owned records stays inside the tenant, checked on
  every write, so an include of a tenant-owned parent proves (area 9); a
  condition or order on a field some caller may not read is
  `policy.output_field_unproved` (area 9); route tests send a fresh
  `Idempotency-Key` on every write unless they write their own (area 13),
  with the examples' workaround in `11-cake-shop` removed.
- [x] **4.8 Phase 4 review: policy soundness** `opus · xhigh`. Proofs and
  gates that let data out: a scoped-role or field line under `actions:`
  never enforced on an action that touches no entity (refuse or gate as
  nobody); an `Output` built in a function never checked; values decided
  by an `if`, a `match`, a filter predicate or a `count` losing where they
  came from; a restricted value keeping its rules when stored with `set:`,
  used as a lookup id or followed through an include's reference; the
  record a write returns proved against its `read:`, with the flow check
  running when no field has a policy; an update whose target may come from
  several records proved for every one; an optional membership `role:`
  read as every role; a caller inside a named object reaching an `Output`,
  `Input` or failure; a field-refused transition answering 403 before its
  `conflict:` arm; `role_field_create_forbidden` followed through
  branches; the fallbacks that run an unpoliced statement or leave an
  action ungated failing closed. Tenancy: a new membership makes its
  member one only for a tenant with no members yet, so no one joins a
  tenant as owner; a listed action moving a record to another tenant
  needs the caller's membership there.
- [x] **4.9 Phase 4 review: request soundness** `opus · high`. The address
  rate limit counted before the work on routes anyone may call, even with
  a credential header; deadlines enforced on work that never waits (the
  interpreter checks the deadline as it runs, and a body finishing past it
  answers 504 and commits nothing); a handled failure undoing its
  `sessions.start` and `sessions.end`; header values that aren't visible
  ASCII answered 400, never dropped; route tests leaving out an omitted
  field rather than sending `null`; OpenAPI's plain variants in mixed
  enums and `Time` as sent, and each route's responses (403 from listed
  actions, 503, 413, 400, replay only where a response is stored) derived
  from the plan; `Idempotency-Key` among the headers an `Input` can't read;
  a route without `input:` refusing query keys and a body; a request body
  read as JSON only with a JSON `Content-Type`; a concurrent change found
  at commit rerunning the action, not answering 503; header, body and
  connection timeouts and caps in the native host; each API's sign-in
  methods in its record.
- [x] **4.10 The runtime owns Phase 4's request path** `opus · xhigh`,
  before Phase 5, from the Phase 4 review: an action's gate checked in
  `jadpo_runtime::action`; the caller a call runs as read from the plan,
  not a type name; route tests' caller and `Idempotency-Key` given to the
  runtime's test request; one `serve_request` that `jadpo dev` and every
  deployable call; rate limits behind a host capability (memory natively,
  and §2.6 settles Workers' default against area 16); the sign-in lookup,
  the idempotency claim and its release each one statement, with `BEGIN`
  sent with the first statement (area 7's "a transaction of its own"
  becomes "one statement", the same outcomes); a policed `get` that finds
  nothing answered without a second statement; `/health/ready` cached for
  a second; the interpreter yielding as it checks its deadline, so a
  route that computes without waiting doesn't hold its worker thread
  (chunk 4.9); §2.4 and §2.6 and the stale comments brought up to date.
- [x] **4.11 Incremental policy and route checking** `opus · xhigh`, before
  6.1: one positionless per-callable query of what a body reads, writes,
  calls and loads, which isolation, the store rules, policy reach and the
  flow check all build on; the flow summary per action, with recursive
  calls solved as a fixpoint; a test body reading the routes only when it
  sends a request; routes, identity and policy in the scale test with
  asserted recheck counts.

## Phase 5 — Configuration, time, translations and the standard library (area 10)

- [x] **5.1 Configuration:** `sonnet · high` `config.jadpo`, environments, secrets, signing
  keys, `jadpo config`, the check before the app starts, and a test's own
  `config:`. The trusted proxies a client's address is read through are
  5.1c.
- [x] **5.1b Signed sessions** `opus · high`: area 8's
  `revocation_delay:` once 5.1 gives signing keys: sessions and tokens
  signed and checked without a read, expiring within the delay and
  renewed against the record; `RouteAuthentication.fresh` reading the
  record; `POST /auth/exchange` for service keys; a browser session's
  `origin:` from configuration.
- [x] **5.1c Trusted proxies** `sonnet · high` (area 10, question 12,
  decided 2026-10-16): which
  proxies a deployment trusts for a client's address (`X-Forwarded-For`,
  `Forwarded`, Cloudflare's `CF-Connecting-IP`), declared once in
  `app.jadpo` and read from the request, so the default rate limits count
  the client and not the proxy.
- [x] **5.1d Restricted values stored elsewhere** `opus · high` (area 9,
  decided 2026-10-16): a create or update storing a value in a field some
  of whose readers may not read the value is `policy.output_field_unproved`
  at the `set:`, following the value as 4.8's flow check does; copies
  between fields with the same readers stay allowed.
- [x] **5.2 Time:** `opus · high` durations, instants, zones and the
  `temporal` library, `ZonedTime` in place of the time of day, time on the
  wire and in queries, and a test's clock. Formatting is 5.2b.
- [x] **5.2b Time formats** `opus · high` (area 10, "Showing dates and
  times to people"), split from 5.2: `TimeFormat` declared and built in,
  `ComponentStyle`, `PresentationText` and
  `type.presentation_text_persistence`, and `temporal.format` and
  `format_relative` checked (`zone:` required for an `Instant` and refused
  otherwise), the text they give staying `toolchain.not_built_yet` until
  5.3; area 10's formatting errors rewritten with real output.
- [ ] **5.2c A type name is not a value** `sonnet · high`, found in 5.2b:
  `var g: Label = Placed`, a type's or other declaration's bare name where
  a value is expected, checks clean today; it is an error with its own
  rule, a checked example and the one-step fix where one exists (a
  construction, a variant), and every place a value is read is covered.
- [ ] **5.3 Translations and locales.** `sonnet · high` Besides locales and
  translations, it gives the text 5.2b left refused: `temporal::format`
  and `format_relative` in `jadpo-value` over locale data (the format
  arrives as `TimeFormatSpec` text), `locale:` checked against the
  supported locales, and the `toolchain.not_built_yet` refusal in
  `jadpo-check` removed.
- [ ] **5.4 Standard library: the plan** `opus · high`. Only `collection`
  and `temporal` are specified, piecemeal across areas 2, 3 and 10, and
  real code keeps needing more: splitting and trimming text, formatting and
  rounding numbers, hashing, encodings. Collect what the decided areas and
  the example apps already call or will need, compare the standard
  libraries of TypeScript, Rust, Go, Elixir and Python, and write one new
  area page: the namespaces, every operation's signature and failures,
  which have a SQL form (for queries and backfills), and what is left out
  on purpose. **Needs Richard** to decide the page before 5.5.
- [ ] **5.5 Standard library: the build** `opus · high`: the operations 5.4
  decided, in the shared operation table, the interpreter and the runtime,
  each with its page examples as tests. Split here before starting if 5.4's
  page is more than one session.
- [ ] **5.6 Computed backfills** `opus · high` (area 5, "Computed backfills",
  2026-10-15): `backfill:` as an expression over each row's own fields,
  filled in batches, one `UPDATE` each where it has a SQL form and by
  `jadpo migrate apply` otherwise, then made required.
- [ ] **5.7 Teaching examples for Phase 5** `opus · high`: configuration,
  time, translations and the standard library, passing `--require-examples`.

## Phase 6 — Events and schedules (area 12)

- [ ] **6.1 Events:** `opus · high` outbox, subscribers, retries, `Access.system`, trace
  context, and `events: { store: }`, with the compile error for an action
  that writes no store but needs one for its outbox or idempotency keys
  (until then such an action keeps no key, chunk 4.2). From the Phase 4
  review: a system caller, so `Access.system` grants schedules and
  subscribers (today every policed statement refuses them), and events
  delivered after the response is sent, through the host (Workers'
  `waitUntil`), not before it. From chunk 5.1d: what a subscriber stores
  from an event is checked against what every `emit` of it put there, as
  area 9's stored values are (an `emit` itself stores nothing a caller
  reads).
- [ ] **6.2 Schedules:** `opus · high` leases, the native tick, schedule monitoring basics,
  and the runtime's own sweeps, starting with deleting expired idempotency
  keys (area 7).
- [ ] **6.3 Routes that wait for follow-on work** `opus · high` (area 7,
  "Waiting for follow-on work", 2026-10-15): `settle:` delivers an action's
  events and their follow-ons at once, answers with the named query when
  they settle and with `202` past `within:`.
- [ ] **6.4 Teaching examples for Phase 6** `opus · high`: events and
  schedules, passing `--require-examples`.

## Phase 7 — Packages, capabilities and email (areas 11, 16)

- [ ] **7.1 Packages and connections:** `opus · high` the package format, lazy loading,
  `connections:`, the network report.
- [ ] **7.2 Importing and faking:** `opus · high` the OpenAPI importer, hand-written
  packages, generated fakes, `jadpo provider record`.
- [ ] **7.3 Email:** `opus · high` the capability, SMTP driver, `emails/` templates, the dev
  inbox, the health score.
- [ ] **7.4 Outside calls hold no transaction** `opus · xhigh` (areas 6 and
  11, 2026-10-15): reads before a call in a read-only snapshot closed before
  it, the transaction opened after it, a write to a record loaded before the
  call checked by its row version, and a change rerunning the action with
  the call repeated under its key. Done when a test holds a provider call
  open and shows no transaction open meanwhile, and a change made during
  the call reruns the action.
- [ ] **7.5 Teaching examples for Phase 7** `opus · high`: packages,
  capabilities and email, passing `--require-examples`.

## Phase 8 — Compiled deployables

- [ ] **8.1 Native:** `opus · xhigh` graph to Rust; every test passes interpreted and
  compiled. Includes TLS for database connections (a hook on the driver's
  `Transport`, rustls natively), and an outcome and effect record (SQL, row
  counts, events) both executors write, for the differential comparison.
  From the Phase 4 review, decided in §2.5 first: a typed per-route entry
  so generated code decodes a request straight into its structs; plan data
  embedded as constants rather than rebuilt at start; a route trie and no
  per-request allocation for paths, labels and failure lookups; the native
  host accepting asynchronously and spreading connections round-robin,
  with one rate-limit count per caller across its worker threads (today
  each thread counts its own, so a caller on several gets several
  allowances).
- [ ] **8.2 WebAssembly on Workers:** `opus · xhigh` the cron tick, outbox delivery after
  commit, PostgreSQL through Cloudflare's pooling; the client's address from
  `CF-Connecting-IP` (`ClientAddress::read` with `Target::Workers`, built in
  5.1c), and a `client_address:` source other than `cloudflare` refused when
  a project is built for Workers (area 10, "Client addresses"), which needs
  deployables declared first.
- [ ] **8.3 Performance spike** `opus · high`: one route compiled to native and to
  WebAssembly and measured against an equivalent hand-written Bun and
  TypeScript app, comparing Wasm with native and Bun with native as well
  as Wasm with Bun. Moved from 4.5, which needed Phase 8's compiled code
  (Richard, 2026-10-14). **Needs Richard** if Wasm is more than 25% behind
  Bun.

## Phase 9 — Performance

Richard, 2026-10-13: developers won't touch this if it's slower.

- [ ] **9.1 Benchmarks:** `opus · high` the golden todo plus focused workloads (reads,
  writes, signed-in lists, large rows, cold start) on native, Wasm under
  wasmtime and on Workers, against an equivalent Bun and TypeScript app and
  Node; results published with the method.
- [ ] **9.2 Optimise WebAssembly** `opus · xhigh` until it matches the Bun app on throughput
  and p95, or Richard accepts a measured gap. The earlier experiments reached
  about 79% of Bun's throughput and list what to try next
  ([optimisation results](experiments/wasm-optimization-results.md)).

## Phase 10 — Golden todo

- [ ] **10.1 The golden todo end to end** `opus · high` on both targets, rewritten in the
  decided language, its contract cases passing. One independent review of
  the whole first build.
- [ ] **10.2 How easily agents write Jadpo** `opus · high`: the same 10–20
  realistic changes to one app (the golden todo, with its TypeScript
  baseline and adversarial changes) done by fresh agent sessions in Jadpo
  and in TypeScript, several runs each, with hidden acceptance tests.
  Measures correctness, attempts and tokens to green, where each mistake
  was caught (check, test or escaped), how easily a person reviews the
  diff, and how often a tempting unsafe shortcut gets through. The agent
  gets the language pages, since no model has seen Jadpo (Richard,
  2026-10-14). The method is designed at the start of the chunk; its
  results steer the order of the work after the first build.

## After the first build, in order

1. Personal data (area 15).
2. Remaining built-in services (area 16): flags, files, audit, search,
   telemetry thresholds.
3. Debugging and review (area 17): `why`, timelines, `test new --from`,
   `call`.
4. Records and acceptances (area 18): the approval report, acceptance
   matching, package upgrade plans.
5. Tooling completeness (area 14): interactive tests, `rename`, `impact`,
   full completion.
6. Product work: the developer console and MCP (E11), the graph review UI
   (E06, Revset-inspired), the package registry, acting as an OAuth provider,
   D1 and row-level security.

Older epics (E03–E12) are described in the
[archived roadmap](archive/implementation-roadmap-2026-10-11.md) and are
replanned against this list when reached.

## How we work

- Branch from `main` for each chunk; commit and push often; merge through a
  pull request. Never put a worktree under `/private/tmp`.
- `tools/verify.py` output and the tests are the evidence. No evidence
  packets, per-step reviews, timing logs or plan documents.
- One independent review per phase, not per chunk, by two reviewers:
  soundness, and architecture for the phases ahead.
- Update this page when a chunk finishes or the order changes, and nothing
  else.

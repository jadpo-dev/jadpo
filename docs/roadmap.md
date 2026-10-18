# Jadpo roadmap

**Updated:** 2026-10-18. This page is the only list of outstanding work: the
delivery plan for building Jadpo from the decided language, one chunk per
session. Record decisions in the [decision register](decision-register.md)
and the [language pages](language/README.md), not here.

## The direction (decided 2026-10-13)

- **The language is specified** in [docs/language](language/README.md):
  nineteen areas, all decided. Those pages are the specification and their
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

## Phase 5 — Configuration, time, translations and the standard library (areas 10 and 19)

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
- [x] **5.2c A type name is not a value** `sonnet · high`, found in 5.2b:
  `var g: Label = Placed`, a type's or other declaration's bare name where
  a value is expected, checks clean today; it is an error with its own
  rule, a checked example and the one-step fix where one exists (a
  construction, a variant), and every place a value is read is covered.
- [x] **5.3 Locales and the text for people** `sonnet · high` (area 10,
  "Locales" and "Showing dates and times to people"), the first half of
  "Translations and locales": `locales { }` and its errors, `locale:`
  checked against it, a `Locale` on the wire and in a setting limited to the
  supported ones, and `temporal::format` and `format_relative` in
  `jadpo-value` over ICU4X's compiled CLDR data, with the
  `toolchain.not_built_yet` refusal removed. The second half, translations,
  is 5.3b.
- [x] **5.3b Translations** `sonnet · high` (area 10, "Translations"): the
  root `translations/` folder, one JSON file per supported locale of ICU
  MessageFormat messages (plurals through CLDR's rules, selects, numbers and
  dates), one typed `translations.<key>(locale:, ...)` call per key of the
  default locale's file, every other file checked for the same keys and
  placeholders (their errors, with real output), and the interpreter and
  runtime running them.
- [x] **5.3c Choosing a locale from a request** `sonnet · high` (area 10,
  "Locales"): the library call that picks a supported `Locale` from an
  `Accept-Language` header and falls back to `default`. Named by Richard on
  2026-10-16: `locale.from_accept_language(header)`, giving a `Locale`.
- [x] **5.3d Names private to a domain** `opus · high` (area 1, "Files and
  names", decided 2026-10-16): a private function or type unique only
  within its domain in the checker's name table, resolution and imports,
  with folded collisions in the same scope; the identity key of a function
  or type declared in a domain carrying the domain, and promotion to
  `shared/` writing a registry move line (area 18, question 8; reworks
  part of chunk 1.7's registry, a new manifest `format`);
  `semantic.duplicate_declaration` saying copy or clash and listing the
  names in use that contain the same word, its checked example rewritten.
  `jadpo rename` refusing a name in use comes with `rename` (after the
  first build, item 5).
- [x] **5.3e Constants** `opus · high` (area 3, "Constants", decided
  2026-10-16), after 5.3d: `constant name: Type = value` at the top of a
  file or in a library, `constant` reserved; its type named as a
  signature's is; its value worked out and checked while compiling, with
  calls, `config`, `clock` and cycles refused; `constant` in a body, `const`
  and `constant mut` each one error with an edit, and a top-level `var`'s
  `next:` pointing at `constant`; placement, promotion into a library and
  per-domain names as for helper functions; reads in bodies, queries (as
  literals) and updates by `id` (backfills, 5.6, take them as they come),
  but never a setting's `default:` or `test:`, which stay literals
  (revised 2026-10-16, Richard: a value is a constant or a setting, never
  both); the `constant.similar_exists` warning and constants in the
  library catalogue. Each new error with a checked example and real
  output, and area 3's proposal examples made check blocks. Split on
  2026-10-17: the moves are 5.3f.
- [x] **5.3f Making a constant a setting** `opus · high` (area 10, "Making
  a constant a setting" and "Fields in name order"), split from 5.3e:
  `jadpo config move <constant>` and the editor's "Make this a setting"
  code action through the same code: a `config` field of the constant's
  name, type, value as `default:` and description, the constant deleted,
  every read rewritten to `config.<name>`, refusing a type a setting can't
  hold, a name a setting has and a read in a query, an update by `id`, a
  backfill or another constant; `configuration.default_literal_required`'s
  `next:` naming `jadpo config move` when `default:` names a constant.
  `jadpo fmt` sorting `config { }` fields by name with their comments, and
  `--check` failing on one out of order. Area 10's proposal examples made
  check blocks.
- [x] **5.4 Standard library: the plan** `opus · high`, decided 2026-10-17,
  as recommended ([area 19](language/19-standard-library.md)). Only `collection`
  and `temporal` are specified, piecemeal across areas 2, 3 and 10, and
  real code keeps needing more: splitting and trimming text, formatting and
  rounding numbers, hashing, encodings. Collect what the decided areas and
  the example apps already call or will need, compare the standard
  libraries of TypeScript, Rust, Go, Elixir and Python, and write one new
  area page: the namespaces, every operation's signature and failures,
  which have a SQL form (for queries and backfills), and what is left out
  on purpose. **Needs Richard** to decide the page before 5.5.
- [x] **5.4b Declinable errors** `opus · high` (area 1, question 11,
  decided 2026-10-17): no warning severity; the generic `decline <rule>:
  "<reason>"` line above any declaration, with `decline.unused` and
  `decline.not_declinable`; `jadpo declines list` and `clear` (`--rule`, a
  path, `--json`) and the editor's decline actions on the same code;
  `jadpo approval` listing every decline once it is built;
  `library.similar_exists` and `constant.similar_exists` turned into
  declinable errors, each error's checked example rewritten; and the
  time-triggered rule (judged at today's date by `check`, at the commit's
  date by `jadpo build`) for expired flags when they are built. `warning`
  leaves area 14's error shape and the CLI design language's headline
  words.
- [x] **5.4c Translations as messages** `sonnet · high` (area 10,
  "Translations as messages", decided 2026-10-16 in the packages design
  while 5.3b built JSON files of ICU MessageFormat; reworks 5.3b, whose
  files, calls and `translation.*` errors it replaces). Split on
  2026-10-17 because the whole is more than one session: this half is the
  language itself, `5.4d` the tooling and the text Jadpo writes. Done in
  this half: `type X = Message` and the built-in `PluralCount`; each
  domain's and `shared/`'s `translations/` folder, with one unnamed
  `translations { }` per locale and its mapping and checks (placeholders,
  enum values, CLDR plural forms, every supported locale complete,
  `in_progress:` in `locales { }` as the declared opt-out: an in-progress
  locale falls back to its regional sibling and then the default with no
  diagnostic); the typed calls
  `translations.<domain>.<message>(..., locale:)` returning
  `PresentationText`; enum wording; email subjects; and the removal of
  5.3b's JSON files, calls and `translation.*` errors, with the section
  of area 10 that described them.
- [x] **5.4d Translations: failures, review and commands** `sonnet · high`
  (area 10, "Translations as messages", the rest of 5.4c's text). Split on
  2026-10-17 because the whole is more than one session: this half is the
  app's own text, `5.4e` Jadpo's and `5.4f` the exchange with translation
  services. Done in this half: failures that can reach a response worded in
  every locale (by code, `client:` fields as placeholders formatted by the
  client) and exported as `<api>.failures.<locale>.json` beside the OpenAPI
  documents; stale translations through each folder's
  `sources.jadpo_record` (the first `.jadpo_record` built),
  `TranslationReview` and `accept`; `TextDirection` and
  `locale.direction(value)`; `jadpo translations missing` (which also lists
  what an in-progress locale falls back on), `unused`, `update` and
  `accept`; go to definition from a call to its message type and its
  wording; and the pseudo-locale `Locale.en_xa`. Email templates follow in
  7.3, library packages' translations in 7.1.
- [x] **5.4e Translations: Jadpo's text** `sonnet · high` (area 10,
  "Jadpo's and packages' text" and "Tooling"), after 5.4d. Split on
  2026-10-17 because the whole is more than one session: this half is
  Jadpo's own words, `5.4f` the exchange with translation services. Done:
  Jadpo's own messages as a catalogue in `jadpo-std`, shipped in `en_gb` and
  `en_us`, gated by the features an app has on (requests, signing in, locales;
  magic links catalogued, not on until area 8); the root folder as a place
  (`translations/<locale>.jadpo`, `jadpo: { }`), checked as a folder's are,
  with `translation.jadpo_missing` (one error for each locale Jadpo does not
  ship), `translation.feature_off` and `translation.owner_unknown`; Jadpo's
  failures in every API's failures export; `TextDirection`'s `ltr` and `rtl`
  wording; the root folder's `sources.jadpo_record` (members
  `jadpo.<message>`), listed by `missing` and written by `update` and
  `accept`; and `jadpo translations builtin`.
- [x] **5.4f Translations: the exchange** `sonnet · high` (area 10,
  "Tooling"), after 5.4e. Done: `jadpo translations export` writes XLIFF 2.0
  (or Chrome-style JSON) for each translated locale, or the default
  locale's source file, with each message's description and placeholders;
  `import` reads XLIFF 2.x or JSON, checks the result with `jadpo check`'s
  own rules, and writes the language files and the record or refuses whole
  with `translation.exchange_unit_unknown`, `translation.exchange_file_invalid`
  or the language files' rule at the unit of the file; exporting then
  importing unchanged writes nothing. Lokalise's reading of XLIFF 2.0 and of
  the Chrome JSON shape is unverified (area 10, "Chosen in chunk 5.4f").
- [x] **5.5a The standard type contract** `opus · high` (area 19, "Every
  standard type meets one contract", questions 1 to 5): each standard type
  a stub declaration in `crates/jadpo-check/std/` plus a data entry,
  generating construction, `text.parse`, `text.format`, `text.from`, JSON,
  storage, `==` and order; named validators; one test harness for every
  type; `temporal.format` moved under `text.format` with its habit error;
  `type.standard_exists` on app code, declinable through 5.4b. Done
  when the 13 built standard types run through the contract and the
  harness, area 10's formatting examples use `text.format`, and
  `type.standard_exists` has a checked example with real output. Split on
  2026-10-17 because the whole is more than one session: the clash check
  and `jadpo fix` are 5.5a2.
- [x] **5.5a2 Standard type clashes and `jadpo fix`** `opus · high` (area
  19, question 4), split from 5.5a: each data entry's release; the first
  `check` after an upgrade reporting each app type that clashes with a
  standard type the release added, naming the type and the release, with
  the swap (as `type.standard_exists`'s) and the rename (`jadpo rename`)
  as its fixes; `jadpo fix` applying the chosen one through the editor's
  quick-fix code, designed with the skills in `.agents/skills/` first;
  release notes listing added names. Done when a clash has a checked
  example with real output and `jadpo fix` has its CLI tests.
- [x] **5.5a3 Editor completion** `opus · high` (area 14, "Editor
  support", "Completion"; Richard, 2026-10-17): `jadpo lsp` answers
  completion from the declarations and data the checker uses, never a list
  of its own: names in scope with the `use` line a name needs, a value's
  fields, a namespace's operations with signature, failures and docs, enum
  variants with CLDR names, units straight after a number narrowed to the
  type it goes into, named arguments and record fields not given yet, type
  names, declinable rules after `decline`, `config.` settings,
  `translations.<domain>.` messages and keywords; what fits the required
  type first; calls with required arguments as snippets; VS Code triggers
  on `.`, `:` and digits. Done when each context has an LSP-level test and
  an operation added to a stub is offered with no completion code.
- [x] **5.5b Lists** `opus · high` (area 19, "Lists", questions 6, 7 and
  11): `collection.` renamed `list.` everywhere, with its habit error; the
  21 new list operations that need no language change; `never_negative`,
  the `sort_by` key's ordered kinds, results that keep a named type. Done
  when every list operation's page example passes as a test, interpreted
  and in the runtime, and no `collection.` call is left in the pages,
  examples or tests. `group_by`'s page test reads its map once
  `map.get` exists (5.5c); what the pages left open is area 19's "Chosen
  in chunk 5.5b".
- [x] **5.5c Sets, maps and ranges** `opus · high` (area 19, "Sets",
  "Maps", "Ranges" and "Language gaps", question 8): the 7 `set`
  operations, the 8 `map` operations that take no inline function, and the
  3 `range` operations on `InstantRange`; `for` over a set in canonical
  order, and so over a map's keys; `[]` as an empty map; `range` reserved
  and area 10's `deliveries_in(range:)` renamed. Done when each
  operation's example passes, interpreted and in the runtime, area 19's
  `by_shelf` (`list.group_by`) has a test reading its map, and walking a
  set or a map's keys checks and runs. Split on 2026-10-17 because the
  whole is more than one session: two-parameter inline functions and
  `Range<T>` are 5.5c2. An `InstantRange` object literal now checks and
  runs, which the range examples needed; what the pages left open is area
  19's "Chosen in chunk 5.5c".
- [x] **5.5c2 Two-parameter inline functions and `Range<T>`** `opus ·
  high` (area 19, "Lists", "Maps", "Ranges" and "Language gaps", split
  from 5.5c): `(a, b) -> ...` for standard operations only (area 3's
  question 13 widened), with `list.zip_with`, `map.filter` and
  `map.map_values`; `Range<T>` on the contract, with open ends, and the 3
  `range` operations taking it. Done when each of those operations'
  examples passes, interpreted and in the runtime, and a two-parameter
  inline function anywhere else is an error with a checked example. A
  function with the wrong number of parameters is a new rule,
  `type.inline_function_parameters`; what the pages left open is area
  19's "Chosen in chunk 5.5c2".
- [x] **5.5d Numbers** `opus · high` (area 19, "Numbers", question 25): the
  8 new `integer` and `decimal` operations, `scale:` written out, and
  `Rounding.down`. Done when each operation's example passes and every
  rounding mode gives the same result interpreted and in the runtime.
  `text.format` shows numbers too; what the pages left open is area 19's
  "Chosen in chunk 5.5d".
- [x] **5.5e Decimal maths** `opus · high` (area 19, "Numbers", question
  18): `sqrt`, `power`, `exp`, `ln`, `sin`, `cos`, `atan2` and `pi` on
  `Decimal`, correctly rounded to a named scale by Jadpo's own code. Done
  when each is tested against reference values and gives the same digits
  on native and WebAssembly.
- [x] **5.5f Text** `opus · high` (area 19, "Text", questions 23, 26, 29
  and 30): the 17 text operations, `locale:` on case and sorting through
  ICU4X, NFC at the boundary, `never_empty`, `fill:` and
  `at_most_length_of`; `text` reserved and the clashing page examples and
  crate snippets renamed. Done when each operation's example passes,
  including the Unicode edge cases, and `text` can't name a variable.
  Empty text where it never is is a new rule, `type.argument_empty`;
  what the pages left open is area 19's "Chosen in chunk 5.5f".
- [x] **5.5g Hashing, encodings and JSON** `opus · high` (area 19,
  "Hashing and cryptography", "Encodings" and "Data formats", questions
  14 and 24): `hash.sha256`, the 9 `encoding` operations, `json.read` and
  `json.write` in memory with `mismatch`'s problem path. The batch forms
  and `csv`, `tsv` and `json_lines` come with batches (6.4). Done when
  each operation's example passes and `json.write` gives the same text
  for the same value on every target. Split on 2026-10-17 because the
  whole is more than one session: the `DecodeProblem` a failure mapped
  from `mismatch` receives is 5.5g2. The wire's `Bytes` is now read as
  `encoding.from_base64` reads it, padded only; what the pages left open
  is area 19's "Chosen in chunk 5.5g".
- [x] **5.5g2 The decode problem on a failure** `opus · high` (area 19,
  "Data formats", `json`; split from 5.5g): the built-in `DecodeProblem`
  (the first bad field's path, list positions as numbers, and the rule,
  never the value), and a failure mapped from `json.read`'s `mismatch`
  that declares a `problem: DecodeProblem` field receiving it, in the
  failure's fields, its client JSON and its log. Done when a mapped
  failure's `problem` names `lines.2.quantity` and `max` for that input,
  interpreted and in the runtime, with a checked page example. Only the
  runtime builds a `DecodeProblem` (`type.not_record`, now
  `IssuedToken`'s too); the log is the failure's report until the hosts
  write area 4's event; what the pages left open is area 19's "Chosen in
  chunk 5.5g2".
- [x] **5.5h Money** `opus · high` (area 19, "Money and payments" and "The
  standard operator table", question 16): `Money` and `Currency`, the 7
  `money` operations other than `convert`, the operator table's `Money`
  rows with `currency_mismatch`, `rounding:` and `type.money_split` (the
  rows with `Percentage` come with it, in 5.5j), the two columns, the
  wire shape, `text.format`, and area 2's examples that declare their
  own `Money` renamed. Done when the money examples pass, interpreted, in
  the runtime and stored in PostgreSQL, a mismatched currency fails to
  compile where both types fix it, and `split` and `allocate` never lose
  a minor unit. Split on 2026-10-17; the rest is 5.5h2. Stored always in
  two columns, with a check constraint for a fixed currency; what the
  pages left open is area 19's "Chosen in chunk 5.5h".
- [x] **5.5h2 Exchange rates and money in messages** `opus · high` (area
  19, "Money and payments", question 16, split from 5.5h): `ExchangeRate`
  and `money.convert`, applying a rate the app holds and failing
  `currency_mismatch` for another currency's rate; `{price, number,
  currency}` in translation messages (area 10's placeholder styles). Done
  when `convert`'s example passes, an inverted rate is a type error, and
  a message writes a price in two locales. Built, with
  `translation.placeholder_style`; what the pages left open is area 19's
  "Chosen in chunk 5.5h2".
- [x] **5.5h3 Definition before use** `sonnet · high` (area 1, "Files and
  names"; Richard, 2026-10-17): `jadpo fmt` orders each file as decided:
  opening comment, `use` lines, then every declaration before the first
  one that uses it, the primary declaration last, ties by kind (constants,
  types, failures, helpers) then by the order the primary first reaches
  them, mutual references in written order; `jadpo fmt --check` failing on
  a file out of order; chunk 3.9's "primary first" move removed; every page
  example, example app and test fixture reformatted. Done when the
  ordering has tests for each tie rule and a cycle, and the gate's docs and
  examples steps pass on the reordered sources.
- [x] **5.5i Payment and identity numbers** `opus · high` (area 19, "Groups
  with the baseline only"): the 17 of the money group and the tax and
  identity numbers that are text, each a pattern or check digit (mod-97,
  Luhn, mod-11) on the contract: `PaymentCardNumber`, `CardBin`,
  `PaymentReference`, `InvoiceNumber`, `Iban`, `Bic`, `UkSortCode`,
  `UkAccountNumber`, `AbaRoutingNumber`, `Lei`, `Isin`, `UkUtr`,
  `UkCompanyNumber`, `UkNationalInsuranceNumber`, `UsSsn`, `UsEin` and
  `NhsNumber`. Done when every type passes the contract's harness with
  valid and invalid examples per rule. Split on 2026-10-18; the rest is
  5.5i2. What the pages left open is area 19's "Chosen in chunk 5.5i".
- [x] **5.5j Quantities** `opus · high` (area 19, "Physical quantities",
  question 17): the 17 physical quantities with `DataSize` and `Bitrate`,
  each with its unit enum; SI storage at each kind's scale in one
  `numeric` column; the wire's `{ "value": ..., "unit": ... }`; suffixed
  literals; the product and quotient table; `type.unit_required`; the 3
  `units` operations and `text.format` in a chosen unit through the
  locale. Done when the quantity examples pass, interpreted, in the
  runtime and stored in PostgreSQL, `1km == 1000metre`, `2metre + 45cm +
  3inch` gives the right metres and is shown in centimetres in two
  locales, and a bare number or a mixed kind is a compile error with a
  checked example. Split on 2026-10-18; the rest is 5.5j2. Built, with
  `type.unit_unknown` and quantities compared and ordered in queries;
  what the pages left open is area 19's "Chosen in chunk 5.5j".
- [x] **5.5j2 Percentages, ratios and fractions** `opus · high` (area 19,
  "The standard operator table", questions 16 and 17; split from 5.5j):
  `Percentage`, `Ratio` and `Fraction` on the contract; the `%` literal,
  with remainder keeping its spaced `%` and `20%3` an error; the operator
  table's `Percentage` rows, with `Money * Percentage`, the refused
  `Money + Percentage` (5.5h built `Money` by a number) and a number or
  quantity times a `Percentage` keeping its kind; `Percentage(ratio)`,
  0.25 → 25%; `{n, number, percent}` in messages. Done when the
  percentage examples pass and the showcase `var total: Length = 2metre
  + 45cm + 3inch` then `var trim: Length = total * 15%` gives the right
  metres, shown in centimetres in two locales. Built, with
  `syntax.percent_ambiguous`, a `Mass` held to the microgram so `1mcg` is
  exact (a correction by the lead session), and a percentage stored as
  its fraction and sent as `"20%"`; what the pages left open is area 19's
  "Chosen in chunk 5.5j2".
- [x] **5.5k Time and calendar** `opus · high` (area 19, "Time and
  calendar additions"): ten of the 13 new time types (`LocalTime`,
  `LocalDateTime`, `Month`, `YearMonth`, `Year`, `Quarter`, `IsoWeek`,
  `UtcOffset`, `DateRange`, `CalendarPeriod`), `CalendarPeriod`'s
  operator rows, `temporal.period_between`, and `DateRange` taken by the
  `range` operations. Done when each of the ten passes the harness and
  `period_between`'s example passes across a daylight-saving change.
  Split on 2026-10-17; the rest is 5.5k2.
- [x] **5.5k2 Recurrence and opening hours** `opus · high` (area 19,
  "Time and calendar additions"): `RecurrenceRule` (RFC 5545) with its
  expansion, `temporal.occurrences`; `OpeningHours` with
  `temporal.open_at`. Done when each passes the harness and the two
  operations' examples pass across spring forward and fall back in
  Europe/London and America/New_York, interpreted and in the runtime.
  Split on 2026-10-18; `CalendarEvent` is 5.5k3. Both held as their
  standard text; what the pages left open is area 19's "Chosen in chunk
  5.5k2".
- [x] **5.5k3 Calendar events** `opus · high` (area 19, "Time and
  calendar additions", split from 5.5k2): `CalendarEvent` as an object
  whose parts code reads, with parts that may be `none` (`Part::Optional`,
  built in 5.5l), and an all-day event's dates beside a timed one's
  local times and zone; its canonical text an RFC 5545 `VEVENT` (an
  all-day end exclusive, as the RFC writes it), its JSON its parts, its
  recurrence a `RecurrenceRule`. Done when it passes the harness and an
  all-day event's text, JSON and storage round-trip without moving a
  day. Built, its rules across parts (days or times, a recurrence only
  on a timed event and from its start) proved for a literal and checked
  as text and JSON are read, and stored a column per part (checked in
  PostgreSQL); what the pages left open is area 19's "Chosen in chunk
  5.5k3".
- [ ] **5.5k4 Local times to instants, and all-day recurrence** `opus ·
  high` (areas 10 and 19; decided by the lead session, 2026-10-18, from
  5.5k3's findings): `temporal.resolve` taking a `LocalDateTime` (and a
  `CalendarDate` with a `LocalTime`) with a zone, under area 10's existing
  `dst:` choice, so a timed `CalendarEvent`'s start becomes an `Instant`;
  and `RecurrenceRule` accepting a date-only start, so an all-day event
  recurs (birthdays, anniversaries) without a zone. Done when an event's
  start resolves across both clock changes with each `dst:` choice, and a
  yearly all-day event on 29 February follows RFC 5545's skipping rule.
- [x] **5.5l People and places** `opus · high` (area 19, question 22): 15
  of the 19 new types: the closed lists (`Country`, `CountrySubdivision`,
  `Continent`, `Language`, `WritingScript`, `TextDirection`,
  `Nationality`) as built-in enums with `form:`, `.all` and retired codes
  (`type.variant_retired`, with the currencies 5.5h left out); `GivenName`,
  `FamilyName`, `Username`, `PhoneExtension` and `Uprn`; and `PersonName`,
  `EmailWithName` and `PostalAddress` as objects with optional parts,
  stored in named columns, `PostalAddress` and `PersonName` in each
  locale's order. Done when every type passes the harness and
  `text.format` writes an address in two locales' orders and a name in
  two orders. Split on 2026-10-18; the rest is 5.5l2. Built, with
  `type.variant_retired` and `type.reserved_variant_name`, the lists from
  CLDR 48.2, ISO 639 and ISO 15924, names by CLDR's person-name patterns
  and addresses by libaddressinput's formats, an object stored in a column
  per part (checked in PostgreSQL); what the pages left open is area 19's
  "Chosen in chunk 5.5l".
- [x] **5.5l2 Phone numbers and postal codes** `opus · high` (area 19,
  "Groups with the baseline only", split from 5.5l): `PhoneNumber` with
  libphonenumber's E.164 metadata, `CallingCode` against the assigned
  codes, `PostalCode` per country (and `PostalAddress`'s postal code
  checked by its country), and `UnLocode` against its list. Done when
  each passes the harness with valid and invalid examples per country
  rule. Built, with libphonenumber v9.0.41's metadata (valid, not only
  possible), libaddressinput's patterns and `country:` on `text.parse`;
  `UnLocode` is checked by pattern and country only, since the
  UN/LOCODE list's terms are unclear and **needs Richard** to say whether
  it ships. What the pages left open is area 19's "Chosen in chunk
  5.5l2".
- [ ] **5.5h4 Exchange rate types that fix their currencies** `opus ·
  high` (area 19, "Money and payments"; decided by the lead session,
  2026-10-18, following decision 16), after 5.5h2: a type built on
  `ExchangeRate` may fix its currencies, `type PoundsToEuros =
  ExchangeRate { from: Currency.gbp, to: Currency.eur }`, as a `Money`
  type fixes its currency, so `money.convert` with an inverted or wrong
  rate is a compile error across function boundaries, not only after a
  condition; a rate read at the boundary is checked against them. Purely
  additive. Done when an inverted typed rate passed through a function
  fails to compile with a checked example, and a fixed-currency rate needs
  no `attempt`.
- [x] **5.5i2 Cards and identity documents** `opus · high` (area 19,
  "The standard types", split from 5.5i), after 5.5l: `CardBrand`,
  `MaskedCardNumber` with its conversion from a `PaymentCardNumber`, and
  `CardExpiry`; `VatNumber`, `TaxId`, `PassportNumber`,
  `DrivingLicenceNumber` and `IdentityDocument`, on `Country` (5.5l), each
  checked by its country's published format (at least GB, DE, FR, IE, US
  and one country outside Europe), by length where none is published.
  Done when every type passes the harness with valid and invalid
  examples per rule. Split on 2026-10-18; the rest is 5.5i3. Built, with
  `money.mask` for the conversion and `IdentityDocumentKind` as the
  document's kind; forms from VIES, HMRC, the OECD's and the EU's TIN
  pages and each issuer's own, by length where a country publishes none.
  What the pages left open is area 19's "Chosen in chunk 5.5i2".
- [x] **5.5i3 Prices, discounts and rates** `opus · high` (area 19, "The
  standard types", split from 5.5i2), after 5.5j2: `Price`, `UnitPrice`,
  `TaxRate`, `Discount`, `BasisPoints` and `InterestRate`, on `Money`,
  `Percentage` and the quantities. `Discount` is the first standard type
  that is a choice of other types: the choice goes into the contract
  generically (a stub and a data entry, no per-type code path), for
  `BankAccount` and later choices to reuse. Done when every type passes
  the harness with valid and invalid examples per rule, and a `Discount`
  applied to a `Price` gives the right result for both kinds, interpreted
  and in the runtime. Split on 2026-10-18; `BankAccount` is 5.5i4.
  Built, with `money.apply_discount` for applying a discount,
  `InterestRateBasis` as an interest rate's basis, `UnitPrice` a choice by
  the quantity's kind and `BasisPoints` one of 5.5n's whole numbers;
  storing the four records waits on area 5's structured storage, as an
  `ExchangeRate`'s does. What the pages left open is area 19's "Chosen in
  chunk 5.5i3".
- [x] **5.5i4 Bank accounts** `opus · high` (area 19, "The standard
  types", split from 5.5i3), after 5.5i3: `BankAccount`, a choice of a UK
  sort code and account number, an IBAN, or another country's account,
  on 5.5i3's choice; the UK modulus check across a sort code and an
  account number by Vocalink's published specification (version 9.10, 5
  Sept 2026), with all 34 of its Chapter 3 test cases, cited; masked as
  financial personal data (area 15), as 5.5i did. Done when it passes the
  harness with valid and invalid examples per rule and all 34 Vocalink
  cases pass.
  Built, the check passing all 34 cases; Vocalink publishes its weight
  and substitution tables with all rights reserved, so no release ships
  them and no `BankAccount` runs the check yet, which the specification's
  own rule for a sort code its table doesn't cover allows. Whether they
  ship is Richard's question. What the pages left open is area 19's
  "Chosen in chunk 5.5i4".
- [x] **5.5m Commerce, health, travel and the rest: codes and lists**
  `opus · high` (area 19, "Groups with the baseline only"): the 29 of the
  45 that are codes or closed lists. Held as text with a pattern and,
  where the page names one, a check digit: `Sku`, `Ean13`, `UpcA`,
  `Isbn`, `Sscc`, `HsCode`, `Carrier`, `ContainerNumber`, `LotNumber`,
  `SerialNumber`, `Vin`, `Icd10Code`, `SnomedConcept`, `IataAirport`,
  `IcaoAirport`, `IataAirline`, `BookingReference`, `UnNumber`, `Orcid`,
  `Mpan` and `Mprn`; and the closed lists `Incoterm`, `BloodType`,
  `Allergen`, `CabinClass`, `CefrLevel`, `EpcRating`, `Tenure` and
  `LawfulBasis` as built-in enums. Done when every type passes the
  harness with valid and invalid examples per rule. Split on 2026-10-18;
  the records and measures are 5.5m2. Built, the check digits GS1's,
  ISO 6346's, ISO 7064's, Verhoeff's, an MPAN's and, for North American
  manufacturers, a VIN's; IATA's and ICAO's codes and ICD-10 are checked
  by pattern only, since their lists can't be shipped, and a list CLDR
  doesn't name is shown by its authority's English wording; the ISBN-10
  conversion came with 5.5m2. What the pages left open is area 19's
  "Chosen in chunk 5.5m".
- [x] **5.5m2 Commerce, health, travel and the rest: records and
  measures** `opus · high` (area 19, "Groups with the baseline only",
  split from 5.5m): the other 16, each a record of other types or a
  measure: `Barcode`, `UnitQuantity`, `Rating`, `ClothingSize`,
  `ShoeSize`, `Dimensions`, `TrackingNumber` (on 5.5m's `Carrier`),
  `VehicleRegistration`, `Dose`, `DosageInstruction`, `HeartRate`,
  `BloodPressure`, `BloodGlucose`, `FlightNumber` (on 5.5m's
  `IataAirline`), `Jurisdiction` and `ModelId`; and the contract's ISBN-10
  conversion, `text.parse(raw, as: Isbn, form: CodeForm.isbn10)`, reading
  only, consistent with `Country`'s `form:` (decided by the lead session,
  2026-10-18). Done when every type passes the harness and an ISBN-10
  reads as its `Isbn`, a bad check digit `malformed`. Built, with the
  measures on `Frequency`, `Pressure` and `Concentration` and shown in
  the clinician's units, from four new units with no literal suffix
  (millimetres of mercury, beats a minute, milligrams a decilitre and
  millimoles of glucose a litre, by CLDR's conversions); `Dose` by mass or
  volume, `DosageInstruction` as FHIR `Dosage`'s commonest form, the size
  systems the UK's, Europe's and the US's, and `ClothingSizeSystem`,
  `ShoeSizeSystem` and `RouteOfAdministration` beside the 16, for the lead
  session to accept. What the pages left open is area 19's "Chosen in
  chunk 5.5m2".
- [x] **5.5n Web and network** `opus · high` (area 19, "Web and
  software"): the 20 new web and network types (`UrlPath`, `RelativeUrl`,
  `Slug`, `DomainName`, `Hostname`, `WebOrigin`, `HttpMethod`,
  `HttpStatus`, `HttpHeaderName`, `UserAgent`, `ProblemDetails`,
  `JsonPointer`, `Cookie`, `UtmParameters`, `Cidr`, `NetworkPort`,
  `MacAddress`, `Imei`, `Emoji`, `PushToken`) and `web`'s 3 operations.
  Done when every type passes the harness and the three operations'
  examples pass, interpreted and in the runtime. Split on 2026-10-18; the
  software group is 5.5n2. Built, with UTS 46 by Servo's `idna` 1.1.0 on
  the pinned ICU4X, Emoji 18.0's list, `HttpStatus` and `NetworkPort` held
  as whole numbers, and `ProblemDetails` sent as RFC 9457 sends it
  (`type_` in code); `HttpMethod` keeps the five route methods (decided
  by the lead session, 2026-10-18). What the pages left open is area
  19's "Chosen in chunk 5.5n".
- [x] **5.5n2 Software** `opus · high` (area 19, "Web and software",
  split from 5.5n): the 18 software types (`SemanticVersion`,
  `VersionRange`, `GitRef`, `PackageUrl`, `Cve`, `SpdxLicence`, `TraceId`,
  `SpanId`, `TraceParent`, `LogLevel`, `FilePath`, `FileName`,
  `RegularExpression` on area 2's linear-time engine, `ContainerImage`,
  `Digest`, `Ulid`, `IdempotencyKey`, `Doi`) and `semver.satisfies`. Done
  when every type passes the harness and `satisfies`' examples pass,
  SemVer 2.0.0's pre-release rules included, interpreted and in the
  runtime. Built, with `SemanticVersion` an object ordered
  by SemVer's precedence, `VersionRange` npm's range grammar in one
  spelling with node-semver's pre-release rule, and the SPDX License List
  3.29.0 from Embark's `spdx` crate 0.13.6; `LogLevel` is OpenTelemetry's
  six severities, and area 10's own `LogLevel` example is `Verbosity`.
  What the pages left open is area 19's "Chosen in chunk 5.5n2".
- [x] **5.5o Text formats** `opus · high` (area 19, "Text formats"):
  `Html` (whose construction is the sanitiser, a security boundary) and
  `Markdown`, with `markup.to_html` and `markup.to_plain_text`. Done when
  the examples pass, interpreted and in the runtime, and a sanitiser test
  suite of script, event-handler, link and mutation attacks comes out
  safe. Split on 2026-10-18; `RichText` and `LocalisedText` are 5.5o2.
  Built on ammonia 4.2.4 over html5ever, with a conservative allow-list
  (links only to `https:` and `mailto:`, no images), output that is a
  fixed point and a nesting limit of 512; every way in sanitises and none
  fails, and an `Html` literal must already be safe. Markdown is
  CommonMark by pulldown-cmark. ammonia brings `cssparser` (MPL-2.0), the
  build's first MPL code, accepted by the lead session (2026-10-18).
  What the pages left open is area 19's "Chosen in chunk 5.5o".
- [x] **5.5o2 Rich and localised text** `opus · high` (area 19, "Text
  formats", split from 5.5o), after 5.5o: `RichText`, a structured
  document of blocks and marks, with `markup.rich_to_html` through 5.5o's
  sanitiser; and `LocalisedText`, locales mapped to text, with
  `text.pick` taking the locale's entry, else its language's, else the
  default, as area 10's fallback chain does. Done when both pass the
  harness and `rich_to_html`'s and `pick`'s examples pass, interpreted and
  in the runtime. Built: `RichText` is Portable Text's flat shape with its
  default schema, a named list of blocks whose parts `RichTextBlock`,
  `RichTextSpan` and `RichTextMark` are standard types beside the 247
  (accepted by the lead session), links only to what the sanitiser keeps, so `rich_to_html` writes
  nothing it would change; `LocalisedText` is a named `Map<Locale, Text>`
  sent as a JSON-LD language map, its keys `Locale`s like any other.
  What the pages left open is area 19's "Chosen in chunk 5.5o2".
- [x] **5.5p Media and colour** `opus · high` (area 19, "Colour" and
  "Media", question 13): the 15 types, `colour`'s 5 operations and
  `media.fit`. Done when each operation's example passes and `contrast`
  matches WCAG 2's reference ratios. Built, with CSS Color 4's conversions
  and CSS Color 5's `color-mix()` in Jadpo's own fixed point, `contrast`
  rounded down to 2 places, `media.fit` as `object-fit: scale-down`, and
  `ContentRatingScheme` (BBFC, MPA, PEGI, ESRB) beside the 15, accepted by
  the lead session (2026-10-18). What the pages left open is area 19's "Chosen in
  chunk 5.5p".
- [ ] **5.5q Randomness and cryptography** `opus · high` (area 19,
  "Randomness" and "Hashing and cryptography", questions 9 and 19): the
  generated `random` namespace, actions only, seeded per test and
  recorded for replay; `crypto`'s 8 operations that need no keys held as
  data; `EncryptionKey`, `PasswordHash`, `PasswordCheck`, `PrivateKey`,
  `PublicKey` and `key:` as a compiler-owned place. Done when the
  examples pass, a test's random values repeat on a rerun, and reading a
  key anywhere but `key:` is `configuration.secret_flow`.
- [ ] **5.5r Security types** `opus · high` (area 19, "The standard
  types"): the other 17 security types (`Password`, `Jwt`, `Jwk`,
  `Certificate`, `ApiKey` ...) and `crypto.verify_jwt`; `verify_totp`
  follows keys held as data. Done when every type passes the harness and
  `verify_jwt` refuses `none`, a token-chosen algorithm and an expired
  token.
- [ ] **5.5s Geospatial and AI types' baselines** `opus · high` (area 19,
  "The standard types"): the 17 geospatial types and `Vector`,
  `Embedding`, `TokenCount`, `TokenUsage` and `ChatMessage`, with
  construction, text and JSON from the contract; storage and operations
  come with their own areas. Done when every type passes the harness's
  construction, text and JSON cases.
- [ ] **5.5t Cross-field rules** `opus · high` (area 2, decided by area 19,
  question 15): named rules across an object type's fields (`rule
  ends_after_start: ends > starts`), proved or attempted like any rule.
  Design the syntax and errors on area 2's page first. Done when a
  booking whose end is before its start fails construction with the
  rule's name, a proved one needs no arm, and each new error has a
  checked example.
- [ ] **5.5u Geospatial: the plan** `opus · high` (area 19, question 12):
  write the geospatial area page from area 19's scope (the types in
  WGS84, GeoJSON on the wire, `geo.` construction and operations, PostGIS
  `geography` storage, query conditions and GiST indexes), and place its
  build on this roadmap. **Needs Richard** to decide the page.
- [ ] **5.5v AI: the plan** `opus · high` (area 19, question 20): write the
  AI area page from area 19's scope (the `ai` capability, typed
  `ai.generate`, streaming, embeddings with pgvector, checked prompt
  files, a fake for tests, limits, area 15's report), and place its build
  on this roadmap. **Needs Richard** to decide the page.
- [ ] **5.5w Every boundary through the contract** `opus · high` (area 19,
  "Every standard type meets one contract"; Richard, 2026-10-17), after
  the standard type groups: audit every place a value enters or leaves a
  program (HTTP and OpenAPI, storage and migrations, caches, events and job
  payloads, CSV, TSV and JSON lines, logs and traces, settings, translation
  placeholders, email, outbound provider calls, records and the manifest,
  test fixtures and seeds, files, the interpreter and the runtime on native
  and wasm32) and make each encode a standard type only through its
  contract's JSON, storage and text shapes, so no type loses its currency,
  unit or precision on the way out. Compound types store as named columns
  (`Money` as amount and currency, the currency kept and checked even when
  the type fixes it), quantities as one SI-unit column with the unit in
  the column's comment, ranges as native range types. Every standard
  object and choice type (`Price`, `Discount`, `UnitPrice`,
  `InterestRate`, `ExchangeRate` and the rest, which storing now fails
  with `toolchain.not_built_yet`) is made storable through the named-column
  mechanism or its decided storage shape. Done when a test per
  boundary round-trips one value of every standard type, an entity stores
  and reads back one of each standard object and choice type, and any
  boundary with its own encoder is gone. A read-only audit (2026-10-17) found what
  to fix, beyond what 5.5h fixes for Money (OpenAPI for standard object
  types, enum spellings from `entry.codes()`, storage as named columns):
  `json.write`'s map keys use debug text and don't read back
  (`jadpo-value/src/json.rs` `key_text`); the JSON, schema and enum encoders
  match on builtins instead of reading the entries; enums are stored by
  variant name but sent by code; reading is looser than `text.parse` (an
  `Instant` with an offset, a `CalendarDate` not checked for 30 February);
  settings take `5m` but not `PT5M`; a `Decimal` quantity's `value` must
  follow area 2's Decimal-as-string rule; and nothing yet decides the
  encoding of event, job and batch payloads (with a version rule), CSV
  cells for types with no single text form, the bytes a personal value's
  log hash covers, developer reports' value form, and integration fields
  without `wire:`. Decide each of those on its owning page, then build.
- [ ] **5.5x `jadpo suggest`** `opus · high` (area 14; decided by Richard,
  2026-10-17): first write the command's section on area 14 from this
  design, then build it. A command that hints, never edits, where code
  could use a standard type or operation; edits stay with the agent and,
  for type clashes only, `jadpo fix`. `jadpo suggest` formats, then scans
  the program graph with signals held as data on each standard type's
  contract entry (shape, conversion constants, name hints, format literals,
  validation patterns), matched through canonicalised graph fragments with
  hashes as the index and a matcher to confirm; it reports numbered
  findings (S1, S2…) and ends with a broad instruction to the agent:
  review the rest of the code against `jadpo suggest --catalogue`
  (generated from the contract), run `jadpo suggest found` before
  changing anything it finds, and when done run `jadpo suggest share`. A snippet is the smallest statement or expression at the place, in
  canonical form: the structure normalised (operand order for commutative
  operations, comparison direction, grouping, constant spellings and named
  constants followed to their values, temporaries followed through the
  graph, exactly equivalent forms such as `x / 0.01` and `x * 100`) and the
  names kept, so `total_price * 100` and `100 * total_price` are one
  pattern. Signals match its structure and the words in its names, and an
  ignore holds the canonical form's hash, so every variant stays quiet.
  What is shared is simpler: a plain hash of the snippet's text after
  `jadpo fmt`, comments removed, so it never depends on the canonicaliser's
  version and only lines written the same way in many projects reach the
  threshold; Jadpo HQ canonicalises the texts it receives and merges the
  variants itself.
  - A finding that disappears after a fix is recorded as accepted, from
    the scan's own note of its last findings (local, untracked).
  - `jadpo suggest ignore S3 "<why>"` writes the hash, the signal and the
    reason to `ignored_suggestions.jadpo_record` at the project root
    (committed, shared by the team; area 1's root list gains it); the
    snippet stays quiet until its code changes, and stale entries drop out.
    It also adds the snippet's text, hash, signal and reason to the pending
    file as an `ignored` entry. It sends nothing.
  - `jadpo suggest found <file>:<line> <Type or operation> "<why>"`, run
    before the code is changed, captures the snippet's text, hash,
    canonical form, the suggestion and the reason, and teaches it as a
    project signal so the next scan finds the same pattern itself.
  - Every entry to share goes to `.jadpo/suggest_pending` (local,
    untracked) with its kind, `found`, `accepted` or `ignored`, so HQ can
    tell a signal that was right from one that fired wrongly.
  Done when every signal kind has a finding and a non-finding test, an
  ignored snippet stays quiet until edited, a found snippet's text survives
  the fix, and a taught miss is found by the next scan.
- [ ] **5.5y `jadpo suggest share`, against a fake Jadpo HQ** `opus ·
  high` (area 14; decided by Richard, 2026-10-17), after 5.5x: opt-in
  sharing, built in full against a local fake of Jadpo HQ that speaks the
  same protocol, since HQ doesn't exist yet. Sharing is on unless turned
  off (Richard, 2026-10-17): nothing is ever sent unless someone runs
  `share`, and snippet text always needs a person's approval. Creating a
  project writes `suggestions { project_token: "…" }` to `app.jadpo`, the
  token random and tied to nothing (no repo, user or machine), committed so
  a team counts as one project and HQ counts distinct projects without
  knowing them; `share` recreates it when it is missing, and `share
  --new-token` rotates it (a fork shares its parent's token until it does).
  `suggestions { share: false }` in `app.jadpo`, or `JADPO_SUGGEST_SHARE=off`
  in the environment for a whole machine or organisation, makes `share`
  stop at once saying sharing is turned off and where, and drops the
  report's instruction to share. `share` sends every hash in `.jadpo/suggest_pending` with its kind
  (`found`, `accepted` or `ignored`) and the token; HQ replies with the
  hashes whose text it wants (those seen from enough distinct projects).
  The hashes go at once, with no prompt; the agent runs `share` itself
  at the end of a suggest session, as the report's instruction says. The
  command then shows the snippets HQ wants and stops: a person approves,
  directly or through the agent asking them in its conversation, and only
  then does `share --approve` send the snippets, each with its kind and,
  for an ignored one, its reason (HQ recomputes their hashes). In an unattended run nobody approves, so only hashes go and the
  request is skipped; HQ asks the next project that sends that hash. The
  pending file is then wiped, wanted or not. The first time hashes are
  sent, `share` says so in one line and how to turn sharing off. The protocol is written on area 14 so the real HQ, after the
  first build with the registry, implements it unchanged, including its
  agent that turns shared patterns into signal data (thresholds, generated
  tests, release notes), reassesses a signal whose ignore rate across
  distinct projects is high, reading the ignored snippets and reasons
  beside the accepted ones, and narrows or removes it (tested against the
  accepted snippets so it keeps catching what it got right), and proposes,
  never builds, language features.
  Done when the whole exchange runs against the fake in tests, nothing is
  sent when sharing is off, no snippet text is sent without approval, a
  hash below the threshold never has its text requested, and an ignored
  snippet's text can be shared once approved.
- [ ] **5.6 Computed backfills** `opus · high` (area 5, "Computed backfills",
  2026-10-15): `backfill:` as an expression over each row's own fields,
  filled in batches, one `UPDATE` each where it has a SQL form and by
  `jadpo migrate apply` otherwise, then made required; with the SQL form
  of each standard operation (area 19, "SQL forms"), each tested against
  the runtime on the same inputs.
- [ ] **5.6c Aggregate queries** `opus · high` (area 6, decided by area 19,
  questions 21 and 28): `count`, `sum`, `min`, `max` and `average` in
  queries, grouped by a field or a time bucket, using the standard
  operations' SQL forms. Design the syntax and errors on area 6's page
  first. Done when a usage-per-month and a counts-by-status query each
  run as one SQL statement and give the same result as the interpreter.
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
- [ ] **6.4 Batches** `opus · xhigh` (area 12, decided 2026-10-16):
  `batches.each`, `batches.status`, `cancel` and `key:`, items grouped into
  transactions that commit their writes and position together, problems
  collected, resuming after crashes and deploys; replacing chained events
  for paging; then `csv.read`/`write`, `tsv`, `json_lines` and the batch
  forms of `json.read`/`write` on it, as area 19's "Data formats" tables
  them, and `store.read`'s
  whole-read cap. Split before starting if the format helpers make it more
  than one session.
- [ ] **6.5 Teaching examples for Phase 6** `opus · high`: events,
  schedules and batches, passing `--require-examples`.

## Phase 7 — Packages, capabilities and email (areas 11, 16)

The design is area 11 as revised on 2026-10-16, with its research in
[packages and extension points](language/research/packages-and-extension-proposals.md).

- [ ] **7.0 Generated records** `opus · high` (area 18, decided
  2026-10-16): the API record and identity registry renamed to
  `.jadpo_record` in the decided record format, with the header line, the
  hand-edit check and `jadpo records`; the format 7.1's package surfaces
  and 5.4c's translation sources also use, as
  [the design](language/research/jadpo-record-format.md) sets out (decided
  2026-10-16): the API record loses its `version:` counter, the OpenAPI
  version coming from a hash of the record, and `jadpo.lock` takes the
  format.
- [ ] **7.1 Packages and connections** `opus · high`: the package format
  (`package.jadpo`, `provider.jadpo`, folders, `public` and the public
  surface rule), library and provider kinds, `packages/` installed and
  committed, the app's own integrations in `shared/providers/`,
  `jadpo.lock` with its sources (path, git, link) and verification at
  install, reserved names, `surface.jadpo_record` and `jadpo package show`,
  `connections:` and the network report, lazy loading, library packages'
  translations, and the `jadpo package` commands for adding, linking and
  checking, with `type.standard_exists` on packages' public types in the
  publishing bar (area 19, question 1). The public registry stays after the first build.
- [ ] **7.2 Importing, decoding and faking** `opus · high`: `x-jadpo`
  fields in every API's OpenAPI document and each API serving it at
  `<prefix>/openapi.json` (under its own authentication, `serve_openapi:
  false` to turn off, listed in its own OpenAPI and record); the OpenAPI
  importer reading them, hand-written packages, `wire:`,
  `decode:`, `wire_output:`, `output_at:` and `failed_when:`, recordings
  and `jadpo package record`, generated fakes, provider calls faked by
  default in `jadpo dev` with `development: { sandbox: }` and the call log
  (`jadpo calls`), and derived provider idempotency keys (app,
  environment, caller, step), which also close a collision between users; and mapping imported types
  to standard types (area 11, "Mapping imported types to standard types",
  Richard 2026-10-17): exact format mappings, shape and name detection
  decoded at the edge, declinable findings with `(agent)` steps, one review
  instruction scoped to the types the app's operations reach and tracked per
  type in `jadpo check` and `jadpo approval`, and the import's summary.
- [ ] **7.3 Email:** `opus · high` the capability under design C (whole:
  Jadpo-rendered, attachments), the SMTP driver and a mail package as a
  second connection proving the swap, `emails/` templates and the shared
  layout, the dev inbox, the health score. Chunk 5.3b refuses a dotted
  translation key (area 16's old `"welcome_email.subject"`) as
  `translation.key_invalid`; under translations as messages (5.4c) an
  email's subject is its own member in its domain's language files.
- [ ] **7.3b Magic-link sign-in** `opus · high` (area 8, decided
  2026-10-16): `sign_in:` with `magic_link:` and `providers:`, the
  generated endpoints (the email links to the frontend's `link:` path,
  which posts to `POST /auth/magic-link/confirm`), link and six-digit code,
  the wrong-code and send limits, `on_first_sign_in` for every way in, the
  built-in emails' five levels (area 16), the sign-in failures and events.
  Google and other providers stay after the first build.
- [ ] **7.4 Outside calls hold no transaction** `opus · xhigh` (areas 6 and
  11, 2026-10-15): reads before a call in a read-only snapshot closed before
  it, the transaction opened after it, a write to a record loaded before the
  call checked by its row version, and a change rerunning the action with
  the call repeated under its key. Done when a test holds a provider call
  open and shows no transaction open meanwhile, and a change made during
  the call reruns the action.
- [ ] **7.5 Multi-step calls** `opus · xhigh` (areas 11 and 12, decided
  2026-10-16, with the edge-case review's ten fixes): several keyed
  changing calls per action where the key survives a retry, each recorded
  before it is sent and replayed on a rerun, the path checked and
  `diverged` stopping for an operator, `clock.now` fixed per record, key
  windows from the package, no outside calls inside `for`, one runner per
  record, unfinished runs as operator items, and the late-failure warning.
- [ ] **7.6 What package code may do** `opus · high`: package actions
  calling only their own API, package subscribers on their own webhooks,
  public and non-public package events, `polls:` run only while
  subscribed, `max_concurrent:`, and recipes with `jadpo package recipe`.
- [ ] **7.7 Receiving from outside** `opus · high`: webhook replies and
  handshakes, `occurred_at`, and the `receiving` capability for queues
  (`type_at`, `id_at`, `events`), with the SQS package and, on Workers,
  the Cloudflare Queues driver.
- [ ] **7.8 Teaching examples for Phase 7** `opus · high`: packages,
  capabilities, email, sign-in, multi-step calls and receiving, passing
  `--require-examples`.

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
  From chunk 5.3: a deployable carries locale data only for its
  application's supported locales (today all 533 are compiled in: about
  3.5 MB of a 3.9 MB module, 1.2 MB gzipped).
- [ ] **8.3 Performance spike** `opus · high`: one route compiled to native and to
  WebAssembly and measured against an equivalent hand-written Bun and
  TypeScript app, comparing Wasm with native and Bun with native as well
  as Wasm with Bun. Moved from an earlier chunk 4.5, which needed Phase 8's compiled code
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

1. Personal data (area 15), with encrypted fields and keys held as data
   (area 19, questions 10 and 21) and `crypto.verify_totp`, which waits on
   them.
2. Remaining built-in services (area 16): flags, files, audit, search,
   telemetry thresholds.
3. Debugging and review (area 17): `why`, timelines, `test new --from`,
   `call`.
4. Records and acceptances (area 18): the approval report, acceptance
   matching, package upgrade plans.
5. Tooling completeness (area 14): interactive tests, `rename` (refusing a
   name already in use, 2026-10-16), `impact`,
   full completion.
6. Packages and capabilities after the first build (decided 2026-10-16):
   services run by Jadpo (`runs:`, Docker or Podman hidden behind
   `jadpo services`); the `documents` capability; OAuth grants per owner;
   search, `bot_check` and `scanning` capabilities; the disk driver;
   telemetry's `telemetry:` section, custom metrics and the local
   `jadpo traces`, `metrics` and `logs`; `jadpo watch`; Google and other
   sign-in providers; publishing packages (`jadpo package new`, `check`,
   `publish`, git-tag versions, publisher trust); packages from other Jadpo
   apps and the cross-app break check; choosing a provider when deploying,
   when an app other people run needs it.
7. Product work: the developer console and MCP (E11), the graph review UI
   (E06, Revset-inspired), the public package registry, acting as an OAuth
   provider, D1 and row-level security.

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

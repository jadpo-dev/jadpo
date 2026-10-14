# Jadpo roadmap

**Updated:** 2026-10-14. This page is the only list of outstanding work: the
delivery plan for building Jadpo from the decided language, one chunk per
session. Record decisions in the [decision register](decision-register.md)
and the [language pages](language/README.md), not here.

## The direction (decided 2026-10-13)

- **The language is specified** in [docs/language](language/README.md):
  eighteen areas, all decided. Those pages are the specification and their
  examples become the tests.
- **A new compiler, built around one graph**
  ([compiler-runtime §1.1–1.2](compiler-runtime.md#11-graph-centred-compiler-decided-2026-10-13)).
  The current compiler is archived, not reworked.
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
   From 2.4 on, every chunk adds or extends the commented examples in
   `examples/` so the coverage check still passes with what it built.
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
- [ ] **2.7 Phase 2 review: soundness** `opus · xhigh`. Proofs that pass
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
- [ ] **2.8 Phase 2 review: tooling and docs** `opus · high`. Rerun
  commands and locations that work from where the command was run; a
  callable's undeclared failures no longer give its callers wrong `next`
  steps; `--watch` exits when its output closes and says when errors
  clear, including after a file is removed; location paths never cut; one
  location shape in `--json` and the `jadpo test --json` schema documented;
  `--failed` when the failed tests are gone; decided but unbuilt flags say
  `not_built_yet`; the smaller wording and completion findings. Gate: rich
  snapshots stripped of the run time, the watch latency ceiling near the
  1-second target, README "try next" outcomes checked. Docs: areas 1, 13
  and 14 stop calling archived-compiler behaviour current.
- [ ] **2.9 Phase 2 review: teaching examples** `opus · high`. Types that
  carry meaning, not just the right representation; business rules raised
  with `reject` (a stock problem is a `conflict`, not an `invalid_value`),
  with `if` checks as proof beside rule arms; a genuine `invalid:`
  catch-all; failures handled in application code, not turned into zero in
  tests; fixtures taught before public fixtures (08 before 07); the wrong
  comments and README claims the review lists; sets and maps described as
  far as they work.
- [ ] **2.10 Executor shape for Phase 3** `opus · xhigh`, from the Phase 2
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

## Phase 3 — Data (areas 5, 6)

- [ ] **3.1 Entities, migrations and PostgreSQL:** `opus · high` snapshots, system tables,
  the test database template.
- [ ] **3.2 Queries:** `opus · high` filters, order, pages, includes, selected columns,
  `jadpo sql`.
- [ ] **3.3 Writes:** `opus · xhigh` entity operations, conflicts, concurrent-change rerun,
  counters.
- [ ] **3.4 Lifecycle and purge.** `opus · high`

## Phase 4 — HTTP, identity and policy (areas 7–9)

- [ ] **4.1 Routes:** `opus · high` groups, inputs, outputs, statuses, OpenAPI, the API
  record, default rate limits.
- [ ] **4.2 Request guarantees:** `opus · high` idempotency, deadlines, body limits, health.
- [ ] **4.3 Sign-in:** `opus · high` sessions, API tokens, service keys.
- [ ] **4.4 Policy:** `opus · xhigh` grants compiled into queries, tenancy, the permissions
  report.

## Phase 5 — Configuration, time and translations (area 10)

- [ ] **5.1 Configuration:** `sonnet · high` `config.jadpo`, environments, secrets, signing
  keys.
- [ ] **5.2 Time:** `opus · high` durations, instants, formatting.
- [ ] **5.3 Translations and locales.** `sonnet · high`

## Phase 6 — Events and schedules (area 12)

- [ ] **6.1 Events:** `opus · high` outbox, subscribers, retries, `Access.system`, trace
  context.
- [ ] **6.2 Schedules:** `opus · high` leases, the native tick, schedule monitoring basics.

## Phase 7 — Packages, capabilities and email (areas 11, 16)

- [ ] **7.1 Packages and connections:** `opus · high` the package format, lazy loading,
  `connections:`, the network report.
- [ ] **7.2 Importing and faking:** `opus · high` the OpenAPI importer, hand-written
  packages, generated fakes, `jadpo provider record`.
- [ ] **7.3 Email:** `opus · high` the capability, SMTP driver, `emails/` templates, the dev
  inbox, the health score.

## Phase 8 — Compiled deployables

- [ ] **8.1 Native:** `opus · xhigh` graph to Rust; every test passes interpreted and
  compiled.
- [ ] **8.2 WebAssembly on Workers:** `opus · xhigh` the cron tick, outbox delivery after
  commit, PostgreSQL through Cloudflare's pooling.
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
- One independent review per phase, not per chunk.
- Update this page when a chunk finishes or the order changes, and nothing
  else.

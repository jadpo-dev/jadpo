# Jadpo roadmap

**Updated:** 2026-10-13. This page is the only list of outstanding work: the
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

1. Take the first chunk below that is not ticked. Chunks marked **needs
   Richard** stop for him at the point named; every other chunk runs
   unattended.
2. Plan it in the session, not in a document: read its area pages, list the
   examples and errors it must make pass, and write those as failing tests
   first.
3. Build until its "done when" holds and the gate (`tools/verify.py`) passes.
4. Open a pull request for the chunk, merge it when CI passes, tick it here.
   If the chunk needs a language decision that is not on the pages, stop and
   ask Richard with a recommendation; never special-case the compiler for an
   example.

A chunk is sized for one session. If one turns out larger, split it here
before starting the second half.

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
- [ ] **0.4 Architecture plan.** One session designing what every later chunk
  builds on: the Rust workspace and crates; the graph's node and edge kinds,
  stable identity and canonical JSON; the incremental query engine; the
  executor interface shared by interpreter and code generator; the runtime
  crate's boundaries and the capability driver interface; how area-page
  examples become tests; the gate, including the check that bans example
  application names in compiler source. Written into
  [compiler-runtime.md](compiler-runtime.md). **Needs Richard** to read it
  once before Phase 1.

## Phase 1 — Graph core (areas 1–4)

- [ ] **1.1 Workspace and CLI shell:** `jadpo` with the agreed
  [design language](language/research/14-cli-design-language.md),
  self-describing help, `--json`, completion skeleton.
- [ ] **1.2 Parser, syntax tree and formatter** for the decided grammar.
- [ ] **1.3 Project layout and domains** (area 1).
- [ ] **1.4 Values and types** (area 2).
- [ ] **1.5 Logic** (area 3).
- [ ] **1.6 Failures** (area 4).
- [ ] **1.7 The graph, identity and manifest:** `jadpo check` writes
  `build/manifest.json`; the language server gives diagnostics and hover.

Done when areas 1–4's decided examples check, and each area's errors are
listed on its page with the new compiler's real output (the v0 lists are
reference only).

## Phase 2 — Interpreter and tests (area 13)

- [ ] **2.1 Interpreter** for pure logic, with errors at `.jadpo` lines.
- [ ] **2.2 Test runner:** file, line, name, seed, failure reports as decided,
  `--json`.
- [ ] **2.3 Watch mode** under the 1-second target on a fixture project.

## Phase 3 — Data (areas 5, 6)

- [ ] **3.1 Entities, migrations and PostgreSQL:** snapshots, system tables,
  the test database template.
- [ ] **3.2 Queries:** filters, order, pages, includes, selected columns,
  `jadpo sql`.
- [ ] **3.3 Writes:** entity operations, conflicts, concurrent-change rerun,
  counters.
- [ ] **3.4 Lifecycle and purge.**

## Phase 4 — HTTP, identity and policy (areas 7–9)

- [ ] **4.1 Routes:** groups, inputs, outputs, statuses, OpenAPI, the API
  record, default rate limits.
- [ ] **4.2 Request guarantees:** idempotency, deadlines, body limits, health.
- [ ] **4.3 Sign-in:** sessions, API tokens, service keys.
- [ ] **4.4 Policy:** grants compiled into queries, tenancy, the permissions
  report.
- [ ] **4.5 Early performance spike.** One route compiled to native and to
  WebAssembly and measured against an equivalent hand-written Bun and
  TypeScript app, so a Wasm gap is found now, not at the end. **Needs
  Richard** if Wasm is more than 25% behind Bun.

## Phase 5 — Configuration, time and translations (area 10)

- [ ] **5.1 Configuration:** `config.jadpo`, environments, secrets, signing
  keys.
- [ ] **5.2 Time:** durations, instants, formatting.
- [ ] **5.3 Translations and locales.**

## Phase 6 — Events and schedules (area 12)

- [ ] **6.1 Events:** outbox, subscribers, retries, `Access.system`, trace
  context.
- [ ] **6.2 Schedules:** leases, the native tick, schedule monitoring basics.

## Phase 7 — Packages, capabilities and email (areas 11, 16)

- [ ] **7.1 Packages and connections:** the package format, lazy loading,
  `connections:`, the network report.
- [ ] **7.2 Importing and faking:** the OpenAPI importer, hand-written
  packages, generated fakes, `jadpo provider record`.
- [ ] **7.3 Email:** the capability, SMTP driver, `emails/` templates, the dev
  inbox, the health score.

## Phase 8 — Compiled deployables

- [ ] **8.1 Native:** graph to Rust; every test passes interpreted and
  compiled.
- [ ] **8.2 WebAssembly on Workers:** the cron tick, outbox delivery after
  commit, PostgreSQL through Cloudflare's pooling.

## Phase 9 — Performance

Richard, 2026-10-13: developers won't touch this if it's slower.

- [ ] **9.1 Benchmarks:** the golden todo plus focused workloads (reads,
  writes, signed-in lists, large rows, cold start) on native, Wasm under
  wasmtime and on Workers, against an equivalent Bun and TypeScript app and
  Node; results published with the method.
- [ ] **9.2 Optimise WebAssembly** until it matches the Bun app on throughput
  and p95, or Richard accepts a measured gap. The earlier experiments reached
  about 79% of Bun's throughput and list what to try next
  ([optimisation results](experiments/wasm-optimization-results.md)).

## Phase 10 — Golden todo

- [ ] **10.1 The golden todo end to end** on both targets, rewritten in the
  decided language, its contract cases passing. One independent review of
  the whole first build.

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

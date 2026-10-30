# Jadpo roadmap

**Updated:** 2026-10-30. This page is the only list of outstanding work: the
delivery plan for building Jadpo from the decided language, one chunk per
session. Record language decisions on the [area pages](language/README.md)
and process rules in [AGENTS.md](../AGENTS.md), not here. A finished chunk
becomes one ticked line naming where its work is written; its full text
lives in git history.

## The direction

- **The language is specified** in [docs/language](language/README.md):
  twenty-one areas, all decided. Those pages are the specification and their
  examples become the tests.
- **A new compiler, built around one graph**
  ([compiler-runtime §1.1–1.2](compiler-runtime.md#11-graph-centred-compiler)).
  The first compiler (v0) was not reworked; it stays in git at the tag
  `archive/compiler-v0`.
- **Two ways to run the graph:** an interpreter inside `jadpo` for `dev`,
  `test` and `call`, and compiled Rust for deployables, native and
  WebAssembly (Cloudflare Workers), sharing one runtime crate. Every test runs
  both ways.
- **First build scope:** the graph core, both executors and what the golden
  todo uses. Everything else follows in the order below.

## How a session works

Saying **"continue the roadmap"** starts this routine; it runs unattended
and ends with one report of what was merged and anything that needs a
maintainer.

1. Take the first chunk below that is not ticked. Its tag (`opus · high`)
   is the model and effort it needs. Say in one line which chunk is next and
   its tag, then carry on without waiting. The building is done by a
   subagent started with exactly that model and effort, so the tag applies
   whatever the session itself runs on (setting the subagent's model and
   effort from the tag is authorised); the session plans, reviews the
   result, runs the gate and merges. Chunks marked **needs a
   maintainer** stop for one at the point named; every other chunk runs
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
   phase built.
4. Open a pull request from a branch named `chunk-<number>`, titled with the
   Conventional Commit for the whole chunk (the title is the commit on
   `main`), squash-merge it when CI passes, and tick it here in the same
   pull request. A ticked chunk is
   one line naming the sections where what it built is written
   ([compiler-runtime](compiler-runtime.md) and the area pages, which
   hold every fact, decision and measurement worth keeping); the full text
   of a finished chunk lives in git history.
   If the chunk needs a language decision that is not on the pages, stop and
   ask a maintainer with a recommendation; never special-case the compiler for an
   example.
5. Then take the next chunk the same way. Stop at the end of a phase, at a
   **needs a maintainer** point, when a language question needs one, or when
   the session's context is getting long; the next session picks up from the
   roadmap. Before starting a chunk, check that no other session is on it:
   skip it if an open pull request or a branch named `chunk-<number>` pushed
   in the last three hours exists.

A chunk is sized for one session. If one turns out larger, split it here
before starting the second half. Tags: `opus` or `sonnet`; effort `high`
or `xhigh`, never lower; the hardest chunks (graph,
interpreter, concurrency, policy, compiled targets, WebAssembly
optimisation) are `xhigh`.

## Phase 8 — Compiled deployables

Compiled native code (8.1) and WebAssembly on Cloudflare Workers (8.2),
every test passing interpreted and compiled. A ticked chunk names where
what it built is written: § is a section of
[compiler-runtime.md](compiler-runtime.md), an area a
[language page](language/README.md), and an area's Decisions the
Decisions table of its [design notes](language/notes/); the full text of
a finished chunk is in git history.

- [x] **8.1a One runtime plan and its encoding**: §2.5, "The plan compiled
  code holds"; §2.6, "The start-up state and the runtime's plan".
- [x] **8.1b The upgrade hook and SMTP over it**: §2.6, "TLS through the
  transport"; area 16, "The SMTP driver".
- [x] **8.1c The outcome-and-effect record**: §2.5, "Differential test".
- [x] **8.1d One connections table and the driver factory**: §2.6, "One
  connections table and the driver factory".
- [x] **8.1d2 Provider call sites by number**: §2.6, "Provider call sites
  by number".
- [x] **8.1e A smaller executor and the `testing` feature**: §2.6, "The
  `testing` feature" and "The start-up state and the runtime's plan".
- [x] **8.1f Compiled code: the first route**: §2.5, "Compiled code".
- [x] **8.1g Compiled code: areas 1 to 4**: §2.5, "Compiled code".
- [x] **8.1h Compiled code: data, routes and identity**: §2.5, "Compiled
  code".
- [x] **8.1i Compiled code: events, packages and the rest**: §2.5,
  "Compiled code".
- [x] **8.1j The corpus compiled**: §2.5, "Compiled code"; §2.7, "The
  differential run" and "CI".
- [x] **8.1k The route trie**: §2.5, "The route trie".
- [x] **8.1l Native accept and one rate-limit count**: §2.5, "Native
  accept and one rate-limit count"; §2.6, "Single-threaded by design".
- [x] **8.1m A pooled HTTP client**: §2.6, "A pooled HTTP client".
- [x] **8.1m2 A flaky test after the pooled client**: §2.6, "A pooled HTTP
  client"; §2.7, "No live test skips unseen".
- [x] **8.1n PostgreSQL over TLS**: §2.6, "PostgreSQL over TLS"; area 5,
  "PostgreSQL over TLS".
- [x] **8.1o A native deployable**: §2.6, "A native deployable"; §2.8;
  area 14, "A native deployable".
- [x] **8.1o2 Declared deployables**: §2.6, "Declared deployables"; area
  14, "Declared deployables".
- [x] **8.1o3 A deployable compiled from what it reaches**: §2.6, "A
  deployable compiled from what it reaches"; area 14, "Declared
  deployables"; area 18, "What a build id digests".
- [x] **8.1p A typed per-route entry**: §2.5, "A typed per-route entry";
  §2.7, "The differential run".

- [x] **8.2a The Workers host**: §2.6, "The Workers host"; §2.8,
  "Toolchain pinning".
- [x] **8.2b A Workers deployable from `jadpo build`**: §2.6, "A Workers
  deployable"; area 14, "A Workers deployable".
- [x] **8.2c Locale data for the supported locales**: §2.6, "The locale
  data a deployable carries"; area 10, "Locales".
- [x] **8.2d One route on workerd**: §2.6, "Serving on Workers"; area 14,
  "A Workers deployable" and "A Worker's start".
- [x] **8.2e Route tests on workerd with the same records**: §2.6, "Route
  tests on workerd"; §2.5, "Differential test".
- [x] **8.2f The cron tick**: §2.6, "The cron on Workers"; area 12, "The
  tick"; area 14, "A Workers deployable".
- [x] **8.2g The `trigger:` adapter**: §2.6, "The `trigger:` adapter";
  area 12, "The tick" and question 16; area 14, "A native deployable".
- [x] **8.2h After the response**: §2.6, "After the response on Workers";
  area 12, "Running background work".
- [x] **8.2i PostgreSQL through Hyperdrive**: §2.6, "PostgreSQL through
  Hyperdrive"; area 5, "PostgreSQL through Hyperdrive"; area 14, "A
  Worker's database".
- [x] **8.2j Six connections an invocation**: §2.6, "Six connections an
  invocation"; area 14, "A Workers deployable"; area 11, Decisions.
- [x] **8.2k Cloudflare Queues' consumer**: §2.6, "Cloudflare Queues'
  consumer"; area 11, "The Cloudflare Queues driver".
- [x] **8.2l Passkeys on Workers**: §2.6, "Passkeys"; area 8,
  Decisions, "One passkey implementation".
- [x] **8.2m What `jadpo check` and `jadpo build` say for Workers**: area
  14, "A Workers deployable" and Decisions; area 10, "Client addresses".
- [x] **8.2m2 Rate limits' store on Workers**: area 16, "Rate limits" and
  "Rate limits on Workers"; §2.6, "The Workers host".
- [ ] **8.2n A real deploy** `opus · high`, after 8.2a to 8.2m2, **needs
  a maintainer** (the maintainer's Cloudflare account): a teaching example deployed with
  Hyperdrive's caching off to a real PostgreSQL, the cron and a queue,
  its Wrangler file accepted by `wrangler deploy`; its cold start against
  the one-second limit, its module against the plan's size limit, and
  the client's address and the six connections on the real edge,
  measured; whether the outbound guard's names can reach a private
  address there checked. From 8.2i: the Worker reaching Hyperdrive
  without TLS as its binding's address says, Hyperdrive's default TLS
  checking the database's certificate, and a `vector` column read through
  it (tokio-postgres describes one through a named statement). From 8.2h:
  a Worker reaching its own `JadpoHandOn` (since 8.4b through
  `ctx.exports`, not a binding), each invocation handed on given its own
  30 seconds once the one that called it has ended, and a client leaving
  a streamed answer ending the model's call.
  From 8.2k: a queue's name in Jadpo's form and its consumer accepted by
  Cloudflare, and a batch's `retry({ delaySeconds })` honoured on the edge.
  From 8.4a: a recursion through 400 inline functions, and a thousand
  frames of a large function, answered on the edge, whose V8 stack the
  inline-function limit assumes is about workerd's megabyte, and the
  module's 8 MiB stack within an isolate's memory.
  From 8.4b: the hand-on reaching the Worker through `ctx.exports` on the
  edge, under the file's name and under `wrangler deploy --env`, and the
  dead-letter queue's consumer accepted beside the queue's.

- [x] **8.3 Performance spike**: §2.11.

### Phase 8 review (2026-10-22)

Two independent reviewers, soundness and architecture for the phases
ahead, read everything Phase 8 built, and chunk 8.3's spike measured it;
their confirmed findings are the chunks below, taken before 8.2n and
Phase 9, in this order: 8.4a, 8.4b, 8.4c and 8.4d side by side, then
8.4e (after 8.4c), 8.4f and 8.4g side by side, then 8.4h. 8.4i and 8.4j beside any of them.

- [x] **8.4a Phase 8 review fixes: compiled parity**: §2.5, "Differential
  test"; area 3, "Recursion"; area 7, "Time limits".
- [x] **8.4b Phase 8 review fixes: Workers background work**: §2.6,
  "Cloudflare Queues' consumer", "After the response on Workers" and "Six
  connections an invocation"; areas 11, 12, 14 and 16, Decisions.
- [x] **8.4c Phase 8 review fixes: what a deployable runs**: §2.6, "One
  start for both hosts"; §2.5, "A typed per-route entry"; area 14,
  Decisions.
- [x] **8.4d Phase 8 review fixes: addresses and connections**: areas 5,
  10, 11 and 16, Decisions; §2.6.
- [x] **8.4e Workers speed and memory**: §2.6, "The Workers host"; §2.11.
- [x] **8.4f Before the real deploy**: §2.6, "Release profile, names and the Wrangler file";
  area 14, Decisions.
- [x] **8.4g Builds and CI kept quick**: §2.5, "Keeping rustc time down";
  §2.7, "CI"; area 14, Decisions.
- [x] **8.4h The roadmap and the plan's header**: §2.5, "The plan compiled
  code holds"; "How a session works" above.
- [x] **8.4i Collation for the supported locales**: area 10, "Locales" and
  Decisions; area 19, "Text".
- [x] **8.4j `sort_by` by locale requires `locales { }`**: area 10,
  "Locales" and Decisions; area 19, "Text".

## Phase 9 — Performance

Developers won't touch Jadpo if it's slower.

- [ ] **9.1 Benchmarks:** `opus · high` the golden todo plus focused workloads (reads,
  writes, signed-in lists, large rows, cold start) on native, Wasm under
  workerd and on Workers, against an equivalent Bun and TypeScript app and
  Node; results published with the method. Full scope remains outstanding
  through 9.1a, 9.1d, 9.1b and 9.1c, in that order; the split changes sizing,
  not acceptance (2026-10-22). This is an umbrella: sessions take the first
  unticked concrete subchunk below; tick 9.1 only once all four are complete.
- [x] **9.1a Instrumentation and focused scaffold**: `benchmarks/README.md`; compiler/runtime §2.11.
- [x] **9.1d Golden todo and local comparison**: compiler/runtime §2.6, §2.7 and §2.11; `benchmarks/README.md`.
- [x] **9.1d1 Provisioned external JWT subjects**: compiler/runtime §2.6; area 8, "Ways to sign in" and implementation status.
- [x] **9.1d2 Golden todo and executable contract**: compiler/runtime §2.6 and §2.11; `benchmarks/golden-todo/README.md`.
- [x] **9.1d2a Generic runtime contract fixes:** §2.6; areas 5 (Lifecycle), 10 (connection URLs) and 11 (safe key windows and record modes).
- [x] **9.1d2b Golden todo and all 44 cases**: compiler/runtime §2.11; `benchmarks/golden-todo/README.md`.
- [x] **9.1d3 Strong TypeScript golden todo**: compiler/runtime §2.7 and §2.11; `benchmarks/golden-todo/typescript/README.md`.
- [x] **9.1d4 Local application comparison**: compiler/runtime §2.11; `benchmarks/README.md`.
- [x] **9.1d5 TypeScript on workerd baseline**: compiler/runtime §2.11; `benchmarks/README.md`; `benchmarks/golden-todo/typescript/README.md`.
- [x] **9.1b Runtime performance**: compiler/runtime §2.6 and §2.11; `benchmarks/README.md` (9.1b1–9.1b5, including 9.1b1a).
- [x] **9.1b1 Bounded delivery sweeps and the backlog**: compiler/runtime §2.6, "Bounded delivery sweeps", and §2.11; `benchmarks/README.md`.
- [x] **9.1b1a Warm delivery comparison**: compiler/runtime §2.11; `benchmarks/README.md`.
- [x] **9.1b2 Fast ticks and hourly housekeeping**: compiler/runtime §2.6, "Fast ticks and hourly housekeeping", and §2.11; `benchmarks/README.md`.
- [x] **9.1b3 One-statement AI accounting**: compiler/runtime §2.11; area 21, "How budgets are counted"; `benchmarks/README.md`.
- [x] **9.1b4 Version checks in writes**: compiler/runtime §2.6, "Snapshots before calls", and §2.11; area 6, "Outside calls hold no transaction"; `benchmarks/README.md`.
- [x] **9.1b5 Breakers in delivery claims**: compiler/runtime §2.6, "Breaker snapshots in delivery claims", and §2.11; `benchmarks/README.md`.
- [ ] **9.1c Workers benchmark results:** `opus · high` **needs a maintainer**
  after 9.1b: approve access and deployment before using the Cloudflare account. Run the
  golden todo plus focused workloads on Workers against the equivalent Bun and
  TypeScript app and Node; results published with the method. Include the
  10,000-delivery backlog and the runtime improvements' before/after results.
  Real edge measurement is required before Phase 9 finishes.
- [x] **9.2 Optimise WebAssembly**: not built; 9.1d5 put Jadpo ahead on
  CPU time per request and p95 in every cell, and the Wasm build's 4.6 ms
  cold-start gap to TypeScript on workerd is accepted (§2.11). Its ideas
  for a later gap are in this line's git history.
- [x] **9.2b One passkey implementation**: compiler/runtime §2.6,
  "Passkeys" and "Route tests on workerd"; area 8, Decisions, "One passkey
  implementation" and "Passkey attestation".
- [x] **9.2c `jadpo build` outside this checkout**: area 14, "A native deployable", and its design notes, "Building outside this checkout"; compiler/runtime §2.8, "What a released `jadpo` builds with".
- [x] **9.3 The differential run's last two checks**: compiler/runtime §2.3, "Path numbers"; §2.5, "Differential test" ("Paths" and "Fuzzed bodies"); §2.7, "The differential run".

## Documentation and releases

What the jadpo.dev site, built in its own repository, needs from this one
([documentation](documentation.md)). Claude builds these beside Phase 9,
D1 first: the site starts once D1 has merged.

- [x] **D1 The docs' structure and bundle**: [documentation](documentation.md),
  "Where the content lives", "Topic IDs" and "The bundle"; each area's
  "What is built" and Errors; `docs/errors/`; `docs/language/notes/`.
- [x] **D2 The primer**: [primer](primer.md), and [documentation](documentation.md), "The bundle".
- [ ] **D3 Releases** `opus · high` after D1: semantic versions from 0.1.0,
  release-please over the Conventional Commit types, CI failing a
  language change under a type that says it isn't one, installers (shell,
  Homebrew, PowerShell, npm) on each Release, the bundle attached; an
  app's declared version checked (area 14, "Scaffold"). **Needs a maintainer**
  before 0.1.0 is published.
  Built, switched off: release-please, the commit-type check, dist's
  installers and the bundle on each Release, and `application { jadpo: }`
  ([documentation](documentation.md), "Versions and releases"); waits for
  a maintainer's token, tap, npm account and look at 0.1.0.
- [ ] **D4 Issues triaged by an agent** `opus · high` after D3: issues
  and feature requests read, answered and turned into pull requests by an
  agent that holds no secrets and cannot merge or release, since issue text
  is anyone's; its pull requests pass the same gate and review.
- [x] **D5 No personal attributions**: [AGENTS.md](../AGENTS.md) (rules for
  every contributor) and this page, "How a session works".
- [x] **D6 Packages, services and integrations explained**: [area 11](language/11-integrations.md), "How packages, connections, capabilities and services fit together".
- [x] **D7 One place for the golden todo and the benchmarks**: [benchmarks](../benchmarks/README.md);
  `tools/` holds the gate and its `docs/`, `checks/`, `references/` and `fixtures/`.
- [ ] **D8 The VS Code extension, finished and published**: area 14, "Editor support" and "What is built"; `editors/vscode/README.md`; `.github/workflows/vscode-extension.yml`.
  Waits: the editor's rename, which comes with `jadpo rename` and the identity registry (after the first build, items 6 and 7).
- [x] **D9 Docs for newcomers**: [writing guide](writing.md); the [Learn path](learn/README.md); the [primer](primer.md); [documentation](documentation.md), "The bundle".
- [x] **D10 Area pages to the writing guide**: the area pages in [docs/language](language/README.md), each topic to the [writing guide](writing.md), "How a reference topic reads", and the rationale in each area's design notes.
- [ ] **D11 Task guides** `opus · high`: how-to pages in the bundle, a
  page kind of their own beside `learn`, for the commonest jobs (add
  sign-in, store and change a record, query records, call an outside API,
  handle failures, write tests, deploy to Workers, deploy natively), each
  compiled by the gate and linking the reference. They replace the site's
  own `src/data/tasks.json`.
- [x] **D12 What the Learn path found**: [area 8](language/08-identity.md), "In development and tests"; [area 14](language/14-tooling.md), "What is built"; [documentation](documentation.md), "Where the content lives" and "The bundle".
- [ ] **D13 Error pages that explain** `opus · high`: every page in
  `docs/errors/` rewritten to the format in [docs/writing.md](writing.md),
  "How an error page reads". Each says what happened in one plain
  sentence, why Jadpo refuses it, the example and its output, what the
  next step's `(human)` or `(agent)` marker means in the standard
  wording, each fix with its code and what it changes, and the related
  topics. The errors the Learn path, the examples and `(human)` fixes
  hit come first.
  D13 also rewrites each rule's summary in the compiler's catalogue to the
  guide, starting with "API serve_openapi is not true or false".
- [x] **D14 The toolchain launcher**, in 0.1.0: [tooling](language/14-tooling.md), "Which jadpo runs"; `crates/jadpo-cli/src/launcher.rs`, `releases.rs`, `attestation.rs`, `toolchains.rs`.

## Phase 10 — Golden todo

- [x] **10.1 The golden todo end to end**: [golden todo](../benchmarks/golden-todo/README.md), "Verification"; compiler/runtime §2.7, "The golden todo's contract", and §2.8, "What a released `jadpo` builds with"; area 9, "Field restrictions".
- [ ] **10.2 How easily agents write Jadpo** `opus · high`: the same 10–20
  realistic changes to one app (the golden todo, with its TypeScript
  baseline and adversarial changes) done by fresh agent sessions in Jadpo
  and in TypeScript, several runs each, with hidden acceptance tests.
  Measures correctness, attempts and tokens to green, where each mistake
  was caught (check, test or escaped), how easily a person reviews the
  diff, and how often a tempting unsafe shortcut gets through. The agent
  gets the docs as [documentation](documentation.md) delivers them, primer
  first, since no model has seen Jadpo; the results
  also say whether task guides are needed beyond the primer. The method is designed at the start of the chunk; its
  results steer the order of the work after the first build.
  Built: the method, 18 changes and the harness, in [agent-changes](../benchmarks/agent-changes/README.md); the
  measured runs (108 sessions at `claude-opus-5-5`, about $200 estimated before any was measured, $648 at most by the
  $6 cap a run) wait for a maintainer's approval and a signed-in `claude`.

## Phase 10 review

What the first build's independent review found that this phase didn't
fix, in order. Each chunk keeps every test passing interpreted and
compiled.

- [x] **10.3a A unique rule within a tenant**: [area 9](language/09-policy.md), "A unique rule holds within its tenant".
- [x] **10.3b Founding only a new tenant**: [area 9](language/09-policy.md), "Founding a tenant".
- [x] **10.3c One record of what each invocation did**: [compiler/runtime](compiler-runtime.md) §2.6, "What an invocation did".
- [x] **10.3d A key as a value**: [compiler-runtime](compiler-runtime.md) §2.5, "A key as a value"; area 19, "Hashing and cryptography".
- [x] **10.3e Loading a project from files in memory**: compiler/runtime §2.2, "Loading".
- [x] **10.3f Where a connection's credential comes from**: compiler/runtime §2.6, "Where a connection's credential comes from"; [area 11](language/11-integrations.md), "How provider calls are made".
- [x] **10.3g The graph's nodes for areas 8, 10, 11 and 12**: compiler/runtime §2.3, "Outside, access and services" and "Diff"; area 18, "The manifest" and "What is built".
- [x] **10.3h The runtime's own SQL beyond PostgreSQL**: area 5, "Actions on D1" and "System tables".

## Security audit

Before 0.1.0 is published (D3).

- [x] **S1 A full security audit**: the threat model in [security](security.md), each confirmed finding fixed with its test on its area page, `cargo audit` and `cargo deny` in the gate, and [SECURITY.md](../SECURITY.md); release provenance in S2.
- [x] **S2 Release provenance**: [security](security.md), "CI and releases"; [documentation](documentation.md), "Installing"; `.github/install-dist.sh`.
- [x] **S3 Coverage-guided fuzzing**: [security](security.md), "Fuzzing"; `fuzz/`; `.github/workflows/fuzz.yml`.
- [x] **S4 Code scanning and Scorecard**: [security](security.md), "CI and releases"; `.github/workflows/codeql.yml`, `.github/workflows/scorecard.yml`.
- [ ] **S5 The OpenSSF Best Practices badge** `sonnet · medium`, right
  after 0.1.0 is published: the project's entry
  (https://www.bestpractices.dev/projects/16073) has every passing
  criterion answered but three that need a release: a unique version per
  release (`version_unique`), release notes (`release_notes`) and
  responses to bug reports (`report_responses`). Done when those are
  marked Met with links to the 0.1.0 release and its notes, the entry
  shows "passing", and the README carries its badge:
  `[![OpenSSF Best Practices](https://www.bestpractices.dev/projects/16073/badge)](https://www.bestpractices.dev/projects/16073)`.
  The entry is in the maintainer's name, so the maintainer saves the
  answers (a prefilled link per part).

## What an outside agent's first app found

An outside agent (Codex) built a library API from the docs alone: books,
loans, sales and ISBNs, then magic-link customers, Google staff, two staff
roles and due-date emails. The plain app took 7 diagnostics; the sign-in
work took 119 and 879 lines of Python around the app. These chunks remove
the causes, in order. Each turns the agent's failing shape into a test
first. They are taken before any other unticked chunk on this page
(Richard, 2026-10-30); chunks touching different crates run at the same
time.

- [x] **A0 Every unbuilt protection refused where it is written**: area 15, opening, the capability table and "What is built"; area 9, "Protecting policy"; area 6, "All-or-nothing actions and concurrent changes"; area 16, "What is built".
- [x] **A1 Diagnostics that point at the cause**: area 1, "Reserved words" and "How a promotion happens"; area 2, "Values that may be missing"; area 9, "Actions"; area 14, "Commands for debugging, editors and help".
- [ ] **A2 Listed queries and calls a listing doesn't cover** `opus · xhigh`:
  area 9, "Listing a query" and "A call the listing doesn't cover", with
  their decisions in the area's notes. Done when a staff query restricts
  fields to a role on the same entity as a customer catalogue, a route
  test shows a removed role refused on the next request, and a listed
  action calling an admin-only helper is `policy.call_ungranted`.
- [x] **A3 External tokens in tests and `jadpo dev`**: area 8, "External tokens in tests and `jadpo dev`"; area 13, `as:`; area 14, `jadpo dev`'s "External tokens"; area 19, `Jwk`'s `key_ops`.
- [ ] **A4 Batches run to the end in tests** `opus · high`: area 13,
  "Batches", which is specified and was on no chunk. Done when the
  seed-library example's test of a withdrawn packet passes through its
  batch, with no page action called by hand.
- [x] **A5 `jadpo seed`**: area 5, "Seeds"; area 14, "`jadpo seed`" and "Command errors".
- [ ] **A6 Guidance an agent reads first** `opus · high` after A1 to A5:
  the `AGENTS.md` that `jadpo new` writes gains a workflow (one concern at
  a time, `jadpo check` after each file, read one topic not a whole page,
  declare a feature and check it before designing around it) and a table
  of what is built, generated from each area's "What is built"; `llms.txt`
  lists every example, with the closest one per feature; a new teaching
  example, a lending desk, has magic-link customers, staff on external
  tokens with two roles, a listed staff query, staff from a seed, a
  reminder schedule over a batch, and route tests for each, with no code
  outside Jadpo. Before 10.2's measured runs, since it changes what they
  measure.
- [ ] **A8 Acceptances for what exists today** `opus · xhigh` after A2:
  area 18, "Acceptances": `jadpo acceptance new`, matching and the
  `///` reason, CODEOWNERS on `acceptances/` and `api/`, and the kinds
  whose features are built: any widening of who can do what (a route
  made public included) and declining a security rule (`PolicyChange`);
  an API break and an event record's break (`ApiChange`); a domain
  created or renamed (`DomainChange`); a new outside host
  (`NetworkChange`); a sign-in setting loosened past its limit (area 8);
  a third party's package installed from git (`PackageChange`).
  `access.jadpo_record` and `network.jadpo_record` give them a before
  side. An outside agent made a route public on request and nothing
  stopped it (Richard, 2026-10-30). The kinds whose features aren't built
  yet are named on the items below that build them.
- [ ] **A7 Sign-in providers** `opus · xhigh`: area 8, `sign_in:`'s
  `providers:` (Google first, then Microsoft, through `Driver.oidc`) and
  `on_first_sign_in:` for provider and external-token sign-in. Moved here
  from "After the first build", item 8 (Richard, 2026-10-30). Done when
  the lending desk example signs staff in with Google through Jadpo
  alone, tested with the local issuer.

## After the first build, in order

1. Units and money in plain numbers: a named type built on `Int` or
   `Decimal` whose name is a unit, a currency's minor unit or "percent"
   (`Grams`, `Metres`, `Pence`, `Percent`) is a declinable compile error
   naming the standard type (`Mass`, `Length`, `Money`, `Percentage`),
   reusing `jadpo suggest`'s name signal data; and the gate runs
   `jadpo suggest` over every teaching example, which must come back clean
   or ignore each finding with a reason.
2. The standard type reference, generated: `tools/docs/language_docs.py`
   renders one entry per standard type (what it is, how to make one, its
   fields, operations, text, JSON and storage forms, an example) from the
   compiler's own contract data on every docs build, so the list can't
   drift from what is built; designed with the design skills first. Area
   19's prose stays as the explanation.
3. Personal data (area 15), with encrypted fields and keys held as data
   (area 19, "Encrypted fields" and "Language gaps this page depends on") and `crypto.verify_totp`, which waits on
   them; and the `ErasureNotPossible` acceptance for a connection that
   receives personal data and can't erase it.
4. Remaining built-in services (area 16): flags, files, audit, search,
   telemetry thresholds, with the `ThresholdChange` acceptance for a
   threshold raised or removed.
5. Debugging and review (area 17): `why`, timelines, `test new --from`,
   `call`.
6. Records and acceptances (area 18): the approval report, with
   unaccepted wording (area 10) in it; package upgrade plans, with the
   `PackageChange` acceptance for a risky upgrade; and `jadpo migrate
   new`'s removals (an entity removed, one moved to another store, a
   retired column dropped), with the `DataChange` acceptance.
7. Tooling completeness (area 14): interactive tests, `rename` (refusing a
   name already in use, 2026-10-16), `impact`,
   full completion.
8. Packages and capabilities after the first build (decided 2026-10-16):
   services run by Jadpo (`runs:`, Docker or Podman hidden behind
   `jadpo services`); the `documents` capability; OAuth grants per owner;
   search, `bot_check` and `scanning` capabilities; the disk driver;
   telemetry's `telemetry:` section, custom metrics and the local
   `jadpo traces`, `metrics` and `logs`; `jadpo watch`; publishing packages (`jadpo package new`, `check`,
   `publish`, git-tag versions, publisher trust); a service's new or
   changed container image and its `outbound:` grants, each with a
   `PackageChange` acceptance; packages from other Jadpo
   apps and the cross-app break check; choosing a provider when deploying,
   when an app other people run needs it.
9. Product work: the developer console and MCP, the graph review UI
   (Revset-inspired), the public package registry, acting as an OAuth
   provider, D1 and row-level security.
10. A versioned application contract for frontend clients: a public contract
   derived by the compiler, client consumption declarations and generated
   TypeScript bindings (a React example first); presentation stays in
   ordinary frontend code.
11. `jadpo docs` or `jadpo mcp` serving the bundle offline
   ([documentation](documentation.md), "Not decided yet").

## How we work

- Branch from `main` for each chunk; commit and push often; squash-merge through a
  pull request, its title the Conventional Commit for the whole change.
  Never put a worktree under `/private/tmp`.
- `tools/verify.py` output and the tests are the evidence. No evidence
  packets, per-step reviews, timing logs or plan documents.
- One independent review per phase, not per chunk, by two reviewers:
  soundness, and architecture for the phases ahead.
- Update this page when a chunk finishes or the order changes, and nothing
  else.

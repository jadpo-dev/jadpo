# Jadpo roadmap

**Updated:** 2026-10-31. This page is the only list of outstanding work: the
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
- [x] **D3 Releases**: [documentation](documentation.md), "Versions and releases" and "Installing"; area 14, "Scaffold" and "Which jadpo runs"; 0.1.0 published 2026-10-30 with its installers, npm package, Homebrew formula, docs bundle and VS Code extension.
- [ ] **D4 Issues triaged by an agent** `opus · high` after D3: issues
  and feature requests read, answered and turned into pull requests by an
  agent that holds no secrets and cannot merge or release, since issue text
  is anyone's; its pull requests pass the same gate and review.
- [x] **D5 No personal attributions**: [AGENTS.md](../AGENTS.md) (rules for
  every contributor) and this page, "How a session works".
- [x] **D6 Packages, services and integrations explained**: [area 11](language/11-integrations.md), "How packages, connections, capabilities and services fit together".
- [x] **D7 One place for the golden todo and the benchmarks**: [benchmarks](../benchmarks/README.md);
  `tools/` holds the gate and its `docs/`, `checks/`, `references/` and `fixtures/`.
- [x] **D8a The VS Code extension, published**: area 14, "Editor support" and "What is built"; `editors/vscode/README.md`; `.github/workflows/vscode-extension.yml`.
- [ ] **D8b Rename in the editor** `opus · high`: area 14, "Editor support". Waits: `jadpo rename` and the identity registry (after the first build, items 6 and 7).
- [x] **D9 Docs for newcomers**: [writing guide](writing.md); the [Learn path](learn/README.md); the [primer](primer.md); [documentation](documentation.md), "The bundle".
- [x] **D10 Area pages to the writing guide**: the area pages in [docs/language](language/README.md), each topic to the [writing guide](writing.md), "How a reference topic reads", and the rationale in each area's design notes.
- [x] **D11 Task guides**: [guides](guides/README.md); [documentation](documentation.md), "Topic IDs" and "The bundle"; [writing guide](writing.md), "Kinds of user page"; `tools/docs/language_docs.py`, `GuidePage`.
- [x] **D12 What the Learn path found**: [area 8](language/08-identity.md), "In development and tests"; [area 14](language/14-tooling.md), "What is built"; [documentation](documentation.md), "Where the content lives" and "The bundle".
- [x] **D13a Error pages people reach first**: [writing guide](writing.md), "How an error page reads" and "What the docs check enforces"; [documentation](documentation.md), "The bundle"; `tools/docs/language_docs.py`, `error_format_problems` and `error_format_scope`; the catalogue summaries in `crates/jadpo-diag`.
- [ ] **D13b Command errors that explain**: an umbrella, split by size
  (2026-10-31) into D13b1, D13b2a–D13b2c and D13b3, taken in order; tick
  it once all are.
- [x] **D13b1 The `sh run` block, settings and `jadpo new`**: [writing guide](writing.md), "How an error page reads"; compiler/runtime §2.7, "Command examples"; `tools/docs/language_docs.py`, `check_runs`; the pages of `configuration.local_*`, `cli.config_*` and `toolchain.*` a run raises, and `configuration.value_invalid`.
- [x] **D13b2a Copied output, root runs and the first person-decided pages**: [writing guide](writing.md), "How an error page reads"; compiler/runtime §2.7, "Command examples"; area 14, "What it compiles with"; the pages of `configuration.value_missing`, `configuration.local_changed`, `format.write_failed`, `translation.exchange_file_invalid`, `toolchain.build_failed` and `toolchain.fetch_failed`.
- [ ] **D13b2b Background work and data** `opus · high`: the format of
  "How an error page reads", with a `sh run` example against a disposable
  PostgreSQL, for the person-decided rules a database command raises
  whose page says "No example yet": `deliveries.*`, `schedules.*`,
  `runs.unavailable`, `ai.usage_unavailable`, `seed.production_refused`,
  `seed.unavailable`, `migration.apply_failed`,
  `migration.extension_missing`, `migration.extension_outdated` and
  `migration.store_diverged`, each catalogue summary its page's first
  sentence.
- [ ] **D13b2c The rest a person decides** `opus · high`: the same for
  `cli.dev_start_failed`, `conform.refused`, `recording.refused`,
  `suggest.share_failed`, `format.unsafe` and
  `test.runtime_start_failed`. An error that quotes the operating
  system's error number (a port in use, a refused connection) prints a
  different number on macOS and Linux, so its run can't be checked on
  both: choose a case that prints none, or copy the output. No run raises
  `format.unsafe` (the formatter's own mistake) or
  `test.runtime_start_failed` (no memory for the test thread) at all;
  each shows output copied from a `jadpo` built with that failure forced,
  and says so.
- [ ] **D13b3 The other command errors** `opus · high`: the same for the
  rules only a command raises that an agent decides (`cli.*_arguments`,
  `seed.failed`, `format.changes_required` and the rest) whose page says
  "No example yet"; then
  `error_format_scope` stops leaving any of them out.
- [ ] **D13c The rest of the error pages** `opus · high`: every other page
  in `docs/errors/` to the format, each rule's catalogue summary its
  page's first sentence; `error_format_scope` then covers every page.
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
- [x] **S5 The OpenSSF Best Practices badge**: passing and OpenSSF Baseline at https://www.bestpractices.dev/projects/16073; [README](../README.md); [CONTRIBUTING](../CONTRIBUTING.md), "The project's repositories".

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
- [x] **A2 Listed queries and calls a listing doesn't cover**: area 9, "Listing a query", "A call the listing doesn't cover" and "The permissions report"; area 1, "Which rules can be declined".
- [x] **A3 External tokens in tests and `jadpo dev`**: area 8, "External tokens in tests and `jadpo dev`"; area 13, `as:`; area 14, `jadpo dev`'s "External tokens"; area 19, `Jwk`'s `key_ops`.
- [x] **A4 Batches run to the end in tests**: area 13, "Batches" and "What is built"; compiler/runtime §2.5; `examples/24-seed-library/README.md`.
- [x] **A5 `jadpo seed`**: area 5, "Seeds"; area 14, "`jadpo seed`" and "Command errors".
- [x] **A6 Guidance an agent reads first**: area 14, "`jadpo new`"; [documentation](documentation.md), "How an agent uses it"; `tools/docs/agent_guidance.py`; `examples/29-lending-desk/README.md`.
- [x] **A9 Unknown sections of `application { }`**: area 1, "What `application { }` holds" and "What is built"; the "What is built" of areas 11, 12, 15 and 16.
- [x] **A8 Acceptances for what exists today**: area 18, "Comparing two builds" and "Acceptances"; area 9, "Protecting policy"; area 14, "`jadpo acceptance new`" and "Review and sign-off"; area 1, "Writing a decline".
- [x] **A7a `on_first_sign_in:` for external tokens**: area 8, `external_token` and "A person the app has not seen"; area 5, "Tables for sign-in"; compiler/runtime §2.6, "First sign-in by external token".
- [x] **A7b Sign-in providers**: area 8, "Providers", "What the compiler checks of sign-in" and Failures; areas 7 (generated routes), 11 (sign-in providers), 13 (signing in for a route test) and 14 (`jadpo dev`); compiler/runtime §2.6, "Sign-in providers".
- [x] **A7c Provider sign-in in `jadpo dev`**: area 8, "Providers", "In `jadpo dev`"; area 14, `jadpo dev`; compiler/runtime §2.6, "Sign-in providers".

- [x] **A10 Staff only through their provider**: area 9, "Roles that need a way in" and Decisions; area 8, "How a session started", "How strongly a person signed in", "Company accounts" and Decisions; area 13, "Signing in for a route test"; area 18, "Acceptances" and Decisions; compiler/runtime §2.6, "Roles that need a way in"; `examples/29-lending-desk/README.md`.

## Approvals

Everything a person must approve is found by the compiler and can be
approved (Richard, 2026-10-31). V2 is taken before A10, so 0.2.0 doesn't
wait for A10's decision; the rest come after the A chunks above and
before every other unticked chunk.

**0.2.0 is V2 alone.** When V2 has merged, the 0.2.0 release pull request
is merged; no session merges it before then. Everything else in this
section and the registry chunks below make **0.3.0**, released when V6
and R6 have both merged. V1 and V3 to V5 run at the same time as each
other and as R1 and R4, where they touch different crates.

Already built: acceptances (`jadpo acceptance new`, matching, the reason,
history, the CODEOWNERS gate) for API and event breaks, every widening of
who can do what (routes made public included), security declines,
sign-in settings past their limit, new domains, new outside hosts and
packages from git (every one, until V1). The kinds that belong to
features not built yet come with those features: `ThresholdChange` with
thresholds, a service's image with services, bot checks with bot
protection, a domain renamed with `jadpo domain rename`.

- [ ] **V2 The approval report** `opus · xhigh`: area 17, "Reviewing a
  change"; area 18, "Comparing two builds". `jadpo approval --before
  <ref>` builds the base, and writes Markdown and JSON in `build/review/`
  in the page's order: each decision in plain words with whether its
  acceptance exists, the path that makes it true, newly reachable
  declarations, other behaviour (the API record, schema and migrations,
  failures, events, integrations, the permissions report's diff, wording
  not yet accepted) and the source by declaration. The web view stays
  with the developer console. Screens designed with the design skills
  first. Split here if it is larger than a session.
- [ ] **V1 Your own company's git packages** `opus · high`: area 11,
  "Installing safely"; area 18, `PolicyChange.own_source_added`.
  `packages:`'s `own_sources:` in `application { }` (added to area 1's
  list of sections), its `own_source` lines in `access.jadpo_record`, and
  `PackageChange.git_source` only for a repository under none of them.
- [ ] **V3 Removing data needs a person** `opus · xhigh`: area 5,
  "Migrations"; area 18, `DataChange`. `jadpo migrate new`'s removals (an
  entity removed, one moved to another store, a retired column dropped),
  each refused by `check` until a `DataChange` acceptance names it.
- [ ] **V4 Risky package upgrades need a person** `opus · high`: area 18,
  "Package upgrades". The review compares the operations and declarations
  the app uses, version ignored; a used operation removed, a used field
  removed or retyped, a new failure on a used operation, or a host or
  credential scheme changed needs a `PackageChange` acceptance; `jadpo
  package update` prints the review.
- [ ] **V5 Where personal data goes** `opus · xhigh`: area 15's marking
  checked on entities, events and package types, and the personal-data
  report's list of every connection and host a marked field reaches,
  through provider operations; the same data available to `jadpo package
  check`. Erasure and retention stay where they are.
- [ ] **V6 Personal data a connection can't erase** `opus · high` after V5:
  area 15, "Erasure at providers"; area 18, `ErasureNotPossible`. A
  connection that receives a marked field, whose package declares no
  `erasure:`, needs an `ErasureNotPossible` acceptance with the reason the
  data stays; the approval report shows the personal-data report's diff.

## The public registry

The registry at jadpo.dev ([External services](language/11-integrations.md#the-public-registry)).
These chunks are taken after V2 and before every other unticked chunk,
beside the other approval chunks, and R1 to R6 are part of 0.3.0
(Richard, 2026-10-31). The chunks here build the `jadpo`
side; the registry application itself is built in the jadpo.dev
repository against [the registry protocol](language/11-integrations.md#the-registry-protocol).
R1 and R4 touch different crates and run at the same time.

Already built and used by the registry application: Workers with
PostgreSQL through Hyperdrive, the cron and queues; magic links and
passkeys; policy; jobs and events; email; rate limits; provider calls;
`ai.embed` and `Vector` fields; `hash.sha256`. It also waits for 8.2n (a
real deploy) and, before outside authors can publish, D3 (releases).

- [ ] **R1 Confirming a request with a passkey** `opus · xhigh`: area 8,
  "Confirm one request with a passkey" (Richard, 2026-10-31). Done when a
  route can't run until the signed-in person presents one of their own
  passkeys for that request, and a route test does it with
  `test_passkeys.confirm`.
- [ ] **R3 A release's facts, change record and fingerprints**
  `opus · xhigh` after V5: area 11, "What a release shows", "The
  changelog", "Search" (the fingerprints) and "Yanks, takedowns and
  advisories" (copies). `jadpo package check` prints the facts and the
  change record; the previous release's examples and tests run against
  the new code; `jadpo package verify --json`. Screens designed with the
  design skills first.
- [ ] **R4 The registry protocol and a local registry** `opus · high`:
  area 11, "The registry protocol". The protocol's requests and answers
  as types in one crate, the Merkle log and its proofs, a local registry
  that tests run against, and the conformance test that checks a served
  OpenAPI document against the protocol.
- [ ] **R5 The registry commands** `opus · high` after R3 and R4: area 11,
  "Installing and updating from the registry", "Developing for the
  registry"; area 14, the package commands. `add @publisher/name` with the
  log check and the lock's scoped `registry` line, `update`'s three-day
  wait, `--to` and urgent releases, `search`, `audit`, `publish` and
  `--dry-run`, `preview`. Every screen designed with the design skills
  first.
- [ ] **R6 The verifier** `opus · high` after R3: area 11, "The verifier".
  `jadpo package verify --serve`, fetching over git and then verifying with
  no network, and its container image published with each release (D3).
  It runs on Fly.io, reached over HTTPS (Richard, 2026-10-31); **needs a
  maintainer** for the Fly.io account before it is deployed.
- [ ] **R7 The registry application** **needs a maintainer** to start the
  jadpo.dev session, after R1, R4 and R6: built against the protocol and
  passing R4's conformance test. Until the S3 driver its archives are kept
  in PostgreSQL; until the search capability its word search is its own
  queries over each package's written account.

After the registry is live, in order:

- [ ] **R8 `jadpo suggest` points at the registry** `opus · high`: a
  finding when the app's own function fingerprints like a registry
  package's or a standard function (area 14, "Suggestions").
- [ ] **R9 Blast radius in an update** `opus · high`: `jadpo package update
  --plan` follows each change record entry through the app's graph to
  the actions, routes, jobs and stored fields it reaches, with the
  compile errors and migrations that would follow; after V4.
- Then, each with its own chunk below: signed log heads (keys held as data
  and `crypto.sign`, item 3), the S3 driver for the registry's archives
  (item 4), the search capability (item 8), and GitHub sign-in for
  jadpo.dev accounts (`Driver.github`; Google works now, through A7b).

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
   them.
4. Remaining built-in services (area 16): flags, files, audit, search,
   telemetry thresholds, with the `ThresholdChange` acceptance for a
   threshold raised or removed.
5. Debugging and review (area 17): `why`, timelines, `test new --from`,
   `call`.
6. Records and acceptances: moved to "Approvals" (2026-10-31).
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
   (Revset-inspired), acting as an OAuth
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

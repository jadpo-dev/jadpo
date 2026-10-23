# Jadpo roadmap

**Updated:** 2026-10-23. This page is the only list of outstanding work: the
delivery plan for building Jadpo from the decided language, one chunk per
session. Record decisions in the [decision register](decision-register.md)
and the [language pages](language/README.md), not here. Finished phases,
and the full entries of finished chunks, are in the
[archive of finished chunks](archive/roadmap-finished.md).

## The direction (decided 2026-10-13)

- **The language is specified** in [docs/language](language/README.md):
  twenty-one areas, all decided. Those pages are the specification and their
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

Richard, 2026-10-23: Phase 9 dependent chunks may build and run their full
gate on exact unmerged prerequisite commits, merged into their own chunk
branch in dependency order. Record those commits and dependent pull requests;
review and PR merges remain pending. Codex opens pull requests without auto
merge and never merges them. A prerequisite need not reach `main` before a
dependent chunk can complete its own build and full gate.

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
   passes, and tick it here in the same pull request. A ticked chunk is
   one line naming the sections where what it built is written
   ([compiler-runtime](compiler-runtime.md) and the area pages, which
   hold every fact, decision and measurement worth keeping); a long entry
   goes whole to the [archive](archive/roadmap-finished.md), and so does a
   phase once all its chunks are ticked (chunk 8.4h, 2026-10-22).
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

## Phase 8 — Compiled deployables

Compiled native code (8.1) and WebAssembly on Cloudflare Workers (8.2),
every test passing interpreted and compiled. A ticked chunk names where
what it built is written: § is a section of
[compiler-runtime.md](compiler-runtime.md), an area a
[language page](language/README.md); its full entry is in the
[archive](archive/roadmap-finished.md#phase-8--compiled-deployables-finished-chunks).

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
  consumer"; area 11, "Chosen in chunk 7.7c2".
- [x] **8.2l Passkeys on Workers**: §2.6, "The Workers host"; area 8,
  Decisions, "Passkeys on Workers".
- [x] **8.2m What `jadpo check` and `jadpo build` say for Workers**: area
  14, "A Workers deployable" and Decisions; area 10, "Client addresses".
- [x] **8.2m2 Rate limits' store on Workers**: area 16, "Rate limits" and
  "Chosen in chunk 8.2m2"; §2.6, "The Workers host".
- [ ] **8.2n A real deploy** `opus · high`, after 8.2a to 8.2m2, **needs
  Richard** (his Cloudflare account): a teaching example deployed with
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
8.4e (after 8.4c), 8.4f and 8.4g side by side, then 8.4h. 8.4i and 8.4j beside any of them. The gap to Bun is not accepted: Phase 9 closes
it (Richard, 2026-10-22). Claude builds these; Codex builds Phase 9
once 8.4c, 8.4e and 8.4f have merged (AGENTS.md, "Who builds what").

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
- [x] **8.4e Workers speed and memory**: §2.6, "The Workers host"; §2.11,
  "After chunk 8.4e".
- [x] **8.4f Before the real deploy**: §2.6, "Before the real deploy";
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

Richard, 2026-10-13: developers won't touch this if it's slower.

- [ ] **9.1 Benchmarks:** `opus · high` the golden todo plus focused workloads (reads,
  writes, signed-in lists, large rows, cold start) on native, Wasm under
  workerd and on Workers, against an equivalent Bun and TypeScript app and
  Node; results published with the method. Full scope remains outstanding
  through 9.1a, 9.1d, 9.1b and 9.1c, in that order; the split changes sizing,
  not acceptance (2026-10-22). This is an umbrella: sessions take the first
  unticked concrete subchunk below; tick 9.1 only once all four are complete.
- [x] **9.1a Instrumentation and focused scaffold**: `tools/bench/README.md`; compiler/runtime §2.11.
- [x] **9.1d Golden todo and local comparison**: compiler/runtime §2.6, §2.7 and §2.11; `tools/bench/README.md`.
- [x] **9.1d1 Provisioned external JWT subjects**: compiler/runtime §2.6; area 8, "Ways to sign in" and implementation status.
- [x] **9.1d2 Golden todo and executable contract**: compiler/runtime §2.6 and §2.11; `examples/golden-todo/README.md`.
- [x] **9.1d2a Generic runtime contract fixes:** §2.6; areas 5 (Lifecycle), 10 (connection URLs) and 11 (safe key windows and record modes).
- [x] **9.1d2b Golden todo and all 44 cases**: compiler/runtime §2.11; `examples/golden-todo/README.md`.
- [x] **9.1d3 Strong TypeScript golden todo**: compiler/runtime §2.7 and §2.11; `examples/golden-todo/typescript/README.md`.
- [x] **9.1d4 Local application comparison**: compiler/runtime §2.11, "Complete local application comparison"; `tools/bench/README.md`.
- [ ] **9.1b Runtime performance:** `opus · high` after 9.1d, measure the same
  local protocol before and after each improvement.
  Delivered through 9.1b1–9.1b5 below, with the warm-comparison correction
  9.1b1a between 9.1b1 and 9.1b2. This split changes sizing,
  not acceptance; sessions take the first unticked concrete subchunk.
  From the Phase 6 review: deliveries swept concurrently within each
  sweep, with a bound, and a backlog of 10,000 deliveries among the
  workloads.
  From the Phase 7 review: the tick's housekeeping measured (about 20
  statements per store on an idle tick), then split into a fast tick and
  an hourly pass one instance takes under a lease, its deletes batched;
  the AI budget's reserve and charge each one statement; a write after an
  outside call checking its version in its own `WHERE`; a delivery's
  breaker read joined into its claim.
- [x] **9.1b1 Bounded delivery sweeps and the backlog**: compiler/runtime §2.6, "Bounded delivery sweeps", and §2.11; `tools/bench/README.md`.
- [x] **9.1b1a Warm delivery comparison**: compiler/runtime §2.11, "Warm durable delivery comparison"; `tools/bench/README.md`.
- [ ] **9.1b2 Fast ticks and hourly housekeeping:** `opus · high` after
  9.1b1a: measure idle-tick statements and latency, separate prompt work from
  an hourly cleanup pass one instance takes under a lease, and batch every
  delete. Preserve retention, handled-record safety, failure recovery and
  multi-instance behaviour; report before/after measurements.
- [ ] **9.1b3 One-statement AI accounting:** `opus · high` after 9.1b2:
  make each reserve and charge one statement, including every applicable
  window. Preserve atomic refusal, concurrent limits, repair/retry top-ups,
  usage accounting and conservative failure handling; measure statements
  and latency before and after, using local fixtures without real accounts.
- [ ] **9.1b4 Version checks in writes:** `opus · high` after 9.1b3:
  check a row loaded before an outside call in the write's own WHERE.
  Preserve first-loaded versions, row/policy refusals, repeated writes,
  transaction retries and keyed/unkeyed-call safety; exercise real races
  and measure the same protocol before and after.
- [ ] **9.1b5 Breakers in delivery claims:** `opus · high` after 9.1b4:
  join a delivery's breaker read into its claim. Preserve multi-connection
  holds, the one-probe lease, retry budgets and release/claim races; measure
  statement counts and delivery performance before and after. Tick 9.1b
  only when all five subchunks meet its original acceptance.
- [ ] **9.1c Workers benchmark results:** `opus · high` **needs Richard**
  after 9.1b: approve access and deployment before using his account. Run the
  golden todo plus focused workloads on Workers against the equivalent Bun and
  TypeScript app and Node; results published with the method. Include the
  10,000-delivery backlog and the runtime improvements' before/after results.
  Real edge measurement is required before Phase 9 finishes.
- [ ] **9.2 Optimise WebAssembly** `opus · xhigh` after 9.1a, 9.1d and 9.1b, until it matches the Bun app on throughput
  and p95, or Richard accepts a measured gap. The earlier experiments reached
  about 79% of Bun's throughput and list what to try next
  ([optimisation results](experiments/wasm-optimization-results.md)).
  From chunk 8.1p: typed values in compiled bodies (a struct or enum per
  type, locals and parameters typed, a `Value` made only at the runtime's
  entry points), if profiling shows a body's own work matters beside the
  request's edge and the store; an answer's body sized before it is
  written, and a UUID's or an instant's text written without a string of
  its own (17 allocations write example 23's ticket either way).
  From the Phase 8 review: the module is the runtime, not the app (the
  generated program 32 KB of 13.4 MB): compiled code calling each
  standard operation's own function, `effects::standard` the
  interpreter's dispatcher over the same functions, and the runtime's
  subsystems (HTML, regex, Markdown, geodesy, batches, webhooks, sign-in)
  cargo features chosen from the plan, as `postgres` and `smtp` are;
  validators' patterns compiled when first used if 9.1 shows cold start
  pays for them.
- [ ] **9.2b One passkey implementation** `opus · xhigh`: 8.2l's pure-Rust
  checks used by both hosts, `webauthn-rs` kept only as a test reference,
  the test authenticator on `p256`, so no native deployable links
  OpenSSL and passkey route tests run on workerd.
- [ ] **9.2c `jadpo build` outside this checkout** `opus · high`: the
  runtime's sources and crates found without the path `jadpo` was
  compiled at or a warm cargo cache, so CI or a released `jadpo` can
  build a deployable.
- [ ] **9.3 The differential run's last two checks** `opus · high`
  (after 9.2; low priority, from chunk 8.4h): each path through a body
  numbered and the paths each run took compared, and fuzzed function
  bodies run both ways, as §2.5 once promised.

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

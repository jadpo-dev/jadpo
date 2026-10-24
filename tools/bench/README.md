# Performance harnesses

## Durable delivery backlog (chunks 9.1b1–9.1b1a)

`deliveries.py` measures a focused database-only subscriber, separately from
any golden reminder/provider workflow. Commit the inputs first; each build
requires a clean checkout, builds its own compiler, and records the exact
commit, source/artifact SHA-256 maps, tool versions and flags. Missing or
changed inputs/artifacts are refused before and after measurement. Each label
retains a hashed `compiler-jadpo` copy and a `build.json` naming its original
compiler path, source and artifact map before another build can overwrite it.

```sh
python3 tools/bench/deliveries.py build --label serial
python3 tools/bench/deliveries.py measure --label serial --rounds 1 --count 100
python3 tools/bench/deliveries.py measure --label serial
# After committing a runtime change, build and measure a new label.
python3 tools/bench/deliveries.py build --label bounded
python3 tools/bench/deliveries.py measure --label bounded
```

The default `--mode warm` protocol is three rounds rotating native, workerd,
Bun and Node. Each measured cell starts with exactly 10,000 durable deliveries/
events in disposable PostgreSQL 16 (`synchronous_commit=off`, 400 connections,
256MB shared buffers); 100 ticks of at most 100 drain the backlog. Workerd,
Bun and Node each start one persistent process per cell. Bun and Node use the
same ordinary `node:http` compatibility host, `POST /tick`, event loop and
postgres-js pool capped at four. This specialized Bun host differs from the
strong golden app's native `Bun.serve` binding. Workerd invokes the production
generated `scheduled` handler through local HTTP and a service binding with
`service_binding_extra_handlers`; its Hyperdrive binding reaches the release
transaction-pool stand-in, cap four. No JSONL controller or virtual clock is timed.

An empty readiness tick is excluded. Before **every** measured cell, a real
100-delivery cohort must pass the durable-state audit; the harness then resets
to the exact measured seed. This ordinary-work warmup initializes the persistent
services' pools/JIT outside timing. Negative/race controls run once per target
per campaign, before its first warmup. The clock, lease, lane cursor and counts
are recomputed per tick; measured Bun/Node replies must keep the same owned PID.
HTTP calls have a 120-second deadline; any failure invalidates the cell, retains
its attempt, and stops/reaps the owned services. TypeScript drains every admitted
lane before answering an error and closes its pool on graceful shutdown.

Native uses its ordinary external `app tick` process every time, including
startup. Its warmup cohort cannot warm subsequent processes: it is supporting
external-tick context, outside the warm-service comparison. There is no equivalent
ordinary native HTTP tick host in this fixture.

`--mode external` retains the original per-tick Bun/Node process starts and
empty-readiness protocol. The 9.1b1 raw results and their exact source revisions
remain in compiler/runtime §2.11; their per-target serial/bounded comparisons
are valid, but persistent workerd versus fresh Bun processes is not a warm-service
comparison. Only compare serial and bounded builds under the **same** final
mode, harness and warmup protocol. Record invocation lifetimes next to the numbers.

The fixture's real calibration POST must return the exact 202 ticket response
and commit one handled receipt. Its actual event/subscriber keys and encoding
seed the backlog, with fixed UUID delivery/event identities and ticket payloads.
Each successful delivery creates a UUIDv7 receipt and commits its handled
marker in the same transaction. Audits require exact 10,000 receipt tickets,
valid UUIDv7 identifiers, exact handled/succeeded/attempt counts, no missing or
unexpected effects, and an unchanged fingerprint of the joined event/delivery
identities, payloads, encodings, times, trace/depth and subscriber mapping.
One post-window replay cohort must add attempts without adding effects.

Before timing each target, real controls exercise malformed payload parking,
a refused handled-marker insert rolling back the preceding receipt, live and
expired leases, and two simultaneous sweeps. The ledger refusal must leave
pending/unavailable retries and no writes: area6's atomic transaction and
area12's retry-on-unavailable, implemented by the existing held-commit and
outbox retry paths. It differs from the receipt's authored unique-conflict
failure. Verifier unit faults reject missing/duplicate effects and stale builds.

The specialized `typescript/deliveries.ts` is identical on pinned Bun/Node,
using postgres-js with pool 4 and uuid 13's UUIDv7 generation. It validates
payloads, conditionally claims/fences a 16-minute lease, observes the fixture's
10-attempt/24-hour window, atomically writes receipt+handled marker, and handles
ledger refusal conservatively. This is a focused comparator; it supplies no
full application, auth, tenants, provider or generic-language proof. Its service
shutdown controls do not establish generic Jadpo shutdown equivalence.
Jadpo's existing housekeeping is included in its tick, while the specialized
comparator only drains deliveries. The strong golden TypeScript remains the
separate application comparison. These measurements add the delivery path;
they do not replace or repeat the unchanged 9.1d4 HTTP matrix.

Builds use `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=4`;
production native/Wasm use ordinary `jadpo build` release profiles. All owned
builds/tests stop before timing. Unrelated apps remain running; load and whole
host CPU counters, per-tick durations/responses, external tick-child CPU,
persistent service CPU and service RSS at cell end are retained. Workerd pool
CPU is separate and also summed with workerd for the server-CPU comparison;
Bun/Node include their pool in-process. PostgreSQL and Python CPU are excluded
from those server numbers. Five-second ambient and per-cell whole-host counters
retain unrelated load. Tick p95 is nearest-rank over the 100 ticks in a cell;
a median of those three p95s is not per-delivery or HTTP request latency. Raw attempts and failed setup/smokes stay under
ignored `work/deliveries/<label>/runs-*.json` with logs; none is overwritten or
converted into a passing result. Three busy-host rounds are exploratory and
do not establish edge capacity, a latency SLO or an inherent language win.
The superseded initial `before` protocol used UUIDv4 in the TypeScript receipt;
only matched UUIDv7 pairs support that original external-mode comparison.
The warm-service pair and all-round variation are recorded in compiler/runtime
§2.11, "Warm durable delivery comparison". The new behavioral PostgreSQL/HTTP
controls run in an isolated full-gate step (`delivery_services.py`), after their
own pinned install/build; lint and quick profiles do not run live services.
Shared gate URI/keyword/socket DSNs reuse `tools/golden-todo/postgres.py`, preserve
endpoint/quoted startup settings, and never fall back to another server. Review
must retain these controls, exact state/fingerprint/replay audits and strict
source/artifact guards. Changes to a measured path require the affected matched
campaign again, with unfavorable attempts retained. Reproduce a historical
pair from its exact clean source revisions; later method edits do not authorize
reusing that pair's build stamps at a different HEAD.

## Idle ticks and hourly housekeeping (chunk 9.1b2)

`idle_ticks.py` measures generated native external ticks and a persistent local
workerd scheduled handler against empty business state, separately from warm
delivery throughput. Use the same clean build/source/artifact guard and compiler
snapshot as `deliveries.py`; commit inputs before building, finish/reap all
builds/tests, and record host/process context before timing.

```sh
python3 tools/bench/deliveries.py build --label idle-reference
python3 tools/bench/idle_ticks.py --label idle-reference --ticks 5 --rounds 1
python3 tools/bench/idle_ticks.py --label idle-reference --ticks 100 --rounds 3
python3 tools/bench/deliveries.py measure --label idle-reference --mode warm --count 100 --rounds 1
python3 tools/bench/deliveries.py measure --label idle-reference --mode warm --count 10000 --rounds 3
```

The default idle protocol alternates Workers/native order over three rounds,
100 samples per cell after ten excluded empty ticks. It requires one actual
successful `runtime.ticked` per sample, with no problems, ran schedules,
deliveries or deletions, plus unchanged empty business state. A Workers
scheduled outcome `ok` alone does not establish this. A real negative control
renames the deliveries table, requires the runtime audit to reject the failed
tick, restores it, and audits success. Unit faults reject missing/duplicate/
failed ticks in the gate; no production instrumentation or fast runtime path
is added for the harness.

Each cell separately enables PostgreSQL statement logging for one audited
census tick, retains the SQL/response, excludes migration/readiness/admin
startup and requires the prompt due-delivery SELECT and breaker UPDATE.
Logging is disabled before the latency loop. The count is for this fixture's
completed-hour fastpath; a due hourly pass still performs bounded retention
plus lease claim/release. The final paired census is four unchanged prompt
operations plus four reference cleanup statements (8), versus those four plus
one declining conditional housekeeping claim (5). Other schema holdings can
add work. Bun/Node generic housekeeping/idle metrics are N/A: the specialized
TypeScript comparator drains deliveries only.

Latency uses `perf_counter`, p95 nearest-rank over the 100 ticks; native includes
100 ordinary process starts while Workers stays warm. Resource/service/helper
CPU snapshots bracket only the timed loop, excluding DB state/log audits and
psql children. Workerd's separate pool helper must be included in server CPU;
PostgreSQL/Python are excluded. Short service CPU samples are quantized. No
wall-clock speed assertions run in CI. Raw `idle-<timestamp>.json` retains
per-tick replies/logs/durations, control/census evidence, counts, CPU/load,
empty-state audits, exact source/artifact map and end freshness.

Retain separate timestamped smoke/full attempts; never overwrite a failed
result. Rebuild/repeat both sides when the measured harness/protocol changes.
For b2 the fresh reference's production `crates/*/src` and all generated schema
fixture inputs exactly matched prerequisite cb1, retaining the final measured
harness; the restored candidate's complete tracked tree/modes matched its
passing full gate. Sources, trees, compiler/artifact hashes, all rounds/CPU,
raw paths/digests and every superseded preparation are in compiler/runtime
§2.11, "Fast ticks and hourly housekeeping: matched measurements".

The earlier b2 reference was repeated after proven test-helper workerd orphans
were stopped. Those came from an npm wrapper lifetime bug and a native failed-
readiness child without kill+wait ownership; actual failure-path regressions
cover the narrow fixes. A clean benchmark shutdown is not proof that every
other test descendant stopped: record exact owned executable identities after
the full gate and before/after timing. Preserve unrelated apps/services and
record ambient CPU/load, memory/swap and control drift. The b1/b1a historical
results remain exploratory under an unknown orphan-host condition, and the
separate 8.4e resident-memory question is unchanged. Idle SQL reduction is the
primary b2 result; observed small warm-delivery changes under varying load do
not establish a causal throughput win or full TS/Jadpo housekeeping equivalence.

## AI accounting (chunk 9.1b3)

`ai_accounting.py` builds a release Rust executable calling the public
`jadpo_runtime::ai::generate` path with `AiFake` and real disposable local
PostgreSQL. This is a native runtime microfixture, separate from generated
application HTTP throughput. Workers/Bun timings are N/A: the existing
specialized delivery comparator has no AI path. The actual compiled Worker
refusal regression supplies separate correctness evidence.

```sh
python3 tools/bench/ai_accounting.py build --label ai-reference
python3 tools/bench/ai_accounting.py measure --label ai-reference --calls 500
# Restore/commit the entire final gated candidate tree, including modes.
python3 tools/bench/ai_accounting.py build --label ai-candidate
python3 tools/bench/ai_accounting.py measure --label ai-candidate --calls 500
python3 -m unittest discover -s tools/bench -p test_ai_accounting.py
```

Both sides must use the same final harness. The reference restores production
`crates/*/src` from its exact prerequisite; the candidate restores the entire
passing gated tree, proven by `git write-tree` before its own build. Each clean
build records HEAD, full tracked tree/modes/content, compiler and fixture
hashes, and retains both executable snapshots. NUL-delimited Git index paths
are read literally; symlinks hash their target text, including directory links
and Git's materialized-link checkout mode. A changed HEAD, tree, mode or
original/saved executable refuses timing before and after. Historical build
stamps never authorize a new checkout. Retain selected inherited compiler/
release-profile environment beside each build; the wrapper explicitly sets
`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=4` and otherwise
uses normal dev compiler/release fixture profiles. The compiler is provenance
context and is excluded from the timed path.

Three rounds alternate app/caller/retry order. Each cell resets state, runs
missing-table and full-budget controls, then warms 50 calls. One separate
census call retains all `query_once` SQL/parameters and corroborates the count
with PostgreSQL statement logs. The measured connection must observe
`SHOW log_statement = all` before that census and `none` before timing; admin,
readiness, logging and state audits are outside latency. The local test server
has `fsync=off` and `synchronous_commit=off`; no account-configured database or
provider is used. The fixed prompt estimates seven input tokens, with
`max_output:20`, `attempts:1`, `timeout:1000ms`, request cap1000 and app/caller
caps10^12. Prices are absent. The fake reports six input, one output and two
cached tokens. Retry-caller scripts one unavailable send followed by success;
it is charged 13 input, one output and two cached tokens. `Clock::TEST` has no
monotonic timer here, so retry backoff is skipped: these are immediate fake
resends isolating accounting, not provider latency or real retry wall time.

The default is 500 measured calls per cell. Each sample brackets only
`ai::generate`; p95 uses nearest rank. **Fixture calls/s** also includes each
call's Ctx/AiFake preparation and post-call answer/request-count assertions.
It is not HTTP/service capacity. Exact APP/CALLER/PROMPT counts, caller digest,
reported/cached tokens, zero remaining holds, currency and cost are validated
at warmup50, census51 and final551 calls. Timing uses logging off and requires
the same query count as the census. Missing accounting storage must stop
before asking the fake; full budget must refuse without requests or state
changes. Partial JSON is incomplete until all nine cells pass. Unit controls
reject incomplete/corrupt state, false controls, wrong census/quantiles, stale
source/artifacts, retargeted links and changed modes. There are no speed
assertions in the gate.

Finish/reap all owned builds/tests/services before timing and record bounded
exact-path process inventories, five-second ambient CPU/load, memory and swap.
Whole-fixture child CPU includes setup, warmup, controls, census and reaped
watchdog/client children; daemonized PostgreSQL backend CPU is excluded and
unmeasured. It is not per-call or service CPU. Retain every failed/superseded
attempt and all rounds under ignored `work/ai-accounting/`. The paired sources,
artifacts, all-round SQL/latency/fixture-throughput, the retained initial
app-only regression and the final single-window comparison are in
compiler/runtime §2.11, “One-statement AI accounting”. Host-load drift
prevents a universal speed claim. Changing the measured path or harness
requires fresh own builds and the affected matched campaign again.

## Golden application comparison (chunk 9.1d4)

`golden.py` uses the complete `examples/golden-todo` application and its
strong TypeScript counterpart. Before any timing it runs all 44 current
`todo-v0.2` cases on each of native, workerd, Bun and Node: 176 passing
obligations, from this checkout and its compiler. Missing, duplicated,
unsupported, failed, sliced or preview conformance cannot authorise a run.
The conformance adapters expose controlled clocks/providers and SQL observation
for correctness. They stop before measurement. Their debug/testing binaries
and JSONL controller transport are never timed.

```sh
# Preparation can compile and run correctness while the machine is busy.
python3 tools/bench/golden.py --prepare-only
python3 tools/bench/todo.py --prepare-only
# Run only with no compiler or other benchmark running.
python3 tools/bench/golden.py --skip-build
python3 tools/bench/todo.py --skip-build --routes read,large
```

Both runners reject missing build identities, changed source/compiler inputs
and changed production artifacts under `--skip-build`. Preparation records
the hashes in ignored `work/golden/build.json` or `work/todo/build.json`;
these are reusable build cache stamps, not a substitute for the repository gate.
Comparative timing stays outside CI. `tools/verify.py` runs the fast verifier,
proxy and real-load fault controls, including corrupt success bodies, owner
leaks, missing persisted writes, unchanged/no-op writes, nonfinite or empty
cells, unsupported targets and stale builds.

### Timed application work

| Workload | Request | Verified work |
|---|---|---|
| `read` | `GET /todos/<id>` | Real bounded signed bearer; the caller's exact six-field todo |
| `signed-list` | `GET /todos?page_size=100` | Exactly the owner's 50 visible rows, `created_at desc,id desc`, full six-field projection and null next cursor |
| `write` | `POST /todos` | Real bounded signed bearer; validated title, generated UUIDv7, open status, null due date, creation/update timestamps and a committed row for that exact caller |
| cold start | `GET /health/ready` | Spawn through the complete first valid database-backed readiness answer |

Every cell starts its server afresh and resets the database outside timing.
There are 256 active owners by default, a separately signed durable API
session for each, 50 visible and one hidden todo per owner, and a foreign
owner's 50 visible and one hidden todo. Each credential stays attached to
its read path, complete expected response and write-owner identity. Before
load, every owner's expected response and the missing/forged credential and
invalid-title failures are checked. Seeded rows are checked unchanged after
load. For every successful write, including warmup and drained requests,
the load generator retains the complete six-field output and the exact
credential's owner; each receipt must match its persisted row. Counts alone
cannot establish write correctness. Receipts are discarded after this audit;
results retain the verified count, never credentials.

Credentials use the production signing/verifier codecs and the same real
persisted identities and authority rows. Fixture signing happens before the
HTTP process starts; bounded authority lasts four minutes within the app's
five-minute limit. Setup must leave enough lifetime for measurement and its
maximum drain. Timed routes retain normal auth, output codecs, owner policies,
lifecycle visibility, deadlines, transaction safety and the default 600/minute
caller and 120/minute address limits. Accepted signed requests count against
the caller; unsigned/refused credentials count against the address. A 429
invalidates a cell: increase `--principals`, not the limits. The native/TS
trusted metadata fixture rotates a bounded 16,384-address pool.

Only loopback listeners are opened. For native, the benchmark explicitly
declares one trusted proxy in the copied application:
`client_address: { source: ClientAddressSource.forwarded_for hops: 1 }`;
the small TypeScript transport host trusts one strict IPv4 X-Forwarded-For
value only when the real TCP peer is 127.0.0.1, ::1 or ::ffff:127.0.0.1.
Untrusted peers and malformed/multiple forwarded values cannot supply the
caller address. This declared local transport fixture preserves rate limits;
it is not a claim that arbitrary forwarded headers are trusted in deployment.
Workers requires `ClientAddressSource.cloudflare` instead. Its workerd
incoming socket uses the pinned runtime's `injectRequestHeaders` to remove
any caller-supplied CF-Connecting-IP and set 127.0.0.1, the actual local peer.
A real workerd echo control proves missing and forged headers are overwritten.
This is the local platform metadata fixture, with no additional HTTP forwarder
or app/controller hop. Authenticated golden traffic still counts against each
real signed caller, and its declared local rate-limit bindings retain the
default allowances; unsigned failures retain the normal address allowance.

Native and Wasm are ordinary release deployables from `jadpo build`.
workerd runs the generated shim and its production request handler unchanged;
an additional isolated entrypoint reads the actual module's linear memory,
as in the spike below. Bun uses `Bun.serve` and Node Hono's Node adapter, both
binding the same emitted strict TypeScript production application. The
transport host supplies the declared proxy boundary and external scheduling;
it imports no contract controller, SQL recorder, fake app or test hook.
The complete contract checks reminder delivery, lifecycle, provider failures
and authentication timelines before timing. The measured requests above do
not run a reminder workflow. The Worker declares the required subscriber and
schedule runners; the local configuration supplies no cron. Scheduling is
external and no due reminders are
seeded on any measured target; 9.1b owns idle ticking and the 10,000-delivery
backlog.

The golden contract bounds a title to 200 Unicode scalars, so its large-row
comparison remains the distinct 9.1a `read,large` scaffold below: 100-byte
versus 65,536-byte stored payloads. Those routes use the scaffold's separate
app and TypeScript baseline; they are not golden conformance or whole-app
throughput. Both use full-body checks on production deployables.

### Configuration, sampling and limits

All databases share one disposable local PostgreSQL 16 instance, with
`synchronous_commit=off`, 400 maximum connections and 256MB shared buffers.
Each app first applies its own real migrations. Then a benchmark-only schema
fixture gives every `todo` table the same four business indexes from Jadpo's
generated schema: created/id, owner/created/id and status/created/id partial
on visible rows, plus owner. Constraint indexes are retained. The exact
normalisation DDL and its SHA-256 are in results. This controls query/write
index work; it does not represent natural Drizzle migration output, which
has a single nonpartial owner/created/id index. Runtime-owned tables and
their migrations remain each implementation's own.

Primary measurements preserve the apps' actual pool architecture. Jadpo's
native driver permits 10 open/8 idle connections per driver, with separate
drivers on native worker threads. TypeScript has five general connections
and five exclusive transaction clients. The single workerd isolate uses a
stand-in transaction pool with a cap of 10; unlike the historical repair-café
spike's cap of 20, this matches the driver's per-pool open cap. The pool
stands in for Hyperdrive, without edge latency/TLS or query caching; its
CPU is recorded separately. Native uses multiple server worker threads;
workerd, Bun and Node serve on one each. Throughput and CPU/request both
matter when interpreting these different architectures.

Defaults are three rounds in rotating target order, 1/16/64 connections,
two seconds of warmup and ten seconds measured per cell. Latencies retain
every request admitted during measurement, including responses after the
admission window closes. Throughput divides completions by that window plus
its drain, and server CPU sampling continues through drain. Complete requests
have a 30-second deadline and connections a 10-second deadline. Any warmup
or measured error/status/body mismatch refuses the cell. CPU sampling uses
the operating system's cumulative process counters at the warmup boundary
and after drain, with their finite sampling granularity. RSS is sampled every
250ms. Cold start includes first readiness body validation; its first request
time is also recorded. Fixture/credential preparation and the workerd pool's
startup are outside cold-start timing.

Ignored `work/golden/results-<run>.json` contains all cells, cold starts, median/
min/max per metric, p50/p90/p95/p99/max, drain, statuses/error counts, server
and helper CPU, sampled peak RSS, Wasm linear heap when present, load-generator CPU/
wall time/thread count, machine/tool versions, source/lockfile and generated
artifact hashes, exact configuration and conformance case identities. Only
validated cells can appear in an `ok:true` result, and stale inputs at the end
refuse the whole run. The load generator and PostgreSQL share the machine;
reported load-generator CPU, whole-machine CPU utilisation and start/end load expose potential client/host
saturation. This local workerd result does not establish edge performance.
Every invocation uses a new result path; failed/error rows and unfavourable
runs remain available. A supplied existing output path is refused. Summary
latencies are medians of the per-run percentiles, with their min/max; they
are not pooled percentiles. The workload is closed loop: each connection
waits for a response before sending again, so an overloaded server reduces
offered traffic. These three short rounds are exploratory local comparisons,
not open-loop latency SLOs, stable p99 qualification or production capacity.
Worthwhile measurements and their interpretation belong in compiler-runtime
§2.11, alongside the older experiments below.

## Repair-café performance spike

Roadmap chunk 8.3: one route of a teaching example compiled to native and
to WebAssembly and measured against the same route hand-written in Bun and
TypeScript. The results and what they mean are in
[docs/compiler-runtime.md](../../docs/compiler-runtime.md), "The performance
spike". This is a spike's harness, not Phase 9's benchmarks: it is not in
the gate.

```sh
python3 tools/bench/run.py                 # build everything, then 3 rounds
python3 tools/bench/run.py --skip-build --rounds 5 --duration 15
python3 tools/bench/run.py --skip-build --profile   # where workerd's time goes
python3 tools/bench/run.py --skip-build --soak 600 --targets workers   # memory over a long run
```

It needs Rust (the repository's pinned toolchain), PostgreSQL 16's
`initdb`, `pg_ctl` and `psql` on `PATH` (or Postgres.app), Bun, and Node's
`npm` for workerd. Everything it builds, installs and starts stays in
`tools/bench/work/`, `tools/bench/load/target/`, `tools/bench/bun/node_modules/`
and `tools/workers/node_modules/`; delete them afterwards. Nothing leaves
the machine while it measures.

## What it measures

Teaching example 23 (the repair café) with a staffed bench, so a booking
runs its whole `settle:` path: `POST /repairs` validates the request,
books the repair and its `RepairBooked` event in one transaction, runs
`find_a_bench` and then `print_on_assignment` while the request waits
(each claiming its delivery, running its action in a transaction and
recording it), then reads the ticket back and answers 201: 12 statements
in four transactions, each with its `BEGIN` and `COMMIT`, and 6 outside
them, 26 round trips to the database. `GET
/health/ready` is the readiness check the runtime answers for every app:
outside the rate limits, its `SELECT 1` reused for a second, so it is
the request path with almost no database work.

Four servers, one at a time, each with a database of its own on one local
PostgreSQL 16 (`synchronous_commit` off, trusted on `127.0.0.1`):

| Target | What it is |
|---|---|
| `native` | `jadpo build`'s binary for the example, a worker thread per core. `client_address:` reads `X-Forwarded-For` (one proxy), so the load's per-request addresses reach the rate limit |
| `workers` | `jadpo build`'s Workers module for the example declared as a `Target.wasm` deployable, served by the pinned workerd as the Workers tests serve it, its Hyperdrive binding given to `load/src/bin/pool.rs` |
| `workers-direct` | the same, its Hyperdrive binding given straight to PostgreSQL, as workerd and `wrangler dev` do |
| `bun` | [bun/server.ts](bun/server.ts): the same route by hand with postgres.js (pinned in `bun/bun.lock`), the same validation and error bodies, the same SQL in the same transactions at the same isolation levels (taken from PostgreSQL's statement log of the native build), the same 120-a-minute limit per address and the same answer |

`pool` stands in for Hyperdrive. A Worker opens a database connection for
each request, since a socket belongs to the request that opened it; on
Cloudflare that connection goes to Hyperdrive, which answers its start-up
itself and runs its statements on connections to the database it keeps
open. workerd's own Hyperdrive binding hands `connect()` straight to the
database instead, which starts a backend process for every request.
`pool` answers the start-up and lends a pooled connection by transaction,
as Hyperdrive does (no TLS, no caching).

Each round starts each server afresh, timing its spawn to its first
readiness answer (its cold start), then drives each route with
[load](load/src/bin/load.rs) at 1, 16 and 64 connections: each connection
keeps one keep-alive connection and sends its next request when the last
is answered, for 2 seconds unmeasured and 10 measured, every request
from an address of its own. While it runs, the harness samples the
server's CPU time (`ps`) and memory (resident set, every 250 ms), and
after it a Worker's linear memory, the WebAssembly module's own (chunk
8.4e): workerd serves the kiosk with a main module of the harness's own
that re-exports the shim `jadpo build` wrote, so requests reach its
handlers unchanged, and adds one entrypoint, `JadpoMemory`, which a
second Worker on a port of its own calls to read the size of the
module's memory. Rounds rotate the servers' order; the summary gives each
measure's median over the rounds and its range. `work/results.json` keeps
every run.

`--soak SECONDS` books on each target for that long instead, at 16
connections, a window of `--soak-step` seconds (30) at a time, printing
after each window the bookings so far, its throughput and CPU a booking,
the peak resident memory in it and the linear memory at its end;
`work/soak.json` keeps them. A module that keeps something of every
booking shows it here as linear memory that grows with the bookings
rather than settling at its working size.

## What the numbers are not

- workerd on this machine is not Cloudflare's edge: no network between the
  Worker and Hyperdrive or between Hyperdrive and the database, a
  stand-in pooler, and one isolate on one thread. Cloudflare runs many
  isolates; the per-isolate numbers are what the comparison can say.
- Bun and workerd use one thread each; the native binary uses a thread per
  core. Per-core cost is the CPU milliseconds a request, not requests a
  second.
- The load generator, PostgreSQL and the server share one machine (an M1
  Pro, 8 performance and 2 efficiency cores), and the machine may be
  running other work: each run records its load average.
- The Workers module is as `jadpo build` writes it: no `wasm-opt` (9.2's).

## Where the time goes

`--profile` runs each workerd target's routes at 16 connections
with workerd's V8 inspector on and samples its CPU profile through
[bun/profile.ts](bun/profile.ts), printing the share of samples by kind and
the functions with the most self time; the profile itself is written to
`work/<target>.cpuprofile`, which Chrome's DevTools opens.


## Generic focused microbenchmark scaffold (chunk 9.1a)

`todo.py` builds ordinary Jadpo sources in `fixtures/todo` and the strict
TypeScript source in `typescript`, then runs native, Wasm under workerd,
Bun and Node **one at a time** against one disposable local PostgreSQL 16.
The repair-café spike above remains available as `run.py`; its fixture,
routes and output location are separate.

```sh
python3 tools/bench/todo.py
python3 tools/bench/todo.py --skip-build --rounds 3 --duration 10
python3 tools/bench/todo.py --skip-build --rounds 1 --duration 1 --warmup 0.2 --connections 1
```

This is a generic todo-shaped microbenchmark, **not a golden-todo
implementation or conformance subset**. It has registration, create and a
50-row list, plus public small/large row reads. Golden `LIST-001` instead
requires `created_at desc, id desc`, soft-delete filtering and cursor pages;
this fixture orders by title and returns a bare array. Golden `REL-004`
restricts deletion of users with retained todos; this fixture cascades.
Golden `PUBLIC-001` permits only public app liveness; this fixture has public
registration/sample routes. Golden `CREATE-003/004` due-date rules are absent.
None of the 44 golden acceptance cases is claimed as passed by this scaffold.

Roadmap 9.1 retains the original full scope. Its order is 9.1a instrumentation,
9.1d actual golden todo and strong TypeScript baseline with all 44 contract
cases and local comparison, 9.1b measured runtime improvements including the
10,000-delivery backlog, then 9.1c real Workers measurement with Richard's
account/deployment approval. The existing repair-café subscriber benchmark
remains reproducible; these short routes do not establish whole-app speed.
No comparative performance result is published for this partial scaffold.

### Exercised work

| Workload | Request | Checked work |
|---|---|---|
| `read` | `GET /api/samples/<small-id>` | One stored row, 100-byte payload, exact JSON |
| `large` | `GET /api/samples/<large-id>` | One stored row, 65,536-byte payload, exact JSON |
| `signed-list` | `GET /api/todos` | Real bounded signed bearer; exactly 50 owner rows ordered by title in `C` collation then id |
| `write` | `POST /api/todos` | Real bounded signed bearer; validated title, UUIDv7 identity, one transaction and returned id/title |

Every cell resets and seeds its database before the measurement. Signed
cells have `--principals` owners (256 by default), 50 todos each, and another
owner's 50 lexically earlier todos. Public read cells use one owner plus
the other owner. Each principal has an independent real credential; the
load rotates a credential and its own exact expected response together.
That preserves the built-in 600/minute caller limit. Both public and
signed requests rotate `--address-pool` client addresses (16,384), preserving
120/minute per address while bounding local rate-limit state. The TypeScript
app applies the same limits. A 429 is a failed measurement; increase a pool
if necessary, rather than treating a refusal as throughput.

The runtime issues an opaque token initially; setup authenticates it once
and captures `X-Jadpo-Token`, so timed signed requests exercise the no-read
bounded signed path. The idiomatic TypeScript app uses JOSE HS256 tokens;
Jadpo uses its BLAKE3 signed credentials. Both have a five-minute bound,
zero authority reads in timed traffic, and the same closed user principal
and owner policy. Each cell audits the signed list's PostgreSQL log: exactly
one business SELECT and no session/principal query after renewal. Statement
logging is disabled for measurement. Missing, forged and expired credentials
are refused in setup. Expiry tests use an expired opaque session for Jadpo
and an expired JOSE bearer for TypeScript; expiry of Jadpo's signed part is
a renewal boundary, not necessarily expiration of its underlying session.
Setup must leave enough signed lifetime for the whole cell and drain.

The TypeScript baseline uses Hono, strict Zod request/output/configuration
schemas and generated OpenAPI, Drizzle typed queries, an explicitly typed
CASL policy boundary, JOSE, safe error mapping and redacted fault logs.
Bun runs its TypeScript; Node runs TypeScript's emitted JavaScript from the
same source. Queries select only the output columns, with a mandatory owner
predicate supplied beside policy checks. A single statement reads its own
consistent snapshot; writes use Drizzle transactions. The physical schema,
indexes and constraints are the same Jadpo-generated fixture DDL on all
targets, so migration generators cannot accidentally create different
indexes. Separate Drizzle-kit migrations, whole-app inventories, the full
agent/lint framework and the actual golden contract are outstanding in 9.1d;
the current scaffold does not fulfill the full strong baseline specification.

Native startup checks its migration ledger. The TypeScript app checks all
required application columns before accepting traffic. Readiness reaches the
database and is reused for a second on all servers. Those startup checks
have different implementations; cold start records their actual cost.
Background ticking is disabled with the language's existing external-tick
setting, and the workerd configuration has no cron: 9.1b owns background
costs. On workerd the checked-in rate-limit binding stub and transaction
pool stand in for Cloudflare bindings; this is local workerd, not the edge.

### Measurement and failure rules

Defaults are three rounds, rotating target order, at 1, 16 and 64 connections,
2 seconds of warmup and 10 seconds measured. Servers restart per target per
round; database reset, registration, renewal, seeding and correctness probes
are outside timing. Cold start is spawn to first readiness response, and its
request time is recorded separately. A comparative run must have no compiler or other benchmark running. The
16-cell short smoke verified instrumentation only; final comparison is held
until the actual golden application and baseline satisfy 9.1d.

A request admitted in the measured interval keeps its latency even when it
finishes after the interval. Throughput divides measured completions by the
interval **plus its drain**. Server CPU is sampled after warmup through that
drain and divided by the same measured completions. Connect attempts have a
10-second deadline; complete requests a 30-second deadline. Measured and
warmup errors are counted separately and either invalidates a cell. Every
read body is parsed and compared in full; every write body must have exactly
the requested title and a generated UUID. Persisted writes, including warmup
and boundary requests, must equal every successful 201 across the owner pool.
Read cells must leave the stored count unchanged. Corrupted 200 responses,
owner leaks, wrong ordering, missing writes, empty cells and nonfinite or
negative metrics fail verification. No timing thresholds run in CI.

`work/todo/results-<run>.json` records every cell, p50/p90/p95/p99/max, status/error
counts, drain, CPU, peak RSS and Wasm memory, load-generator CPU/wall time and
thread count, machine/tool versions, source/lockfile SHA-256s and native/Wasm
artifact SHA-256s. Credentials remain only in ignored `work/todo` setup files,
never metadata. The generator normally uses two threads; its CPU is reported
so a client-bound cell can be identified. PostgreSQL, pool, load and server
share this host, so these are local costs with finite headroom. Native uses
multiple worker threads; workerd, Bun and Node use one server thread. Compare
server CPU/request as well as throughput; local workerd includes a stand-in
pool and no real edge/database network.

The load generator's schema 2 added complete-body checks, p95, bounded requests
and retained slow tails. This changes its protocol/client cost, including for
new `run.py` runs: compare targets using the same generator version and rerun
old experiments before comparing numbers. The old spike's distinct-IP stream
also grows the local rate-limit stub's Map; the new bounded address pool
avoids that confound. This observation does not establish the cause of the
old resident-memory growth or claim a production fix.
Schema 3 adds credential-owned read paths and complete golden write receipts;
its timestamp and owner receipts are checked against PostgreSQL after drain.

Fast fixture format/check, strict TypeScript types, Python verifier contracts,
and Rust load/protocol tests are part of `tools/verify.py`; timed measurements
are not. The real load executable is tested against a slow server to prove
p95 retains its drain, and against a corrupt 200 response to prove the
correctness counters detect it. Local results and interpretation belong in
`docs/compiler-runtime.md`, alongside this method.


The scalar validation paths enforce the same bounds with their own implementations.
Jadpo validates the fields it reads from PostgreSQL (including `RETURNING`),
then its generated output writer serialises the known shape without a second
schema-validation pass. Hono's OpenAPI integration validates request inputs;
it does not automatically validate response bodies. The TypeScript handlers'
Zod output parse is therefore one response validation, not a duplicate.
Its text helper uses the built-in `isWellFormed()` and a UTF-16 length fast
path, counting surrogate pairs only when necessary. It enforces the decided
Unicode scalar bounds, rejects unpaired surrogates, and keeps OpenAPI's
`minLength`/`maxLength` contract. Node and Bun boundary tests cover ASCII,
astral characters, combining sequences and large payloads. CI uses Node 24;
the local smoke used Node 24.18.1 and Bun 1.2.20, both supporting
`String.prototype.isWellFormed()`. It adds no query
or transaction to imitate Jadpo's validation implementation.


### Request-path coverage audit

This audits the implemented scaffold against the generated Jadpo entries
and the full runtime HTTP path, rather than inferring equivalence from the
business SQL. Library delegation counts as implementation. The table records
what currently exists; it does not waive missing guarantees or prescribe
extra work merely to reproduce incidental overhead.

| Boundary | Jadpo native / Wasm | Current Bun / Node scaffold | Assessment |
|---|---|---|---|
| Request admission | 1 MiB body cap; readable ASCII headers; route method/OPTIONS/HEAD handling; unused body/query rejection | Hono handles HEAD; strict known JSON object fields and typed path/input schemas; no explicit matching body cap, header/unused-input guards or OPTIONS/405 contract | Required guards/errors are incomplete. Missing/non-JSON content type and malformed JSON are not fully mapped. |
| Bounded signed auth | Setup issues durable opaque session, first request renews to signed BLAKE3 part; five-minute signed authority reuse; expiry can renew from session | Five-minute JOSE HS256 bearer; typed claims; no authority reads in timed path | Only fresh signed happy path is exercised. Durable session renewal/revocation and complete credential selection are absent in TS. |
| Address and caller limits | 120/minute address, 600/minute principal; trusted-hop address parsing/canonicalization, IPv6 /64; Retry-After on refusal | Same numeric limits and controlled IPv4/address pools; raw forwarded-header key, no matching canonicalization or Retry-After | Ordinary scaffold pool work matches; adversarial address/refusal guarantees do not. |
| Owner reads | Required owner predicate, id/title projection, C-collation title/id ordering; one business SELECT after signed renewal | Typed CASL boundary supplies mandatory SQL scope and projection; same SELECT shape/order | Matches this fixture, not golden cursor/visibility semantics. One statement gives its own consistent snapshot; no artificial TS read transaction added. |
| Create and storage | Same physical DDL, ReadCommitted transaction, INSERT RETURNING; DB fields validated before commit; response serialization before commit; retries ConcurrentChange up to three times | UUIDv7, same DDL and BEGIN/INSERT RETURNING/COMMIT shape; Zod output validation currently after transaction returns; no concurrent-change retry classification | Successful probe state matches, but invalid returned data could commit before TS validation fails. Validation must be within transaction for equivalent rollback safety. Retry semantics are not established. |
| Output validation | Read/RETURNING DB fields checked by runtime Validator; generated writer serializes known output shape | Zod request middleware once and explicit output parse once; scalar bounds tested on Node and Bun | No duplicate automatic response parse: Hono OpenAPI response schema is metadata. Jadpo is not entirely free of runtime validation. |
| Deadline/cancellation | Default ten-second route timer covers auth/body/action; abandoning the context drops uncommitted work; commit handled outside timeout | Load client has bounded request time, but app has no matching deadline/rollback fence or DB cancellation linkage | Client timeout does not supply server transaction safety. A naive Promise.race would be insufficient. |
| Idempotency | Non-GET key selection; supplied key can claim/store/replay atomically | No Idempotency-Key behavior | Key is absent from timed traffic, so no key SQL is measured; full write guarantee remains absent. |
| Errors and response identity | Safe boundary errors, storage/unavailable/deadline classification; x-request-id response header | Tested exact missing/forged/expired and invalid-input envelope; redacted unknown fault log; generated ID in error body, not response header | Common probes pass; complete error mapping and response identity remain incomplete. |
| Traces and observation | Creates/continues trace context on every request; clock/deferred/observer dispatch; default deployable observer logs faults only | No trace context propagation; redacted fault log | A real full app can use trace propagation. Empty deferred/context/observer dispatch costs in this fixture are incidental architecture overhead, not a reason to pad TS with unused work. |
| Startup | Migration-ledger validation plus readiness SELECT 1 reused for a second | Required application-column preflight plus readiness SELECT 1 reused for a second | Same migrated business schema, different startup guarantees. Cold-start timings would expose actual differing work, not equivalent migration validation. |

Sources: runtime `http/mod.rs` admission, trace, route/deadline and commit
paths; `read.rs`, `queries.rs` and `write.rs` stored-field validators;
`serve.rs` and `deployable.rs` observation; generated fixture entries;
installed Hono/OpenAPI validator implementation; and `typescript/server.ts`.
The runtime's reusable machinery also serves jobs, events, providers and
full identity flows absent from this scaffold. The repair-café benchmark's
subscriber path is evidence for those costs; these microbenchmarks cannot
stand in for it. Next comparison work must implement the actual golden
contract and strong baseline, fix required reached-boundary gaps, and verify
all 44 cases before making app performance claims.

### Version checks in writes (9.1b4)

`version_writes.py` compares the exact prerequisite's production
`crates/*/src` with the candidate, retaining the same final harness on both
sides. The build label is immutable. `ai_accounting.source()` and
`validate()` guard every tracked file, Git mode (including literal symlink
targets), HEAD, tree and executable at both ends. Each label saves the
compiler, fixture, HTTP load tool, generated native/Worker artifacts,
TypeScript output and the actual Bun/Node/workerd/PostgreSQL client
binaries, hashes and versions. Original artifact paths remain guarded too.
A source or artifact change requires a fresh build; no old-source bypass.

```
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=4 \
  python3 tools/bench/version_writes.py build --label <fresh-label>
python3 tools/bench/version_writes.py runtime --label <fresh-label> --calls 500
python3 tools/bench/version_writes.py patch --label <fresh-label> --warmup 2 --duration 5
```

The first command runs the existing golden application's complete 44-case
contract on all four targets before generating its production HTTP
artifacts. All pinned npm installs disable lifecycle scripts. Selected
inherited and effective **outer benchmark environment** Rust build flags are
recorded separately. Historical 9.1b4 stamps did not observe inner codegen Cargo
settings; those values are unavailable. The 9.1b5 method below observes them
separately. Before building a reference, restore all affected production source directories
from its exact commit and commit that state, retaining the final harness.
Before building the candidate, restore the entire clean gated tree,
including modes, and prove `git write-tree` equals its gated tree. Run both
sides sequentially, with no concurrent builds or benchmarks; retain start
and end host/process inventories and all failed attempts. Never reuse a
label or overwrite a result.

There are two deliberately separate workloads:

- **Compiler-generated-plan runtime fixture.** A release Rust executable
  loads the shared ordinary Jadpo shipping project with `Prepared`, after
  its matched compiler makes a migration. It executes through interpreter
  dispatch and the production action boundary/write runtime, with a local
  capability fake and real disposable PostgreSQL. It is not generated
  native application or HTTP throughput. Each of three rounds performs
  malformed-provider and real-stale-row controls, 50 warmups, one SQL census,
  and 500 measured calls. Each call reads a shipment, closes its snapshot,
  calls a changing keyed provider operation, then updates the shipment.
  It validates the returned label, final persisted note/label, one provider
  call per successful measured action, and no open action transaction at
  every provider call. The stale control must rerun and read the fresh note;
  malformed wire output must preserve stored state. Python negative controls
  reject incomplete rounds, wrong counts/quantiles, missing controls and
  incorrect persisted values. PostgreSQL logging is observed on all three
  active connections before the census offset. File-log readiness is
  bounded by observed statement records, not an assumed flush delay.
  The wrapper counts data SQL and explicit BEGIN/ROLLBACK/COMMIT commands
  separately; PostgreSQL must observe that total plus the provider's one
  independent no-transaction assertion query. Those control queries stay
  in both timings, labelled separately. The raw census includes SQL and
  bound parameter values. Bun and Workers timing are N/A for this fixture.
  Whole-fixture child CPU includes compiler/setup, warmups, census and
  controls; daemonized PostgreSQL backend CPU is excluded and unmeasured.
- **Warm ordinary HTTP PATCH.** The existing golden application's
  title-only `PATCH /todos/{todo_id}` runs on its generated release Worker
  under local workerd and on persistent Bun, each with the same process
  lifetime, fresh seed, real signed sessions, startup/readiness phase and
  two-second pool/isolate warmup. Three rounds rotate target order, with
  one and eight closed-loop keep-alive clients for five seconds per cell.
  Each cell starts a fresh service/limit store and seeds 256 independent
  owners, credentials and owned rows. Client i cycles cases i, i+C, ...
  for C clients, so no overlapping requests share a row. All 256 cases must
  return verified receipts. This distributes demand under the real 600/min
  caller quota; any 429 still rejects the cell. The Rust load tool's opt-in
  partition mapping preserves its existing fixed/default protocols. Every response
  must match the expected identity, owner, title, status, due date and
  creation time; update times must be valid zoned instants within the cell.
  Final persisted rows must agree with the last response for each case/row,
  and all fields except title/update time, including reminder/lifecycle
  fields and every untouched row, must stay identical. Invalid-title and
  foreign-owner refusals preserve all data. Deliberately bad receipt
  timestamps and untouched-row mutations must fail the independent guard.
  Server, helper/pool and load-generator CPU are reported separately;
  PostgreSQL daemon CPU is unmeasured. This is local workerd evidence,
  not Cloudflare edge capacity or an open-loop latency SLO.

The focused statement reduction is the primary invariant. PATCH is the
common-write regression control and the supported Worker/Bun comparison.
Report every round and its range, including adverse cells. Investigate a
material common-path slowdown rather than dropping a case or counting a
short smoke as qualification. Raw files live under the ignored
`tools/bench/work/version-writes/`; fact sections in `docs/compiler-runtime.md`
record exact paired identities, results, failures, digests and limitations.


The final 9.1b4 comparison used `b4-reference-partition-final` at
`58933ff74cc9fdac860aea20567acc2e52537253` (production exactly prerequisite
`e7647abbdfc39e9b4cacd55e12cf7520f999469c`) and
`b4-candidate-partition-final` at `050aa8fd3c434518deb782ba3f61b0f060d0a143`.
The entire candidate tree/modes equals gated `404ad02e` tree
`6f538200443eff32d0c29975b570a5dbe00dd9eb`; each side had its own build.
Runtime/PATCH outputs are respectively `runtime-1792798879011432000.json` /
`patch-1792798890696328000.json` reference and
`runtime-1792803710126051000.json` / `patch-1792803747338864000.json` candidate.
Both `build.json` files contain all 68 original/saved artifact-map entries per
side. Exact source/artifact/raw digests, every round, CPU scopes, attempts and
limits are recorded in compiler-runtime §2.11, “Version checks in writes”.

Observed successful-action census: 4→3 **data** SQL, four transaction commands
unchanged, hence 8→7 runtime roundtrips; PostgreSQL logs 9→8 including the
independent provider assertion. The release runtime median cell p95 worsened
0.580958→0.606792 ms (+4.45%); median fixture throughput rose 3.62%. Ordinary
PATCH workerd medians were 863.13→865.05 / 1558.32→1566.42 requests/s at 1/8
clients, with about the same 71.6%/79.0% of Bun throughput. This confirms SQL
removal, not a latency gain or positive outside-call HTTP performance claim.
Retain adverse rounds and higher candidate host CPU; matched memory/swap/thermal
history is unavailable. Preparation also took 532.91→2940.60 s, outside request
timing; causal attribution is not supported.

The first immutable label `b4-reference-final` remains retained: its runtime
passed, but fixed-caller PATCH exhausted the genuine 600/min quota and the guard
rejected 429s. Short smoke success had missed this duration-dependent refusal.
The final disjoint partitions preserved limits, required every one of 256 cases,
and passed all 24 cells with at most 54 calls/case including warmup plus bounded
setup probes. Do not reuse the rejected cell as throughput. Avoid another cold
full comparison merely to chase favorable latency; inspect controls/source and
report supported invariants and variation. The first full gate also correctly
caught SQL-layer ownership and a stale CLI snapshot; final full gate passed all
93 steps including the final harness, actual Worker route8/3 and both44-case
golden targets. Subsequent edits are facts/method prose only.


### Breakers in delivery claims (9.1b5)

`connected_deliveries.py` compares a connected subscriber through the real
production generated delivery path and the original connectionless backlog
control. Build and measure each clean side sequentially with the **same final
harness**, fresh immutable labels, no concurrent timed/build work, and full
tracked byte/mode/literal-link/HEAD/tree plus original/saved artifact guards.
Restore the entire reference `crates/` tree from the exact prerequisite and
prove equality; restore the entire gated candidate tree/modes before its build.
Default application/load behaviour is unchanged.

```sh
env -u CARGO_ENCODED_RUSTFLAGS -u CARGO_TARGET_DIR \
  python3 tools/bench/connected_deliveries.py build --label <fresh-label>
env -u CARGO_ENCODED_RUSTFLAGS -u CARGO_TARGET_DIR \
  python3 tools/bench/connected_deliveries.py measure --label <fresh-label> --count 10000 --rounds 3
```

Builds use effective incremental=0, dev debug=0, jobs=4; production native and
Wasm release profiles; pinned offline npm dependencies with scripts disabled.
The executable Cargo observer records only selected flags plus actual real-tool
path/argv/cwd, including inner encoded remaps, target/locale directories and
source epoch, while transparently preserving delegation and status. Build
stamps retain all runtime paths/versions/hashes and originals/snapshots, including
Python, workerd, Bun, Node, psql, rustc/cargo, compiler and generated artifacts.
Source/artifact guards run at both ends; each Worker round also guards the exact
host files it consumes. Host memory/swap/pressure/load/CPU and selected outer
inherited/effective flags are captured on both sides, before builds/campaigns
and around every cell. Raw data lives under ignored `work/connected-deliveries/`.

Each of three rounds rotates four targets; every cell processes 10,000 seeded
deliveries in up-to-100-delivery ticks and four lanes. Workerd and Bun/Node are
matched persistent warm services; native external ticks include startup and are
supporting evidence. The connected subscriber makes one actual local HTTP safe
GET through its Live package driver before a unique committed receipt/handled
marker. Its TCP_NODELAY provider validates every method/path/cohort/credential.
Bun/Node use the same idiomatic **joined claim from the outset** on both sides.
Ten connected/six backlog controls cover duplicates, leases, malformed data,
rollback and claim failure, with connected provider refusal/unavailability and
recovery, open hold and closed breaker. A held provider exposes the actual
expired running claim and structured JSON failure before completion; every
failure path releases and joins. Complete calibration resets include all owned
mutable tables/housekeeping, preserve migration metadata/extension rows and
record table lists and before/zero-after counts between workloads.

An independent SQL census observes actual logging on every active runtime/pool
backend before its accepted offset, waits for settlement/COMMIT logs, and then
checks persisted state. New native sessions inherit confirmed logging;
administrative psql is excluded. Data, transaction and provider-control SQL are
separate. **This is a one-delivery tick census**, with fixed DUE/sweep/housekeeping
and native cold-session checks; do not linearise it as SQL cost per delivery in
timed 100-delivery ticks. Timed windows independently require SQL logging off.
Every cell validates all delivery columns/clocks, receipts/handled identities,
immutable event/trace/depth/payload/encoding, breaker columns and exact provider
counts. CPU records service/pool, Python controller/provider and native children
separately; PostgreSQL daemon CPU is excluded, and child CPU closes before host
diagnostic subprocesses start.

The accepted reference `b5-reference-final-1` is
`9f383b263d63a8fce1383598e9a22d354c46926a` (all crates exactly prerequisite
`97aba24a4afc7bb0ad43a4eac7c62ee420f670ee`); candidate
`b5-candidate-final-1` is gated `dc5193ac84a77dc9f0d35a45eb531980a8d25ec4`, tree
`3519f1800e28eddf76a70bd07a5011f9b5a3c1c8`. Gate: 93/93 in 375 s. Each campaign
passes all 24 cells; raw files are `runs-1792818430938323000.json` reference and
`runs-1792818956859881000.json` candidate. Compiler-runtime §2.11 “Breakers in
delivery claims” records exact source/tree/file/artifact/raw digests, every
performance round, CPU scopes, host/build conditions, all failed attempts and
limitations; §2.6 records current-state arbitration and the query-error tradeoff.
All 4,910 saved snapshots and 42 consumed Worker host files per side were
independently rehashed after the pair. The reference shared compiler original
was subsequently superseded; its snapshot remains intact.

Observed connected Jadpo one-delivery-tick census is Worker 12→11 / native 14→13
data statements with unchanged transaction counts; its independent breaker READ
is replaced by the joined claim. Bun/Node 8 and backlog Worker 10 / native 12 / Bun or Node 6
remain unchanged; backlog has zero breaker reads/joins throughout. Median
connected Worker throughput is 1323.240→1435.087 deliveries/s (+8.45%), tick
p95 88.083→85.166 ms; **unchanged Bun also rises 7.42%**, so relative Worker/Bun
throughput is only 62.586%→63.187% (+0.601 percentage points). The initial Worker round is adverse,
Node median p95 worsens, and connectionless Worker/Bun is 60.248%→59.030%.
Matched host observations retain pressure/swap variation and +1.274pp
pre-campaign ambient busy CPU; do not claim a causal 8.45% gain. Keep every
round and all bad/superseded setup data, including the compiler namespace/write
plan defect, structured-JSON red, two failed gates (the kiosk 202 cause remains
unknown), and workload-transition/backlog-metadata corrections. No extra HTTP
comparison or favourable-latency rerun was selected. Final subsequent changes
are facts/method prose only.


### Whole-request CPU attribution (9.2a1)

`request_profile.py` reuses the real `golden.py` signed read and `todo.py`
public sample endpoints. It profiles whole generated production requests,
retaining full-body/state/audit checks and genuine caller/address quotas. The
four focused payloads vary the full real PostgreSQL response; they are not
standalone JSON microbenchmarks or equivalent golden authentication workloads.
Commit source before preparation. The fresh immutable label builds its own
compiler, passes all 44 golden contract cases on all four runtimes, builds
production native/workerd fixtures and strong TypeScript dependencies, and
captures full source/mode/link/HEAD/tree and original/saved artifact maps.

```sh
env -u CARGO_ENCODED_RUSTFLAGS -u CARGO_TARGET_DIR \
  RUSTUP_TOOLCHAIN=1.99.0 RUSTC="$HOME/.cargo/bin/rustc" \
  RUSTDOC="$HOME/.cargo/bin/rustdoc" \
  python3 -B tools/bench/request_profile.py build --label <fresh-label>
python3 -B tools/bench/request_profile.py run --label <fresh-label> \
  --output production-c16-r3 --profile production --rounds 3 \
  --connections 16 --duration 8 --warmup 2
python3 -B tools/bench/request_profile.py run --label <fresh-label> \
  --output named-c16-r3 --profile named --rounds 3 \
  --connections 16 --duration 8 --warmup 2
python3 -B tools/bench/request_profile.py run --label <fresh-label> \
  --output context-none-c16-r3 --profile none --targets workers,bun \
  --workloads golden-read,ascii-large --rounds 3 \
  --connections 16 --duration 8 --warmup 2
python3 -B tools/bench/request_profile.py run --label <fresh-label> \
  --output serial-none-c1-r3 --profile none --targets workers \
  --workloads golden-read,ascii-large --rounds 3 \
  --connections 1 --duration 8 --warmup 2
```

Use a new label/output on every attempt; existing captures are never replaced.
Builds use offline pinned npm installs with scripts disabled, incremental=0,
dev debug=0/jobs=4 and ordinary production release deployables. The Cargo
observer captures real inner cargo argv/cwd/selected flags, including encoded
remaps and target/ICU directories; RUSTC/RUSTDOC are outside its selected-variable
whitelist. Record their explicit launch paths/toolchain separately when used,
as the accepted invocation JSON does. Actual child executable/loader selection
was not independently traced. `request-current-16d24dc4` failed correctness
preparation at rustc `--check-cfg`; direct-toolchain `request-current-pinned-16d24dc4`
then failed the rust-lld library loader. Neither was accepted. Explicit Rustup
proxies with the pinned toolchain passed as `request-current-rustup-16d24dc4`.

Production remains stripped. The small `jadpo-profile-worker` Rust example
rebinds the exact saved production compiler input with names retained solely
for diagnostics. Its JavaScript and every encoded noncustom Wasm section must
be byte-identical to production. Custom sections and module sizes differ.
Each cell guards and saves the actual host JS/Wasm/config it consumes; use
saved per-cell copies after a later cell supersedes live host files.

The exact loopback `kiosk` inspector target is required; no first-target fallback.
Absolute command/partial-frame deadlines, errors, truncation, malformed or empty
profiles, wrong/duplicate/remote identity, overwrite and consumed-artifact drift
have fault controls in `test_request_profile.py`. Every accepted profile has
nonempty consistent nodes/samples/timeDeltas and finite valid times. The profile
starts before load spawn after fixture validation: **warmup + admission + drain
+ stop-command overhead**. Separate service/pool CPU counters start after warmup
and close before inspector transfer/validation. This avoids counting download
CPU against the measured request denominator; sample and CPU windows still have
their own explicit scopes. Services, helpers, load processes, inspector sockets
and PostgreSQL are closed/reaped on every outcome.

PostgreSQL duration logging is enabled only by startup options on the owned
disposable process, with parameter logging disabled. This adds diagnostic
overhead even for `--profile none`. `server_phase_duration_ms` records
PostgreSQL protocol phases (possibly separate Parse/Bind/Execute), not correlated
SQL-statement/request wall times; raw prefixes omit SQL and parameter text.
Never add/subtract them into HTTP p95 or sampled CPU. Service CPU excludes
PostgreSQL; golden records pool-helper CPU separately; the 9.2a1 focused captures do
not (9.2a2 adds it for new captures). No clean production throughput or isolated profiler-overhead claim follows
from the ordered contexts.

Raw `.cpuprofile` files contain every callFrame/tree edge/sample/timeDelta,
including zero-self internal frames. Assign each delta to its sampled leaf for
self time, and once to each ancestor for inclusive time. Inclusive rows overlap;
sampling observations are not call counts. Full function tables should use the
raw tree, not the convenience top-50 summary. Historical 16d24dc4 saved summaries
used SELF sample counts even inside their inclusive lists; derive inclusive
counts from those raw captures. The corrected final summary retains node/parent/
script identity and source coordinates, and computes matching self/inclusive
observation counts, covered by an independent three-node weighted-tree test.
The correction does not rewrite old captures or alter production execution.
Wasm line/column values are module offsets, not Rust source lines; unresolved
native frames stay unresolved.
Percentages include idle/program/GC/inspector costs and are not per-request stage
durations. Do not turn them into p95 component times.

Accepted data is four complete campaigns: 48 verified cells and 30 profiles,
under ignored `work/request-profile/request-current-rustup-16d24dc4/`. Later
`serial-named-c1-r3` is incomplete: three verified cells followed by one measured
504/invalid body in ASCII-large round 2. All four profiles remain retained but
that campaign is excluded from accepted medians; the exact 504 cause is unknown,
and no favourable retry was selected. Setup/protocol smokes are also excluded.
Compiler-runtime §2.11 “Whole-request CPU attribution” records exact identities,
every throughput/p95/CPU round, granular named-node examples, failed attempts
and limits. Across large named captures the generic `json::write_string` has
17.7–20.3% self sampled time across calling contexts, while small signed golden
reads show socket/scheduling/GC costs. This selects a bounded next optimisation
candidate; this chunk implements no production optimisation. The broad
four-runtime application throughput matrix remains deferred.


The corrected-summary final harness is gated at
`0722190157965defdbf16063afbeba5557a12c00` (tree
`82c59069450ca05e5cf7dd7bbae3573c03f87084`), 93/93 steps in 493 s; captures
remain exact earlier `16d24dc48a4ed734f3a4ad3faf65403f790ead27`. The count
correction has no production effect and needs no benchmark rerun. Subsequent
changes record facts/method prose only.


### JSON string writing and fast Wasm screens (9.2a2)

The kept writer copies ordinary UTF-8 up to its first ASCII escape, then keeps
the original scalar suffix. This preserves canonical bytes and avoids the
mixed/dense escape regression of repeated bulk spans. The original allocation
budget failed with one allocation plus 14 reallocations; the kept writer passes
the unchanged at-most-one ordinary-output allocation check. Canonical fixtures,
serde round trips and a deterministic scalar corpus cover the semantic boundary.

Compiler-runtime §2.11 “JSON string writing and bounded Wasm screens” contains
**every round**, retained raw/build hashes, source identities, size/cold/RSS
observations, rejected variants and failure lessons. Kept source is
`288a677879092449b25b182ea72969e046c41ab8`, tree
`62f889aad1b25ea77c09fe5fd515b5319fcf70da`; later source comments/facts do
not change its executable behavior. Against the saved awake original 190,
Worker median throughput changes are signed **−1.33%**, small **−0.15%**,
large ASCII **+15.54%**, mixed Unicode **−1.45%**, late escape **+16.58%**.
Signed/small/mixed p95s also worsen. The selected tradeoff is explicit, with
no universal/no-regression claim. Baseline/current time and host/control drift
limit causal attribution; preserve Bun controls and all adverse rounds.

Full qualification and capacity use `request_profile.py`. Its finalized
qualification files stay byte-identical from the committed pre-edit baseline
through the kept executable source:

| File | SHA-256 |
|---|---|
| request_profile.py | b684b274afb0c27862a40f7c3f545951cfc9e3563ceb53a91fea49ea9e148901 |
| test_request_profile.py | eb5160274ee3fc3880d624b9d429859df175860b1da48fbf9f6dd6a22f82ed16 |
| todo.py | 5425aaff6b2200c3b7ab3ab06eaa7bcafa4bf24ea65055db173efe793f9ba352 |

The new `--postgres-durations on|off` preserves old default **on**; NONE
inspector alone still leaves database diagnostic overhead on by default.
Use **NONE + duration OFF on both Workers and Bun** for capacity. The actual
server setting and unexpected duration-log faults are checked. Focused
pool-helper CPU is now measured separately over the same post-warmup window;
service CPU excludes the pool and PostgreSQL. Read the saved per-cell served
snapshots after later cells reuse live host paths.

Commit the complete source before any build. The source guard includes HEAD,
tree, modes, symlink payloads and all tracked bytes. Existing labels cannot be
reused from a later source/comment/facts commit; check out their recorded
producer source or create a fresh label. Never bypass guards or rewrite old
manifests/results. Use pinned Rustup proxy paths (resolving the rustc proxy
symlink would invoke rustup under the wrong argv0), offline pinned npm with
scripts disabled, and the supported scoped cache when avoiding the observed
invalid globally cached dependency metadata:

```sh
bench_env() {
  env -u CARGO_ENCODED_RUSTFLAGS -u CARGO_TARGET_DIR \
    RUSTUP_TOOLCHAIN=1.99.0 RUSTC="$HOME/.cargo/bin/rustc" \
    RUSTDOC="$HOME/.cargo/bin/rustdoc" SOURCE_DATE_EPOCH=0 \
    CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=4 \
    PYTHONUNBUFFERED=1 \
    XDG_CACHE_HOME="$PWD/target/jadpo-compiled/gate-cache-1db7e872" "$@"
}
bench_env caffeinate -i python3 -B tools/bench/request_profile.py build \
  --label <fresh-label>
bench_env caffeinate -i python3 -B tools/bench/request_profile.py run \
  --label <fresh-label> --output capacity-awake-none-pg-off-c16-r3 \
  --profile none --postgres-durations off --targets workers,bun \
  --workloads golden-read,ascii-small,ascii-large,unicode-large,late-escape \
  --rounds 3 --connections 16 --duration 8 --warmup 2 --principals 2048
```

The full build passes all 176 contracts and saves production, named diagnostic
and original/snapshot artifact guards. The kept label is
`work/request-profile/json-first-288a6778`; original 190 and rejected 1db
labels/raws remain intact. This final qualification is done once for the chosen
source, not per speculative writer. The outer pinned proxies/Rustup resolution
and actual inner Cargo argv/flags are retained; they do not independently trace
child rustc/linker selection. Scoped XDG/EPOCH0 is a disclosed build-environment
difference from the original; no global cache was deleted.

`worker_screen.py` is a separate **experimental Worker-only** path. It reuses
guarded load/pool/workerd/Bun/real TS dependencies, builds one identical generated
source with release opt-level s/2/3 and fatLTO/CGU1, and does no npm/native/named/
176-case build per screen. Its manifest cannot claim full qualification.
Complete dependency/source/artifact guards run once before and once after the
campaign; per-cell actual served/full-body/state/security guards remain.
Postguard faults invalidate the whole attempt. Default gzip6 sizes and optional
gzip9 proxies are labelled separately from provider upload sizes.

```sh
bench_env caffeinate -i python3 -B tools/bench/worker_screen.py build \
  --label <fresh-screen-label> --tools-from <qualified-reference-label>
bench_env caffeinate -i python3 -B tools/bench/worker_screen.py run \
  --label <fresh-screen-label> --output screen24-none-pg-off-c16 \
  --duration 2 --warmup 1 --principals 2048
```

The recorded `reserve-profiles-7a7c9347` screen uses a reserve-only original
character loop. Its fixed 24-cell order is golden/ASCII/mixed × s/Bun/2/3,
then the complete reverse, c16/NONE/PGoff. Levels 2/3 offered no convincing
broad gain: golden adverse rounds and roughly 23–26% gzip growth were retained.
Shipping Os is unchanged; no favorable retry or longer rejected-profile
confirmation followed.

`string_screen.py` is **Node-hosted standalone Wasm diagnostic evidence,
not workerd/HTTP/Bun capacity**. It compiles exact original/reserve/byte-bulk/
char_indices/skip-empty/first-escape writers into one Os/fatLTO/CGU1 module.
Five rotated rounds cover small/large ASCII, late LF, pure Unicode, the exact
qualified dense mixed-Unicode fixture and dense ASCII. Input preparation is
outside timing, output allocation is fresh per write, retained LLVM shows an
opaque writer call/barriers inside the loop, and an O(1) observation checksum
avoids a full-byte checksum in the timed path. Independent full byte equality,
prefix/control/Unicode fixtures and deliberately wrong escape/truncation
writers run before timing. Expected variants, counts and codegen guards have
negative controls. There are no CI timing thresholds.

```sh
bench_env caffeinate -i python3 -B tools/bench/string_screen.py build \
  --label <fresh-writer-label>
bench_env caffeinate -i python3 -B tools/bench/string_screen.py run \
  --label <fresh-writer-label>
```

Keep the first five-variant schema 1 `writers-548d4a2c` attempt
(150 rows, 110 byte checks+2 negatives) separate from final schema 2
`writers-first-58c0b447` (180 rows, 132+2). Reproduce an old schema with its
recorded producer commit/snapshots, not a rewritten current manifest.
Repeated spans regressed mixed/dense inputs; first-escape's mixed case remains
3.37–6.64% slower than reserve-only in all five micro rounds, while roughly
matching original. Only the subsequent fully qualified HTTP confirmation
supports the kept application tradeoff.

Request a parent quiet window only after builds/checks are complete and reaped.
Run the whole declared matrix without unrelated host work, preserve every
failure/adverse round, and release quiet explicitly only after services/DB/
helpers are stopped/reaped and source/artifact/state/clock guards pass.
Transient `caffeinate -i` must cover builds, captures and handoff gaps; never
change permanent/display settings. Require absolute UTC load-span minus Rust
load-wall span below 1 s per cell and no Sleep/DarkWake in the narrow interval.
An earlier unprotected candidate failed with 16 measured 504/invalid bodies after
host idle sleep; the entire original/candidate pair was replayed awake,
retaining both original attempts. No selective favorable-cell retry occurred.

The earlier three full gates failed with lint+ENOSPC, then unresolved/ENOSPC
diagnostics, then a 92/93 E0786 native-preparation failure with invalid metadata in globally
cached thiserror_impl (underlying cause unestablished). A scoped-cache native check passed 44/44; it is not a full-gate claim.
Approved inactive-cache cleanup preserved every recorded artifact; a flawed .d
staleness filter removed some reusable own dependencies, honestly recorded in
§2.11, with no protected loss. Do not infer physical APFS reclamation or a
specific cause for untraced disk drift. After final chosen source/facts review,
run the required full gate directly to stdout, with the same scoped environment:

```sh
bench_env caffeinate -i python3 tools/verify.py
```


The final reviewed code/tests/facts passed the full gate at
`fbd1241944529a521de0d9f7e5f9635b6dc300af` (tree
`a3763713ff2f6e4d99659114231110773337f5ab`), **93/93 in 568 s**, direct
stdout with all processes reaped. Later gate-result/completion metadata does
not alter the exact 288a measured artifacts.

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

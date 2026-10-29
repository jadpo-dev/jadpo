# Benchmarks

The golden todo, the contract it is held to, and the harnesses that
measure Jadpo's performance against TypeScript. None of it ships in a
deployable; the gate runs the contract and each harness's fault controls,
never a timed run.

| Folder | What it is for |
| --- | --- |
| [`golden-todo/`](golden-todo/README.md) | The golden todo: an ordinary Jadpo application, its contract cases (`acceptance*.json`), the adversarial changes, the expected audit and the contract's reviews. |
| [`golden-todo/typescript/`](golden-todo/typescript/README.md) | Its TypeScript twin, the baseline every comparison and chunk 10.2's agent comparison measure against, with the adapter that runs the same contract on it. |
| [`agent-changes/`](agent-changes/README.md) | Chunk 10.2's experiment: fresh agent sessions make the same changes to the golden todo in Jadpo and in TypeScript, scored by hidden tests. |
| [`contract/`](contract/run.py) | Runs the golden todo's contract cases against its native and compiled workerd builds; `native/` and `workers/` are the hosts they run in. |
| [`harness/`](harness/run.py) | The benchmark harnesses below, their result checks and the tests of those checks. |
| [`fixtures/`](fixtures/) | The small Jadpo applications the focused harnesses build: `todo`, `deliveries` and `connected-deliveries`. |
| [`typescript/`](typescript/package.json) | The focused workloads (`todo`, `deliveries`, `connected-deliveries`) written in TypeScript, run on Bun and Node. |
| [`bun/`](bun/server.ts) | Example 23's booking route hand-written for Bun, for the repair-café spike. |
| [`load/`](load/src/bin/load.rs) | The load generator (`load`) and a PostgreSQL pooler standing in for Hyperdrive under workerd (`pool`), a Rust crate of its own. |

The PostgreSQL image and the pinned workerd and wrangler they use are the
gate's, in [`tools/fixtures/`](../tools/fixtures/).

## Performance harnesses

How Jadpo's performance is measured: how to run each harness, what it
measures and how its comparisons are kept fair. The latest result of each
comparison is the one summary table in [compiler/runtime
§2.11](../docs/compiler-runtime.md); each optimisation's pull request
carries its full results. Timed runs are not in the gate; each harness's
fault controls are.

### Rules every harness keeps

- **Like with like.** Jadpo native against TypeScript on Bun and Node;
  Jadpo's Wasm build on workerd against TypeScript, leading with CPU time
  per request, cold start and p95. Name the workload and its
  scope beside every number.
- **Process lifetimes match and are disclosed.** A warm service is
  compared with a warm service; a per-tick or per-request process start
  is reported as such, never set against a warm one. Warmup is stated.
- **Builds are clean and guarded.** Commit the inputs first. Each build
  needs a clean checkout, builds its own compiler, and records the exact
  commit, tree, file modes, source and artifact SHA-256 maps, tool
  versions and flags (`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
  CARGO_BUILD_JOBS=4`; ordinary `jadpo build` release profiles for
  deployables). A changed input or artifact refuses timing before and
  after. A build label is immutable and never reused; a build stamp never
  authorises a different checkout. A comparison's reference restores its
  production sources from the exact prerequisite, the candidate the whole
  gated tree (`git write-tree` must match), and both use the same final
  harness.
- **Nothing else is building.** Finish and reap every owned build, test
  and service before timing. Leave unrelated apps running and record
  ambient CPU, load, memory and swap, before and around every cell.
- **Correctness first.** Every response and every persisted write is
  checked in full; a status, body or state mismatch, a 429, an error in
  warmup or measurement, or a stale input invalidates the cell. A failed
  attempt is kept, never retried into a pass or overwritten.
- **Honest summaries.** Latency is nearest-rank percentiles per run; a
  summary is the median of the per-run values with their range, never a
  pooled percentile. Throughput's denominator includes the drain of
  requests admitted in the window. The load is closed loop, so these are
  not open-loop latency targets or capacity. Local workerd is one isolate
  on one thread with a stand-in transaction pool, not Cloudflare's edge.
  Native serves on a worker thread per core; workerd, Bun and Node on
  one, so compare CPU a request as well as throughput. Report every round
  and adverse cells.
- **CPU is attributed.** Server, pool helper and load generator CPU are
  reported separately; workerd's pool helper counts in its server CPU.
  PostgreSQL and the Python controller are excluded from server numbers.
- Raw results and logs go under the ignored `benchmarks/harness/work/`; the pull
  request carries the numbers. No wall-clock assertion runs in CI.

All harnesses need Rust (the pinned toolchain), PostgreSQL 16's `initdb`,
`pg_ctl` and `psql` on `PATH` (or Postgres.app), Bun, and Node's `npm`
for workerd. They run one disposable local PostgreSQL 16
(`synchronous_commit=off`, 400 connections, 256 MB shared buffers) and
open only loopback listeners. Everything they build, install and start
stays in `benchmarks/harness/work/`, `benchmarks/load/target/`,
`benchmarks/bun/node_modules/` and `tools/fixtures/workers/node_modules/`.

### Golden application comparison

`golden.py` times the complete `benchmarks/golden-todo` application against
its strong TypeScript counterpart: Jadpo native and its Wasm build on
workerd (`workers`), TypeScript on Bun, Node and workerd (`ts-workerd`).
The headline pairs are native against Bun and Node, and `workers` against
`ts-workerd`.

```sh
# Preparation can compile and run correctness while the machine is busy.
python3 benchmarks/harness/golden.py --prepare-only
python3 benchmarks/harness/todo.py --prepare-only
# Run only with no compiler or other benchmark running.
python3 benchmarks/harness/golden.py --skip-build --principals 2048
python3 benchmarks/harness/todo.py --skip-build --routes read,large
```

Before any timing it runs all 44 `todo-v0.2` cases on each target from
this checkout and its compiler: 220 passing obligations. A missing,
duplicated, unsupported, failed, sliced or preview case can't authorise a
run. The conformance adapters' clocks, providers, SQL observation, debug
binaries and JSONL controller are never timed. `--skip-build` refuses
missing build identities and changed sources, compiler or artifacts.

| Workload | Request | Verified work |
|---|---|---|
| `read` | `GET /todos/<id>` | Real bounded signed bearer; the caller's exact six-field todo |
| `signed-list` | `GET /todos?page_size=100` | Exactly the owner's 50 visible rows, `created_at desc,id desc`, full six-field projection and null next cursor |
| `write` | `POST /todos` | Real bounded signed bearer; validated title, generated UUIDv7, open status, null due date, creation/update timestamps and a committed row for that exact caller |
| cold start | `GET /health/ready` | Spawn through the first valid database-backed readiness answer |

Every cell starts its server afresh and resets the database outside
timing. Owners (`--principals`, 256 by default; native's reads now pass
600 a minute per owner at 256, so the comparison runs 2,048) each have a separately
signed durable session, 50 visible and one hidden todo; a foreign owner
has the same. Each credential stays attached to its read path, expected
response and write owner. Every owner's response and the missing,
forged and invalid-title failures are checked before load; seeded rows
are checked unchanged after it; every successful write, warmup and drain
included, is matched against its persisted row and its credential's
owner. Credentials use the production codecs and persisted identities,
signed before the server starts, with four-minute authority inside the
five-minute limit; setup leaves enough lifetime for the cell and its
drain.

Timed routes keep normal auth, codecs, owner policies, lifecycle
visibility, deadlines, transactions and the default 600/minute caller and
120/minute address limits. A 429 invalidates a cell: raise
`--principals`, not the limits. Native declares one trusted proxy in the
copied application (`client_address: { source:
ClientAddressSource.forwarded_for hops: 1 }`), and the TypeScript host
trusts one strict IPv4 `X-Forwarded-For` only from a loopback peer;
together with a bounded 16,384-address pool this keeps the address limit
real. workerd uses `ClientAddressSource.cloudflare`; its incoming socket
overwrites any caller-supplied `CF-Connecting-IP` with the real peer, as
an echo control proves. The TypeScript Worker takes that header as its
peer too. Only unauthenticated requests meet the address limit, so the
timed routes are held by the caller limit on every target. Jadpo's Worker
counts limits in the stand-in rate-limiting binding; TypeScript keeps its
own in-process limiter, one per isolate.

Native and Wasm are ordinary release deployables from `jadpo build`;
workerd runs the generated shim and handler unchanged, with an extra
isolated entrypoint that reads the module's linear memory. Bun uses
`Bun.serve` and Node Hono's Node adapter, both binding the same emitted
TypeScript application; the transport host imports no controller, SQL
recorder or test hook. `ts-workerd` is the same application behind its
Workers entry (`src/worker.ts`), bundled by the pinned Wrangler's `deploy
--dry-run` with its defaults (esbuild, no minification), and served by the
same workerd in one isolate. A Worker can't reuse a connection another
request opened, so it opens a Store per request over its Hyperdrive
binding: up to two general and two transaction connections, unprepared
statements and no type lookup, as Cloudflare's Hyperdrive guidance for
postgres-js has it. No reminder is seeded and scheduling is external, so
the timings exclude background work.

Each app applies its own migrations; a benchmark-only fixture then gives
every `todo` table Jadpo's four business indexes (created/id,
owner/created/id and status/created/id partial on visible rows, plus
owner), keeping constraint indexes, and its DDL and digest are in the
results. That controls index work; it is not Drizzle's natural schema.
Pools are left as built: native 10 open and 8 idle per worker driver;
TypeScript five general connections and five transaction clients; both
Workers one isolate through the same stand-in transaction pool
(`load/src/bin/pool.rs`) capped at 10, its CPU recorded separately and
counted with the server's in CPU a request.

Defaults: three rounds in rotating target order, 1/16/64 connections, two
seconds of warmup and ten measured per cell, 30-second request and
10-second connect deadlines. CPU comes from cumulative process counters at
the warmup boundary and after drain; RSS is sampled every 250 ms. Cold
start includes first readiness body validation; for both Workers it runs
from workerd's spawn, with the pool already listening. Results hold every cell,
p50/p90/p95/p99/max, drain, statuses, server and helper CPU, peak RSS,
Wasm heap, load-generator CPU and threads, versions, hashes,
configuration and conformance identities. Every invocation writes a new
result path.

### Focused microbenchmark scaffold

`todo.py` runs ordinary Jadpo sources in `fixtures/todo` and strict
TypeScript in `typescript` on native, workerd, Bun and Node, one at a
time. It is a todo-shaped microbenchmark, not the golden contract: it
orders by title, returns a bare array, cascades deletes and has public
sample routes, so no golden case is claimed by it. Its `read,large`
routes are the large-row comparison the golden title bound rules out.

```sh
python3 benchmarks/harness/todo.py
python3 benchmarks/harness/todo.py --skip-build --rounds 3 --duration 10
python3 benchmarks/harness/todo.py --skip-build --rounds 1 --duration 1 --warmup 0.2 --connections 1
```

| Workload | Request | Checked work |
|---|---|---|
| `read` | `GET /api/samples/<small-id>` | One stored row, 100-byte payload, exact JSON |
| `large` | `GET /api/samples/<large-id>` | One stored row, 65,536-byte payload, exact JSON |
| `signed-list` | `GET /api/todos` | Real bounded signed bearer; exactly 50 owner rows ordered by title in `C` collation then id |
| `write` | `POST /api/todos` | Real bounded signed bearer; validated title, UUIDv7 identity, one transaction and returned id/title |

Signed cells have `--principals` owners (256), 50 todos each and another
owner's 50 lexically earlier ones; public cells one owner and the other.
Requests rotate `--address-pool` addresses (16,384) and credentials with
their own expected responses, keeping both default limits; a 429 fails
the cell. Setup authenticates the opaque token once and captures
`X-Jadpo-Token`, so timed signed requests take the bounded signed path;
the TypeScript app uses JOSE HS256. Both have a five-minute bound and no
authority reads in timed traffic, which each cell audits from
PostgreSQL's log (one business SELECT, no session query). The TypeScript
baseline uses Hono, Zod, generated OpenAPI, Drizzle, CASL and JOSE on the
same Jadpo-generated DDL; Bun runs its TypeScript and Node the emitted
JavaScript. Background ticking is off and workerd has no cron.

The scaffold's TypeScript does less than Jadpo at the request's edge: no
matching body cap, header or unused-input guards or OPTIONS/405 contract;
no durable session renewal or revocation; no address canonicalisation or
`Retry-After`; output validated after commit rather than inside the
transaction; no deadline or rollback fence, idempotency keys, response
`x-request-id` or trace context. Its scalar validation enforces the same
Unicode scalar bounds with its own code, and its one Zod output parse is
its only response validation. Read its numbers with that in mind; the
golden comparison is the whole-application one.

Defaults and failure rules are the golden harness's. Every read body is
compared in full, every write's title and UUID checked, and persisted
writes must equal every successful 201; read cells must leave the stored
count unchanged. Compare targets only with the same load-generator schema
(it is in each result), and rerun an older experiment before comparing
with it.

### Repair-café spike

`run.py` times teaching example 23's booking, `POST /repairs` with a
staffed bench, natively, as a Workers module under workerd, and
hand-written in Bun with postgres.js, plus `GET /health/ready`.

```sh
python3 benchmarks/harness/run.py                 # build everything, then 3 rounds
python3 benchmarks/harness/run.py --skip-build --rounds 5 --duration 15
python3 benchmarks/harness/run.py --skip-build --profile   # where workerd's time goes
python3 benchmarks/harness/run.py --skip-build --soak 600 --targets workers   # memory over a long run
```

A booking validates the request, books the repair and its `RepairBooked`
event in one transaction, runs `find_a_bench` and `print_on_assignment`
while the request waits (`settle:`), reads the ticket back and answers
201: 26 round trips to the database. The readiness check answers outside
the rate limits from a `SELECT 1` reused for a second.

| Target | What it is |
|---|---|
| `native` | `jadpo build`'s binary, a worker thread per core; `client_address:` reads `X-Forwarded-For` (one proxy), so the load's per-request addresses reach the rate limit |
| `workers` | `jadpo build`'s Workers module under the pinned workerd, its Hyperdrive binding given to `load/src/bin/pool.rs` |
| `workers-direct` | the same, its Hyperdrive binding given straight to PostgreSQL, as `wrangler dev` does |
| `bun` | [bun/server.ts](bun/server.ts): the same route by hand with pinned postgres.js, the same validation, error bodies, SQL, transactions and isolation levels (taken from the native build's statement log), the same 120-a-minute address limit and answer |

`pool` stands in for Hyperdrive: it answers a Worker's start-up and lends
a pooled connection by transaction (no TLS, no caching), where workerd's
own binding starts a backend per request. Each round starts each server
afresh, times its cold start, then drives each route with
[load](load/src/bin/load.rs) at 1, 16 and 64 keep-alive connections, two
seconds unmeasured and ten measured, sampling server CPU, resident memory
every 250 ms and, after each measurement, a Worker's linear memory
through a second entrypoint, `JadpoMemory`. `--soak SECONDS` books at 16
connections for that long, reporting each window's throughput, CPU a
booking, peak RSS and linear memory, so a module that keeps something of
every booking shows it. `--profile` samples workerd's V8 CPU profile
through [bun/profile.ts](bun/profile.ts) and writes
`work/<target>.cpuprofile`, which Chrome's DevTools opens. The module is
as `jadpo build` writes it, with no `wasm-opt`.

### Durable delivery backlog

`deliveries.py` measures a database-only subscriber draining a backlog,
separately from any HTTP or reminder workflow.

```sh
python3 benchmarks/harness/deliveries.py build --label serial
python3 benchmarks/harness/deliveries.py measure --label serial --rounds 1 --count 100
python3 benchmarks/harness/deliveries.py measure --label serial
# After committing a runtime change, build and measure a new label.
python3 benchmarks/harness/deliveries.py build --label bounded
python3 benchmarks/harness/deliveries.py measure --label bounded
```

Each label keeps a hashed `compiler-jadpo` copy and a `build.json` naming
its compiler, source and artifact maps. The default `--mode warm` runs
three rounds rotating native, workerd, Bun and Node; each cell starts with
exactly 10,000 deliveries and events, drained by 100 ticks of at most 100.
Workerd, Bun and Node each run one persistent process per cell: Bun and
Node the same `node:http` host (`POST /tick`) and postgres-js pool capped
at four; workerd the production generated `scheduled` handler through a
local service binding, its Hyperdrive binding on the stand-in pool capped
at four. Before every cell an excluded readiness tick and a real
100-delivery cohort, audited and then reset to the exact seed, warm the
services' pools and JIT outside timing. Native runs its ordinary external
`app tick` every time, start-up included, and is reported only as
supporting context. `--mode external` keeps per-tick Bun and Node process
starts; compare two builds only under the same mode, harness and warmup.

The fixture's real calibration POST must return its exact 202 and commit
one handled receipt; its event and subscriber keys seed the backlog.
Audits require exactly 10,000 receipts, valid UUIDv7 identifiers, exact
handled, succeeded and attempt counts, no missing or unexpected effects
and an unchanged fingerprint of the joined event and delivery identities,
payloads, encodings, times, trace and depth. A post-window replay cohort
must add attempts without effects. Before each target's first warmup,
real controls cover malformed payload parking, a refused handled-marker
insert rolling back its receipt (pending for retry, no writes), live and
expired leases and two simultaneous sweeps; unit faults reject missing or
duplicate effects and stale builds.

`typescript/deliveries.ts` is a focused comparator, the same on Bun and
Node: postgres-js pool four, UUIDv7, payload validation, a fenced 16-minute
lease, the 10-attempt/24-hour window and an atomic receipt and handled
marker. It only drains deliveries, while Jadpo's tick also runs
housekeeping and workerd goes through a pool stand-in, so the gap includes
costs a Bun service wouldn't pay. Tick p95 is nearest-rank over a cell's
100 ticks, not per-delivery or HTTP latency. HTTP calls have a 120-second
deadline; a failure stops the owned services and invalidates the cell.
The live PostgreSQL and HTTP controls of the comparator run in the full
gate's own step (`delivery_services.py`).

### Idle ticks and hourly housekeeping

`idle_ticks.py` measures native external ticks and a persistent local
workerd scheduled handler against empty business state.

```sh
python3 benchmarks/harness/deliveries.py build --label idle-reference
python3 benchmarks/harness/idle_ticks.py --label idle-reference --ticks 5 --rounds 1
python3 benchmarks/harness/idle_ticks.py --label idle-reference --ticks 100 --rounds 3
python3 benchmarks/harness/deliveries.py measure --label idle-reference --mode warm --count 100 --rounds 1
python3 benchmarks/harness/deliveries.py measure --label idle-reference --mode warm --count 10000 --rounds 3
```

Three rounds alternate the targets, 100 samples a cell after ten excluded
ticks. Each sample needs one successful `runtime.ticked` with no problems,
runs, deliveries or deletions and unchanged empty state; a Workers `ok`
alone doesn't count. A negative control renames the deliveries table and
requires the audit to reject the tick. Each cell logs one census tick's
SQL, migrations and readiness excluded, with logging off for timing; the
count is the fixture's completed-hour fast path. Native includes 100
process starts while Workers stays warm. Bun and Node don't apply: the
comparator has no generic housekeeping. The SQL count is the primary
result; small warm-delivery changes under varying load are not a
throughput claim.

### AI accounting

`ai_accounting.py` builds a release Rust executable calling
`jadpo_runtime::ai::generate` with `AiFake` and real PostgreSQL: a native
runtime fixture, not HTTP throughput. Workers and Bun don't apply.

```sh
python3 benchmarks/harness/ai_accounting.py build --label ai-reference
python3 benchmarks/harness/ai_accounting.py measure --label ai-reference --calls 500
# Restore/commit the entire final gated candidate tree, including modes.
python3 benchmarks/harness/ai_accounting.py build --label ai-candidate
python3 benchmarks/harness/ai_accounting.py measure --label ai-candidate --calls 500
python3 -m unittest discover -s benchmarks/harness -p test_ai_accounting.py
```

Three rounds alternate app, caller and retry scenarios. Each cell resets
state, runs missing-table and full-budget controls (no request, no state
change), warms 50 calls, logs one census call's SQL (checked against
PostgreSQL's log, `log_statement` observed `all` then `none`), then times
500 calls, each sample bracketing only `ai::generate`. The fixed prompt,
caps, fake token counts and one scripted unavailable send are in the
fixture; `Clock::TEST` skips retry backoff, so a retry is an immediate
fake resend, not provider latency. Exact counts, tokens, holds and cost
are validated at 50, 51 and 551 calls, and partial output is incomplete
until all nine cells pass.

### Version checks in writes

`version_writes.py` compares an outside-call write's statements and a
common write's latency between two builds, guarding every tracked file,
mode and symlink target, and saving the compiler, fixture, load tool,
artifacts and client binaries with their hashes.

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=4 \
  python3 benchmarks/harness/version_writes.py build --label <fresh-label>
python3 benchmarks/harness/version_writes.py runtime --label <fresh-label> --calls 500
python3 benchmarks/harness/version_writes.py patch --label <fresh-label> --warmup 2 --duration 5
```

`build` first runs the golden contract on all five targets; npm installs
disable lifecycle scripts. Two workloads:

- **Runtime fixture.** A release executable runs the shipping project's
  action through the interpreter and the production write runtime with a
  capability fake and real PostgreSQL: read a shipment, close the
  snapshot, call a keyed provider operation, update the shipment. Three
  rounds of malformed-provider and stale-row controls, 50 warmups, one
  census (data SQL and BEGIN/ROLLBACK/COMMIT counted apart, logging
  observed on every connection) and 500 measured calls, each validating
  the label, persisted values, one provider call and no open transaction
  at the call.
- **Warm PATCH.** The golden application's title-only `PATCH
  /todos/{todo_id}` on local workerd and persistent Bun, same lifetime,
  seed, sessions and two-second warmup; three rotating rounds at one and
  eight clients for five seconds. 256 owners each own a row, and client i
  cycles cases i, i+C, … so no two requests share a row. Every response
  and final row is checked, untouched fields and rows unchanged, and
  refusals preserve data.

The statement count is the primary result; PATCH is the common-write
regression control.

### Breakers in delivery claims

`connected_deliveries.py` compares a connected subscriber through the
production delivery path with the connectionless backlog control.

```sh
env -u CARGO_ENCODED_RUSTFLAGS -u CARGO_TARGET_DIR \
  python3 benchmarks/harness/connected_deliveries.py build --label <fresh-label>
env -u CARGO_ENCODED_RUSTFLAGS -u CARGO_TARGET_DIR \
  python3 benchmarks/harness/connected_deliveries.py measure --label <fresh-label> --count 10000 --rounds 3
```

A Cargo observer records the build's effective flags and real tool
invocations without changing them. Three rounds rotate four targets; each
cell processes 10,000 deliveries in ticks of up to 100 over four lanes,
workerd and Bun/Node as matched warm services and native external ticks
as supporting context. Each delivery makes one local HTTP GET through its
package driver before a unique committed receipt; the provider validates
every request. Bun and Node use a joined claim on both sides. Ten
connected and six backlog controls cover duplicates, leases, malformed
data, rollback, claim failure, provider refusal and recovery, an open hold
and a closed breaker; full resets record table counts between workloads.
The SQL census is of a one-delivery tick, fixed housekeeping included, so
it is not a cost per delivery in 100-delivery ticks.

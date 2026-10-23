# Performance harnesses

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

`work/todo/results.json` records every cell, p50/p90/p95/p99/max, status/error
counts, drain, CPU, peak RSS and Wasm memory, load-generator CPU/wall time and
thread count, machine/tool versions, source/lockfile SHA-256s and native/Wasm
artifact SHA-256s. Credentials remain only in ignored `work/todo` setup files,
never metadata. The generator normally uses two threads; its CPU is reported
so a client-bound cell can be identified. PostgreSQL, pool, load and server
share this host, so these are local costs with finite headroom. Native uses
multiple worker threads; workerd, Bun and Node use one server thread. Compare
server CPU/request as well as throughput; local workerd includes a stand-in
pool and no real edge/database network.

The load generator's schema 2 adds complete-body checks, p95, bounded requests
and retained slow tails. This changes its protocol/client cost, including for
new `run.py` runs: compare targets using the same generator version and rerun
old experiments before comparing numbers. The old spike's distinct-IP stream
also grows the local rate-limit stub's Map; the new bounded address pool
avoids that confound. This observation does not establish the cause of the
old resident-memory growth or claim a production fix.

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

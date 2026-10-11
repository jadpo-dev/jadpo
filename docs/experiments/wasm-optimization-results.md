# WASM-EXP1 optimisation follow-up — 30 September 2026

A subsequent [boundary/HTTP follow-up](wasm-http-experiment-results.md) is complete:
small-read performance is near Bun in this fixture; larger returned rows still
carry a material cost. The measurements below remain the earlier checkpoint.

**Recommendation: continue evaluating Wasm. Instance reuse removes the major
prototype penalty; retain Bun as the working target while testing the remaining
boundary cost and realistic HTTP workloads.** This owner-requested extension
changes the interpretation of the earlier traffic concern substantially. It
does not select a production backend or a Rust/direct compilation-route winner.

## Why the SQLite number was low

The earlier number timed the entire generated operation, including creating a
Wasm instance, value validation, host messages, SQL and the return path. It did
not measure SQLite in isolation. Three short attribution runs on the unchanged
code found these median throughputs:

| Stage | Operations/second | p95 time |
| --- | ---: | ---: |
| Direct prepared SQLite read | 150,481 | 0.0070 ms |
| Direct read, preparing SQL each time | 81,038 | 0.0138 ms |
| Existing checked storage adapter | 29,106 | 0.0413 ms |
| Construct an otherwise unused Wasm instance | 4,423 | 0.6696 ms |
| Fresh Wasm request with an in-memory host result | 3,693 | 0.7380 ms |
| Fresh Wasm request with real SQLite | 2,916 | 0.9008 ms |

This identifies instance lifecycle/allocation as the dominant initial cost.
A slow path was present even without a database. These stages are not additive:
GC, allocation and scheduling differ, and a partial JSON-codec control does not
measure all of the core's serialization costs.

## Changes tested separately

1. **Exclusive instance reuse.** The runtime can reset a completed request.
   The host leases one instance per active request, clears request ownership
   before reuse and discards faulted instances. Fresh request IDs reject stale
   resumes; pending instances cannot reset.
2. **Prepared SQL reuse.** The Bun SQLite adapter caches up to 128 statements
   instead of preparing the same statement every time.
3. **Immutable checked-plan caching.** Policy metadata and storage-node lookup
   are computed once. Incoming descriptors, principal identity, policy, types,
   freshness and update fields remain checked on every request.

The Wasm core still uses JSON values and the same host capability boundary.
There is no binary ABI, removal of validators, weakened policy, new language
syntax or native-code replacement. Core pooling is an explicit extension to
the original fresh-instance protocol; the earlier frozen evidence is preserved.

## Paired end-to-end results

Each variant ran five times at concurrency one and eight, with one second of
warmup and three seconds timed. Targets ran sequentially with rotated order.
Every warmup and timed result was checked. **80 runs, 5,496,882 operations,
zero measured errors.** All operations used the frozen 256-byte request and
verified the same authored result. This is local SQLite operation throughput,
not a deployed HTTP capacity test; concurrency is not multiple CPU cores.

| Variant | Operations/s, concurrency 1 | p95, concurrency 1 | Operations/s, concurrency 8 | p95, concurrency 8 |
| --- | ---: | ---: | ---: | ---: |
| Bun baseline | 38,149 | 0.031333 ms | 38,735 | 0.037500 ms |
| Original fresh-instance Wasm, rerun | 1,332 | 1.259792 ms | 1,332 | 6.907125 ms |
| Extended core, still fresh instances | 1,352 | 1.262500 ms | 1,329 | 6.873416 ms |
| Instance reuse | 18,658 | 0.062041 ms | 19,518 | 0.089750 ms |
| Reuse plus prepared SQL | 22,684 | 0.052292 ms | 22,945 | 0.059458 ms |
| Reuse plus prepared SQL and checked-plan cache | **30,013** | **0.040583 ms** | **30,802** | **0.043000 ms** |

The final candidate sustains approximately **79% of Bun's throughput** in this
workload, or about 21% lower. Its p95 is **29.5% higher** at concurrency one and
**14.7% higher** at concurrency eight: absolute differences of about **9 and
6 microseconds**, respectively. The original 20% latency gate still fails at
concurrency one. At concurrency eight, the ratio of medians passes, but one
paired repetition exceeded 20%; no blanket gate-pass claim is made.

The unchanged fresh implementation reran more slowly than the original long
benchmark (about 1,332/s here versus 2,118/s originally). Durations, target mixing,
GC/allocation history and workstation conditions differ. Fresh-instance timing
was particularly unstable. The same-run controls demonstrate the large benefit
of reuse; exact improvement factors should not be treated as production capacity
multipliers. Remaining JSON/validation costs are candidates for profiling, not
a claim that all remaining overhead has been attributed.

Without database work, the reused core handled approximately 68,000–69,000
operations/s. The full per-run medians/ranges, p99s and variant definitions are in
[comparison.json](../../experiments/wasm-exp1/optimization/comparison.json) and
[the complete table](../../experiments/wasm-exp1/optimization/table.md).

## Correctness and Cloudflare

Both reuse-only and reuse-plus-cached-storage configurations passed the unchanged
**P01–P15 probe cases and A01–A16 runtime cases locally**. Separate A17 negatives
were rerun: unsupported reachable `if`, handled nested mutation, unavailable
freshness, cross-store atomicity and an unapproved host import still reject.
A18 changed the original source minimum length 3→5, passed ordinary checking,
changed generated behavior and rebuilt the original module byte-identically
from a clean Cargo target.

Eighteen additional tests passed with 10,289 assertions. They include 10,000
sequential reuses, 25 groups of eight simultaneously suspended requests with
different principals, stale handles, attempted pending reset, invalid/domain
outcomes, malformed and oversized host results, and the existing forged-policy
and storage matrix. Four retained adversarial cases verify full rollback after
a second-write throw, malformed row, oversized result and wrong principal entity.

The identical optimised core also passed **15 probe and 16 runtime cases on
actual Cloudflare**. The authority reported 234 reused leases, zero active leases
at completion, and a six-page retained instance. Outer asynchronous probe calls
reused ten leases and reached two simultaneously active instances. Captured
provider logs contained 11 safe fault events; the secret sentinel was absent
from the checked body, headers and logs.

Cloudflare used reuse and checked-plan caching with its **original Cloudflare
SQL adapter**. The Bun prepared-statement cache is not a Cloudflare feature and
was not deployed there. No cloud throughput or real-user HTTP load claim follows
from its correctness suite. The tested module SHA-256 is
`a43140701c468c22448c3c05fdad0bf198ef594eb50c8cf9a6d2eb6d56909844`;
version `5d537e06-5271-4252-b9c8-c37e6ad9203d` identifies the disposable Worker.

The Worker and its synthetic SQLite authority/data were removed after testing.
API readback confirmed no experiment Worker or Durable Object namespace, with
the six existing Workers retained. See [cloud evidence](../../experiments/wasm-exp1/optimization/cloud.json)
and [cleanup](../../experiments/wasm-exp1/optimization/cleanup.json).

## Limits and next decision

This is a bounded fixture, not a complete language backend. Logical reset drops
request-owned values and allocation ownership; it does not cryptographically
zero every byte of linear memory. Reuse assumes the compiler-owned runtime and
accepted memory-safe application subset. Idle retention is bounded to eight
instances per module and 32 pages per instance; production active-request admission,
cancellation and general resource budgets still need design/evidence.

The earlier oversized-host-result diagnostic gap remains: rollback is safe,
but the event can fall back to the outer action rather than the actual inner
operation. That issue was not hidden or relaxed to obtain faster timings. Direct
Wasm generation was not optimised in this extension; route selection remains
inconclusive. The local machine was not load/thermal controlled, timing groups
were short and some lightweight tests/packaging overlapped early measurements.
RSS includes shared host and measurement support, not isolated request memory.

The next useful experiment should profile the remaining JSON/descriptor and
value-validation work, then compare representative HTTP/read/write workloads
at explicit latency and capacity budgets. It should retain correctness gates
and measure server compute and actual concurrency before making hosting-cost
or production-capacity claims. This report does not start that next experiment
or authorise a backend migration. Bun remains the working target while Wasm
now has a much stronger case for further investigation.

[Reproduction and source files](../../experiments/wasm-exp1/optimization/README.md),
[bounded plan](../../experiments/wasm-exp1/optimization/plan.md),
[effort accounting](../../experiments/wasm-exp1/optimization/effort.json),
[source/module hashes](../../experiments/wasm-exp1/optimization/artifacts.json), and
[retained raw evidence](../../experiments/wasm-exp1/optimization/evidence/manifest.json)
make the extension independently inspectable. Runtime/generator snapshots and
host variants are contained under the experiment; production files are unchanged.

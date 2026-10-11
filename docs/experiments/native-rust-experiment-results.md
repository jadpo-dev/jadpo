# Bounded native Rust backend experiment — 30 September 2026

This experiment compiles the existing WASM fixture's generated Rust into a native
executable, backed by **rusqlite 0.32.1 and system SQLite**, with Hyper 0.14.27 and
Tokio 1.34.0 for a private loopback test server. It does not change Jadpo's default
target, language, persistence engine or production architecture. No cloud work
was performed.

The [protocol](../../experiments/native-exp1/README.md) was committed before timing.
The current selected WASM remains the guest-scans SIMD/bulk candidate, SHA-256
`63358b4df468b9e99dcc1b3ef0faeff7d362b4e297bd382c3b9be2d36080448c`.
The shared-frame follow-up remains unselected. Native compilation does not modify
that candidate, its generator, fixture, projection or driver.

## What is shared, and what is still an adapter

The native build includes the **entire identical 816-line generated Rust module**,
SHA-256 `6c6494681e7c25abe6f79b7fbb2618fda3d6aa43bb9b036503058de56f0ada30`.
All six fixture callables are shared: `probe`, `Item.read`, `Item.read_title`,
`Item.change`, `Item.rename` and `update_pair`. Their input validators, effective
field/refinement validators, Unicode-scalar length rules, omission/null distinction,
returned-row validation, authored missing/conflict/empty failure selection, recovery
arms, sequential continuation and immutable row values come from the existing
checked lowering. There is no second handwritten native application.

This is stronger evidence than merely translating a fixture into Rust, but weaker
than a finished shared runtime architecture. Most of those 816 lines are runtime,
codec and test support. Native dead-strips unused WASM exports and codecs; counting
all lines as active common application logic would be misleading. The native build
uses scalar native code, not the WASM SIMD instruction path. The bridge calls
`make_entry` and polls its future directly, never WASM's 32-bit pointer exports.
It supplies owned `serde_json::Value` host results, preserving exact logical size
checks without the host-result JSON encoding/parsing or WASM memory transfer.
Full pending descriptors still serialize and parse on each storage suspension.
Read receipts, compact-plan transport and typed binary ingress are unnecessary on
this path and are disabled. HTTP results are serialized normally.

The native storage binding derives SQL/schema/ownership plans from the checked
projection. It checks incoming operation identity, descriptor, freshness, allowed
mutation fields and trusted principal; runs a fresh parameterized ownership-scoped
read/update; validates returned rows with the same generated validator; and wraps
mutations in `BEGIN IMMEDIATE` transactions. The generated application decides
which declared failure to return. The adapter commits only a successful terminal
outcome and rolls back domain, invalid and internal failures. Transaction control
and error-code interpretation remain host responsibilities.

This adds a nontrivial Rust adapter and plan-binding script alongside the existing
TypeScript WASM host. Policy enforcement, SQL execution, transactions, driver error
classification, HTTP and lifecycle remain target-specific work. Sharing validators
reduces drift, but does not remove that work or establish full language coverage.
The runtime still has global mutable state: the native server services each entire
application invocation synchronously on one Tokio thread. It is not safe to move
this bridge unchanged onto concurrent application threads. The WASM instance pool's
suspension, cancellation and isolation guarantees are not native threading evidence.

## Semantic checks and a real mismatch

The differential HTTP suite covers owned/missing/outsider reads, authored recovery,
invalid/extra input fields, Unicode length, invalid patches, omitted/null/empty
notes, uniqueness conflicts, missing/conflicting second-write rollback, successful
pair commit, ownership/content changes between requests, corrupt stored rows,
malformed input, large/escaped rows, field extraction, driver faults and repeated
principal changes. WAL/FULL and DELETE/FULL are both exercised.

A SQLite `RAISE(ABORT, ...)` trigger exposed a failure classification difference:
**Bun returns `ItemConflict`; the native/WASM-style adapter returns an internal
fault.** Both roll back the whole pair. The original failed assertion and diagnostic
are retained; the suite explicitly asserts the different observed outcomes rather
than describing them as parity. An independent integer-overflow SQL error trigger
executes only on the second row and checks common internal-error classification
and rollback. A second, harness-level mismatch appears at the 64 KiB boundary: the Bun
experiment wrapper checks response size **after** the generated callable has
committed, so it can return an internal response while persisting an oversized
update. Native checks the host/completion budget **inside** its transaction and
rolls back. Exact 65,536-byte reads succeed and 65,537-byte reads fail in both.
This is a boundary-placement problem in the experiment wrapper, not evidence that
Bun's database loses atomicity. It does mean the two full adapters are not equivalent
outside the measured, in-budget workloads. No baseline application
code was altered to hide these discrepancies. They block a claim of universal equivalent
failure handling and need explicit adapter conformance work before broadening the backend.

The inherited Rust tests and native bridge tests cover malformed host envelopes,
forged raw host domain failures, safe faults, size bounds, reset, immutable values,
forged authority descriptors and transaction reopen. Checked source variants rename
Item/User/owner_id and raise the minimum title length; native tests then use the
new schema/refinement. Unsupported freshness, cross-store and field-policy plans
fail before the native build. These tests do not claim arbitrary language support,
real authentication, nested handled mutation/savepoints or crash recovery.

## Measurement interpretation

All fresh measurements here compare **native Rust versus generated Bun hosted by
Bun**. The machine is an Apple M1 Pro with 16 GiB RAM, macOS 14.1/arm64;
Rust 1.78.0, Bun 1.2.20 and Node 24.18.1 are used. Node is the load generator,
on the same machine. The historic
[Bun-hosted read-path WASM campaign](wasm-read-path-results.md) and
[workerd guest-scans campaign](wasm-guest-scans-results.md) are separate evidence.
No new workerd run occurred here, and their request rates must not be ranked against
these rates as though the protocols and hosting plumbing were identical.

The bounded HTTP protocol is five rotated repetitions at concurrency 1/16 with
0.5-second warmup and two-second measurement, fresh process per target/repetition/
concurrency. This is shorter than the old 2+10-second qualification. Every response
is checked, as are final database snapshots and server request counts, without
request retries. Latency is closed-loop end-to-end, including client decoding and
verification; it is not a fixed-arrival latency SLO. CPU is server process CPU;
RSS samples include the whole server process. Native and Bun no-work controls
return the same payloads without application/SQL work. Repetition ranges, paired
ratios, p50/p95/p99 and per-cell maxima remain in the evidence.

Both targets use SQLite **3.39.5**, 4096-byte pages, `synchronous=FULL`, foreign
keys enabled, `fullfsync=0`, `busy_timeout=0`, and WAL autocheckpoint 1000. Primary
cells use WAL; a separate DELETE/FULL write campaign preserves the old storage
setting as a control. Every opened Bun connection is configured and inspected.
No production journal setting changes. SQLite documents the different sync and
checkpoint behavior of [FULL](https://www.sqlite.org/pragma.html#pragma_synchronous)
and [WAL](https://www.sqlite.org/wal.html); matching those pragmas is not a
power-loss recovery test. Rusqlite's existing
[statement cache](https://docs.rs/crate/rusqlite/0.32.1/source/src/cache.rs) is used;
no persistence engine is implemented here.

**SQL work differs on writes.** Both reads execute one fresh full-row SELECT with
identity and owner predicates. Native uses one scoped `UPDATE ... RETURNING` per
mutation. Bun additionally reads the old row, runs its general write policy
machinery and records semantic changes; its patch SQL uses CASE expressions for
field presence. Both use immediate atomic boundaries. Native's smaller SQL and
host workload contributes to any write gain. Those results are a comparison of
generated systems, not a pure Rust-versus-JavaScript speed measurement. General
change tracking, field policies, relations and migrations are not implemented by
this bounded native adapter.

## Primary WAL/FULL results

The independent HTTP campaign completed **160 cells and 4,301,353 verified
requests with zero response errors, count mismatches or snapshot failures**.
Concurrency-16 medians across five repetitions:

| Workload | Bun req/s | Native req/s | Bun/native p95 ms | Bun/native CPU µs/request |
| --- | ---: | ---: | ---: | ---: |
| Small probe | 26,843 | 28,557 | 1.095 / 0.687 | 37.00 / 34.48 |
| Full 16 KiB ASCII | 16,756 | 17,646 | 1.708 / 1.148 | 57.32 / 53.02 |
| Full Unicode (18,000 text bytes) | 11,064 | 14,872 | 2.530 / 1.333 | 86.63 / 63.47 |
| Single note update | 7,936 | 10,747 | 3.603 / 1.775 | 112.59 / 71.10 |
| Atomic pair update | 5,649 | 7,352 | 4.837 / 2.528 | 175.31 / 110.17 |

Paired median native/Bun throughput ratios at concurrency 16 are **1.064, 1.056,
1.353, 1.308 and 1.293**, respectively. Read throughput, p95 and CPU improve in
all five pairs for each read family. At concurrency 1, paired throughput gains
are 9.9% small, 6.8% large ASCII, 21.2% Unicode, 39.6% single write and 36.7% pair;
CPU falls 21.3%, 9.2%, 27.5%, 39.6% and 43.0%. Ratios of medians and medians of
paired ratios are different calculations; the table and these figures intentionally
retain that distinction.

**There are losses.** At concurrency 16, native p50 is 7.1% worse for small reads
and 5.9% worse for large ASCII in paired medians, with no winning pairs. The
paired per-cell maximum is 67.0% worse for single writes and 21.4% worse for pair
writes. The largest individual WAL observation is 75.64 ms native versus 117.88 ms
Bun, both pair writes; those single maxima do not erase the paired regressions.
The [full table](../../experiments/native-exp1/table.md) and
[results](../../experiments/native-exp1/results.json) include p50/p95/p99/max, ranges,
all no-work cells and paired wins/losses.

No-work/read throughput headroom is only **1.09–1.58×**, below the preregistered
2× criterion. These results establish observed local throughput, distributions and
CPU costs, **not maximum capacity**. The large p95 difference includes HTTP runtime
and scheduling behavior; it must not be attributed solely to generated Rust.

End-of-cell RSS across the primary campaign ranges from **3.03–6.06 MiB native**
and **51.22–159.30 MiB Bun**. These are external `ps` samples of whole server
processes, not WASM page counts or allocator-only measurements. Process peak RSS
was also retained. The initial wrapper mistakenly multiplied this Bun/macOS build's
already-byte-valued maxRSS by 1024; raw readings remain untouched and the summary
normalizes them. External RSS was unaffected. The wrapper now reports the correct unit; a separate
two-process check confirms it against external RSS. Archived measured wrapper
bytes are retained, and no performance scores were replaced. No unbounded growth is observed in
these short runs; this is not a long-running leak test.

## Storage tails and isolated calls

The DELETE/FULL control completed **40 cells and 134,903 verified requests**, with
zero response or snapshot failures. Concurrent single-write throughput medians
are **2,381/s Bun versus 1,116/s native**; the paired ratio is 0.469. Native p99 is
about 1.96× Bun and the paired per-cell maximum is 12.42×. Concurrent pair-write
medians are 1,256/s Bun versus 1,500/s native, but the **paired** throughput ratio
is only 0.716: different repetitions contain severe stalls, so a ratio of those
medians would give a misleadingly favorable result. Paired pair-write p95/p99
ratios are 1.33/1.89. Worst observed DELETE requests are **3.121 seconds native**
and **2.391 seconds Bun**. Native compilation does not solve storage stalls.
Shorter wall times or fewer SQL statements cannot establish durable-write quality.

Sixty isolated read cells verify **120,000 measured calls** after 200 warmups per
cell. These include assertions inside each host and use different equality-check
implementations; they are diagnostic complete-call timings, not a pure generated
instruction benchmark. Medians:

| Isolated workload | Bun calls/s | Native calls/s | Paired native/Bun rate |
| --- | ---: | ---: | ---: |
| Small probe | 43,214 | 45,036 | 1.043 |
| Full 16 KiB ASCII | 37,679 | 32,844 | 0.866 |
| Full Unicode, 18,000 text bytes | 22,407 | 25,242 | 1.131 |
| Full 48 KiB ASCII | 26,804 | 20,909 | 0.781 |
| Full late-escaped text | 32,158 | 35,665 | 1.109 |
| Title from fresh full 16 KiB row | 40,350 | 38,561 | 0.947 |

The 48 KiB case also uses 7.3% more native process CPU in paired medians and has
16.7% worse p95. The full ASCII and field-extraction losses rule out a general
claim that moving the same application to native makes every read faster. The
HTTP result includes different HTTP stacks and response pipelines. The Unicode
fixture contains 18,000 UTF-8 text bytes, and the late-escape fixture 9,692; the
nominal ASCII sizes are 16,384 and 49,152 text bytes. All successful complete
logical row envelopes stay below 65,536 bytes.

## Startup, build cost and artifacts

Twenty alternating fresh processes per target each initialize and seed their private
SQLite database and return a verified row, with no application warmup before the
first response. OS caches remain warm. Twenty additional verified calls per process
measure a warm median. This is local fixture startup, not hosted cold start:

| Median | Bun | Native |
| --- | ---: | ---: |
| Process to ready | 77.82 ms | 5.85 ms |
| First HTTP after ready | 3.33 ms | 1.39 ms |
| Process to first verified response | **80.95 ms** | **7.31 ms** |
| Warm HTTP median | 0.247 ms | 0.149 ms |
| RSS after those warm calls | 64.65 MiB | 2.59 MiB |

Process-to-first ranges are 78.47–98.69 ms Bun and 6.94–8.56 ms native. The Bun
process uses the existing experiment entry adapter, including its imported helper
modules; this is not a claim about the minimum possible Bun bootstrap.

With warm source/dependency download caches, a fresh native Cargo target directory
takes **14.62 seconds** for a release build including dependencies (one clean
sample). Regenerating and recompiling with cached dependencies takes a median
**8.61 seconds** across three runs, plus **92 ms** for the unchanged Rust lowering.
A no-op Cargo build is 69 ms. The existing already-built Jadpo projection compiler's
check plus Bun generation is **25.79 ms** median across three runs. Bun's runtime
transpilation/JIT is paid at startup/execution; there is no matching AOT link step.
The native build figures exclude rebuilding Jadpo itself and the shared checked
projection step; dependency downloads and cold filesystem caches are not measured.
These are distinct build pipelines, not interchangeable compilation tasks.

The measured stripped executable is **769,072 raw / 366,755 gzip bytes**, dynamically
linked to system SQLite/libSystem/libiconv. It is not a self-contained static
binary. The frozen WASM module is 178,919 / 68,751 bytes and needs its host. Bun's
app/persistence source alone is 179,574 bytes and needs other generated helpers
and Bun; comparing those source bytes directly with the executable would be unfair.

Measured native executable SHA-256:
`70d54e248e4f3e04ea945953137a48240d263cad0e2dd0c851bfc701d857ce2e`.
Same-directory regeneration reproduces it. The fresh target-directory build has a
different full hash, with **46 differing bytes**, confined to the Mach-O UUID and
code-signature regions. Bytes outside those regions match exactly. Both artifacts
and the comparison are retained; full byte-identical clean reproducibility is
**not** claimed.

## Evidence and adapter cost

The final differential suite passes **976 cases on four servers**, twelve Rust
tests pass (nine inherited plus three native tests), both actual source mutation
variants pass, and three unsupported-plan mutations are rejected. Eight additional
boundary observations retain the two known mismatches rather than relabel them as
parity. Primary plus DELETE measurements verify **4,436,256 requests**; the separate
smoke verifies another 38,917. Startup verifies forty first responses and 800 warm
calls. No cloud tests are included in these counts.

The handwritten native production bridge is 67 formatted lines, storage/transaction
adapter 309, HTTP/control/measurement shell 184, and projection-binding script 58;
tests are additional. These are descriptive line counts, not an effort percentage.
The 816-line shared generated module includes much unused WASM transport support.
This demonstrates genuine sharing of the six application functions and generated
validation/failure logic, alongside several hundred lines of platform integration.
It does not yet demonstrate a clean independent common-core crate or a broad native
backend. The next slice should make that separation explicit, not duplicate the
application lowering.

See the [reproduction commands](../../experiments/native-exp1/README.md),
[all metrics and paired ranges](../../experiments/native-exp1/results.json),
[HTTP tables](../../experiments/native-exp1/table.md), and
[evidence manifest](../../experiments/native-exp1/evidence/manifest.json). Raw results,
logs, generated source, source variants, checked projection, measured harnesses,
Bun baseline files and native artifacts are retained. The WASM candidate and the
unrelated `examples/test1/test.jadpo` / `editors/vscode/jadpo-language.vsix` changes
remain byte-identical to their initial hashes.


## Limits and decision

**Recommendation: pursue the shared generated Rust → native/WASM architecture
through one more bounded conformance slice; do not approve migration.** The measured
startup/memory benefits, several CPU wins and byte-identical application code make
that work worthwhile. Isolated ASCII losses, DELETE write tails and the two adapter
discrepancies prevent an across-the-board performance or semantic-parity claim. The identical shared
application is a useful result independently of throughput. This is not approval
for a production migration or a default-target switch.

Before adoption, resolve driver failure classification and response-budget transaction placement,
replace runtime globals
with explicit per-invocation state, and exercise a representative checked vertical
slice with real authentication/principal freshness, richer policy, cancellation,
transactions/savepoints and operational fault reporting. Account for duplicated
host policy/transaction adapters instead of treating WASM portability as automatic
backend coverage. Keep using existing database drivers.

Other limits: warm filesystem caches and uncontrolled workstation load/thermals;
short storage runs; one SQLite connection and serialized native application work;
no sustained checkpoint, disk-full, multi-process contention, crash/power-loss,
production authentication, hosted cold-start or cloud evidence. Native has bounded
HTTP/host frames and a bounded suspension loop but does not inherit WASM's 8 MiB
linear-memory sandbox, trap isolation or per-instance resource limits. The trusted
fixture control endpoints can mutate test storage and must not be deployed.

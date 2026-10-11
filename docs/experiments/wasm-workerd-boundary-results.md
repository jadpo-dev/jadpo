# Framework-informed workerd boundary results — 30 September 2026

This pass turns the [framework research](wasm-framework-boundaries.md) into a
bounded host-driver experiment. The Jadpo guest remains byte-identical:
`cf3716529f29fd0ababa6e4d99982908ba7f9326035b486f9c03d1ffe2d99c0b`.
No default target, language contract, generated validator, authority check or SQL
read is changed. Implementation checkpoint: `0285b19`.

## What the research changed

workers-rs also converts Cloudflare's JavaScript database values into Rust values.
Go's JS/WASM bindings use handles but copy when materialising native strings.
Native Rust APIs can keep values in one runtime; that is a different pipeline
from Workers. Wasmtime offers some lifetime-constrained guest-memory borrowing,
while wasm-bindgen uses direct encoding and borrowed memory views in its glue.
There is no universal framework trick that makes rich values cross for free.
Primary sources and their practical limits are in the linked research.

The selected driver writes JSON UTF-8 directly into a bounded guest allocation
and decodes results directly from a synchronous guest-memory view. It returns
owned values before any further guest call or await. The module still parses and
validates every row and only emits a receipt for an eligible unchanged checked
row. It is not zero-copy end to end and it does not cache data or permissions.

Two further alternatives remain measured but unselected. The binary encoder
derives the exact legacy JSON byte bound while encoding scalars, avoiding a
whole-row JSON/UTF-8 temporary. The adaptive variant uses that binary path only
when strings need JSON escapes. Both preserve the guest's independent bound and
validation checks, but their plain-ASCII measurements regress. Neither is silently
promoted on the strength of its escaped-text win.

## Attribution and selection

The existing-driver profile completes 24 cases: three nominal sizes, ASCII and
escaped text, full-row and title-only results, JSON and binary ingress. It checks
2,000 results per case after 200 warmups. Local stage timers are coarse and
instrumentation changes costs. These are diagnostic means, not production p95 or
additive predictions.

For the 16 KiB ASCII full-row JSON case, observed stage means are about 11.5 µs
for SQL/host validation, 6 µs for row JSON serialization, 3.5 µs for UTF-8 encoding,
2 µs for allocation/copy, and 6.5 µs for guest resume/decode/validate/return. The
escaped case increases JSON/UTF-8 and guest parsing costs. The old binary encoder
still builds a full JSON/UTF-8 representation for its host size check. This makes
conversion work a concrete target, rather than attributing the whole gap to SQL.

First selection: 120 cells with six variants. Final selection: 140 cells adding
the adaptive alternative. Five rotated repetitions per workload/target, 200 warmup
and 2,000 measured calls per cell, all responses checked inside the Object. External
elapsed time includes amortized RPC and verification; local elapsed is retained
as a cross-check. The final selection checks 280,000 measured calls. Medians in
microseconds per verified call:

| Target | Small probe | 16 KiB ASCII | Escaped Unicode | 48 KiB ASCII |
| --- | ---: | ---: | ---: | ---: |
| Generated JS | 16.36 | 18.83 | 34.05 | 25.78 |
| Previous selected WASM | 21.27 | 38.17 | 111.65 | 78.78 |
| Previous typed ingress | 23.88 | 41.24 | 102.01 | 84.92 |
| **Selected direct-write/borrowed-view driver** | **19.65** | **36.55** | **107.49** | **67.97** |
| Revised binary encoder | 20.18 | 44.12 | 89.12 | 89.79 |
| Adaptive encoder | 20.46 | 47.64 | 89.41 | 101.70 |

The selected path reduces isolated median time about 7.6% for small calls, 4.2%
for 16 KiB ASCII, 3.7% for escaped text and 13.7% for 48 KiB ASCII versus the
previous driver, using ratios of medians. The binary alternative helps escaped
text about 20% but loses elsewhere. The adaptive alternative does not solve that
tradeoff. All targets still trail generated JS on these isolated medians. The
escaped fixture has a 7,828-byte JSON success envelope; it is not a 16 KiB payload.

These are complete in-Object calls, including fresh SQL/authority checks. They
avoid one RPC per operation for diagnosis; they are not HTTP request latency or
maximum server capacity. The selected path is frozen before HTTP measurement.

## Independent HTTP result

The frozen selected path completes **120 cells and 608,612 verified requests**,
with no errors, request-count mismatches or database-snapshot failures. All 21
behavioural preflights and the 12-cell smoke pass. The same four targets share
workerd HTTP, Durable Object RPC, SQL authority adapter and response format.
Five rotated repetitions use 0.5 seconds warmup and two seconds measurement.

Paired median improvements over the previous WASM driver:

| Concurrency | Workload | Throughput change | p95 change | Throughput/p95 winning pairs |
| --- | --- | ---: | ---: | --- |
| 1 | Small | +5.4% | −2.2% | 4/5, 5/5 |
| 1 | 16 KiB ASCII | +4.2% | −1.1% | 3/5, 3/5 |
| 1 | Escaped Unicode | +5.2% | −3.9% | 3/5, 3/5 |
| 16 | Small | +1.4% | −2.9% | 4/5, 3/5 |
| 16 | 16 KiB ASCII | +3.0% | −1.1% | 3/5, 3/5 |
| 16 | Escaped Unicode | +0.8% | −1.1% | 3/5, 4/5 |

All aggregate paired medians improve, but several comparisons win only three of
five pairs. This is a modest descriptive gain, not a stable universal speedup or
a completed longer controlled-host qualification. No-work headroom remains too
small for a capacity claim. Tail behaviour is not uniformly better: for concurrent
16 KiB reads, the median p99 rises from 17.95 to 44.53 ms even while p95 improves
slightly. The raw ranges/tails are retained; the cause is not established. Retain
the candidate and evidence without promoting a
default backend or replacing the previous experiment's frozen driver.

Against generated JS on this same host, selected WASM's concurrent large-ASCII
paired medians show **10.6% lower throughput and 3.9% higher p95**. Escaped text
shows **23.0% lower throughput and 32.2% higher p95**, losing all five pairs on both
metrics. Small concurrent calls favour selected WASM in three of five pairs, with
paired medians about 4.6% higher throughput and 2.2% lower p95. This mixed result
still does not justify an all-metrics claim. Absolute values must not be compared
to a native Bun server with different host plumbing.

The useful outcome is a tested reduction in avoidable host copies and a sharper
remaining problem: escaped-text conversion and guest decoding. The binary
alternative helps that isolated case but regresses ASCII; automatic per-row
selection has not solved it. Profile those alternative encoder/decoder costs
before another design change, and use a longer controlled-host HTTP run before
claiming robust gains. Hosted cold starts and full backend coverage remain open.

## Correctness and resource limits

The new driver passes **48 tests with 13,232 assertions**, including the reused
adversarial suites, source mutation/renaming artifacts, fresh ownership changes,
stale handles, malformed frames, concurrent suspended leases, scalar ownership,
invalid Unicode and exact envelope limits. New differential cases compare the
derived binary budget/frames against the original codec for Unicode schema keys,
all escape categories, null/presence, safe integers and deterministic mixed data.
The unchanged guest is not rebuilt, and this pass makes no new native-test claim.

The direct writer may allocate up to three bytes per UTF-16 code unit, capped at
64 KiB, and passes only the actual UTF-8 length. Incomplete encoding fails closed.
This reduces intermediate copies at the cost of extra allocation space. It does
not relax the 64 KiB message or 8 MiB guest-memory limits. The new pool retains
up to seven pages (448 KiB per instance), compared with six (384 KiB) for the old
path in the HTTP run. That is neither total process memory nor per-request memory.

The HTTP harness supplies a trusted fixture principal; full authentication and
the remaining backend inventory are outside this pass. Public HTTP is still JSON.
No Cloudflare deployment, CPU/request measurement or hosted cold-start measurement
is implied. Local Node drives HTTP; workerd executes the application without Bun.

## Evidence

The [harness](../../experiments/wasm-exp1/workerd-boundary/README.md) documents commands,
protocols and limits. The [full table](../../experiments/wasm-exp1/workerd-boundary/table.md)
and [paired results](../../experiments/wasm-exp1/workerd-boundary/results.json) retain
all repetitions, ranges, tails, no-work headroom and losses. Compressed evidence
includes the instrumented profile, both selection rounds, exact worker bundles,
module, source snapshots, selection hashes and checks. Early setup mistakes in
property-order comparison and a copied helper import were corrected before
selection; no application expectation was weakened.

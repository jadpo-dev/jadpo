# Large-read receipts and compact query plans — 30 September 2026

This is the third bounded [large-read investigation](wasm-large-row-plan.md),
following the [immutable-row experiment](wasm-typed-values-results.md). Bun remains
the working target. Both targets in these measurements use Bun hosting; no EC2,
standalone WASM host or new Cloudflare deployment is implied. A separate
[local workerd comparison](wasm-workerd-read-results.md) evaluates the same module
without Bun. The two campaigns have different host plumbing and timing protocols;
their absolute request rates must not be compared as a platform ranking.

The selected candidate improves concurrent large-read throughput about 11.4%
against previous WASM, but still trails generated Bun by about 19%. It does not
establish an all-metrics WASM win or justify changing the default target.

## What changed

The selected mode combines two changes. First, a fresh read still sends the whole
row to WASM, where the generated code validates it and executes the authored
function. If the result is that exact immutable row, WASM emits a 48-byte receipt
instead of copying the whole row back. The host resolves it against a private
scalar snapshot belonging only to this invocation, verifies the schema and
request/operation identity, and returns a fresh ordinary object. Materialised or
field-extracted values use ordinary transport. There is no cross-request data or
authorisation cache and no unchecked raw JSON response shortcut.

Second, a query sends its checked plan ID and runtime predicate. The host restores
the compiler-generated full descriptor and runs the same authority adapter and
policy checks, including the fresh scoped SQL read. For this fixture the pending
message shrinks from 694 to 86 bytes. The plans are cloned/frozen private metadata;
the guest cannot override their policy graph. Both ends retain the original JSON
envelope limit even when the compact frame is smaller.

Direct binary ingress was also implemented: the decoder constructs owned typed
fields and runs validators generated from the same effective types. It remains an
alternative, because it did not beat JSON ingress in the selection measurements.
No source syntax or default backend changes.

The selected module is
`cf3716529f29fd0ababa6e4d99982908ba7f9326035b486f9c03d1ffe2d99c0b`,
mode 14 (compact plans, JSON ingress, receipts, ordinary binary fallback).
The frozen previous module and driver are the immutable-row candidate
`631ea8e20a3cc55936f857acefe3fe68ada60b83a7b737fc651a51fae40b8b91`.
Implementation checkpoint: `8098469`. Selection preceded independent HTTP timing.

## What failed, and what the focused revision fixed

The first implementations lost. On the 16 KiB isolated workload, previous WASM
ran about 19,563 operations/s, direct typed ingress 13,972, and the first receipt
variant 18,088. These results, their exact modules and source snapshots are retained.
The first compact-query revision helped, but still left these host-side costs:

- JSON serialization of a frozen private row snapshot took about 9.2 microseconds,
  versus 2.0 for an ordinary object in the component diagnostic. Freezing added no
  protection the private scalar ownership contract needed. The final snapshot is
  never exposed or mutated; callers only receive a fresh object.
- The proposed JS escape-counting scan took about 24.6 microseconds per large row.
  Native JSON serialization/UTF-8 measurement was faster, so it supplies the host
  size check again. The guest independently enforces the same JSON budget.

These are diagnostic component means, not additive predictions of request p95.
The initial profiler put copying/decoding the old large return at roughly six
microseconds. Avoiding that return only helps if preparation costs stay smaller.

## Isolated candidate selection

Five rotated runs per variant, 0.2 seconds warmup plus one second measured,
full response checks, on-disk SQLite DELETE/FULL:

| Variant | Small operations/s | Small p95 µs | 16 KiB operations/s | 16 KiB p95 µs |
| --- | ---: | ---: | ---: | ---: |
| Bun | 40,882 | 27.79 | 31,511 | 36.12 |
| Previous WASM | 42,055 | 31.13 | 18,694 | 62.00 |
| Typed binary ingress + egress | 41,290 | 31.08 | 18,329 | 62.88 |
| Receipt only | 41,306 | 31.17 | 20,861 | 57.29 |
| Compact query only | 44,435 | 28.79 | 19,291 | 59.92 |
| **Compact query + receipt, selected** | **43,563** | **29.83** | **22,149** | **53.96** |
| Compact query + receipt + typed ingress | 43,167 | 29.71 | 21,808 | 53.67 |

The selected mode has about 18.5% higher large-read throughput and 13.0% lower
p95 than previous WASM by ratios of medians. These are application-boundary
measurements, not HTTP capacity. All nine variants are retained in raw evidence;
this table omits only the two diagnostic transport controls.

## Independent HTTP qualification

Five rotated repetitions, concurrency 1/16, two seconds warmup and ten seconds
measured per cell, fresh server processes, SQLite DELETE/FULL. All 80 cells in
the completed campaign passed, with **12,441,614 verified requests and no errors**.
Every server also passed eight preflight cases; responses, SQL state and server
request counts were checked. Medians:

| Concurrency | Workload | Target | Requests/s | p95 ms | CPU µs/request |
| --- | --- | --- | ---: | ---: | ---: |
| 1 | Small | Bun | 10,492 | 0.1268 | 50.78 |
| 1 | Small | Previous WASM | 10,705 | 0.1231 | 49.06 |
| 1 | Small | Selected WASM | 10,722 | 0.1232 | 49.50 |
| 1 | 16 KiB | Bun | 7,681 | 0.1808 | 65.23 |
| 1 | 16 KiB | Previous WASM | 6,614 | 0.2100 | 86.70 |
| 1 | 16 KiB | Selected WASM | 6,898 | 0.2028 | 81.07 |
| 16 | Small | Bun | 24,419 | 1.1796 | 40.75 |
| 16 | Small | Previous WASM | 25,190 | 1.1265 | 39.28 |
| 16 | Small | Selected WASM | 25,892 | 1.1150 | 38.46 |
| 16 | 16 KiB | Bun | 17,017 | 1.7032 | 56.55 |
| 16 | 16 KiB | Previous WASM | 12,372 | 2.2563 | 79.44 |
| 16 | 16 KiB | Selected WASM | 13,783 | 2.0606 | 70.65 |

At concurrency 16, the candidate improves large-read throughput about 11.4%,
p95 about 8.7%, and CPU/request about 11.1% versus previous WASM, using ratios of
medians. Against Bun it still has about 19.0% lower throughput, 21.0% higher p95
and 24.9% more CPU/request. The practical parity gate remains failed. These are
local descriptive measurements; the no-work ceiling is too close to establish
maximum server capacity. Raw paired ratios and ranges are retained in the
[results](../../experiments/wasm-exp1/read-path/results.json), including losses.

Bun is the experimental host in this campaign. This does not make it a required
runtime for the WASM deployment. The costs measured here include its SQLite
adapter, JS-to-WASM transfer and HTTP implementation. Cloudflare has a different
engine and host APIs; the same boundary must be measured there before concluding
that large-row costs disappear.

## Size, escaping, startup and retention

The supplemental isolated matrix completes all 240 cells: full-row and title-only
returns, four nominal sizes, ASCII and escaped-Unicode fixtures, five rotated
repetitions and three targets. Full-row candidate throughput improves about
8–28% over previous WASM across paired medians. That improvement does not deliver
Bun parity. At 48 KiB ASCII the candidate reaches about 50% of Bun throughput;
the largest escaped fixture reaches about 29%. Escaped fixtures deliberately use
fewer characters to fit the original envelope limit: the latter full-row frame
is 17,812 bytes, not 48 KiB. Compare like fixtures, not nominal size across kinds.

Title-only returns still read and validate the full database row. At the largest
ASCII size, the candidate is almost unchanged from previous WASM and about 50%
of Bun throughput. A return receipt cannot eliminate ingress/validation costs.
These are isolated local measurements with harness checks included, not HTTP
latencies or workerd results. Workerd's current timing pass covers ASCII only;
escaped and larger workerd cases remain open.

The small-probe regression gate passes at both concurrency levels. The 24-cell
write smoke passes **3,839 requests**, with response/count/snapshot checks; it is
not renewed write-throughput or durability qualification. The full-read candidate
pool has no discards, no active requests at sampled ends, and retains at most six
pages (384 KiB per instance). This does not measure total process memory.

Twenty fresh processes per target, alternating order, measure local SQLite setup
and a first verified HTTP read with warm OS caches. Median process-to-first-response
is **30.69 ms for the WASM candidate versus 72.29 ms for generated Bun**. Ready time
is 26.95 versus 68.93 ms, while the first HTTP exchange itself is 4.02 versus
3.35 ms. Both processes still use Bun hosting. This is a local startup win, not a
Cloudflare cold-start result. All 40 first responses pass.

The selected module is 178,745 raw bytes / 68,081 gzip bytes, versus the previous
172,443 / 66,219. Build size therefore increases slightly; there is no all-metrics
win. The retained result file includes ranges and every paired gate outcome.

## Correctness and scope

The final local checks pass 45 Bun tests with 12,785 assertions and nine native
Rust tests. The selected mode passes 15 probe cases, 16 full-slice runtime cases,
five unsupported-feature rejection gates and four adversarial rollback cases.
Two pre-existing hosted cases remain unrun. Clean Cargo-cache rebuilding produces
the identical module; actual source renaming and constraint mutation govern the
new decoder, receipts and compact plans.

Checks include malformed/truncated/oversized frames, wrong schema/digest/handles,
exact JSON limits and all ASCII control/escape positions, stale resumes, traps,
reset, interleaved requests, invalid returned fields, missing rows, field extraction,
private snapshot ownership and source constraint changes. A real SQLite test
changes row contents and ownership between calls: the old owner immediately loses
access and the new owner sees the new contents. No cached authorisation passes it.

The existing oversized-host-frame attribution fallback and pinned Bun SQLite
leading-BOM limitation remain documented in earlier results. The fixture uses a
trusted test principal, not a complete production authentication stack. General
language coverage, arbitrary row transformations, controlled-host throughput,
crash recovery, hosted cold starts and current-artifact Cloudflare qualification
remain separate evidence requirements.

## Evidence and reproducibility

Implementation and commands are in
[`experiments/wasm-exp1/read-path`](../../experiments/wasm-exp1/read-path/README.md).
Toolchains remain Bun 1.2.20, Node 24.18.1 and Rust/Cargo 1.78.0. Source/projection,
module, driver and harness hashes are recorded with the raw evidence.

The first HTTP campaign ended with a timeout during confirmed clamshell sleep
(macOS logged sleep at 09:28:24 +0100; the run failed at 09:28:46). Its eleven
cells, failed request, log and system evidence are retained separately. The whole
campaign was restarted with no individual request retries. This failure is not
removed from the experiment's record or described as an application success.

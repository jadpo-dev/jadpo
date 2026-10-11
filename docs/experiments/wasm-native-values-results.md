# Native-runtime-inspired WASM value results — 30 September 2026

This pass follows the [native-runtime research](wasm-native-runtime-values.md)
and the [previous workerd boundary experiment](wasm-workerd-boundary-results.md).
Implementation checkpoint: `00ea477`. The guest remains byte-identical, SHA-256
`cf3716529f29fd0ababa6e4d99982908ba7f9326035b486f9c03d1ffe2d99c0b`.
The tested changes are isolated experimental host glue. No default backend,
language contract, generated validator, authority check or SQL plan changes.

## What changed

Bun's native SQLite binding constructs JavaScriptCore values directly. Node and
CPython have corresponding native integrations. Our Workers WASM path also
converts Cloudflare's JS row into guest memory and Rust values. workers-rs faces
that conversion too. The research links exact source locations and distinguishes
this architectural observation from any cross-runtime performance claim.

The successful isolated change avoids unnecessary escaped JSON serialization
used solely to enforce the original size budget. The host knows the actual UTF-8
bytes it wrote; a conservative upper bound can prove the JSON envelope fits.
Only ambiguous cases need exact escape spelling. The guest still independently
checks the exact JSON size and validates every row and source refinement.

A second change limits content selection to the first 256 UTF-16 code units per
scalar string. For large rows, sampled escapes or non-ASCII characters select
binary ingress; otherwise the previous JSON path remains. Escapes outside the
sample still get complete JSON encoding, parsing and validation. This is a bounded
performance heuristic, not a proof that the rest of the string is ASCII or safe.

Direct typed writes into guest memory were also implemented and tested. They
remove one explicit copy but do not improve the key escaped-text median and
regress ASCII in isolation. They remain archived alternatives, not the selection.

## Three isolated selection rounds

All rounds use local workerd, fresh SQL/authority checks and the same module.
Each cell warms 200 calls and checks 2,000 measured calls in one Object batch.
External elapsed time per call includes amortized RPC and verification. Internal
local timing is retained as a coarse cross-check. These numbers are neither HTTP
latency nor maximum server capacity.

The first round covers 140 cells, seven targets and four workloads. The
conservative-budget encoder reduces escaped-text time from 90.52 µs for the
previous binary encoder to 59.17 µs; direct writes score 59.14 µs. Both remain
slower than the previous JSON driver for ASCII. This supports removing redundant
serialization; it does not support promoting direct writes.

The second round adds bounded content sampling, a full-string scan control,
late escapes and Unicode-only data: 270 cells. The full-scan control measures
48.02 µs for 16 KiB ASCII and 94.68 µs for 48 KiB; prefix sampling measures 37.64
and 62.49 µs, versus the previous JSON driver's 36.77 and 60.30 µs. This supports
bounding selection work. It is not an engine-level profile proving why a regexp
has a particular cost. Binary helps Unicode-only data too, motivating the final
sample predicate's non-ASCII check. Small rows bypass content selection.

The final round freezes that candidate against the previous selected driver,
generated JS and no-op: 120 cells. All three rounds retain **1,060,000 verified
measured calls** and every alternative. Final medians, in microseconds:

| Target | Small | 16 KiB ASCII | Escaped Unicode | 48 KiB ASCII | Unicode only | Late escapes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Generated JS | 15.60 | 19.11 | 33.96 | 25.09 | 81.95 | 36.50 |
| Previous selected WASM | 20.43 | 36.59 | 107.45 | 61.36 | 147.95 | 133.16 |
| Sampled candidate | 19.44 | 38.38 | 58.52 | 60.73 | 134.72 | 133.16 |
| No-op | 0.33 | 0.48 | 0.41 | 0.45 | 0.48 | 0.48 |

Against previous WASM, ratios of medians show **45.5% lower escaped-text time**
and **8.9% lower Unicode-only time**. The 16 KiB ASCII median rises 4.9%, 48 KiB
falls 1.0%, and late-escape time is unchanged. The candidate still trails generated
JS in every isolated workload. A prefix heuristic misses late escapes, as expected.
There is no all-metrics victory over JS or native Bun.

Payloads differ between workload families; compare targets within a family.
Escaped success JSON is 7,828 bytes, Unicode-only 18,580, late escapes 16,020,
16 KiB ASCII 16,532, and 48 KiB ASCII 49,300. The small operation returns a title.
The 48 KiB workload is isolated only. The generated JS baseline and SQL adapter
are the same in each target, and public HTTP/RPC representations remain JSON.

## Independent HTTP qualification

The frozen candidate completes **200 cells and 924,283 verified requests** with
zero errors, count mismatches or snapshot failures. All 21 behavioural preflights
and the 20-cell smoke pass. Five rotated repetitions use concurrency 1/16,
0.5-second warmups and two-second measurements. All targets share the local
Worker → Durable Object RPC → SQLite → JSON HTTP pipeline. Node is the external
client; Bun is not the application host.

Paired median changes versus the previous selected WASM driver (negative p95
change means faster):

| Concurrency | Workload | Throughput change | p95 change | Throughput/p95 winning pairs |
| --- | --- | ---: | ---: | --- |
| 1 | Small | -3.4% | -2.6% | 2/5, 4/5 |
| 1 | 16 KiB ASCII | -5.3% | +0.9% | 2/5, 1/5 |
| 1 | Escaped Unicode | +17.8% | -7.3% | 5/5, 5/5 |
| 1 | Unicode only | -4.2% | +1.4% | 1/5, 2/5 |
| 1 | Late escapes | -2.0% | -5.9% | 2/5, 3/5 |
| 16 | Small | -2.9% | +5.7% | 1/5, 1/5 |
| 16 | 16 KiB ASCII | +0.3% | +1.1% | 3/5, 2/5 |
| 16 | Escaped Unicode | +16.6% | -14.8% | 5/5, 5/5 |
| 16 | Unicode only | -3.9% | +1.6% | 2/5, 2/5 |
| 16 | Late escapes | +1.9% | -0.0% | 3/5, 3/5 |

Escaped text improves throughput and p95 in all five pairs at both concurrency
levels. At concurrency 16, the candidate's paired medians are roughly equal to
JS: +0.4% throughput and −0.5% p95, winning only three of five pairs. This is
observed local HTTP parity for that fixture, not demonstrated equal CPU efficiency
or production capacity. Isolated escaped execution still takes 58.52 versus
33.96 µs for JS. At concurrency 1, escaped HTTP still trails JS by 7.5% throughput
and 9.3% p95.

Other workloads do not establish parity. At concurrency 16, the candidate trails
JS by 11.9% throughput for large ASCII, 8.7% for Unicode only and 18.5% for late
escapes; corresponding p95 increases are 6.8%, 14.5% and 27.8%. Each loses all
five throughput pairs. The isolated Unicode gain fails to translate into HTTP
throughput: it is about 4% below previous WASM at both concurrency levels. Small
concurrent requests also regress about 2.9% throughput and 5.7% p95 versus previous
WASM. These losses are retained, not averaged away into the escaped-text win.

Tails are mixed. Concurrent large-ASCII p99 improves in all five pairs against
previous WASM (paired median ratio 0.358), but Unicode-only p99 regresses (ratio
1.130, one winning pair). The cause of long-tail variation remains unestablished;
a prior p99 spike is not declared fixed by these short measurements. Full ranges,
maxima and all pairs are in the retained table/data.

Both drivers retain at most seven guest pages (448 KiB per pooled instance) in
this HTTP run. That is not total process memory. No-work control headroom remains
below the experiment's 2× criterion, so this shared local client/RPC setup cannot
establish maximum capacity. Longer controlled-host and independent-generator
qualification remains open.

## Correctness and evidence boundaries

**53 tests, 13,647 assertions, zero failures.** The selected sampling mode runs
through the inherited adversarial host suites via an explicit test shim. Separate
tests exercise direct writes, offset memory views with guard bytes, incomplete
encoding, malformed surrogates, exact JSON limits, fallback after oversized
binary data, guest validation failures, repeated memory growth and suspended
concurrent leases. Sampling tests put escapes before, at and beyond the prefix.
The budget fallback reads accessors only once and excludes inherited values.
Existing malformed-frame, stale-handle, source mutation, schema renaming, fresh
ownership and receipt-isolation checks remain in the passing suite.

The guest is unchanged and is not rebuilt here; this is not a new Rust compiler
or native-guest test claim. Original 64 KiB frames, 8 MiB maximum guest memory,
pool bounds and independent validation remain. The fixed trusted fixture principal
is not a complete authentication benchmark.

The exact bundle, source hashes and selection rationale are frozen before HTTP.
Earlier selection source/bundles, losing alternatives, raw results and checks are
retained. The first two raw selection reports inherit an HTTP workload list in
one metadata field; actual result cells and archived loops define the complete
four/six-workload matrices. The final runner corrects the field. No measurements
are dropped or reinterpreted to hide that bookkeeping issue.

The [protocol](../../experiments/wasm-exp1/workerd-native-values/README.md),
[full HTTP table](../../experiments/wasm-exp1/workerd-native-values/table.md),
[paired statistics](../../experiments/wasm-exp1/workerd-native-values/results.json)
and [evidence manifest](../../experiments/wasm-exp1/workerd-native-values/evidence/manifest.json)
provide reproduction details and all raw ranges. This local experiment makes no
CPU/request, total process RSS, hosted cold-start, full backend coverage or
production capacity claim.


## Next engineering decision

Retain the sampled candidate as an experiment, alongside the frozen previous
JSON path. It has a substantial isolated escaped-text win, but workload-sensitive
results do not justify a universal target change. A useful next attribution pass
is to separate the now-cheaper host encoder from typed guest UTF-8 decoding,
allocation and generated validation, using matched payload sizes and both full-row
and scalar extraction. Do not add those component means to predict HTTP tails.

A broader text corpus should vary where escapes occur, script, string length,
null/presence and transformed outputs before adopting a content heuristic.
Controlled-host qualification still needs longer runs, independent generator
headroom and CPU measurements. Full authentication/backend coverage and actual
hosted cold starts remain separate open gates. A native host that avoids the
JS row representation would be a distinct experiment, not a fix automatically
available through Cloudflare's current SQLite binding.

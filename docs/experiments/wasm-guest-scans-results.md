# WASM guest string and bulk-memory results — 30 September 2026

This pass continues the [native-values experiment](wasm-native-values-results.md)
toward parity with generated JavaScript on workerd. Implementation checkpoint:
`e81e7a2`. The checked application, source rules, SQL authority adapter, ownership
checks and public JSON responses remain the same. The experimental guest and
host transport change; the default backend does not.

## Evidence leading to the changes

A 16-case local profile covers four text families, full-row/title extraction, and
JSON/binary ingress. Every case checks 2,000 calls after 200 warmups. Stage timers
are coarse and instrumentation changes execution; these means are diagnostic,
not additive predictions of HTTP p95.

Unicode-only binary reads spend about 79 µs in SQL/host validation, 15 µs encoding
and 26.5 µs in guest resume/decode/validate/return. Late-escape binary reads spend
about 65.5 µs in host encoding; ambiguous conservative bounds still serialize the
entire string to compute its exact escaped size. The guest independently computes
that exact size again. Plain ASCII also pays for buffer allocation/copies. The
large note field is unconstrained text, so long application refinement checks are
not the explanation for these particular fixtures.

Three changes follow:

1. **Keep the exact logical size check in the guest.** The host still bounds every
   physical frame. It may omit its duplicate JSON-size computation only after
   negotiating `row_budget_version() == 1`. The guest calculates the complete
   canonical JSON success-envelope size, including escapes, and rejects excess
   before validation or a row receipt. `row_budget_exceeded()` preserves the old
   generic host size-fault envelope; state clears on reset and typed resume.
2. **Use SIMD for guest string work.** Pinned `simdutf8 0.1.5` validates typed UTF-8.
   A 16-byte SIMD counter computes exact JSON escape costs, with a scalar suffix.
   It checks every byte; it does not sample the data. Loads are bounded by exact
   chunks and malformed UTF-8 still fails closed.
3. **Enable bulk memory.** The earlier build explicitly disabled it. Enabling
   `memory.copy`/`memory.fill` allows efficient initialized buffer copies/fills;
   it does not introduce uninitialized memory or borrowed lifetimes.

Cloudflare documents SIMD support. The pinned library documents compile-time
WASM SIMD selection; the selected bulk-memory module is also actually loaded and
executed by this local workerd version. These are different kinds of evidence,
not a claim that every prospective WASM host supports the same feature set.
Sources: [Cloudflare WebAssembly](https://developers.cloudflare.com/workers/runtime-apis/webassembly/),
[simdutf8 0.1.5 source and configuration](https://github.com/rusticstuff/simdutf8/tree/v0.1.5),
[Rust WASM intrinsics](https://doc.rust-lang.org/core/arch/wasm32/).

## Isolated selection

First selection has eight targets, six workloads and five rotations: 240 cells.
Moving the exact budget into the scalar guest reduces late-escape calls from
133.91 to 75.36 µs, while SIMD reduces them further to 67.31 µs. Always using typed
ingress for eligible large rows also beats retaining the content heuristic.

Second selection has six targets, six workloads and five rotations: 180 cells.
Each cell warms 200 calls and verifies 2,000 calls in one Object batch, using an
external elapsed clock and retaining local timing as a cross-check. Both rounds
retain **840,000 verified measured calls**, including all losing alternatives.
Medians below are microseconds per complete in-Object call, not HTTP latency:

| Target | Small | 16 KiB ASCII | Escaped Unicode | 48 KiB ASCII | Unicode only | Late escapes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Generated JS | 16.12 | 18.90 | 33.81 | 25.04 | 81.13 | 36.77 |
| Previous sampled WASM | 20.95 | 37.77 | 58.42 | 62.14 | 133.68 | 133.59 |
| Typed SIMD, bulk disabled | 19.74 | 32.03 | 50.60 | 55.96 | 119.62 | 66.97 |
| **Selected typed SIMD + bulk memory** | **19.83** | **28.95** | **49.33** | **44.11** | **115.22** | **64.23** |
| SIMD + bulk, JSON ingress | 21.10 | 30.53 | 104.22 | 48.77 | 143.53 | 124.93 |
| No-op | 0.30 | 0.46 | 0.41 | 0.50 | 0.46 | 0.49 |

The selected variant improves every isolated median over the previous sampled
candidate: approximately 5% small, 23% 16 KiB ASCII, 16% escaped, 29% 48 KiB ASCII,
14% Unicode-only and 52% late escapes. Ratios here are ratios of medians, not
HTTP paired statistics. Every selected isolated workload still trails JS.

The selected path uses typed transport for all eligible large rows, without a
content-selection heuristic. Small/unsupported shapes retain the original JSON
path. The identical compiler-produced schema and source validators govern both.
Compared targets have identical fixture bytes within each family; families differ
in payload size. Unicode-only JSON is 18,580 bytes, escaped 7,828, late escapes
16,020, 16 KiB ASCII 16,532, and 48 KiB ASCII 49,300. The 48 KiB case is isolated
only. All calls perform fresh SQL/authority work.

## Independent HTTP result

The frozen candidate completes **200 cells and 949,050 verified requests** with
zero errors, count mismatches or snapshot failures. All 21 preflights and the
20-cell smoke pass. Five rotated pairs at concurrency 1/16 use 0.5-second warmup
and two-second measurement. Node drives HTTP; workerd executes the application.

Paired median changes versus the prior sampled WASM candidate:

| Concurrency | Workload | Throughput | p95 latency | Throughput/p95 wins |
| --- | --- | ---: | ---: | --- |
| 1 | small | -4.6% | +4.4% | 1/5, 1/5 |
| 1 | large | +7.0% | -2.5% | 4/5, 5/5 |
| 1 | escaped | +1.9% | -4.2% | 3/5, 4/5 |
| 1 | unicode | +12.7% | -7.4% | 4/5, 5/5 |
| 1 | late | +16.2% | -13.6% | 5/5, 5/5 |
| 16 | small | +0.0% | -4.7% | 3/5, 4/5 |
| 16 | large | +3.6% | -1.6% | 3/5, 3/5 |
| 16 | escaped | +4.9% | -4.1% | 4/5, 4/5 |
| 16 | unicode | -3.6% | +2.4% | 2/5, 2/5 |
| 16 | late | +15.7% | -11.0% | 5/5, 5/5 |

At concurrency 1, large-row throughput is within −4.1% to +1.6% of JS, but p95
remains 5.9–7.6% higher. Small calls trail JS by 8.4% throughput. At concurrency
16, large ASCII/escaped/Unicode/late-escape throughput trails JS by 8.0%, 5.0%,
14.1% and 8.1%; p95 is 7.3%, 8.7%, 7.9% and 7.9% higher respectively. Small
concurrent calls favour WASM by 2.2% throughput and 3.6% p95, with four/five and
five/five winning pairs. This is not general parity.

Unicode's concurrent throughput also regresses 3.6% against previous WASM, with
only two winning pairs, despite its 14% isolated improvement. Its paired p99 ratio
is 2.36 against previous WASM. Large ASCII's p99 ratio against JS is 2.34. The
causes of these tails are not established and the results are retained. Late
escapes improve throughput and p95 against previous WASM in all five pairs at
both concurrency levels. Broader improvements do not erase regressions.

Selected guest retention falls from seven to six pages (448 → 384 KiB per pooled
instance). This is not total process memory. No-work headroom remains below the
2× criterion, preventing a capacity conclusion; isolated CPU work and HTTP
throughput must not be conflated. The meaningful next optimization test is owned
buffer sharing for typed row strings, which are still copied into separate Rust
allocations after validation. It must preserve ownership, UTF-8, mutation and
exact-bound checks; it is not permission to return unchecked host rows.

## Correctness and resource checks

The selected guest passes **52 host tests with 15,234 assertions**, plus **9 Rust
tests**. Native tests are not treated as evidence that SIMD instructions execute;
the differential WASM tests run old, scalar, SIMD and SIMD+bulk modules. They check
mixed valid Unicode, every ASCII control/escape category, exact JSON bounds,
short binary frames with oversized logical JSON, overlong/surrogate/out-of-range
UTF-8, truncated tails and malformed sequences around 16/64-byte boundaries.

Inherited suites run against the selected bulk guest: stale handles, malformed
frames, bounded reuse, concurrent suspended leases, owned snapshots, transformed
outputs, fresh ownership changes and schema/source mutation checks. The new host
rejects an old guest without the budget capability. Checked source variants with
renamed entities/principals and changed refinements are rebuilt through this
compiler. Restoring the original projection, removing the selected Cargo cache
and rebuilding produces byte-identical selected WASM.

An initial test caught a changed internal-size-error envelope. The implementation
was fixed to preserve it; the equality expectation remains. The mutation harness
also initially compared an edited source to a stale pre-edit artifact; it now
builds its baseline first. CamelCase variant names initially violated fixture-key
syntax and were corrected in the harness before measured cells. These failures
and their logs remain archived. No correctness expectation was weakened.

The selected module is 178,919 raw / 68,751 gzip bytes, versus the original
178,745 / 68,081: about 1% compressed growth. Disassembly contains 123
`memory.copy`, three `memory.fill`, 308 `v128.load` and 21 `i8x16.bitmask`
instructions; counts prove emission, not a speedup by themselves. SIMD adds a
pinned optional dependency in this experimental compiler. Frame limits remain
64 KiB, maximum memory 8 MiB, and stack reservation 256 KiB. Actual hosted cold
starts and total process memory are not measured.

Selected module SHA-256:
`63358b4df468b9e99dcc1b3ef0faeff7d362b4e297bd382c3b9be2d36080448c`.
The selected worker bundle and all module hashes are identical between isolated
selection and independent HTTP. A subsequent explanatory source comment changes
only esbuild input-byte metadata. Final tests use the selected bulk module.

## Evidence and remaining scope

See the [protocol](../../experiments/wasm-exp1/workerd-guest-scans/README.md),
[HTTP table](../../experiments/wasm-exp1/workerd-guest-scans/table.md),
[paired results](../../experiments/wasm-exp1/workerd-guest-scans/results.json), and
[evidence manifest](../../experiments/wasm-exp1/workerd-guest-scans/evidence/manifest.json).
Evidence includes earlier variants, diagnostics, failures, frozen bundles/modules,
compiler sources, generated Rust/schema, checked projection and mutation evidence.

This is a local workerd experiment with a trusted fixture principal, not full auth
or complete backend coverage. It does not qualify native Bun, a different WASM
host, cloud cold starts or production capacity. Broader text sizes and transformed
result shapes, controlled-host CPU/headroom measurements and backend coverage
remain necessary before switching the default target.


## Shared-frame follow-up decision

A [separate ownership experiment](../../experiments/wasm-exp1/workerd-shared-rows/README.md)
also completes in this work session: 52 host tests/14,733 assertions, ten native
Rust tests, source mutation and clean-rebuild checks, plus 120 isolated cells and
240,000 verified measured calls. Typed strings share an owned validated input
frame, while materialized results remain independent. The exact frozen SIMD/bulk
module serves as its control. Most gains are only 1–2%, and 16 KiB ASCII regresses
slightly. It is unselected and receives no new HTTP qualification. Retain the
simpler bulk-memory candidate and the negative result rather than adding lifetime
complexity for an unconvincing speedup.

This makes another large-string-copy rewrite a lower priority. The next useful
attribution is the remaining optimized host/control-message path and the source
of concurrent Unicode/tail regressions, with stronger generator/headroom evidence.

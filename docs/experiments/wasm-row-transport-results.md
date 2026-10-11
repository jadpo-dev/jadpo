# Wasm row transport experiment — 30 September 2026

Subsequent checkpoint: [immutable row values and SQLite write attribution](wasm-typed-values-results.md).
The measurements below describe this earlier candidate.

**Result: a repeatable, modest large-row read improvement; practical Bun parity
failed. Retain this as an experimental row-egress candidate, not a production ABI
or default-target change. Concurrent single-write performance remains adverse.**

## What this pass tests

The [large-row plan](wasm-large-row-plan.md) identified repeated representation and
encoding as the next question. This pass profiles the actual full-row return, then
tests an optional schema-bound binary format while keeping the same SQL adapter,
policy checks, generated validators, instance leasing and external HTTP JSON.
Both HTTP targets run inside Bun; no standalone Wasm host, EC2 deployment or new
Cloudflare qualification is implied.

The compiler generates field order, record identifiers, result schemas and a full
projection digest. Frames preserve omitted versus null fields and carry bounded
UTF-8/scalar values. The guest retains generic `serde_json::Value` internally. This
is a transport experiment, not the proposed complete specialised value runtime.

## Attribution and rejected variants

The original module's instrumented 16 KiB ASCII full-row read spent approximately
16.4 microseconds in guest resume/decode/validation/execution/encoding. Returning
only the title after reading the same row spent 6.4 microseconds in that phase.
Host SQL plus validation was about 8.7 microseconds in both cases. Instrumentation
adds overhead; these means cannot be summed to predict uninstrumented p95.

Both backends validate entrypoint inputs and database rows. The unconstrained
large note is a type check, not a Unicode-length scan; constraints on identifiers
and titles remain. The Wasm storage adapter and guest enforce separate boundaries.
Removing those checks was not an optimisation candidate.

The first binary implementation lost badly: isolated 16 KiB throughput fell from
about 16.7k/s for previous Wasm to 10.6k/s for both directions. Its JSON-budget
counter scanned text inefficiently, and encoding incoming rows still incurred
conversion costs. This negative result is retained. One focused revision added
eight-byte skipping for escape-free JSON-size counting, avoided a redundant size
scan when allocating output, and retained JSON for small rows. Ingress still did
not help; **egress only** was selected before HTTP qualification.

During review, an additional edge case was fixed: a binary frame can exceed the
byte limit even if JSON fits, particularly with short field names. The corrected
runtime falls back to JSON. The preliminary HTTP run was stopped and retained;
the final run uses the corrected, frozen module. A sandboxed smoke attempt failed
to listen before any timed requests; loopback permission was then obtained.

## Final isolated confirmation

Five rotated samples, 0.2 seconds warmup and one second measured, every output
checked. These are application-boundary calls with the inherited harness overhead,
not HTTP or maximum-capacity results.

| Target | Small operations/s | Small p95 µs | 16 KiB operations/s | 16 KiB p95 µs |
| --- | ---: | ---: | ---: | ---: |
| Generated Bun | 40,706 | 28.75 | 31,423 | 36.88 |
| Previous Wasm | 43,327 | 29.83 | 16,765 | 73.17 |
| Selected row egress | 43,354 | 29.67 | 18,381 | 66.29 |

The selected module improves large-row throughput by approximately 9.6% and p95
by 9.4% against previous Wasm, with essentially unchanged small-read results.
It remains far from Bun parity: approximately 59% of Bun's isolated throughput.

The supplemental matrix was repeated on the corrected artifact with 256 B, 4 KiB,
16 KiB and 48 KiB text and ASCII/escaping/Unicode cases. Compared with previous
Wasm, the 48 KiB ASCII cell improved throughput about 22% and p95 about 18%; the
largest escaped cell improved throughput about 33% and p95 about 25%. These shorter
isolated runs do not establish HTTP gains at those sizes. Escaped cells use fewer
characters to fit the JSON limit; actual frame sizes are recorded. Small values
did not universally improve: the 256 B full-record ASCII cell had about 7.8% worse
p95, despite the primary small-probe HTTP regression gate passing.

## HTTP qualification and gate outcomes

Five rotated repetitions per read cell used two seconds warmup and ten seconds
measured at concurrency one and sixteen. Every response and final database
snapshot was checked, with identical file-backed SQLite DELETE/FULL settings.
Eighty read measurements completed 11,789,656 requests with zero errors. Separate
write checks completed 36 measurements and 165,144 requests with zero errors.
Each of the 58 server processes passed eight preflight cases and exited.

At concurrency sixteen, medians of five repetitions for the 16 KiB row:

| Metric | Generated Bun | Previous Wasm | Row-egress candidate |
| --- | ---: | ---: | ---: |
| Requests/s | 16,470 | 10,914 | **11,676** |
| End-to-end p95 ms | 1.7647 | 2.5442 | **2.4108** |
| Server CPU µs/request | 58.29 | 90.19 | **84.01** |

Against previous Wasm, paired gains occurred in all five repetitions: throughput
5.8–9.3% higher, p95 3.7–6.9% lower, CPU/request 5.8–6.9% lower. Ratios of medians
give approximately +7.0%, -5.2% and -6.8%, respectively. At concurrency one the
candidate also improved all three metrics in every pair, but HTTP overhead diluted
the throughput gain to about 3.5% by ratio of medians.

The preregistered large-row parity gate **fails** at both concurrencies. No paired
run met the Bun ±10% thresholds for throughput, p95 or CPU. At concurrency sixteen,
the candidate remains about 29% lower in throughput, 37% higher in p95 and 44%
higher in CPU/request than Bun. The primary small-probe regression gate **passes**:
paired medians versus previous Wasm remain within 5% for all three metrics at both
concurrencies. The small-probe candidate still modestly outperforms Bun in this run.

No-work controls reached about 31.1k small and 19.0k large responses/s at concurrency
sixteen. They lack the required 2x headroom: these are observed local closed-loop
rates, not established maximum capacity or fixed-arrival-rate SLOs. Workstation
load, thermal state and filesystem caches are uncontrolled. p99 is retained;
maximum individual HTTP latency was not retained by this inherited harness. Pool
bounds are tested, but this is not a per-request/peak-memory qualification.

## Writes remain an adoption blocker

At concurrency sixteen, medians of three shorter write-check repetitions:

| Workload / metric | Bun | Previous Wasm | Candidate |
| --- | ---: | ---: | ---: |
| Single update requests/s | 1,203 | 881 | **486** |
| Single update p95 / p99 ms | 11.72 / 17.80 | 51.64 / 132.92 | **45.33 / 1,259.11** |
| Atomic pair requests/s | 1,171 | 1,386 | 1,987 |
| Atomic pair p95 / p99 ms | 13.28 / 16.54 | 11.97 / 53.09 | 12.63 / 30.49 |

The single-update result is markedly worse; the favorable pair-update throughput
does not cancel it out. These workloads return small rows, so they do not activate
binary row egress. This observation does not establish a cause or exonerate the
candidate. The earlier server-side write stalls remain unresolved; controlled
storage/host measurements with per-SQL-call timing are required before adoption.

## Current startup evidence

Twenty fresh processes per target, alternating order, initialize private SQLite
storage and perform one verified HTTP read. There is no application preflight or
warmup in this separate startup server. External process-to-response timing excludes
shutdown/cleanup; OS caches remain warm.

| Median | Bun | Candidate |
| --- | ---: | ---: |
| Process to ready | 70.406 ms | 27.377 ms |
| First HTTP request after ready | 3.625 ms | 4.681 ms |
| Process to first verified HTTP response | **73.949 ms** | **32.086 ms** |

Ranges for process to first response were 72.800–105.104 ms and 31.663–33.285 ms.
The Wasm route has faster overall readiness in this local experiment, while its
first request after readiness is slightly slower. This is not Cloudflare cold-start
latency and is not directly comparable with the older setup/cleanup-inclusive
first-operation benchmark. No latest-artifact Cloudflare deployment was performed.

## Validator diagnostic

A separate diagnostic module compares existing validators on already loaded values,
amortising boundary calls in batches of 128. Five rotations per case include valid
and invalid rows, UUIDs, ASCII and Unicode titles. Median times for valid full rows
were about 0.258 µs Bun / 0.164 µs Wasm for small rows, and 0.260 / 0.164 µs for
16 KiB rows. Valid Unicode titles measured 0.175 / 0.023 µs.

These are implementation diagnostics, not request-speed multipliers. Bun constructs
validated records or throws `ValidationError`; the Rust probe returns a boolean.
Invalid-case ratios therefore include different error mechanisms and must not be
presented as equivalent error-reporting costs. Decode, transport and full failure
construction are excluded. The row result supports focusing on data movement:
validation of this unconstrained large text adds little compared with transport.

## Correctness and retained limitations

Thirty Bun tests passed with 11,296 assertions, as did four native Rust tests, the
15 probe and 16 runtime cases, five rejection gates and four adversarial rollback
cases. Coverage includes malformed/truncated/schema-mismatched frames, integer
bounds, Unicode and escaping, exact/oversized JSON budgets, stale handles, concurrent
leases, 10,000 sequential reuses, safe faults and generated renamed entities/fields.
The source constraint mutation changes behavior; a clean rebuild reproduces bytes.

The selected module SHA-256 is
`b488c657e7184ed1c8689504941d5a3db8cd77e555fa800cbdb93405bb3956a9`.
It is 154,462 raw / 60,516 gzip bytes, against 142,030 / 56,640 previously: roughly
8.8% / 6.8% larger. This experimental module retains rejected ingress support;
it is not a minimised production bundle. Runtime and codec source are retained
independently of ignored build outputs.

A pre-existing host issue was reproduced: pinned Bun 1.2.20 SQLite strips an
initial U+FEFF when binding text, even in `SELECT ?`. Direct-host codec tests
preserve it. This limits the storage fidelity claim and is not fixed by this pass.
The previous oversized-host-frame diagnostic fallback, durable-write stalls,
production authentication, broader language coverage and hosted evidence remain
separate issues.

## Next decision

Keep the read result as evidence, without adopting this ABI or changing Bun's
default status. The next bounded candidate should examine specialised internal
values and carrying proven validation/size properties across unchanged values,
instead of repeatedly scanning generic values. Such properties must be invalidated
when a transformation or external boundary makes them uncertain. This is a proposed
compiler optimisation, not permission to remove freshness or authorisation checks.

The original plan is only partly executed: no general transform workload, typed
internal-value lowering, complete backend inventory, controlled-host capacity test
or new hosted capability qualification was completed here. Those remain explicit
follow-ups. A genuine runtime fault-location gap and the write results still block
a broad adoption claim.

[Reproduction](../../experiments/wasm-exp1/row-transport/README.md),
[full HTTP table](../../experiments/wasm-exp1/row-transport/table.md),
[paired results and gate outcomes](../../experiments/wasm-exp1/row-transport/results.json),
and [evidence manifest](../../experiments/wasm-exp1/row-transport/evidence/manifest.json).

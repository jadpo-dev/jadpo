# Wasm value ownership and speed-build experiment — 30 September 2026

Subsequent checkpoint: [schema-bound row transport results](wasm-row-transport-results.md).
The measurements below describe this earlier candidate.

**Result: a repeatable small-read win in this local fixture, not overall backend
superiority. Keep Bun as the working target; continue investigating larger-value
transport and resolve the durable-write stalls on a controlled host.**

This owner-requested extension asks whether further optimisation can beat the
generated Bun target on particular metrics. The same local HTTP wrapper hosts
both execution paths in Bun. It does not compare a standalone Wasm server with
Bun's server, and it does not measure EC2 or Cloudflare.

## What changed

Two low-complexity improvements were tested separately and together before
considering a different wire format:

- Release compilation prioritises speed (`opt-level=3`) rather than code size (`s`).
  LTO, one codegen unit, panic handling, memory limits and disabled Wasm features
  remain unchanged.
- The runtime transfers ownership of completed values rather than copying them
  into an envelope immediately before discarding the original. The same principle
  applies to checked host results, pending-call arguments, validated returned rows,
  and validated entrypoint argument arrays. No general last-use analysis or new
  assumptions about authored variable lifetime are involved.

The source language, JSON ABI, SQL adapter, exact policy-description checks,
validators, prepared statements, instance lease/reset protocol and resource bounds
remain unchanged. This pass creates a new module with its own evidence; prior
Cloudflare passes do not qualify it automatically.

## Isolated results and candidate selection

Five rotated runs, 0.2 seconds warmup and one second measured for each variant and
workload, with generated Bun rerun alongside the candidates. These are application
boundary calls, not HTTP requests. Each returned value was checked.

| Variant | Small operations/s | Small p95 ms | 16 KiB row operations/s | Row p95 ms |
| --- | ---: | ---: | ---: | ---: |
| Generated Bun | 40,219 | 0.028750 | 31,317 | 0.036750 |
| Previous Wasm | 37,658 | 0.033458 | 15,000 | 0.080500 |
| Speed build only | 40,195 | 0.031625 | 15,518 | 0.078209 |
| Ownership transfers only | 41,728 | 0.030917 | 16,325 | 0.074583 |
| Combined candidate | **44,189** | **0.029250** | **16,829** | **0.072667** |

The combined build won small-operation throughput against Bun in all five pairs,
by approximately 9–17%, with a ratio of medians of 1.099. This is a repeatable win
in this isolated metric. Median p95 was approximately 1.7% higher than Bun, and
paired latency ratios straddled equality (0.97–1.03), so it is not a latency win.

For the larger row, the combined build improved throughput by roughly 12% over
the preceding Wasm candidate but reached only about 54% of Bun throughput. Its
p95 remained about twice Bun's. Faster compilation and fewer owned copies help,
but do not remove the cost of representing, encoding and decoding larger values
across the existing generic JSON boundary. The combined candidate was selected
and its hash recorded before starting HTTP measurement.

The combined module grew from 109,452 to 142,030 bytes, or from 50,050 to
56,640 bytes gzipped. That is approximately 30% larger raw and 13% larger
compressed. Cold-start latency was not measured in this pass, so the speed/size
trade-off has not been qualified for edge startup.

## Correctness

The candidate passed 21 tests with 10,586 assertions, two native Rust runtime
tests, the unchanged 15 probe and 16 runtime cases, five rejection gates and
four adversarial rollback cases. Tests include interleaved principals, stale
handles, pending reset refusal, malformed/oversized host results, policy forgery,
10,000 sequential leases, and large text containing Unicode, quotes, backslashes,
newlines, tabs and NUL. All build variants preserve the tested values and reject
incorrect entrypoint arity.

The source minimum-length mutation changed generated behavior. Removing the Cargo
build cache and rebuilding reproduced the selected candidate byte-identically.
Its SHA-256 is
`84cddcfff2fdfebd5eb5ef5013e2e1f8f9ee470cc3e51e6453551421aa85e90a`.
The known oversized-host-result diagnostic fallback remains unchanged: safe rollback
can still be attributed to the outer action rather than its inner operation.

## HTTP methodology

The preceding [HTTP protocol](wasm-http-experiment-results.md) was repeated with
fresh measurements of generated Bun, the previous Wasm module, the selected
candidate and no-work controls. Five rotated repetitions use concurrency one and
sixteen, one second warmup and three seconds measured. A separate Node 24.18.1
process generates requests; each target runs in its own Bun 1.2.20 process on the
same workstation. File-backed SQLite uses DELETE journaling and FULL synchronous
durability for both paths. The same trusted fixture owner, request/response
format, validation and persistence checks apply. No measured request is retried.

Small reads use a 256-byte probe input; larger reads return a full row with a
16 KiB note. Single updates change a 256-character note; pair updates atomically
rename two rows. Every response and post-run snapshot is checked. These are
bounded fixture operations, not a full application workload or production auth.

Server CPU includes runtime, HTTP and storage work. Handler timing starts at
handler entry, excluding earlier socket queueing; end-to-end latency includes the
HTTP path and queueing. The shared workstation is not load/thermal controlled.
Short closed-loop measurements cannot establish fixed-arrival-rate SLOs or
maximum production capacity, especially near the no-work client/HTTP ceiling.

## HTTP result: a specific win, with important losses

The complete comparison ran **140 measurements and 3,693,399 requests with zero
errors**. The isolated comparison ran 50 measurements and 1,494,876 operations.
All responses and final database snapshots were checked. Each of the 40 HTTP
server processes passed eight preflight cases and exited after its target run.

At concurrency sixteen, medians of five repetitions:

| Small-read metric | Generated Bun | New Wasm | Difference |
| --- | ---: | ---: | --- |
| Requests/second | 23,071 | **25,121** | **8.9% higher** |
| End-to-end p95 | 1.2942 ms | **1.1492 ms** | **11.2% lower** |
| Server CPU/request | 43.3 µs | **39.7 µs** | **8.3% lower** |

The candidate beat Bun on all three metrics in every paired repetition.
Throughput gains ranged from 3.6–12.6%; p95 reductions were 7.1–14.9%; CPU
reductions were 3.5–9.4%. That is consistent evidence within this bounded local
workload, without claiming statistical significance or general backend superiority.
At concurrency one, small HTTP reads remained essentially level: 9,586/s for Bun
and 9,555/s for the candidate, p95 0.1560 ms versus 0.1583 ms.

The larger-value path remains slower. At concurrency sixteen, the candidate
handled 10,561 large-row reads/s versus Bun's 16,143/s (about 35% lower), with
p95 2.6208 ms versus 1.8143 ms (about 44% higher), and 94.5 µs CPU/request versus
59.3 µs (about 59% more). The isolated and HTTP evidence both support targeting
the generic JSON/value boundary next. This pass did not remove it.

Durable-write results must not be presented as a general win:

| Concurrent write metric | Generated Bun | New Wasm |
| --- | ---: | ---: |
| Single-update requests/s | 2,233 | 1,186 |
| Single-update p95 / p99 | 12.36 / 19.00 ms | 10.76 / 79.51 ms |
| Atomic pair requests/s | 2,122 | 1,292 |
| Atomic pair p95 / p99 | 12.01 / 15.18 ms | 13.36 / 31.11 ms |

These are medians; the raw pairs show severe throughput/tail variation. Lower
CPU/request does not compensate for observed stalls, and the candidate did not
win sustained concurrent-write throughput in this run.

## Bounded write-stall diagnostic

The unfavorable write result triggered a separate diagnostic: three rotated
repetitions per target and write workload at concurrency sixteen, with the same
one-second warmup and three-second measurement. It adds maximum client latency,
maximum handler/application-call time and ten-millisecond client/server event-loop
probes. It does not replace or discard the earlier unfavorable measurements.

All twelve diagnostic runs returned correct results. Long pauses occurred on both
targets. One Bun pair-update call took about 1.09 seconds; candidate calls reached
about 2.51 seconds. Server event-loop delay tracked those pauses, while client
loop delay stayed below 8 ms. The diagnostic therefore locates a major part of
the stall inside server-side application/storage execution and rules out a
client-only explanation for those events. It does **not** identify which SQL
syscall, disk event or server scheduling condition caused the pause, or exonerate
the candidate's worse write throughput. This remains unresolved evidence that
requires a controlled storage/host comparison before adoption.

The no-work HTTP control reached 30,354 small responses/s and 18,825 large
responses/s at concurrency sixteen. That is only about 1.21× the candidate's
small-read rate and 1.17× Bun's large-read rate. The client/HTTP ceiling limits
maximum-capacity conclusions. Server CPU and isolated measurements provide
additional evidence, not an excuse to extrapolate these rates to hosting bills.

## Recommendation

Wasm can beat this generated Bun target on a useful bounded workload: concurrent
small reads. We now have a specific, repeated throughput/latency/CPU result,
rather than assuming a compiled target must be faster. It is not a blanket reason
to switch backends. Large rows still lose, write stalls are unresolved, and the
runtime/language coverage and external assurance gates remain open.

Keep the combined variant as the performance candidate. The next compiler/runtime
question is a typed or more compact value boundary; the next benchmarking question
is a controlled machine and storage test with a separate load generator. This pass does not introduce a new
ABI, deploy to the cloud, adopt a backend or select the Rust/direct route. The direct-generation route was not optimised here.

## What EC2 would establish

A controlled server test can reduce the uncertainty of workstation measurements;
it cannot establish that Wasm must outrun generated JavaScript. Both execution
paths benefit from the same hardware, and CPU architecture, runtime versions,
SQLite/storage settings, connection handling and workload can change the result.
This is an inference about experimental design, not an EC2 benchmark result.

For a future comparison, use fixed CPU capacity rather than a burstable T-family
instance, pin the architecture/toolchain, and run the load generator on a separate
host. AWS documents that burstable instances vary CPU availability through a
baseline and CPU-credit mechanism; those credits are an avoidable confounder for
a sustained comparison. See [AWS CPU-credit documentation](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/burstable-credits-baseline-concepts.html)
and [EC2 compute instance specifications](https://docs.aws.amazon.com/ec2/latest/instancetypes/co.html).
No paid infrastructure was provisioned by this extension.

[Reproduction](../../experiments/wasm-exp1/value-path/README.md),
[complete HTTP table](../../experiments/wasm-exp1/value-path/table.md),
[paired summaries](../../experiments/wasm-exp1/value-path/results.json),
[artifact hashes](../../experiments/wasm-exp1/value-path/artifacts.json),
[raw evidence](../../experiments/wasm-exp1/value-path/evidence/manifest.json), and
[effort accounting](../../experiments/wasm-exp1/value-path/effort.json)
are retained in the repository. No previous source or measured artifact was
changed to obtain these results.

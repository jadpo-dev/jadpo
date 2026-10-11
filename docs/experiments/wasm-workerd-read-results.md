# Local workerd read-path comparison — 30 September 2026

The selected Jadpo WASM module runs in **workerd/V8 with SQLite Durable Objects,
without a Bun application runtime**. The local checks include fresh ownership
changes, invalid input, missing rows and authored recovery. This is an experiment
on Cloudflare's runtime implementation, not a hosted Cloudflare deployment or a
complete production backend. **The large-row gap persists without Bun**: the
selected WASM is about 10% behind generated JavaScript in concurrent throughput
and about 7% higher in p95 by paired medians. This does not support an all-metrics
WASM win.

## What this comparison means

The previous [Bun-hosted measurements](wasm-read-path-results.md) use Bun as a
convenient controlled host for generated JavaScript and WASM. They do not make
Bun a dependency of the intended WASM target. They also cannot establish the
performance of a different engine or storage binding.

Here both arms use the same workerd HTTP entry point, Worker-to-Durable-Object RPC,
SQLite authority adapter, fresh owner-scoped SQL and response serialization. The
JavaScript arm keeps the compiler-produced functions, validators and failure
handling, with an explicit read-only persistence test port and its unused Bun CLI
removed during bundling. It is generated JavaScript on workerd, not native Bun.
The WASM arm keeps the frozen module, generated validators, compact plans and
request-local row receipts from the third read experiment.

Cloudflare supports precompiled WASM modules and host bindings; JavaScript glue
around those bindings does not require Bun. See the official
[WASM JavaScript API documentation](https://developers.cloudflare.com/workers/runtime-apis/webassembly/javascript/).
Database results still have to reach guest memory for guest validation. A different
engine can change this cost; it does not remove the boundary by definition.

Node runs the local tooling and external HTTP generator. The bundle has no Bun or
Node imports/accesses and the runtime check finds no Bun global. Workerd does
expose a `process` compatibility global; that is not evidence of a Node runtime.
Binding types were generated with Wrangler. The bundle was built with esbuild;
this is not a claim of a complete TypeScript typecheck.

## Protocol and limits

Five rotated repetitions at concurrency 1 and 16, small probe and full 16 KiB row,
0.5 seconds warmup and two seconds measured per cell. Five targets produce 100
cells: generated JS, previous WASM, selected WASM, typed-ingress WASM, and a no-SQL
control. Each response, final database snapshot and server request count is
verified. Four application targets each pass seven behavioural preflight checks.
The fixture supplies a trusted principal; full authentication is outside this pass.

The selected module is
`cf3716529f29fd0ababa6e4d99982908ba7f9326035b486f9c03d1ffe2d99c0b`.
Mode 14 uses JSON ingress; mode 15 adds direct typed ingress. Both use separate
module identities. The previous artifact is
`631ea8e20a3cc55936f857acefe3fe68ada60b83a7b737fc651a51fae40b8b91`.
The candidate was selected before this host comparison; a result for the typed
alternative is additional evidence, not a retroactive change to that selection.

This shorter exploratory protocol does not replace the longer Bun parity gate.
The campaigns have different HTTP/RPC/storage plumbing, so their absolute rates
must not be used to rank Bun against Cloudflare. Local scheduling and generator
overhead affect end-to-end timings. CPU/request, total memory, production capacity,
real network latency, cloud cold starts and production sharding are unmeasured.
Pool counters are cumulative/shared by the two candidate modes in each isolate,
not independent per-target memory measurements.

Tooling is pinned to workerd 1.20260926.1, Miniflare 5.20260926.1-alpha,
Wrangler 4.144.0, esbuild 0.28.1 and Node 24.18.1; compatibility date 2026-09-30.
No cloud resource was provisioned. Early setup probes found that `sqlite_version()`
is unavailable and that workerd exposes the compatibility `process` global. Those
diagnostic assumptions were corrected before qualification; no application check
was removed. The final smoke run passed all ten cells.

## Results

All **100 cells and 585,339 measured requests** passed, with no response errors,
request-count mismatches or snapshot failures. All 28 behavioural preflight checks
passed. No pool discards or active requests remained at sampled cell ends; the
candidate pool retained at most six WASM memory pages (384 KiB per instance).
That is not total runtime memory.

Five-run medians for the main comparison:

| Concurrency | Workload | Generated JS requests/s | Selected WASM requests/s | JS p95 ms | WASM p95 ms |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | Small | 2,732 | 2,718 | 0.4226 | 0.4511 |
| 1 | 16 KiB | 2,465 | 2,039 | 0.4922 | 0.5520 |
| 16 | Small | 3,673 | 3,284 | 8.5951 | 8.7946 |
| 16 | 16 KiB | 3,309 | 3,019 | 9.1714 | 9.9943 |

Paired medians differ from ratios of the above aggregate medians. For concurrent
large reads, selected WASM/JS is **0.8987 throughput and 1.0734 p95**; WASM loses
both metrics in all five pairs. At concurrency 1 the paired ratios are 0.8294
and 1.0850. Small-read paired ratios are 0.9709/1.0148 at concurrency 1 and
0.9118/1.0018 at concurrency 16. This is mixed, noisy exploratory evidence, not
proof that every workload is intrinsically slower in WASM.

Previous WASM reaches 2,843 requests/s and 10.4356 ms p95 for concurrent large
reads; the selected mode improves both aggregate medians to 3,019 and 9.9943.
Typed ingress reaches 2,731 and 10.1738 in that cell. It improves single-request
large-read throughput over the selected mode but loses under concurrency, so it
is not a demonstrated general replacement. Every alternative and range is in the
[complete table](../../experiments/wasm-exp1/workerd-read-path/table.md) and
[paired results](../../experiments/wasm-exp1/workerd-read-path/results.json).

No-op headroom is only about 1.07–1.32x across paired medians. Tail latencies are
variable, and the shared laptop, external generator, local RPC and workerd
scheduling affect these results. The experiment is **capacity-inconclusive**.
It nevertheless directly contradicts the assumption that removing Bun necessarily
removes the observed large-row difference. Next optimisation should isolate
workerd's actual boundary costs before changing the runtime design again.

## Reproducibility

The [harness and commands](../../experiments/wasm-exp1/workerd-read-path/README.md)
describe the adapted JavaScript test port and host boundaries. Full results,
paired ratios, all ranges and no-work headroom are retained beside the harness.
Compressed evidence includes both campaigns, the frozen worker bundle and WASM,
generated JavaScript, binding types, source snapshots and SHA-256 manifest.

The next useful evidence is an isolated workerd component profile of SQL result
conversion, host-to-guest validation and return encoding, followed by a longer
controlled-host comparison. A hosted Cloudflare pass remains necessary for actual
network behaviour and cold starts. General backend coverage remains a separate
decision gate; successful read fixtures do not complete that inventory.

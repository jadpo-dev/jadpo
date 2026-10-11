# Framework-informed workerd boundary experiment

Follow-up to `workerd-read-path`, informed by primary-source research into
workers-rs, wasm-bindgen, native Rust APIs, Go JS/WASM and Wasmtime. See the
[research](../../../docs/experiments/wasm-framework-boundaries.md) and
[measured results](../../../docs/experiments/wasm-workerd-boundary-results.md).

The guest remains the exact frozen read-path module
`cf3716529f29fd0ababa6e4d99982908ba7f9326035b486f9c03d1ffe2d99c0b`.
No compiler, language syntax, SQL policy or public HTTP representation changes.
Bun is used only for host-driver unit tests, never the workerd application runtime.

## Variants

- `candidate`: frozen previous driver, JSON ingress, compact plans, row receipts.
- `typed`: frozen driver with binary ingress enabled.
- `views`: selected new driver, JSON ingress. Encode directly into guest memory;
  decode directly from a short-lived guest-memory view.
- `fused`: new driver and binary encoder. Derive the exact JSON bound while
  encoding scalars instead of serializing the whole row into JSON/UTF-8 first.
- `adaptive`: use the new binary encoder when a row contains JSON escapes,
  otherwise the new JSON writer. This experiment lost on ASCII and is not selected.
- `js`: compiler-generated JavaScript with the same authority adapter.
- `noop`: matching response shape without SQL/application execution.

The selected path returns owned JS values synchronously. No borrowed byte view
survives an await, allocation, guest call or memory growth. The input allocation
uses a bounded UTF-8 capacity estimate and passes the actual written byte count;
incomplete encoding fails closed. This can retain more memory than the old exact-
length allocation. Both drivers retain the 64 KiB frame and 8 MiB memory limits.

The binary alternative preserves the exact original JSON bound, including Unicode
keys, missing/null fields, booleans, safe integers, all ASCII escapes and UTF-8.
The guest independently checks it. Only eligible owned scalar snapshots receive
row receipts; transformed values keep the ordinary output path. Fresh SQL and
owner checks are unchanged. A fixed trusted principal is a fixture, not full auth.

## Measurement sequence

1. Instrument 24 existing-driver workerd cases: three nominal sizes, two text
   kinds, two return shapes, JSON and binary ingress. Each measures 2,000 verified
   calls after 200 warmups. Stage means are diagnostic, coarse and non-additive.
2. First isolated selection: 120 cells, six targets, four workloads, five rotated
   repetitions. Preserve source/bundle snapshots and every loss.
3. Add the adaptive alternative; final selection: 140 cells. Each cell warms 200
   calls and measures 2,000 in one Object batch. External elapsed time includes
   amortized RPC/verification; this is not HTTP request capacity. Internal local
   elapsed time is retained as a cross-check. Choose `views` before HTTP timing.
4. HTTP smoke: 12 cells, three application targets with seven preflights each,
   plus no-op. Full HTTP: 120 cells, five rotated repetitions, concurrency 1/16,
   0.5 seconds warmup and two seconds measurement. Workloads are a small probe,
   16 KiB ASCII full row, and 512 repeats of escaped Unicode text (7,828-byte
   success envelope). Every response, final snapshot and request count is checked.

The 48 KiB case is isolated selection only. HTTP and isolated values are different
metrics. No component mean is added to predict HTTP p95. Shared local generator,
RPC and runtime scheduling limit capacity conclusions; no-work headroom is retained.
Neither CPU/request, total process RSS, production sharding nor hosted cold starts
is measured. The 48 host tests reuse the established adversarial suites and add
encoder differential/exact-bound tests. The unchanged guest is not rebuilt.

## Reproduce

Use the existing tooling and generated baseline prerequisites from
[`workerd-read-path`](../workerd-read-path/README.md). Same Node/workerd/Miniflare/
Wrangler/esbuild versions and compatibility date. No cloud deployment occurs.
Run builds, tests and benchmarks sequentially:

```sh
bun --no-install --env-file=/dev/null test experiments/wasm-exp1/workerd-boundary/*.test.ts experiments/wasm-exp1/boundary-http/cached-adapter.test.ts experiments/wasm-exp1/boundary-http/descriptor.test.ts
node experiments/wasm-exp1/workerd-boundary/build.mjs
node experiments/wasm-exp1/workerd-boundary/runner.mjs --profile
caffeinate -i node experiments/wasm-exp1/workerd-boundary/runner.mjs --micro
node experiments/wasm-exp1/workerd-boundary/runner.mjs --smoke
caffeinate -i node experiments/wasm-exp1/workerd-boundary/runner.mjs
python3 experiments/wasm-exp1/workerd-boundary/summarize.py
```

`caffeinate` prevents idle sleep on macOS; omit elsewhere. It does not prevent lid
sleep. Precompiled WASM imports have distinct identities per mode. Local test
endpoints require a fresh random token and bounded bodies. Miniflare disposes in
`finally`. Never deploy these fixture endpoints as a production API.

`evidence/manifest.json` identifies compressed raw measurements, frozen bundles,
source snapshots, selection and checks. Initial setup mistakes (fixture property
order and a missing test-helper import) were corrected before selection; no
application expectation was weakened. Failed alternatives remain in evidence.

# Native-runtime-inspired value transport

Local workerd experiment continuing `workerd-boundary`. See
[research](../../../docs/experiments/wasm-native-runtime-values.md) and
[results](../../../docs/experiments/wasm-native-values-results.md).

The guest remains the frozen `read-path` module, SHA-256
`cf3716529f29fd0ababa6e4d99982908ba7f9326035b486f9c03d1ffe2d99c0b`.
No compiler, schema, guest validator, SQL authority adapter or public response
contract changes. Bun runs unit tests only; workerd runs the application.

## Variants and selection

- `views`: previous selected JSON ingress driver, imported unchanged.
- `fused`: previous binary encoder, including exact host JSON budget calculation.
- `bounded`: typed encoder with a conservative budget proof and exact fallback.
- `direct`: the bounded encoder writes straight into guest memory.
- `adaptive`: direct writes selected by a full-string escape scan.
- `scan`: scratch-buffer typed writes selected by a full-string escape scan.
- `hybrid`: selected candidate, scratch-buffer typed writes when any scalar
  string's first 256 UTF-16 code units contain an escape or non-ASCII character.
  Otherwise use existing JSON ingress. Skip content selection for small rows.
- `js`: generated JS on the same workerd/DO/SQLite stack.
- `noop`: matching response shape, without SQL/application execution.

Selection is a performance heuristic, not a validation shortcut. Later escapes
still pass through the complete JSON parser and validators. Conservative host
bounds can accept only when the worst case fits; ambiguous bounds use exact
escape lengths. Guest bounds remain exact and independent. Invalid UTF-16 cannot
be silently replaced in typed encoding. Request-local checked-row receipts and
fresh policy checks retain their original contracts.

The experimental direct writer holds a borrowed memory view only synchronously.
It allocates before obtaining that view and performs no further allocation,
await or guest call until resume. Unsupported or over-capacity binary encodings
fall back to the original bounded JSON path. It is not the selected candidate.

## Measurement sequence

1. First selection: 140 cells, seven variants, four workloads, five rotations.
2. Second selection: 270 cells, nine variants, six workloads, five rotations.
   Adds bounded prefix sampling plus full-scan control, late escapes and Unicode.
3. Final selection: 120 cells, selected candidate/previous JSON/generated JS/no-op,
   six workloads, five rotations. Also includes non-ASCII in the sample and skips
   selection on small rows. Source/bundle and earlier alternatives are retained.
4. Freeze source and bundle hashes before independent HTTP smoke and qualification.
   HTTP uses five workloads, four targets, five rotations, concurrency 1/16,
   0.5-second warmup and two-second measurement: 200 cells. Smoke uses 20 cells.

Every isolated cell warms 200 calls and verifies 2,000 measured calls in one DO
batch. External elapsed includes amortized RPC and verification; it is not HTTP
latency. All HTTP responses, counts and final snapshots are checked. Seven
behavioural preflights per application target check fresh ownership and failures.
The fixed principal is a fixture, not full authentication.

Workloads: small probe; 16 KiB ASCII; escaped Unicode (512 repeats of
`é中😀` plus quote/backslash/newline); Unicode only (2,048 repeats of `é中😀`);
escapes after an 8 KiB ASCII prefix. The 48 KiB ASCII case is isolated only.
Workload frame sizes are retained in the raw isolated results. They are not all
equal-size payloads, so compare targets within a workload.

Use the pinned tooling and generated prerequisites from `workerd-read-path`.
Same workerd/Miniflare/Node versions and compatibility date as the preceding pass.
Run tests/builds/measurements sequentially; never change a selected artifact
mid-run. `selection.json` fixes the final artifacts before HTTP measurement.

```sh
bun --no-install --env-file=/dev/null test experiments/wasm-exp1/workerd-native-values/*.test.ts experiments/wasm-exp1/boundary-http/cached-adapter.test.ts experiments/wasm-exp1/boundary-http/descriptor.test.ts
node experiments/wasm-exp1/workerd-native-values/build.mjs
caffeinate -i node experiments/wasm-exp1/workerd-native-values/runner.mjs --micro
node experiments/wasm-exp1/workerd-native-values/runner.mjs --smoke
caffeinate -i node experiments/wasm-exp1/workerd-native-values/runner.mjs
python3 experiments/wasm-exp1/workerd-native-values/summarize.py
```

The final runner reproduces final selection; archived runners reproduce earlier
rounds. In the first two raw selection reports, `protocol.workloads` inherits the
HTTP list; the actual `results[].workload` cells and archived loops define their
four/six-workload matrices. The final runner corrects that metadata. This does not
change those measurements. See `evidence/manifest.json` for raw source/bundle/data
hashes. The inherited adversarial tests use `selected-test-driver.ts` to exercise
the selected sampling mode, with separate tests for unselected direct writes.

No Cloudflare deployment, CPU/request, total process memory, hosted cold start or
production capacity is measured. Generator/DO/RPC headroom must be considered.
Endpoints bind locally and require a fresh random token; do not deploy them.

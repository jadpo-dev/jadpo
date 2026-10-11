# Guest string scans and bulk-memory experiment

Follow-up to `workerd-native-values`. Local workerd only; see the
[results and rationale](../../../docs/experiments/wasm-guest-scans-results.md).

## Changes and contract

The compiler experiment preserves the checked application projection, source
validators, storage authority adapter, row schema/digest, receipt ownership and
public JSON responses. Two host/guest ABI exports make the exact logical JSON
budget the guest's responsibility: `row_budget_version() == 1` opts in, and
`row_budget_exceeded()` lets the driver preserve the original generic host size
fault. The new driver rejects modules without the capability. Host frames remain
bounded to 65,536 bytes; the guest independently computes the original exact JSON
success-envelope bound and rejects excess before validation/receipts. No row or
permission cache is introduced. Budget state is cleared on reset and typed resume.

The optional `vector` Cargo feature uses pinned `simdutf8 = 0.1.5` for typed-ingress
UTF-8 validation. A 16-byte SIMD escape counter replaces the previous 8-byte SWAR
counter for exact JSON size. All loads stay inside `chunks_exact(16)`; the suffix
uses scalar counting. Native tests retain a scalar implementation; WASM differential
tests exercise the real vector instructions. The selected build also enables
bulk-memory so the compiler can emit `memory.copy` and `memory.fill`. This does
not relax ownership, initialization, memory limits, or any validation rule.

## Variants

- `js`: generated JavaScript, same workerd/DO/SQLite authority adapter.
- `hybrid`: frozen prior sampled host driver and prior scalar guest.
- `views`: earlier JSON-only driver, first isolated round only.
- `scalar`: new guest budget contract, existing scalar UTF-8/SWAR checks, typed rows.
- `vector`: SIMD UTF-8/exact escape counting, typed rows; bulk memory disabled.
- `scalarSample`, `vectorSample`: retain the previous bounded content heuristic.
- `bulk`: **selected**, SIMD plus bulk memory, all eligible large rows use typed ingress.
- `bulkJson`: same module using JSON ingress, a control for the transport decision.
- `noop`: matching response shape without SQL or application work.

The selected path needs no content sampling. Unsupported row shapes and oversized
binary representations retain the existing bounded JSON fallback. The experimental
compiler is isolated here; no production/default target change occurs.

## Protocol

1. Profile 16 cases on the previous module: ASCII, escaped Unicode, Unicode only,
   late escapes; full-row and title extraction; JSON/binary ingress. Each checks
   2,000 measured calls after 200 warmups. Local stage clocks are coarse and change
   costs; means are diagnostic, not additive HTTP latency predictions.
2. First isolated selection: eight targets, six workloads, five rotations, 240 cells.
3. Second isolated selection: six targets, six workloads, five rotations, 180 cells.
   Every cell warms 200 calls and verifies 2,000 measured calls inside one Object.
   External elapsed includes amortized RPC/verification; it is not HTTP capacity.
4. Freeze the chosen module, source, worker bundle and rationale before independent
   HTTP. Same five workloads as the prior pass, four targets, concurrency 1/16,
   five rotations, 0.5-second warmup and two-second measurement: 200 cells. Smoke
   is 20 cells. Seven preflights per application target check ownership and errors.

Five HTTP workloads: small probe, 16 KiB ASCII, escaped Unicode, Unicode only,
late escapes after an 8 KiB ASCII prefix. Isolated selection also includes 48 KiB
ASCII. Fixture definitions and actual frame sizes are retained in raw results.
Every HTTP response, final database snapshot and request count is checked.
No-work headroom, all losses, tails and raw ranges are retained. Fixed fixture
principals are not a complete authentication benchmark.

## Reproduce

Use the previously pinned tooling/generated prerequisites from `workerd-read-path`.
Rust 1.78.0; the lockfile adds simdutf8 0.1.5. Fetch the lockfile dependencies once
if not cached; subsequent builds below are offline. Keep builds, tests and
benchmarks sequential, and never change the chosen artifacts during HTTP runs.

```sh
sh experiments/wasm-exp1/workerd-guest-scans/build-guests.sh
python3 experiments/wasm-exp1/workerd-guest-scans/mutation-rebuild.py
cargo test --offline --locked --features vector --manifest-path experiments/wasm-exp1/workerd-guest-scans/compiler/Cargo.toml --target-dir experiments/wasm-exp1/workerd-guest-scans/compiler/build/native-tests -- --test-threads=1
bun --no-install --env-file=/dev/null test experiments/wasm-exp1/workerd-guest-scans/*.test.ts experiments/wasm-exp1/boundary-http/cached-adapter.test.ts experiments/wasm-exp1/boundary-http/descriptor.test.ts
node experiments/wasm-exp1/workerd-guest-scans/build.mjs
node experiments/wasm-exp1/workerd-guest-scans/runner.mjs --profile
caffeinate -i node experiments/wasm-exp1/workerd-guest-scans/runner.mjs --micro
node experiments/wasm-exp1/workerd-guest-scans/runner.mjs --smoke
caffeinate -i node experiments/wasm-exp1/workerd-guest-scans/runner.mjs
python3 experiments/wasm-exp1/workerd-guest-scans/summarize.py
```

The final runner reproduces second selection. Earlier profile/selection sources
and bundles are archived. The profile uses the previous encoder and guest; it is
not presented as a post-change component profile. `mutation-rebuild.py` rebuilds
renamed/refined checked sources, restores the original projection, removes the
selected Cargo build cache and verifies a byte-identical clean rebuild.

Tests cover old/scalar/vector/bulk guest agreement on mixed Unicode, all ASCII
controls, exact limits and invalid UTF-8 around SIMD/block boundaries. Existing
adversarial suites run against the selected bulk guest. Native Rust tests run
serially because this prototype runtime has mutable global state.

Initial failures are retained: delegated size rejection initially changed the
internal-error envelope and was fixed in code; the mutation harness initially
compared to a stale pre-edit module and now builds its baseline first; CamelCase
variant names initially violated lowercase fixture keys and were corrected in the
runner. No assertion was weakened. A post-selection comment changes esbuild input
byte-count metadata only; the actual worker and all module hashes remain identical.

No deployment, hosted cold-start, CPU/request or production-capacity claim follows.
Local fixture endpoints require a random token and must not be deployed.

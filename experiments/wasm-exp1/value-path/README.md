# Value ownership and speed-oriented build experiment

This owner-requested extension tests whether the generated Wasm target can beat
the generated Bun target on specific workloads. Both HTTP application paths run
inside the same Bun server implementation. This is not a comparison of Bun's HTTP
server against a standalone Wasm runtime, and no EC2 instance was provisioned.

## Changes and invariants

The baseline is the previous JSON-boundary candidate. The new runtime transfers
owned values into terminal and pending envelopes, removes the checked host result
from its completed envelope, and consumes validated multi-argument arrays instead
of cloning every argument. Generated updates transfer an already validated row
out of the terminal storage result. Shape/type checks precede extraction. The
`unwrap` calls are guarded by exact arity/key checks; no authored variable is
assumed dead based on an unimplemented liveness analysis.

The compiler also offers a speed-oriented Rust release build (`opt-level=3`) in
place of the earlier size-oriented build (`s`). LTO, one codegen unit, aborting
panic behavior, memory bounds and disabled Wasm features remain unchanged.
`variants.py` records isolated ownership-only, speed-only and combined artifacts,
including source hashes, flags, build times and sizes. The selected build is
combined; the default Cargo profile reproduces that candidate. The selection was
recorded after isolated timing and before HTTP timing.

The JSON wire protocol, prepared SQL cache, exact host descriptor checker,
authorisation, validators, instance leasing and fault handling are unchanged.
No request/row cache, binary ABI or new language syntax was introduced. The
frozen source and earlier results remain unchanged.

## Reproduce

Use the original pinned toolchain and existing projection/Bun baseline. Run from
repository root. Run benchmark programs sequentially without builds/tests:

```sh
sh experiments/wasm-exp1/value-path/build.sh
python3 experiments/wasm-exp1/value-path/variants.py
bun --no-install --env-file=/dev/null experiments/wasm-exp1/value-path/micro.ts
python3 experiments/wasm-exp1/value-path/mutation-rebuild.py
python3 experiments/wasm-exp1/value-path/negatives.py
bun --no-install --env-file=/dev/null test experiments/wasm-exp1/value-path/reuse.test.ts experiments/wasm-exp1/value-path/values.test.ts experiments/wasm-exp1/boundary-http/cached-adapter.test.ts experiments/wasm-exp1/boundary-http/descriptor.test.ts
cargo test --offline --locked --manifest-path experiments/wasm-exp1/value-path/compiler/Cargo.toml --target-dir build/wasm-exp1/value-path/native-test -- --test-threads=1
bun --no-install --env-file=/dev/null experiments/wasm-exp1/value-path/run-cached.ts experiments/wasm-exp1/value-path/compiler/build/application.wasm --mutated=experiments/wasm-exp1/value-path/compiler/build/mutated.wasm
bun --no-install --env-file=/dev/null experiments/wasm-exp1/value-path/atomic.ts
node experiments/wasm-exp1/value-path/http.ts --smoke
node experiments/wasm-exp1/value-path/http.ts
node experiments/wasm-exp1/value-path/diagnostic-http.ts
python3 experiments/wasm-exp1/value-path/summarize.py
```

The HTTP protocol is the [previous protocol](../boundary-http/README.md): 5 rotated
repetitions, 1 and 16 outstanding requests, 1 second warmup plus 3 seconds measured,
separate Node client/Bun server, identical file-backed SQLite durability settings,
response verification and final-state checks. Only the module selection changes.
A matched no-work control and CPU measurements expose client limits. Measurements
are local closed-loop samples, not maximum hosted capacity or arrival-rate SLOs.
`previous` now means the completed JSON-boundary candidate, not its predecessor.
The smoke suite uses two short repetitions at each concurrency.

No new Cloudflare qualification is implied for this changed module. Ownership
and compiler-option changes preserve the tested interface but require their own
hosted evidence before adoption. The known oversized-host-result diagnostic
fallback and production resource/cancellation gaps remain.

## Write-tail follow-up

The main run showed unfavorable concurrent-write throughput/tails. A separate
bounded diagnostic adds client/server loop-delay and maximum application-call
measurements; it does not replace those results. Reproduce after the main run:

```sh
node experiments/wasm-exp1/value-path/diagnostic-http.ts
python3 experiments/wasm-exp1/value-path/summarize.py
```

The diagnostic keeps the same storage and application code, checks every response,
and runs three rotations of both write workloads at concurrency sixteen. Long
server-side application/storage stalls occurred on both backends; their exact
cause remains unresolved. Do not infer a general write-performance win.

See the [results report](../../../docs/experiments/wasm-value-path-results.md),
[HTTP table](table.md), [paired summaries](results.json) and
[raw evidence manifest](evidence/manifest.json).

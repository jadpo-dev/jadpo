# Instance reuse and host-plan optimisation

This is the owner-requested follow-up to the completed WASM-EXP1 experiment.
The [bounded plan](plan.md) preceded attribution and implementation. The original
fixture, compiler routes, host, artifacts and measurement reports remain unchanged.
See [the results](../../../docs/experiments/wasm-optimization-results.md).

## What changed

The compiler/generator is a snapshot of the tested full Rust generator. Its
compiler-owned runtime adds optional `reset() -> i32` and increments request IDs
between leases. A pending frame cannot reset. Terminal reset drops request values,
owned allocations and output length, resets operation counters and starts a new
request identity. IDs never wrap: exhausted instances are discarded.

This is an explicit lifetime/ABI extension to the original fresh-instance
protocol, **not** a retrospective Rust/direct-route comparison. Direct generation
has not been optimised or retested with this extension; its earlier result remains
valid. Fresh-instance callers can still use the extended module's original ABI.

The changed host gives every request an exclusive lease, including across awaits.
It keeps at most eight idle instances per module, each at most 32 pages (2MiB), and
rechecks reset before reuse. Internal faults, traps, malformed protocol, excess
memory and reset refusal discard the instance. Imported mutable state is not pooled.
Active requests remain governed by the host; the idle cap is not an active-request
admission policy. Reset is a logical ownership reset, not cryptographic erasure of
all linear memory. Arbitrary guest memory access and general cancellation are not
implemented by this bounded language/runtime.

The second optimisation caches prepared statements (up to 128 per SQLite adapter) and
immutable checked policy/node metadata. Incoming operation IDs, full descriptors,
principal/entity identities, fields, values, freshness and policy are still checked.
Cloudflare uses the original SQL adapter; only reuse/immutable-plan caching is
shared with that deployment. No app control flow is interpreted in JavaScript.

## Reproduce

Use the original pinned tool installation and checked projection. From repo root:

```sh
sh experiments/wasm-exp1/optimization/build.sh
python3 experiments/wasm-exp1/optimization/mutation-rebuild.py
bun --no-install --env-file=/dev/null test experiments/wasm-exp1/optimization/reuse.test.ts experiments/wasm-exp1/optimization/cached-adapter.test.ts
bun --no-install --env-file=/dev/null experiments/wasm-exp1/optimization/run-local.ts experiments/wasm-exp1/optimization/compiler/build/application.wasm --mutated=experiments/wasm-exp1/optimization/compiler/build/mutated.wasm
bun --no-install --env-file=/dev/null experiments/wasm-exp1/optimization/run-cached.ts experiments/wasm-exp1/optimization/compiler/build/application.wasm --mutated=experiments/wasm-exp1/optimization/compiler/build/mutated.wasm
bun --no-install --env-file=/dev/null experiments/wasm-exp1/optimization/atomic.ts
bun --no-install --env-file=/dev/null experiments/wasm-exp1/optimization/attribute.ts
bun --no-install --env-file=/dev/null experiments/wasm-exp1/optimization/compare.ts experiments/wasm-exp1/optimization/compiler/build/application.wasm
python3 experiments/wasm-exp1/optimization/summarize.py
```

Run timing programs sequentially. `compare.ts` rotates eight variants across five
runs at concurrency 1/8, using 1 s warmup+3 s measurement. All warmup and timed outputs
are checked; SQL table matching must be unique. This shorter protocol does not
rewrite the original 40-run evidence. Concurrency means outstanding operations in
one event loop; the SQL driver is synchronous. Throughput is not HTTP capacity.
`fresh_reset` isolates pooling from the changed module, using original fresh-instance
glue with the same extended module. Mock paths retain the same synchronous core
boundary and omit the database; they are not the original asynchronous CPU workload.

`build-cloud.py` only creates the deployable fixed synthetic test package. The
actual deployment and cleanup are retained in `cloud.json` and `cleanup.json`;
reproduction does not silently recreate external resources. The summarizer collects
existing cloud evidence if present. Unapproved imports, reachable-if, handled nested
mutation, cross-store and unsupported freshness rejection results are in the retained
negative evidence. Source mutation and clean rebuild run through the original checks.

`comparison.json` retains all 80 raw run aggregates. Raw acceptance, test, attribution,
negative, mutation and cloud-log reports are gzip files under `evidence/`, with their
uncompressed hashes in `evidence/manifest.json`. `artifacts.json` pins the measured
sources and modules. Timing samples are descriptive on an uncontrolled workstation;
allocation/GC/order effects are large for fresh instances. No statistical-significance,
production-capacity or pure native-call-overhead claim follows.

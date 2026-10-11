# Large-read boundary experiment

Bounded follow-up to `typed-values`, preserving its module and driver as the
previous target (`631ea8e20a3cc55936f857acefe3fe68ada60b83a7b737fc651a51fae40b8b91`).
Bun remains the working language target. All measurements here are local and both
application targets use Bun hosting. No Cloudflare/EC2 resources are provisioned.

## Designs and selected mode

The runtime exposes experimental feature bits: binary ingress (1), binary egress
(2), request-local row receipt (4), compact checked query (8). The selected mode is
14: JSON ingress, binary fallback, row receipts, compact queries. The isolated
five-rotation comparison chose it before HTTP qualification. Direct typed binary
ingress remains a measured alternative, not the selected path.

A generated query validates every returned row inside WASM. Eligible flat rows
contain owned immutable scalar fields and a private request/operation origin.
Returning that exact row with the declared result schema can emit a 48-byte `JRR1`
receipt (projection digest, schema, request, operation). The driver retains a
private scalar snapshot only for the invocation that dispatched the fresh read;
it resolves the receipt against that map and returns a new ordinary object.
There is no result or permission cache across requests and no raw HTTP forwarding.
Field extraction/materialisation loses the row origin and uses ordinary transport.
Every guest read still validates UUIDs, types, presence, refinements and bounds.

The host snapshot does not need `Object.freeze`: it contains only immutable
primitives, is never exposed, and is never changed while the guest runs. Freezing
it made JSON serialization slower in the initial design. Mutation of an external
dispatch row or a returned object cannot change it. Negative zero is canonicalised
like the JSON/integer transport. Only explicit `resume_ref`/`resume_row_ref` calls
can attach an origin; ordinary binary/JSON resume cannot accidentally claim one.

A `JRP1` pending message contains the projection digest, request/operation handles,
checked semantic plan ID and JSON predicate. It replaces a 694-byte descriptor
with 86 bytes for the fixture's UUID query. The host restores a generated plan
and passes the original full descriptor to the existing authority/policy adapter.
Plans are private cloned/frozen metadata, never guest-supplied policy overrides.
The pending JSON envelope limit is independently preserved in guest and host,
including escaped UTF-8 and the decimal lengths of handles. Fresh SQL, owner
restrictions and full descriptor verification remain unchanged.

Binary ingress directly builds schema-ordered owned fields and runs generated
validators from the same effective types. Both host and guest enforce the original
65,536-byte JSON envelope limit, even if binary fits more. The initial JS regular
expression byte-counting code was much slower than native JSON serialization;
its source and measurements are retained as negative evidence.

## Qualification

Use the unchanged LR-3 gate in `docs/wasm-large-row-plan.md`: five paired rotated
HTTP repetitions, concurrency 1/16, 2 seconds warmup and 10 seconds measurement,
small-probe and full 16 KiB reads, fresh Bun/previous-WASM/no-work baselines, full
response and snapshot checks. Practical large-read parity requires at least four
of five pairs and paired medians within 10% of Bun's throughput, p95 and CPU.
Small-read regressions versus previous WASM must remain within 5%. A no-work
ceiling below 2x measured throughput prevents a maximum-capacity claim.

The supplemental matrix includes full-row and title-only reads, nominal 256 B,
4/16/48 KiB ASCII and escaped-Unicode cases. Escaped cases are smaller to fit the
same original JSON bound; actual frame bytes are recorded. A write smoke detects
obvious regressions without revisiting journal-mode optimisation. Local fresh-
process startup is measured separately; none of this establishes hosted cold starts.
The earlier oversized-host-frame fault attribution fallback and pinned Bun SQLite
leading-BOM issue remain; production authentication and full language coverage are
still separate gates.

## Reproduce

Pinned environment: Bun 1.2.20, Node 24.18.1, Rust/Cargo 1.78.0. Existing checked
projection, generated Bun baseline and frozen previous module are prerequisites.
Run builds/tests/benchmarks sequentially. HTTP requires loopback permission.

```sh
sh experiments/wasm-exp1/read-path/build.sh
python3 experiments/wasm-exp1/read-path/mutation-rebuild.py
python3 experiments/wasm-exp1/read-path/negatives.py
bun --no-install --env-file=/dev/null test experiments/wasm-exp1/read-path/*.test.ts experiments/wasm-exp1/boundary-http/cached-adapter.test.ts experiments/wasm-exp1/boundary-http/descriptor.test.ts
cargo test --offline --locked --manifest-path experiments/wasm-exp1/read-path/compiler/Cargo.toml --target-dir build/wasm-exp1/read-path/native-test -- --test-threads=1
bun --no-install --env-file=/dev/null experiments/wasm-exp1/read-path/run-cached.ts experiments/wasm-exp1/read-path/compiler/build/application.wasm --mutated=experiments/wasm-exp1/read-path/compiler/build/mutated.wasm
bun --no-install --env-file=/dev/null experiments/wasm-exp1/read-path/atomic.ts
bun --no-install --env-file=/dev/null experiments/wasm-exp1/read-path/micro.ts
bun --no-install --env-file=/dev/null experiments/wasm-exp1/read-path/micro.ts --selected
node experiments/wasm-exp1/read-path/http.ts --smoke
node experiments/wasm-exp1/read-path/http.ts
node experiments/wasm-exp1/read-path/http.ts --writes --smoke
bun --no-install --env-file=/dev/null experiments/wasm-exp1/read-path/matrix.ts
node experiments/wasm-exp1/read-path/startup.ts
python3 experiments/wasm-exp1/read-path/summarize.py
```

`profile-previous.ts`, `profile-variants.ts` and `snapshot-profile.ts` are separate
instrumented/component diagnostics. Never add their means to predict request p95.
The copied runtime/harness files freeze earlier experiments independently.


## Completed evidence

The [result report](../../../docs/experiments/wasm-read-path-results.md) records 80 HTTP cells,
240 supplemental matrix cells, 40 local startup processes and 24 write-smoke cells.
Large-read Bun parity fails; the small-probe regression gate passes. Interrupted
sleep-related evidence and losing variants are retained in `evidence/` with hashes.

The separate [workerd experiment](../workerd-read-path/README.md) uses the frozen
selected module without a Bun application runtime. It is an exploratory comparison
against generated JavaScript on workerd, with different host plumbing; absolute
rates across the two campaigns are not a platform ranking.

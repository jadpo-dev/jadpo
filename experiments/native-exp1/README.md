# Bounded native Rust experiment

Completed results and recommendation: [native Rust report](../../docs/experiments/native-rust-experiment-results.md).

Decision protocol, recorded before implementation measurements. No production
target change, language extension, cloud deployment, or new persistence engine.
The selected workerd SIMD/bulk WASM and its compiler remain frozen; hashes are in
`evidence/freeze.json`. The unselected shared-row experiment is not the baseline.

Compile the identical generated Rust from the guest-scans compiler natively.
Use rusqlite and SQLite, keeping generated validators, authored application control
flow, failure selection and immutable values shared. Native host code owns database
policy enforcement, transaction scope, HTTP and lifecycle. Do not use WASM's i32
pointer exports on a 64-bit host. Restrict execution to a single synchronous
application invocation at a time because the inherited runtime uses globals.

Before timing: differential fixture correctness, invalid inputs/stored rows,
ownership freshness, omission/null, conflict/missing rollback, safe internal
faults, clean reopen, source rename/refinement rebuild, unsupported-plan rejection.
Retain exact work and SQL differences from generated Bun. Fixed trusted principals
are fixture context, never production authentication evidence.

Measurements: fresh processes, sequential rotated native/Bun pairs, five repetitions,
concurrency 1/16, read-small/full 16 KiB/Unicode and single/pair writes; disk WAL/FULL
primary and DELETE/FULL write control. Use 0.5 s warmup + 2 s measurement per cell
for this bounded exploratory campaign (shorter than prior 2+10 s qualification).
Check every response, final snapshots and request counts; no request retries.
Record p50/p95/p99/max, throughput, process CPU/request, resident memory and
no-work headroom. Add isolated calls and 20 rotated startup processes per target,
clean/incremental build timing. Retain raw cells, losses and failed attempts.

Recommendation criteria: equivalent tested semantics are mandatory; repeatable
native CPU/read improvements and shared application code justify a further bounded
slice, not migration. Lack of 2x no-work headroom blocks capacity claims. Report
coverage, adapter duplication, SQLite versions, durability/recovery limitations,
warm OS caches and same-machine generator contention. Prior Bun-hosted WASM and
workerd measurements remain separately labelled historical evidence.

## Reproduce

Prerequisites are the existing checked projection compiler and generated Bun
baseline described in `../wasm-exp1/baseline/README.md`. Native compilation uses
Rust 1.78.0, the checked-in Cargo.lock, system SQLite and cached dependencies.
`prepare.py` writes only this experiment's ignored build directory; the WASM
candidate's generated files are never rewritten.

```sh
python3 experiments/native-exp1/prepare.py
cargo build --offline --locked --release --manifest-path experiments/native-exp1/Cargo.toml --target-dir build/native-exp1/target
cargo test --offline --locked --manifest-path experiments/native-exp1/Cargo.toml --target-dir build/native-exp1/target -- --test-threads=1
python3 experiments/native-exp1/mutation.py
# Restore/rebuild the original after mutation tests.
cargo build --offline --locked --release --manifest-path experiments/native-exp1/Cargo.toml --target-dir build/native-exp1/target
node experiments/native-exp1/correctness.mjs
node experiments/native-exp1/boundaries.mjs
node experiments/native-exp1/measure.mjs --smoke
node experiments/native-exp1/measure.mjs
node experiments/native-exp1/measure.mjs --delete
node experiments/native-exp1/measure.mjs --micro
node experiments/native-exp1/measure.mjs --startup
python3 experiments/native-exp1/build-times.py
python3 experiments/native-exp1/summarize.py
```

Keep builds and all measured campaigns sequential. These commands need permission
to open local loopback listeners. All raw run outputs/databases go under ignored
`build/native-exp1`; archived evidence is copied/compressed separately. A normal
measurement run creates fresh files and does not overwrite another run's database.
The JSON/log filenames are fixed, so archive previous runs before repeating them.

## Frozen campaign and telemetry correction

The completed campaigns were measured at implementation checkpoint `1a56fef`.
Raw logs, source snapshots and hashes are in `evidence/manifest.json`; the measured
Bun server and runner snapshots are authoritative. After timing, the Bun/macOS
peak-RSS unit conversion was corrected. `summarize.py` normalizes the original raw
readings, while new runs mark their corrected units in the protocol. A two-process
telemetry check passed without replacing performance results. The external `ps`
RSS measurements never had this conversion error.

The exact-boundary follow-up deliberately records a semantic gap: the Bun fixture
wrapper checks result size after commit, whereas native checks inside the atomic
boundary. The trigger-constraint classification gap is recorded separately. Neither
is counted as full adapter parity. See the report before interpreting write gains.

`archive.py` asserts that the current artifacts match the completed primary
campaign before archiving; it must not silently attach later source bytes to an
older measurement. Existing archives already preserve the measured bytes.

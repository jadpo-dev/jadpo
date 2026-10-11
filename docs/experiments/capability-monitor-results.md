# Single-pass capability monitor results

This is the next bounded stage after the [capability-host experiment](capability-host-results.md).
The current WASM candidate remains frozen. The new monitor is a separate local
prototype and does not change the default target, the CLI, or the existing
authority experiment.

The trusted Rust monitor performs live credential/session/user checks, validates
the route input, starts a fresh application guest, accepts only the fixture's
declared `entity.read`/`entity.update` sequence, reconstructs the pinned SQL
plan, applies current row and field policy, redacts restricted fields, and binds
the guest completion to the row returned by SQL. It controls begin/commit/rollback.
The application computation runs once. The existing SQLite driver and transaction
semantics are reused; the monitor keeps a fixture-local adapter so its optional
statement-cache probe cannot change the frozen capability-host adapter.

The prototype does not let a guest send SQL or transaction commands. A hostile
guest attempting raw SQL, commit, an unauthorized plan, a forged principal, or a
forged final projection is rejected; writes remain unchanged and transactions are
rolled back. Ten tests pass with 39 assertions, including the typed ABI frame and
the cache-enabled revocation/rollback path.
The fixture also checks
revocation before guest entry, ownership changes during a request, fresh guest
memory and private-field redaction.

The monitor's effect sequence and output projection now come from the checked
projection and contract through the deterministic `generate-manifest.py` build
step. The Rust monitor rejects any route whose generated binding is absent.
This removes the fixture operation IDs and output-field branches from the Rust
adapter, while leaving the generator fixture-local until compiler integration
is promoted. The monitor does not qualify instruction-fuel, memory-exhaustion
or disconnect cancellation isolation.

## Shared code and adapter boundary

The generated application guest, typed status/request/operation ABI, route
manifest, input/output validators, policy predicates, redaction rules, failure
mapping and transaction decision protocol are shared across the monitor's native
and WASM-facing experiment paths. The target adapters still own SQLite calls,
transaction handles, HTTP framing, guest lifecycle and driver-error conversion.
The monitor's trusted Rust core checks the same generated metadata, but it does
not eliminate those adapter responsibilities or make Bun-hosted WASM equivalent
to workerd. The prepared-statement cache probe added only a small local adapter
variant and remains disabled by default.

## Measurements

Three fresh-process repetitions used WAL, twelve clients, 128-request blocks and
300 ms per cell. `monitor` and `wasm` are Bun-hosted; `raw-wasm` is the frozen
previous WASM host; `native` is the existing native executable. The short cells
are attribution probes, and their full distributions are retained in
`build/capability-monitor/breakdown.json`.

| Workload | Native | Raw WASM | Scoped WASM | Single-pass monitor |
| --- | ---: | ---: | ---: | ---: |
| Missing auth req/s | 31,338 | 31,597 | 28,924 | 32,068 |
| Authorized read req/s | 17,907 | 7,792 | 1,434 | 1,577 |
| Denied pair req/s | 10,949 | 4,950 | 1,248 | 1,366 |

CPU microseconds/request were 56.9 / 204.2 / 1,503.9 / 1,308.6 for authorized
reads and 95.2 / 239.4 / 1,458.3 / 1,280.4 for denied pairs in the same target
order. The monitor saves about 13% CPU for reads and 12% for denied pairs versus
scoped WASM in this rerun, while throughput improves about 10% and 9%. Fresh
guest construction and Bun WASM scheduling still dominate. These are short
attribution cells, not a production capacity claim.

The first ABI optimization adds typed status/request/operation/capability fields
and reuses adapter buffers while leaving application payloads as bounded JSON.
Three 50,000-iteration guest-read runs averaged 502.103 ms for the previous
full-JSON frame and 453.248 ms for the optimized frame, a 10.8% reduction. The
serialized output fell 48.1% (11,838,894 to 6,150,000 bytes per run). This is a
bridge-only attribution probe; it does not replace the HTTP/SQLite results
above. The raw runs are retained in `experiments/capability-monitor/bridge-bench-results.json`.
The append-only campaign ledger and resume checkpoint are in
`experiments/capability-monitor/performance-ledger.md`.
An in-process confirmation of the real auth/policy/SQLite path is retained in
`experiments/capability-monitor/in-process-bench-results.json`; it is not an
HTTP capacity result.
The opt-in prepared-statement cache probe is retained in
`experiments/capability-monitor/sqlite-cache-bench-results.json`: six paired
samples improved the in-process cell by 1.8% throughput and 0.7% CPU, which is
below the acceptance threshold and does not justify enabling it by default.

## Performance roadmap and gates

Performance work is a required phase, with correctness as the gate for every
change:

1. Promote the generated effect sequences, completion projections and route
   failure bindings into the compiler's normal backend artifacts. Re-run all
   policy, mutation and hostile-guest checks.
2. Measure a bounded binary representation for the remaining JSON payloads.
   Compare p50/p95/p99, CPU, RSS, startup and SQL counts against the frozen
   monitor baseline before accepting more adapter complexity.
3. Evaluate a scrubbed guest instance pool only after proving memory/global reset
   and cross-principal isolation. Fresh instances remain the reference guarantee.
4. Measure SQLite scheduling and transaction overhead separately. Do not reduce
   freshness, validation, redaction or commit gating to improve a chart.

The prepared-statement cache probe was the permitted local adapter candidate for
this pass. Its 1.8% in-process gain did not meet the 2× gate, so no further
optimization candidate is accepted without a workerd or native end-to-end
campaign that can attribute the remaining cost.

The prototype target is at least **2× the scoped-WASM authorized-read throughput**
with no conformance regression. Failure to reach that gate keeps native as the
recommended production direction and the monitor as a correctness experiment.

All WASM numbers here are Bun-hosted. A workerd result requires its own controlled
campaign and must not be inferred from these measurements.

# What other frameworks do at the WASM boundary

Research checked against primary documentation and source on 30 September 2026.
This informs the [workerd boundary experiment](../../experiments/wasm-exp1/workerd-boundary/README.md).
These are implementation comparisons, not benchmarks of those frameworks.

| System | How data crosses | Useful lesson for Jadpo |
| --- | --- | --- |
| Native Rust: SQLx + Axum | SQLx maps database columns into Rust fields; Axum serializes the response into a byte buffer. | Keeping storage, application values and serialization in one runtime avoids our extra JS-to-WASM row conversion. Native Rust performance does not predict Workers performance. |
| Cloudflare workers-rs | The SQLite cursor wraps Cloudflare's JS API. Typed rows use `serde_wasm_bindgen::from_value`; raw rows convert JS scalar values into Rust values. | A Rust Worker still has a host boundary. Typed conversion is established practice, but not automatically free or faster. |
| wasm-bindgen / serde-wasm-bindgen | Supports direct JS-value conversion and JSON conversion; string glue writes into guest memory and can decode directly from a guest-memory view. | Measure representation choices on the actual engine. Avoid temporary byte arrays when lifetimes allow safe borrowing. |
| Go `js/wasm` | `syscall/js.Value` represents JS values through references; conversion to Go strings and byte slices uses explicit transfers. | Opaque handles can defer copying. They cannot replace checking a value that Jadpo promises to validate inside the guest. |
| Wasmtime Component Model | A canonical ABI defines cross-language layouts. `WasmStr` can sometimes expose a validated guest-memory string without a host copy. | Borrowing works when the lifetime and runtime API permit it. These native-host facilities are not automatically available through Cloudflare's JS bindings. |

SQLx's derived `FromRow` calls `Row::try_get` for fields; Axum's JSON response
uses `serde_json::to_writer` into `BytesMut`. Together they illustrate a native
pipeline, without an intermediate JavaScript row representation.
Sources: [SQLx FromRow](https://docs.rs/sqlx/latest/sqlx/trait.FromRow.html),
[Axum JSON response source](https://docs.rs/axum/latest/src/axum/json.rs.html).

The workers-rs SQLite implementation deserializes cursor values through
serde-wasm-bindgen and also offers raw scalar rows. Its Rust interface does not
turn the underlying database result into native Rust memory without conversion.
Source: [workers-rs SQLite source](https://docs.rs/worker/latest/src/worker/sql.rs.html).

The wasm-bindgen guide explicitly says that direct JS-value conversion involves
many boundary calls, while native JSON implementations can be competitive. It
recommends profiling the actual values and engine. Its generated string glue uses
`encodeInto` where available and avoids a defensive `slice` when it can decode a
non-shared guest-memory view synchronously. Our implementation takes those
principles, not the framework's code or a claimed universal speedup.
Sources: [Serde interoperation guide](https://wasm-bindgen.github.io/wasm-bindgen/reference/arbitrary-data-with-serde.html),
[wasm-bindgen glue generator](https://raw.githubusercontent.com/wasm-bindgen/wasm-bindgen/main/crates/cli-support/src/js/mod.rs).

Go's JS interoperation uses references for values that cannot be passed directly
to WASM; materialising strings/bytes still transfers data. This resembles the
purpose of our request-local row receipts, although the contracts and lifetimes
differ. Jadpo's receipt is only issued after guest validation and does not cache
rows or permission decisions across requests.
Source: [Go syscall/js implementation](https://go.dev/src/syscall/js/js.go).

The Component Model standardises layouts; it does not promise that every rich
value crosses without copying. Wasmtime's `WasmStr` exposes a narrower borrowing
opportunity with store/lifetime restrictions. This is a possible direction for a
native WASM host, not proof that Cloudflare has an equivalent interface.
Sources: [Canonical ABI](https://component-model.bytecodealliance.org/advanced/canonical-abi.html),
[Wasmtime WasmStr](https://docs.wasmtime.dev/api/wasmtime/component/struct.WasmStr.html).

## Applied experiment

The local workerd stage profile covers small, 16 KiB and 48 KiB ASCII and escaped
fixtures, full-row and title-only returns, and JSON/binary ingress. Every result
is checked. Timers advance locally but have coarse resolution in this run; stage
means include instrumentation effects and must not be added to predict HTTP p95.
Cloudflare freezes CPU-only timers in deployed Workers, so the same profiler is
not a valid hosted CPU timer.
Source: [Cloudflare performance timers](https://developers.cloudflare.com/workers/runtime-apis/performance/).

The profile identifies a redundant conversion in the old binary encoder: it
serializes the whole row into JSON and UTF-8 to enforce a byte limit, then encodes
the row into binary. The new encoder derives the identical JSON bound while
writing each scalar. UTF-8 lengths come from the actual encoder; only strings
requiring JSON escapes need their JSON spelling. Column-key costs are precomputed
from the checked schema. The unchanged guest independently enforces the same bound.

The second change writes JSON directly into a bounded guest allocation with
`TextEncoder.encodeInto`, and borrows result bytes only while decoding
synchronously. No borrowed memory survives an await, allocation, subsequent guest
call or memory growth. The returned JS values own their data. Public HTTP remains
JSON and all fresh SQL/policy/guest validation checks remain in place.

The first selection comparison is deliberately retained: the revised binary path
helps escaped data but loses on plain ASCII. An adaptive alternative tests binary
only for rows with JSON escapes and preserves JSON ingress otherwise. It also
regressed plain ASCII. The simpler driver with direct writes and borrowed result
views was selected before independent HTTP measurement; framework
inspiration alone is not a qualification result.

## Further options, ordered by evidence needed

1. The [HTTP comparison](wasm-workerd-boundary-results.md) now completes 608,612
   requests. Median gains are modest and several win only three of five pairs;
   longer controlled-host qualification remains necessary before adoption.
2. Profile generated typed decoding and validation after host conversions are
   cheaper. Avoid adding a second generic value tree between the database and the
   compiler's checked representation.
3. Consider raw SQLite cursor arrays only with measured benefit and a generated
   column mapping. Cloudflare exposes them, but replacing objects also affects the
   authority adapter and is not automatically beneficial for one-row reads.
   [SQLite cursor API](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/).
4. Keep native-host/WASI experiments separate. They can avoid this particular JS
   representation, but require their own storage, authentication, scheduling and
   deployment coverage. Choosing them would not qualify a Cloudflare backend.

No check is skipped because data happens to be unchanged, and no borrowed row is
accepted as validated just because another language or host returned it.

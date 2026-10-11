# Native runtimes and the extra WASM value boundary

Research and local experiment, 30 September 2026. This continues the
[framework comparison](wasm-framework-boundaries.md). It does not change Jadpo's
default backend or claim that native implementations never copy data.

## Why Bun is different

Bun 1.2.20's SQLite binding is native C++ integrated with JavaScriptCore. Its
`toJS` function reads SQLite text and produces an engine string. The implementation
has separate short-text and longer-text conversion paths. BLOB handling allocates
a JS typed array and explicitly copies bytes. The pinned source therefore supports
“direct runtime integration”, not “zero-copy SQLite”.

Source: [Bun 1.2.20 SQLite binding, toJS](https://github.com/oven-sh/bun/blob/bun-v1.2.20/src/bun.js/bindings/sqlite/JSSQLStatement.cpp#L492).
The raw tagged source was retrieved and inspected locally. Current
[Bun SQLite documentation](https://bun.com/docs/runtime/sqlite) also describes the
native driver. Its advertised cross-runtime benchmark numbers are not used here.

For our Bun target, the row becomes the application's JS values. For our Workers
WASM target, Cloudflare first exposes a JS row; the host glue then encodes a second
representation in the guest's linear memory. Rust decodes and validates it before
executing the authored operation. Row receipts already avoid encoding an unchanged
validated row all the way back out, but do not remove inbound conversion.

This explains a structural difference, not the exact magnitude of any measured
gap. Native code can construct its runtime's own values. A sandboxed Rust/WASM
module cannot just dereference JavaScriptCore or V8 strings as Rust `String`s.
Native Bun and local workerd also have different HTTP/storage plumbing; only
same-host pairs below are direct performance comparisons.

## Other implementations checked

| Implementation | Native database-to-language path | Lesson for Jadpo |
| --- | --- | --- |
| Node built-in SQLite | SQLite text goes to V8 strings; the current source uses an ASCII fast path and UTF-8 fallback. | Specialise common cases without weakening the general decoder. |
| CPython SQLite | SQLite text becomes Python Unicode objects inside its C extension. | A native implementation still converts and allocates, but need not serialise a second language boundary. |
| workers-rs | Cloudflare JS cursor values are deserialised into Rust values. | A Rust framework on Workers also faces conversion; its language alone does not remove it. |
| Native SQLx/Axum | Database values become Rust fields, then JSON is written for HTTP. | Avoiding JS rows is possible on a native host, but this is a distinct backend qualification. |

Primary sources: [Node SQLite source](https://raw.githubusercontent.com/nodejs/node/main/src/node_sqlite.cc),
[CPython cursor source](https://raw.githubusercontent.com/python/cpython/main/Modules/_sqlite/cursor.c),
[workers-rs SQLite source](https://docs.rs/worker/latest/src/worker/sql.rs.html),
[SQLx FromRow](https://docs.rs/sqlx/latest/sqlx/trait.FromRow.html),
[Axum JSON implementation](https://docs.rs/axum/latest/src/axum/json.rs.html).
Node and CPython `main` are design references retrieved on the research date,
not benchmarked versions. This pass does not rank these frameworks by speed.

V8's own JSON work uses specialised string representations, SIMD/SWAR escape
scanning and fallback paths. That motivates avoiding unnecessary full-string
passes and making any dispatch check bounded. It does not establish that our
host regexp uses the same scanning implementation or explain all timings.
Source: [V8 JSON.stringify engineering account](https://v8.dev/blog/json-stringify).

## Fixes under test

1. **Conservative size proof.** The typed encoder already knows the UTF-8 byte
   count. Add a conservative maximum of five extra JSON escape bytes per UTF-16
   code unit. If the entire envelope's upper bound fits, no escaped JSON spelling
   is needed. Otherwise compute the exact escape cost and enforce the original
   65,536-byte limit. The guest independently checks the exact bound as before.
2. **Direct typed writes.** Encode into an exclusive guest allocation rather than
   a scratch buffer followed by a copy. Views cannot survive a guest call,
   allocation or await. This remains a separately measured alternative.
3. **Bounded content selection.** Inspect at most the first 256 UTF-16 code units
   of each scalar string for JSON escapes or non-ASCII characters. If found, use typed ingress; otherwise
   use the existing JSON path. This is a performance heuristic only: a later
   escape still goes through complete JSON encoding and guest validation.
   It can miss an optimisation opportunity, never a validation requirement.
   Small rows skip content selection entirely.

The full-string selection variant is retained as a control. Additional workloads
put escapes after an 8 KiB prefix and use Unicode with no JSON escapes. These
check the heuristic's limits, beyond the original repeating escaped fixture.
No fixture name, operation name, result cache or permission cache selects a path.
The compiler-produced module, SQL adapter, policy checks and receipt contract
are unchanged. Public HTTP and Durable Object RPC representations remain JSON.

See the [measured results](wasm-native-values-results.md) and
[experiment protocol](../../experiments/wasm-exp1/workerd-native-values/README.md).

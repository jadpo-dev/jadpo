# Shared Rust conformance slice — 30 September 2026

**The two adapter discrepancies are closed in a new local conformance variant,
and native/WASM now compile the same standalone Rust application crate with
per-invocation state.** This continues the
[native experiment](native-rust-experiment-results.md). It does not migrate
production, change the default target or select a new performance candidate.
The prior native experiment and selected SIMD/bulk WASM remain frozen.

## Shared implementation

The checked projection lowers once to `application.rs`: six application callables,
their entry checks and the effective-type/refinement validators. Both the native
binary and the new WASM library depend on `jadpo-shared-core`, which includes that
same generated file. There is no native-specific application implementation or
second WASM-specific lowering. The experimental generator derives from the frozen
lowerer, preserving its supported subset and rejecting unsupported plans.

The core contains no database or HTTP dependency and forbids unsafe Rust. All
mutable execution state lives in an `Invocation`: its suspended future, request
identity, pending operation, host response, effect count and completion. Immutable
compiler metadata can still use `OnceLock`; no mutable request globals remain.
Generated functions explicitly pass invocation context when calling other functions
or issuing storage effects. An invocation is `Send`, while poll/resume require
exclusive access. Tests move a suspended invocation between threads and run eight
threads through 100 independent calls each.

The previous WASM ABI/runtime/codec amalgamation is replaced in this variant by:

| Layer | Responsibility |
| --- | --- |
| Generated application | Checked validators, application calls, failure selection/recovery, effect sequencing |
| Shared Rust runtime | Owned immutable row values, per-invocation futures, bounded host messages, handle validation, completion and cancellation |
| Native adapter | rusqlite, fresh scoped SQL, transaction ownership, HTTP and operational logging |
| WASM adapter | One leased invocation per module instance, input/output buffers and ABI exports |
| TypeScript WASM host | Existing authority/policy adapter and Bun SQLite, transaction lifecycle and host replies |
| Bun comparison wrapper | Existing generated Bun program plus checks inside its transaction/storage callbacks |

The WASM ABI slot uses a `RefCell` local to the instance; that transport slot is
not shared application state. Separate instances can suspend independently. The
native HTTP shell still serializes application work and uses one database
connection; the core's thread-transfer tests do not establish multithreaded database
scheduling or production server capacity.

For this conformance variant, large validated rows share an owned `Arc<Value>`;
materialization yields an owned value. Target-specific row receipts, SIMD scans,
compact queries and typed binary ingress are omitted. JSON budgets are counted
with a bounded serialization sink that stops at 65,536 bytes. This deliberately
simplifies the common core; it is not presented as a transport optimization or as
performance-equivalent to the frozen selected WASM.

## Failure classification follows the existing language contract

The initial experiment found that a SQLite trigger constraint became `ItemConflict`
in generated Bun but an internal fault in the native/WASM adapter. The repository
already defines the correct behavior: an unqualified `conflict:` handles remaining
normalized constraints, as described in the
[failure model](../archive/specs-v0/failure-model.md) and [grammar](../archive/specs-v0/grammar-v0.1.md).

The new native adapter therefore classifies SQLite's constraint error class,
including extended trigger constraints, as the host `conflict` status. Its generated
Rust application chooses the authored failure. The new TypeScript WASM binding
makes the corresponding normalization change. Ordinary SQL errors remain internal.
No language contract or generated Bun failure implementation was changed.

Native, Bun and the new WASM variant now return **`ItemConflict` and roll back the
whole pair** for the trigger case. An integer-overflow SQL error executed only on
the second row remains an internal fault and rolls back. Unique conflicts, missing
second rows and successful pair commits retain their existing behavior. Precise
constraint-identity mappings beyond the fixture's fallback remain outside the
experimental Rust lowering.

## Budgets now fail inside the owning transaction

The old Bun fixture wrapper checked the completed response after the generated
callable had committed. Returning an internal response for an oversized write
therefore did not undo the write.

The new conformance wrapper leaves generated Bun files unchanged. It decorates
persistence clients through `withPolicy`, `withOperationTime` and transaction
callbacks, preserving the existing transaction/savepoint implementation. It checks
storage envelopes before returning them to application code and checks the outer
callback's completion before that callback returns to the transaction owner. A
budget failure escapes as an operational fault, causing the owning transaction
to roll back. This does not add an independent outer database transaction or
handwritten application logic.

All three adapters now agree on the boundary probes:

| Probe | Native | Bun | New WASM |
| --- | --- | --- | --- |
| 65,536-byte logical row envelope | success | success | success |
| 65,537-byte logical row envelope | internal fault | internal fault | internal fault |
| Invalid stored blob | internal fault | internal fault | internal fault |
| Oversized update result | internal + rollback | internal + rollback | internal + rollback |

The native adapter validates entry arguments before opening its transaction. The
WASM host starts the invocation and validates input before acquiring its atomic
boundary, then executes pending storage effects inside it. Neither caches row
contents or ownership decisions. Fresh SQL still includes both identity and the
trusted fixture principal's ownership predicate.

## Completed checks

- **10 Rust tests** pass with normal parallel test execution: seven shared-runtime
  tests and three native SQLite tests. They cover independent/reversed resumes,
  moving a suspended invocation across threads, 800 parallel calls, malformed host
  failures, exact budgets/escaping, immutable value ownership, stale/duplicate
  handles, cancellation/drop and sequential effects, descriptor forgery,
  source refinements, and committed/rolled-back data after clean reopen.
- **7 WASM host tests / 432 assertions** instantiate the actual new module in Bun.
  They cover independent instances, repeated reuse, stale/duplicate/cancelled
  handles, malformed UTF-8/JSON, malformed host/domain replies, stored validation,
  exact limits, Unicode/escapes, sequential effects and source refinement.
- **1,464 differential HTTP cases** pass on six fresh servers: native, Bun and
  WASM, each with WAL/FULL and DELETE/FULL. The two former mismatches now use common
  expectations, with no target-specific exception in the assertions.
- **12 boundary observations** pass across the three targets, including rollback
  of the oversized update.
- **3,360 further calls** pass with 16 client lanes: alternating trusted principals,
  successful atomic pairs and conflicting pairs. Final snapshots remain coherent;
  a rejected pair cannot leave only its first write visible.
- Actual checked source variants rename Item/User/owner_id and raise the title
  minimum length. Native tests and actual WASM execution pass for both variants.
  Unsupported freshness, cross-store and field-policy plans are rejected. The
  original generated source is restored afterward.

Both native and WASM have an explicit cancellation/transaction integration test:
the first update actually executes and is inspected inside a real SQLite
transaction, the invocation suspends before its second effect, and cancellation
rolls back the first update. Native additionally verifies rollback when both the
invocation and the rusqlite transaction guard are dropped. **This is core/driver
lifecycle evidence, not HTTP-disconnect or async cancellation integration.**

The request and host frame limits remain 64 KiB, with at most 64 storage effects
per invocation. The new WASM module retains an 8 MiB maximum linear memory and
256 KiB stack reservation. Native does not acquire WASM's sandbox or trap-isolation
properties merely by sharing this crate.

## Artifacts, frozen controls and limits

Generated application SHA-256:
`e87b8c7b396be2bef2307809c940761c9a6b5bb90b29e5b64427f65960d13607`
(9,749 bytes). This is the single file consumed by both crate builds.

New conformance WASM SHA-256:
`f9dd46c53c94dfa01c86a534b5f38f64df824299f4f28a42962fa9ab40524f2c`
(166,291 bytes). A fresh Cargo target directory produces identical module bytes.
This is a separate artifact; it does not replace the selected SIMD/bulk module
`63358b4df468b9e99dcc1b3ef0faeff7d362b4e297bd382c3b9be2d36080448c`.

The native executable, generated files, source variants, logs and HTTP results are
retained in the [evidence manifest](../../experiments/native-conformance/evidence/manifest.json).
[Reproduction commands](../../experiments/native-conformance/README.md) use the same
pinned Rust 1.78.0, Bun 1.2.20 and existing database drivers. The previous native experiment’s sources/evidence, selected WASM inputs/artifact
and both unrelated user files
are checked against the follow-up's starting hashes.

No performance campaign was repeated. Earlier throughput, CPU, memory, startup
and build numbers belong to the older frozen implementations. These changes alter
runtime representation, synchronization, transport and boundary checking, so those
numbers must not be attributed to the new crates. All new WASM execution here is
**Bun-hosted**, with no new workerd or cloud result.

The remaining platform work is substantial: database/transaction adapters and
policy enforcement are still separate; HTTP disconnect cancellation and real
asynchronous I/O are not integrated; trusted fixture principals do not test
production authentication; general field policies, relations, savepoint lowering,
crash recovery and broader language coverage are not qualified. The newly corrected
Bun budget checks are an experiment wrapper, not a production route change.

**Recommendation:** retain this shared-core boundary as the basis for the next
experiment. It demonstrates that application validation, authored failures and
suspended execution can be shared without mutable request globals, and that the
two observed adapter gaps can be closed without changing the language. Before
any migration decision, qualify a representative authenticated/policy-aware slice
and remeasure the exact conforming artifacts. Bun remains the working/default target.

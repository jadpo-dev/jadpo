# Authenticated shared-Rust conformance results

The bounded authentication/policy slice passes across the generated Bun target,
native Rust and the same Rust compiled to WASM. It extends the previous
[shared-core conformance experiment](shared-rust-conformance-results.md) with real
opaque credentials, live user resolution, direct owner/editor permissions,
restricted fields and atomic writes. Bun remains the default target. This is a
local experiment, not approval for a production migration.

**Recommendation:** continue to a performance comparison of these exact conforming
implementations. Authentication and policy decisions can be shared between native
and WASM; the proof now extends beyond trusted fixture principals. Before broader
backend adoption, separately review the trust boundary, authentication runtime and
unsupported policy/transaction forms described below. The earlier performance
numbers do not apply to these artifacts.

## Implemented boundary

The [auth-policy fixture](../../experiments/auth-policy/fixture/app.jadpo) uses existing
Jadpo constructs. The experiment's projection compiler runs the existing syntax,
semantic, type, failure and policy checks. It retains authentication declarations,
authority mappings, route metadata and checked policy obligations. The bounded
lowerer then emits one application Rust file and one checked contract.

Both native and WASM compile the same `jadpo-auth-policy-core` crate. It owns:

- Generated nominal/closed-record validation, output construction, nested calls,
  domain failures, and the authored application control flow.
- Opaque bearer parsing and HMAC-SHA256 verification using the existing `base64`,
  `hmac` and `sha2` crates. These consume the credential format produced by Jadpo's
  existing generated first-party authentication host; no new credential format or
  cryptographic primitive was introduced.
- Current/previous key selection, two live session checks, authoritative user
  resolution, active-state validation and construction of a per-invocation identity.
- Parameterized SQL with checked row-policy predicates and conjunctive narrowing
  field policies. Client input never selects SQL identifiers or policy subjects.
- Transaction effects, safe failure mapping and completion validation before commit.

Authentication/SQL planning is handwritten compiler-runtime support consuming
checked metadata; it is **not** all generated application code. The lowerer remains
Python. Native and WASM share these decisions without maintaining a second JS
implementation for the WASM guest. The independent Bun reference still uses the
production-generated TypeScript authentication, policy and persistence code.

The native adapter supplies Hyper HTTP, rusqlite, configuration and time. The WASM
adapter supplies the ABI, Bun's SQLite driver, configuration and time. Existing
SQLite storage is used throughout. There is no custom database or persistence
engine. The Bun comparison wrapper preserves its generated transaction/savepoint
machinery and adds the same experiment budgets before commit.

Build from a checkout with the updated CLI:

```sh
jadpo build experiments/auth-policy/fixture --target native
jadpo build experiments/auth-policy/fixture --target wasm
```

Neither build invokes Bun. [Direct shell commands and reproduction steps](../../experiments/auth-policy/README.md)
are available when the CLI is not installed. The native executable also runs
without Bun; Bun is used by the comparison harness and to issue synthetic test
credentials through the existing authentication host. Issuance is not newly
implemented in Rust.

## Measured conformance

The final HTTP run made **888 application requests**: 52 scenario calls on each
of six servers (three targets × WAL/DELETE), plus 96 concurrent calls per server.
That is 312 scenario calls and 576 concurrent calls. Another **51 HTTP calls**
checked a changed authored policy/refinement on all three targets. Database
controls, setup, snapshots and readiness requests are excluded from those counts.

All compared HTTP statuses, stable JSON bodies and committed application snapshots
agree. Random request IDs are normalized for comparison. The final 888-request run also
asserts `Cache-Control: no-store` and a request-ID header, with the same ID in
error bodies. Runtime logs are checked for the synthetic credentials and key
values; none appeared.

| Area | Verified behavior |
| --- | --- |
| Credentials | Missing, empty, malformed, tampered, noncanonical and duplicated/conflicting bearer credentials; case-insensitive scheme; current and previous keys |
| Authentication order | Missing/revoked/conflicting credentials and disabled authority are handled before malformed application JSON; headers/body cannot supply a principal |
| Live authority | Expiration, revocation, removed/disabled user, changed user identity, malformed session/authority and unknown key |
| Row policy | Owner/editor public reads; outsider and missing row have the same concealed response; owner/editor changes take effect on the next request |
| Field policy | Owner-only private reads; owner may set/clear the private field; editor may rename but may neither set nor clear the private field |
| Validation | UUID, nominal title length, Unicode scalar examples, unknown input fields, nullable field behavior, and SQL-looking text remaining bound data |
| Transactions | Pair commit; second-row denial/missing, unique/trigger conflict, ordinary SQL fault and authored rejection all roll back; an oversized stored row prevents commit |
| Concurrent callers | 96 requests per server across at most 12 HTTP connections, mixing two principals and denied second writes; final pairs remain coherent |

The 96-request cases contain 64 successful pairs and 32 denied pairs per server.
These are concurrent clients against serialized local request execution, not
parallel SQL execution or a connection-pool scalability claim.

Additional checks passed:

- **Five native lifecycle tests:** raw callable entries cannot bypass auth;
  cancellation after a real first write rolls back; revocation between session
  verification and the live recheck is observed; independently suspended
  principals remain isolated; stale/forged host responses fail closed.
- **Four tests of actual WASM, 50 assertions:** equivalent cancellation and live
  revocation checks, reverse instance completion, 40 reuses alternating principals,
  forbidden direct entry and invalid request handles.
- **Three projection tests**, plus eight focused CLI tests and the diagnostic
  package checks. The existing default Bun build remains intact.
- A checked source mutation grants editors private-field **update** permission and
  raises the title minimum from three to five. All three targets change behavior
  accordingly; the original generated Rust, contract and WASM are restored by hash.
- Six unsupported plans are rejected without publishing replacement Rust/contract
  output: signed validation, bounded revocation, indirect scope, unknown role,
  non-authoritative freshness and public routing. Widening the private-output
  audience is rejected by the existing Jadpo checker.
- A fresh Cargo target directory produces byte-identical WASM.

The cancellation tests exercise explicit invocation cancellation and host rollback.
They do not establish cancellation on HTTP disconnect. Immediate revocation means
fresh checks on each request, including the live-session recheck; this is not a
promise to abort an already-authorized in-flight transaction. Ownership predicates
are evaluated in the actual SQL statements, not taken from a cached principal.

## Comparison boundary correction

A final malformed-string probe found Bun returning **500** for a title containing
an escaped lone UTF-16 surrogate, while Rust's JSON decoder rejects it as invalid
input. The probe failure is retained in `unicode-probe.log` in the archive. The
Bun experiment now checks JSON strings for Unicode scalar validity at the
**post-authentication** request-JSON boundary, producing the same **400** without
attempting the write. Valid surrogate pairs/emoji remain accepted. This correction
is in the comparison wrapper; the generated production Bun target is unchanged.

## SQL work and transaction equivalence

All servers reported SQLite **3.39.5** here. Every opened connection was checked:
WAL or DELETE as selected, `synchronous=FULL`, foreign keys on, busy timeout zero,
`fullfsync=0` and `wal_autocheckpoint=1000`. Bun has its generated persistence
connection plus the harness control connection; native and WASM use one each.

Statement counts below include credential/user reads and transaction commands;
fixture setup and control queries are excluded. Counts matched in both journals.
Full SQL and bound parameters are retained in the evidence.

| Request | Bun | Native | WASM in Bun |
| --- | ---: | ---: | ---: |
| Authorized read | 4 | 4 | 4 |
| Authorized single rename | 7 | 6 | 6 |
| Successful pair | 13 | 7 | 7 |
| Second row denied, pair rolled back | 13 | 7 | 7 |
| Ordinary SQL fault on second write | 14 | 7 | 7 |

Each authorized request performs two session reads and one user-authority read.
Bun's mutative path also performs pre-reads and nested savepoints. The Rust path
uses guarded `UPDATE … RETURNING` inside one immediate transaction and joins
propagating nested calls to it. The selected fixture always propagates failures
out of the owning boundary, so the observed commit/rollback outcome is equivalent.
Handled nested mutations requiring partial savepoint rollback remain rejected.
These SQL differences must be separated from execution-language effects in any
subsequent performance comparison.

Both targets materialize complete internal rows. A public projection omits the
private field from the response; this is not database-level column redaction.
The existing checker proves the output's audience, while the shared runtime
applies the narrower operation/field predicates. Large internal rows still incur
transport and validation work even when the public output is small.

## Sharing and adapter cost

Counts are physical lines, not a productivity or correctness score; the generated
file and JS helpers are compactly formatted.

| Component | Lines | Role |
| --- | ---: | --- |
| Generated application Rust | 110 | Same 14,347-byte file compiled for both targets |
| Shared invocation runtime | 342 | Futures, request ownership, budgets and host protocol |
| Shared auth/policy/HTTP boundary | 457 | Verification, authority, guarded SQL, routes and failures |
| Native adapter | 298 | HTTP, driver, time/config, tracing and local test controls |
| WASM ABI adapter | 106 | Memory/framing and invocation lifecycle |
| WASM JS driver + SQLite adapter | 29 | Host transport and existing database driver |

The shared contract is another 12,628 bytes of checked metadata. The main gain is
one auth/policy/application implementation for native and WASM. Adapter work still
includes HTTP behavior, database error normalization, transaction cleanup,
configuration, clocks, ABI ownership and test controls. The Bun HTTP wrapper and
credential seeding are comparison infrastructure, not part of the native runtime.
The experiment copies the prior runtime/lowerer into a new variant to keep frozen
controls intact; it does not yet consolidate the repository's experimental copies
into a maintained compiler backend.

## Limits and next decision

The threat model exercised here is untrusted HTTP input against trusted compiled
code, configuration, database drivers and hosts. The WASM host now executes SQL
planned by the trusted Rust core. This does **not** independently requalify the
frozen guest/host capability boundary against a malicious or replaced guest
module. Decide whether production WASM requires a separately enforcing host before
adopting this arrangement; this is a material architectural tradeoff.

Other unqualified areas include browser/signed credentials and CSRF, JWT/service
authentication, memberships and indirect scopes, issuance/revocation APIs in Rust,
startup/configuration lifecycle parity, secret zeroization, general savepoints,
multi-connection/asynchronous scheduling, disconnect cancellation, crash recovery,
and an independent authentication security review. The test set does not establish
complete Unicode/JSON parser equivalence or exhaustive equality at every byte-budget
boundary. The 32 KiB body cap and 64 KiB bridge/host/completion budgets are local
experiment policies; the Bun wrapper is not a production-target change.

The executables include loopback-only harness controls and are not deployment
artifacts. All WASM execution here is **Bun-hosted**. No workerd or cloud run was
performed. No new latency, throughput, CPU, memory, startup or build-time campaign
was conducted; earlier measurements stay attached to their frozen implementations.

This is sufficient evidence to pursue the dual-target direction for another
bounded stage. Next, measure these conforming artifacts and decide the WASM host
trust model. Broader project support and moving Python lowering into the Rust
compiler should follow that decision, rather than being implied by these build
commands.

## Evidence

[Machine-readable results](../../experiments/auth-policy/results.json) and the
[archive manifest](../../experiments/auth-policy/evidence/manifest.json) retain raw
responses/snapshots, SQL traces, source mutation results, generated source and
contracts, binaries, the generated Bun reference, synthetic seed data, source
snapshots and logs. The run used Rust 1.78.0, Bun 1.2.20, Node 24.18.1 and Python
3.14.0 on the local macOS/arm64 machine.

WASM: **272,698 bytes**, SHA-256
`0fa980e39ea16f57b95bda7b32a0d35e85f0345b39ce477edf61d0b4a66146b4`.
The native artifact's exact size/hash and all source hashes are in the manifest.
**1,064 frozen file hashes** were verified unchanged, including prior experiment
controls, the selected WASM artifact and the user's unrelated Jadpo/VSIX files.

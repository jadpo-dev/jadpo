# WASM authority boundary decision

Keep independent host authorization for a WASM execution mode that promises
isolation from guest code. Do not promote the authenticated experiment's raw-SQL
ABI as a replacement for the frozen, separately enforcing capability host.
For trusted first-party generated applications, shared Rust remains a useful
native/WASM architecture; explicitly classify that mode as trusting the compiled
application, just as the native executable does. Neither choice changes today's
default target or approves a production migration.

## Evidence, not an assumed sandbox

The frozen `auth-policy` experiment establishes policy enforcement against
untrusted **HTTP clients**. Its shared Rust core verifies credentials, resolves
live authority, applies row/field predicates, validates output and controls
transactions. The Bun-hosted WASM adapter then executes SQL chosen by that core.
It does not authenticate or authorize a guest's host calls independently.

The new [executable probes](../../experiments/auth-policy-measure/trust.test.ts)
exercise the actual frozen SQLite adapter with forged guest effects and disposable
synthetic databases:

- A raw `sql.query` reads a private field without any credential or principal.
- `sql.update` outside a transaction is rejected, but `sql.query` containing
  `UPDATE … RETURNING` succeeds and commits in autocommit mode. Capability names
  do not constrain SQL semantics.
- A guest can begin, issue an update changing ownership, and commit without any
  host policy check. Cleanup correctly rolls back an unfinished transaction but
  cannot undo an already committed unauthorized effect.
- The reviewed real guest still denies an outsider and observes session revocation.

These are direct host-call simulations of malicious guest behavior, not an HTTP
exploit against the admitted application, and not a separately built malicious
WASM payload. They establish the host API's trust boundary without changing the
frozen module or weakening its ordinary request conformance checks.

## Implemented admission check

The small [admission helper](../../experiments/auth-policy-measure/admission.ts)
checks module and external contract bytes against a host-owned reviewed SHA-256
[lock](../../experiments/auth-policy-measure/artifact-lock.json). It copies the bytes,
checks them, then instantiates that same copy, avoiding reopening a verified path.
Tests reject a modified module, a different valid WASM module and a changed
contract. The approved module executes the same successful, denied and freshly
revoked requests as before.

This helper is a separate tested prototype. It is **not installed in the frozen
HTTP adapter**, and its startup cost is not included in the benchmark. It is not a
signature system, a deployment supply-chain solution, an independent security
audit or protection if an attacker can replace the host, lock or compiled code.
The lock must never come from a client or candidate module. It also does not detect
logic defects in reviewed code. The benchmark harness separately verifies all
frozen file hashes before and after its runs.

## Production boundary to retain

If guest compromise/replacement is within the promised threat model, the host must
own credential secrets, the trusted clock, live session/user resolution and the
request's authority. The current frame passes secrets to the guest; that design
cannot meet this stronger isolation promise.

Use checked operation/plan identifiers and bound values as the capability ABI,
with invocation-bound authority handles. Do not accept arbitrary SQL or a
self-asserted principal from the guest. The host must select allowed tables,
columns and predicates from its trusted checked contract, enforce field access
and live row policy in the database operation, and own transaction state and
commit eligibility. Host validation must reject stale/cross-invocation handles,
invalid call order, invalid fields and completion sizes before committing. Merely
checking that SQL starts with SELECT is insufficient.

Full internal rows also expose restricted fields to the guest even if the public
response omits them. Decide which fields the guest itself is authorized to see,
and enforce that at the host projection boundary if guest confidentiality matters.
Output/audience validation cannot undo data already disclosed to a malicious guest.
The richer transaction model must preserve savepoint recovery when handled nested
mutations are eventually supported; this fixture proves only propagating failure.

This does not require hand-maintaining policy logic in both Rust and JavaScript.
Factor the trusted authentication/policy runtime from the application runtime:
link it into native, and run the same trusted Rust authority implementation in the
WASM host (or a separately pinned authority module controlled by that host). The
application guest gets capabilities, not that authority module's secrets or raw
SQL access. This is a proposed implementation boundary, not something this pass
has built or performance-qualified. A second trusted module and more host calls
have costs that the current raw-SQL measurements do not capture.

## Decision and next acceptance gate

Continue the dual-target compiler work for a bounded trusted-application mode.
Keep the old independently enforcing WASM candidate frozen. Do not silently weaken
its guarantees to obtain the new experiment's sharing or performance figures.
A production replacement must either retain independent enforcement or explicitly
change the product threat model through a separate architectural approval.

Before qualifying an independently enforcing replacement, implement the scoped
capability boundary and rerun the same conformance/performance workload plus
malicious guest tests for forged principal/plan, SQL injection, unauthorized
columns, replayed handles, stale authority, transaction ordering and premature
commit. Compare its additional SQL/ABI work separately. The present benchmark
supports the viability of sharing Rust; it does not qualify that future host.

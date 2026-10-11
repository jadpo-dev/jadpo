# Generated JWT integration findings

**Date:** 2026-09-30. **Scope:** compiler-generated JWT application, real local
HTTP, real JOSE signature verification, SQLite and disposable local PostgreSQL.
This is bounded AUTH-P6 integration evidence, not production identity-provider
certification or completion of the golden application.

## Contract correction

The golden application already declares `UserDisabled` with kind
`NotPermitted`, and the accepted failure model maps that kind to HTTP 403.
Authentication promises to use the declared inactive-principal failure. The
compiler's former restriction to `Rejected` invented a narrower rule; its
repair does not require a new owner decision or changing golden expectations.
The coordinating task repaired that check to accept `Rejected` and
`NotPermitted`, with diagnostic copy and a core regression.

The JWT fixture now uses `NotPermitted` and expects `403 user_disabled` on both
ordinary and fresh requests. Existing first-party/service fixtures retain
`Rejected` and their 422 expectations, preserving evidence for both declared
failure kinds. Sources: [golden declaration](../../examples/golden-todo/app.jadpo),
[failure catalogue](../../docs/failure-model.md#3-standard-failure-kinds), and
[authentication contract](../../docs/authentication-plan.md).

## What the suite exercises

The suite copies the authored
[`jwt-auth` fixture](../runtime/fixtures/jwt-auth/app.jadpo) into an isolated
temporary directory and invokes the current Jadpo CLI's checked build. It does
not edit generated application code. Generated package and lock declarations
must exactly match compiler-owned canonical files and the explicitly installed
test cache. The installed JOSE version must match the compiler pin and declare
no transitive dependencies. No network installation runs inside the suite.

The application uses its generated `initializeApplication` configuration and
authentication initialization. A real localhost HTTP listener serves its
generated request handler. Test tokens use real 2048-bit RSA keys, RS256
signatures and the pinned `jose` implementation. The only provider substitute
is a test-owned `globalThis.fetch` installed before application import and
initialization: it answers the one configured HTTPS JWKS URL, checks fetch
policy, and rejects unexpected destinations. Signature verification is not
faked, and no fake-verifier production API was added.

The fixture combines JWT and opaque-user validators in the same bearer slot,
plus a signed browser cookie strategy. All resolve to the same declared user
principal through a unique authoritative subject lookup. HTTP assertions cover:

- Valid ordinary/fresh requests, missing authentication before malformed body
  decoding, wrong issuer/audience, unknown subject, expiration and signature
  corruption.
- Disabled authority on both ordinary and fresh external-JWT paths, proving
  that cached provider keys do not cache an active principal.
- JWT/opaque/cookie dispatch, JWT in the wrong cookie slot, malformed opaque
  credentials, duplicate slots and invalid credentials beside a valid JWT.
- Public liveness ignoring malformed/conflicting credentials without invoking
  the provider.
- Profile, local-ID, principal-kind, role and permission claims failing to
  replace the resolved local identity or bypass another user's note policy.
- Concurrent requests preserving each resolved local identity and sharing only
  the permitted provider-key fetch.
- Provider unavailability returning a safe operational failure, without the
  token, signing key, profile sentinels, provider detail or a stack in public
  responses.
- Invalid issuer/JWKS/audience configuration rejecting initialization and
  leaving authentication unavailable. A separate process also launches the
  generated production entry point with invalid configuration and must exit
  with `RUNTIME_STARTUP_FAILED`, no `runtime.ready` event, and no secret/config
  value disclosure.

## Query-count evidence and limits

The SQLite-only case wraps `Database.prototype.prepare` to count the exact
native authority SELECT while forwarding every call to the original native
method. It observes one principal query for the ordinary JWT request and one
additional query for the fresh request. It does not replace query results,
principal resolution or verification.

The PostgreSQL run deliberately skips this SQLite-specific observer. It proves
the corresponding HTTP behavior and state checks against PostgreSQL, **not an
exact PostgreSQL authority-query count**. Neither the count assertion nor JWKS
fetch count proves general query minimization for other applications.

## Execution record

The final focused rerun used the rebuilt CLI containing the declared-kind
correction. SQLite passed **14 tests, 178 assertions**. PostgreSQL passed
**13 tests, 172 assertions**, with the one SQLite-only query-counter test
explicitly skipped. Both runs passed ordinary and fresh disabled-principal
requests as `403 user_disabled`; no other expected status was changed.

Commands:

```text
bun --no-install --env-file=/dev/null test tests/runtime/jwt-integration.test.ts
bash tests/runtime/postgres.sh jwt-authentication
```

The PostgreSQL harness supplies `JADPO_JWT_AUTH_DATABASE_URL`; the suite refuses
nonlocal or non-`jadpo_auth_test_*` databases and never inherits an unrelated
`DATABASE_URL`. The observed PostgreSQL version was 16.3 and Bun was 1.2.20.
Local listener access required the tool environment's approved sandbox
escalation; this was not a product authentication bypass.

## Remaining boundary

All **44 golden application cases remain unexecuted**. The prior 403 discrepancy
is explained by a compiler restriction and the existing declared kind, not a
missing product decision; this integration suite does not by itself make the
golden source executable or pass its other contracts. Real provider operations,
deployed HTTP, broader principal/profile mappings, exact PostgreSQL query
accounting and complete AUTH-P7/P8 evidence remain separate. The HTTPS fetch
stub proves use of the configured JWKS boundary, not real-world TLS, provider
availability or key operations.

No further blocking generator/runtime defect was observed in the final bounded
integration runs after the declared-kind correction. The dependency closure was also
inspected: generated JWT dependency artifacts are opt-in and canonical, and
the external import exception is limited to `jose` in the compiler-owned JWT
module. The coordinating task owns the full validation gate and dependency
provenance evidence; this report does not claim those from an integration pass.

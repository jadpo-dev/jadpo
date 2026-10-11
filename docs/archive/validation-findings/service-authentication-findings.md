# AUTH-P5 service authentication evidence

## Contract and implemented boundary

Authority: `docs/authentication-plan.md` accepted §2 and AUTH-P5; exactly-one
selection remains AUTH-P3, and service policy remains POLICY-001. This is bounded
local compiler/runtime evidence, not full authentication, golden-application,
production, or independent security qualification.

The service adapter accepts the existing bearer strategy syntax with an
`api_key` validator and an optional `signed` service validator in the same
reserved credential slot. The key validator explicitly names ownership, for
example `owner: Worker.responsible_operator`. The binding must be a required,
nonnullable reference from the authoritative service entity to another
persistent entity's identity. Neither the entity nor the owner field name is
inferred. The fixture uses `Worker` and `Operator` to exercise that fact.
Service resolution has an explicit unique subject, identity mapping and active
predicate. Credentials live in the compiler-owned
`__jadpo_auth_service_credentials` table, separate from user sessions; they are
not arbitrary authored credential tables.

`authenticationHost()` exposes the following trusted host integration APIs:

- `issueServiceCredential(strategy, subject, expires, now)`: resolves the active
  service, returns one newly generated 256-bit random key plus service/credential
  identifiers and expiry, and persists only an HMAC verifier and identity/lifecycle
  metadata. There is no secret-reveal/read API.
- `exchangeServiceCredential(strategy, key, now)`: verifies the live key and
  active service before minting an identity-only signed service bearer. Its
  expiry is at most both the original credential expiry and the declared
  maximum revocation delay.
- `revokeServiceCredential(strategy, credentialId, now)`: revokes one record;
  overlapping replacement credentials remain independently usable.
- Existing `refresh`, `revoke` and `authenticate` support the service envelopes.
  Refresh rechecks the originating credential and service and never extends the
  credential's absolute expiry. Fresh requests also recheck origin-record
  revocation, expiry, key retirement and service identity/lifecycle.

The host must authorize provisioning/revocation callers and supply operation
time. No provisioning, login or exchange HTTP endpoint was added, and these
APIs are not authored Jadpo callables. Deployment-specific exposure of exchange
remains host integration work.

Direct service keys check authority every time, including under a bounded
application default. Ordinary exchanged bearers are lookup-free until expiry;
service disable/key revocation does not pretend to invalidate these immediately.
They retain the original service identity and `api_key` authentication strength.
Token prefixes and closed signed payloads distinguish user and service kinds;
no roles, permissions, owner data or provider profiles are carried.

Service issue/exchange/refresh/revoke operations emit closed `authentication_audit`
records containing only event name, schema version, strategy, service ID,
credential ID and operation timestamp. A compiler-owned optional callback can
receive those same frozen records; its default emits JSON to stderr. Credential
records, subjects, verifiers, secrets, provider data and raw errors are never
passed to this sink. Durable audit storage/retention is outside this local proof.

## Independent regressions and corrections

`tests/runtime/service-authentication.test.ts` builds its retained authored
fixture from `tests/runtime/fixtures/service-auth/app.jadpo` into a new temporary
project, creates its own SQLite database or uses only the runner's dedicated
PostgreSQL URL, and exercises generated handlers over a real localhost HTTP
server. Deterministic expiry boundaries additionally use the generated host API.

Initial 12-case run: 9 passed, 3 failed. One actual integration defect was
retained and fixed by the parent in policy identity resolution: a declared
service mapped from a differently named entity and a `Uuid` principal field
could authenticate but not access its own role-scoped resource. Target generation
now uses the declared authority mapping rather than guessing `worker_id`.
The other two initial failures were authoring/harness corrections: storage is
exported from `persistence.ts`, and the existing declared `Rejected` inactive
failure maps to HTTP 422. The test now asserts that existing contract; the
golden candidate's HTTP 403 expectation was not changed or claimed satisfied.
Subsequent parent review found the golden source already declares
`NotPermitted`; a Rejected-only compiler restriction has now been repaired.
The JWT integration suite covers that 403 path, while this fixture deliberately
retains the valid `Rejected` / 422 contract. No owner decision was missing.

A subsequent bounded review retained a failing strength regression: fresh
resolution of an exchanged service token returned `signed` instead of the
originating `api_key`. Runtime authority resolution now preserves the service
proof across direct, ordinary bounded, fresh and refreshed requests. Before the
fix, its focused test failed at the exact strength assertion; after rebuilding,
the complete suite passed on both adapters.

## Verification

Final evidence: **15 cases / 209 assertions passed in SQLite and again in
PostgreSQL**. This is one case set executed on two adapters, not 30 independent
behaviors. Coverage includes issuance/verifier-only storage, explicit ownership,
policy isolation, equal UUIDs across user/service namespaces, overlapping
credentials, individual revocation, whole-service disable, bounded exchange and
exclusive expiry, lookup-free operation during storage outages, fresh failure,
exactly-one rejection before lookup, signed-payload tampering and closed claims,
key overlap/retirement, subject reuse, separate-process persistence, malformed
stored verifier data, authentication strength and secret-free audit records.

Earlier compatibility run: all 24 existing first-party user cases plus the 15
service cases passed together (39 cases, 450 assertions before the additional
strength assertions). The parent owns the subsequent complete integration gate.

```sh
cargo build --manifest-path jadpo/Cargo.toml -q
bun --no-install --env-file=/dev/null test tests/runtime/service-authentication.test.ts
bash tests/runtime/postgres.sh service-authentication
```

The PostgreSQL runner creates and removes its own localhost cluster and exports
only `JADPO_SERVICE_AUTH_DATABASE_URL` for this suite; it does not reuse an
application database. Final logs and initial regression evidence are retained
under `build/validation/service-authentication/`.

## Remaining limits

This evidence does not execute any of the 44 complete golden obligations or
close AUTH-P7/P8, JWT dependency/provider qualification, external approval,
production deployment, arbitrary principal mappings, arbitrary authored
credential storage, long-running fuzzing or independent security review. The
shared optional external-verifier hook supports the separately owned JWT
integration through the existing exactly-one selector; these 15 cases do not
claim JWT coverage. Generated source/schema/artifact consistency and all-suite
registration are part of the parent's integration gate.

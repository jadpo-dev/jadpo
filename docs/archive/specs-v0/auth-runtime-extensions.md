# Service and external JWT runtime boundaries

This extends the browser/API runtime under the accepted
[AUTH-001 contract](authentication-plan.md). The service and JWT adapters share
the existing exactly-one-credential selector, protected-route default, typed
principal, authoritative resolution and policy context. The generated service
exchange has scoped independent runtime/security review; this is not external
security assurance or deployment qualification.

## Service credentials

Declare service validators in the single bearer strategy. An `api_key`
validator supplies secret textual `secret` configuration, an `audience`, and
an explicit `owner: Worker.responsible_operator` binding. The owner must be a
required persistent reference on the service authority; entity and field names
are not prescribed. A `resolution service` declares that authority's unique
subject, active predicate, service identity mapping and inactive failure.
An optional service `signed` validator in the same slot enables bounded exchange.
See the [checked fixture](../tests/runtime/fixtures/service-auth/app.jadpo).

An API-key validator can bind a declared credential entity with `credentials`:
`identity`, `principal`, `verifier`, `active`, `expires` and `revoked`. See the
[golden migration declaration](../examples/golden-todo-migration/authentication.jadpo).
All field roles belong to one persistent authority in the service's store.
Identity is a UUID; principal is a required reference to the service identity;
verifier is unique Text; expiry is an Instant; revocation is nullable Instant.
Active must be a required local field equal to a unit enum variant or Boolean
literal. For this binding, the service active predicate uses the same subset.
Field roles must be distinct. Generated lifecycle roles are rejected; generated
UUID identity is supported. Other required fields must be generated creation
timestamps, so the host can construct a complete row. Unsupported shapes fail
target generation. Names of entities, fields and enum variants are not prescribed.

Issuance writes the declared row and private metadata in one transaction, with
an active-service recheck protected by the database transaction. Private metadata
retains identity, strategy, key version and a verifier integrity digest; it cannot
replace declared status, expiry or revocation. The verifier lives only in the
declared row. Revocation writes its `revoked` timestamp, without requiring a
particular inactive enum variant. Rotation creates a separate credential.
The golden service subject is the unique nonsecret service name; the stable
service UUID prevents a reassigned name from inheriting an existing credential.
Applications without this binding retain their internal credential storage.

The compiler's trusted host integration exposes:

- `issueServiceCredential(strategy, subject, expires, now)`: verify the active
  service and return a newly generated secret once; persist only its verifier
  and nonsecret identity/lifecycle data.
- `exchangeServiceCredential(strategy, credential, now)`: verify the key,
  its current record and active service, then issue an identity-only bearer
  token capped by both the application revocation bound and key expiry.
- `revokeServiceCredential(strategy, credentialId, now)`: revoke that one
  record. Other overlapping credentials continue to work.
- `refresh` and `revoke`: retain the originating service credential boundary.

These are compiler-owned host operations, not authored business callables. The
embedding host owns provisioning authorization; this repository's selected
issuance path is trusted host provisioning with one-time reveal. A strategy may
declare a bounded exchange with:

```jadpo
exchange {
    path: "/auth/exchange"
    key: service_key
    signed: service_signed
}
```

The compiler lowers that declaration to `POST` and owns credential selection,
failure mapping, the minimal `{ access_token, token_type, expires_at }` response,
audit and generated route/OpenAPI metadata. It inventories credential locations
once, rejects missing, malformed or competing presentations before validation,
and passes the sole selected key directly to the host operation. Business
callables receive neither the raw key nor the returned bearer. The original key
and stored verifier remain private. The scoped implementation is approved by the
independent runtime/security review
and documented in the [RM-104 history](archive/implementation-history.md#2026-10-02--rm-104-generated-service-credential-exchange).

Direct keys check credential and service authority on every request. Ordinary
bounded requests validate locally; fresh requests, refresh and exchange check
current authority. A revoked/disabled service may retain ordinary access until
the bounded token expires; no immediate guarantee is claimed for that path.

User and service credentials have separate wire namespaces and records. Policy
uses the declared authority-to-principal mapping and checks principal kind;
equal user and service UUIDs do not confer each other's roles. Lifecycle audit
events contain only strategy, service ID, credential ID, operation and time.

## External JWT bearer validation

A `jwt` user validator accepts `issuer`, `audience`, and optional `jwks_uri`.
These are nonsecret textual/URL configuration or literals. The compiler rejects
cookie JWTs, authored algorithm/package settings, duplicate validators for the
same mode/kind, and unsupported principal shapes. Invalid URL/configuration
fails startup. When no JWKS URI is supplied, discovery must return the exact
configured issuer. No redirects or token-provided key URLs are followed.

The adapter allows RS256 and ES256, requires `iss`, `aud`, `sub`, `iat`, `exp`,
checks `nbf` when present and applies 30 seconds of clock tolerance. It rejects
duplicate JSON keys, noncanonical encoding, hostile headers, unsupported keys
and malformed claims. Limits are 8 KiB per token, 2 KiB per header, 128 KiB per
provider document, 32 keys and two seconds per fetch. Key cache lifetime is five
minutes with a 30-second refresh cooldown and coalesced concurrent refreshes.
Expired-cache/provider failure is unavailable; stale keys are not an outage
fallback. These fixed bounds are compiler policy, not author-selected options.

Only the verified subject leaves the JOSE adapter. The generated boundary
performs one authoritative principal lookup on **every external JWT request**,
including fresh routes, and reuses that result only within that request. Local
IDs and lifecycle state come from the authority; token profile/role claims do
not populate the principal. Credential checks are counted separately from the
principal lookup, as chosen by the owner. Discovery is lazy; startup validates
settings, but successful startup does not attest provider availability/readiness.

## Typed authentication strength

The first-party boundary supports a declared `authentication_strength` enum
whose complete vocabulary is `primary` and `multi_factor`; the enum's type name
is not prescribed. Built-in signed, opaque, API-key and JWT proofs map to
`primary` before principal normalization, including bounded, fresh and refresh
paths. These adapters do not establish multi-factor assurance. Unsupported enum
vocabularies fail target generation instead of accepting values by type name.
Legacy Text strength retains the existing adapter-mode strings.

The general adapter boundary validates declared subject and strength scalar/enum
representations. It rejects undeclared strength values and malformed primitives;
it does not coerce an independent trusted adapter's valid `multi_factor` result.
The generated TypeScript principal type carries the declared enum vocabulary.
This change does not implement additional nominal Text refinement validation.

The golden migration has an explicit service credential binding. Its dedicated
regressions exercise declared expiry/revocation/status, service disablement,
identity substitution, metadata corruption, verifier non-disclosure and rollback
of either issuance write. Ordinary signed requests retain their issued bound;
direct and fresh requests consult current lifecycle state.
See the [current delivery checkpoint](archive/work-plans/golden-delivery-planning.md#rm-102-implementation-slices-and-evidence).

## Dependency and build contract

Only a supported application declaring JWT emits `target/package.json`,
`target/bun.lock`, the JOSE adapter and `audit/runtime-dependencies.json`.
The compiler pins `jose` 6.2.12 with zero transitive packages. The manifest/lock
must match compiler-owned bytes; only that adapter may import the bare `jose`
specifier. Other package imports/manifests remain rejected. Non-JWT targets
retain the existing dependency-free runtime boundary.

Install deliberately after a checked build:

```sh
python3 tools/install-jwt-dependency.py --directory path/to/application/build/target
bun --no-install --env-file=/dev/null path/to/application/build/target/app.ts
```

Rebuilding replaces derived output, so install again before executing it.
Windows uses `--env-file=NUL`. Runtime package auto-install stays disabled.
The pin records MIT licensing, tarball integrity, upstream commit, the dated
published-advisory review, and verified npm registry signature/SLSA attestation.
The upstream Git tag is not signature-verified. These are bounded provenance
facts, not a claim that the dependency has no vulnerabilities. Updating the pin
requires another explicit integrity/provenance/advisory and conformance review;
applications cannot choose a version.

## Evidence and remaining exits

The registered service suite runs against SQLite and disposable PostgreSQL.
The JWT suite uses actual pinned JOSE cryptography with bounded provider fakes,
and the generated integration suite exercises the full HTTP/authority boundary.
The upstream harness preserves both the supported-algorithm subset and broader
failures for algorithms Bun 1.2.20 does not implement. It does not relabel those
failures as a complete upstream pass.

The full golden todo and broader principal/profile mappings remain separate
P11 exits. Its disabled-principal cases declare `NotPermitted` and expect 403; the
compiler now accepts that category as well as `Rejected` (422). An earlier
Rejected-only restriction was a compiler defect, not an unresolved HTTP decision.
The golden expectations are unchanged. Provider production readiness, external
security review, deployment qualification and human/comparative studies are
not established by these local suites.

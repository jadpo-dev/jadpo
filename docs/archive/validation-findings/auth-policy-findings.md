# Independent authentication/policy validation — 2026-09-30

## Contract authority

Expectations were stated before inspecting implementation or existing tests:

- `docs/authentication-plan.md` §§1–2: authentication precedes input decoding and
  business work; exactly one credential is accepted; identities never merge;
  bounded signed authentication does not promise immediate revocation; fresh
  routes and opaque credentials consult authority; public routes have no principal.
- `docs/policy-plan.md` D12–D16, D21/D23 and §§5.4, 9.5, 12: omission grants no
  authority; independent policy obligations compose conjunctively; role authority
  is fresh independently of credential freshness; denied mutations are atomic;
  inaccessible and missing required rows share the declared failure.
- `docs/policy-proof-v0.1.md` is explicitly historical source vocabulary. Its
  conservative distinction between tests and proof remains relevant, but the
  accepted policy plan controls current syntax and authorization semantics.

Existing tests already exercise isolated selector matrices, CSRF, key rotation,
expiry, credential persistence, direct policy predicates and basic principal
concurrency. This package focuses on interactions through generated HTTP handlers
and nested transactions, plus independently triggered compiler boundary errors.

## New executable evidence

### Compiler boundary suite

`jadpo/crates/core/tests/validation_auth_policy.rs`: **7 tests passed**.

```sh
cargo test --manifest-path jadpo/Cargo.toml -p jadpo-core --test validation_auth_policy
```

Cases cover:

- protected source without configured adapters remains target-unsupported;
- signed-only validation cannot claim immediate revocation, while the same
  supported source with opaque validation can target immediate mode;
- bounded mode without a maximum delay produces a source-anchored diagnostic;
- a public route cannot hide principal access behind a transitive query;
- principal variants cannot enter public outputs directly, through List/Map,
  or through an intermediate record;
- public and internal failure contexts both reject nested principal values;
- field policy cannot introduce an effect denied by its containing entity.

Each negative case asserts the actual rule and applicable source span, or the
explicit unsupported-target rule. Repair checks prohibit executable edits that
silently add `auth: none` or `Access.public`. Repairs exercised by the suite
restore configuration, select the already-supported opaque mode, preserve route
authentication, or narrow field permissions. They do not create new authority.

### Runtime interaction suite

`tests/runtime/validation-auth-policy.test.ts`: **7 tests passed in SQLite mode
and the same 7 passed against disposable PostgreSQL 16.3**, using real signed
browser and opaque API credentials and generated routes.

```sh
bun test tests/runtime/validation-auth-policy.test.ts
bash tests/runtime/postgres.sh validation-auth-policy
```

The suite compiles temporary source with the current `JADPO_BIN` (or repository
debug compiler). It reuses the supported first-party example's declarations and
adds nested mutation workflows, with independently authored expected responses
and database states. It deletes inherited `DATABASE_URL`, assigns a fresh SQLite
path by default, or accepts only a dedicated
`JADPO_VALIDATION_AUTH_POLICY_DATABASE_URL` naming a local `jadpo_auth_test_*`
database. The PostgreSQL harness creates and removes an isolated temporary
cluster. Both modes seed both users and their notes. No application database is
accessed. PostgreSQL required the normal local-test sandbox escalation for shared
memory and sockets; the successful run exercised all seven cases without skips.

Cases establish:

- a later denied nested mutation rolls back an earlier authorized write, for
  both supported browser and API transports;
- two authorized nested writes commit, but a subsequent declared business
  rejection rolls back an otherwise authorized write;
- missing and inaccessible second rows produce the same public failure, apart
  from the independent request identifier, and both roll back prior writes;
- a bounded signed credential does not freeze record ownership: changing the
  authoritative owner revokes the old owner's policy access immediately;
- same-principal and conflicting-principal mixed credentials both reject before
  malformed body decoding and leave business state unchanged;
- revoked opaque and revoked fresh-route signed credentials cannot begin a
  mutation;
- 24 interleaved authorized and attacking transactions preserve principal
  isolation, return the appropriate result and leave each owner's committed
  state correct.

There were no reproduced production defects in this bounded package. Initial
runtime harness invocation omitted the host API's explicit clock argument;
correcting that test setup was not a compiler/runtime fix. All final expectations
remain those stated by the accepted contracts.

## Limits

This is not static policy proof or full AUTH-P7/P8 completion. SQLite/PostgreSQL
parity is established only for these seven workflows, not the complete policy
language. No JWT/service provisioning, arbitrary principal profiles, distributed revocation,
concurrent external role-transfer race, tenant-membership join workflow, CSRF
fuzz campaign, or cryptographic implementation audit is claimed.

The suite does not replace the existing supplied-field patch-denial tests or
prove every route/auth/policy/transaction combination. Mutation review of this
package remains separate work. Candidate mutations are committing a child
transaction before its parent, retaining a previous request's principal, caching
role authority in signed credentials, or selecting one credential from an
ambiguous pair. The corresponding assertions above should fail; that claim has
not yet been established by an actual mutation run.

The parent registers the runtime file and owns common validation-gate evidence.

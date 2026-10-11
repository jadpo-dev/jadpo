# Generated runtime acceptance tests

The repository-wide entry point is `python3 tools/verify.py`; it builds all
required examples and runs these suites in separate processes, including
disposable PostgreSQL clusters. See the [validation guide](../validation/README.md).
The commands below are useful when working on an individual suite.

Build the Jadpo seed, then run its Bun HTTP acceptance suite:

```text
cd jadpo
cargo run -p jadpo-cli -- build ../examples/jadpo-seed
cargo run -p jadpo-cli -- build ../examples/outcome-sequencing
cargo run -p jadpo-cli -- build ../examples/persistence-seed
cargo run -p jadpo-cli -- build ../tests/compile/pass/120_entity_dossier_queries.jadpo
cargo run -p jadpo-cli -- build ../tests/compile/pass/lifecycle/170_lifecycle_initial_values.jadpo
cargo run -p jadpo-cli -- build ../examples/authentication-selector
cargo run -p jadpo-cli -- build ../examples/first-party-authentication
cargo run -p jadpo-cli -- build ../examples/temporal
cargo run -p jadpo-cli -- build ../examples/policy-runtime
cargo run -p jadpo-cli -- test ../examples/test-fixtures
cd ..
bun --no-install test tests/runtime/jadpo-seed.test.ts
bun --no-install test tests/runtime/readiness-recovery.test.ts
bun --no-install test tests/runtime/outcome-sequencing.test.ts
bun --no-install test tests/runtime/persistence-seed.test.ts
bun --no-install test tests/runtime/entity-dossier.test.ts
bun --no-install test tests/runtime/entity-lifecycle.test.ts
bun --no-install test tests/runtime/authentication-selector.test.ts
bun --no-install test tests/runtime/first-party-authentication.test.ts
bun --no-install test tests/runtime/golden-migration-authentication.test.ts
bun --no-install test tests/runtime/golden-protected-route.test.ts
bun --no-install test tests/runtime/service-authentication.test.ts
bun --no-install test tests/runtime/service-adapter.test.ts
bun --no-install test tests/runtime/service-fakes.test.ts
bash tests/runtime/first-party-authentication-postgres.sh
bash tests/runtime/postgres.sh entity-dossier
bash tests/runtime/postgres.sh lifecycle
bash tests/runtime/postgres.sh golden-todo-routes
bun --no-install test tests/runtime/temporal.test.ts
bun --no-install test tests/runtime/policy-runtime.test.ts
DATABASE_URL=postgres://postgres@127.0.0.1:5432/postgres \
  bun --no-install test tests/runtime/persistence-postgres.test.ts
```

`--no-install` is mandatory evidence: generated targets and their acceptance
tests must run without package resolution, a dependency manifest, or a
`node_modules` directory.

The golden Todo route suite executes the migrated UserWithTodos endpoint on
SQLite with zero, one and more than 100 active child rows, plus deleted and
foreign rows. Its statement capture separates application reads from
authentication resolution and proves one self-scoped parent lookup plus one
filtered Todo page query. The PostgreSQL mode checks the same bounded ordered
projection and keyset plan. The real HTTP/JWT suite supplies a conflicting
email claim and verifies that the self-service response omits email.

The same SQLite/PostgreSQL golden route suites exercise RM-106: owner-scoped
Todo deletion returns 204 while retaining the row, and subsequent GET, PATCH,
list and repeat-delete requests conceal it. Self-disable requires fresh
browser authentication, returns 204, retains child Todos, rejects a repeated
disable and rejects credential refresh; signed bearer expiry remains bounded
to five minutes. The SQLite suite also races HTTP PATCH against DELETE and
checks that the Todo finishes hidden with no partial lifecycle state. The
separate lifecycle suite covers transition/update races on both adapters.

The persistence-free outcome suite proves generated sequential execution for a
successful call, compatible local recovery, explicit failure mapping, and exact
propagation through real HTTP boundaries.

The entity-dossier suite runs the same handled nested-action savepoint and
same-process concurrency scenarios against SQLite and the verifier's disposable
PostgreSQL cluster. It proves the inner write and authority change record roll
back while the outer transaction commits, then runs 24 same-entity updates
concurrently and checks isolated operation timestamps, contiguous revisions,
and a final row matching the last revision. PostgreSQL also runs 24 updates
from two separate Bun processes and checks the same invariants. A separate
SQLite multi-process probe surfaced `SQLITE_BUSY` at transaction start; TX-001
does not yet choose whether that contention is returned or retried. These are
bounded P10.7 checks; they do not prove general serializable isolation,
deadlock retries, or physical derived-store delivery.

The lifecycle suite creates Users and Todos through generated routes, checks
compiler-owned initial values and rejects forged lifecycle fields. Direct host
calls through the exported persistence client also cannot override lifecycle
initial values or supply a shorter retention cutoff. Purge eligibility uses
the validated application configuration supplied during initialization (or the
environment loaded by the maintenance worker when no explicit initializer is
present), plus a wall-clock-clamped attempt time; direct purge fails closed
until that configuration is bound. The regression suite sets a
different process environment retention value and confirms that the initialized
five-day policy controls direct host purge eligibility.
Todo `status` remains caller-supplied business data and can be updated
independently; logical delete preserves it while setting `deleted_at`. It applies
identity-bounded disable/delete transitions and checks both adapters use the
operation timestamp for lifecycle and generated change fields. Repeated delete
maps to the existing 404 failure, and a nested route-to-action rename stays
concealed after deletion. Ordinary reads hide the disabled row, and a raw hard
delete of that disabled User remains blocked by the retained Todo foreign key.
The generated persistence surface omits hard-delete and lifecycle-field setters.
It races transitions against ordinary updates, checks disabled-owner concealment
through required includes, and proves 501 disabled owners and 100 soft-deleted
Todos are filtered before bounded pages. Retention checks cover the 500-row cap,
final-predicate eligibility, a competing connection changing eligibility while
purge runs, rollback on a retained reference and redacted audit outcomes.

The authentication-selector suite proves that credential inventory happens
before validation, zero/one/multiple and malformed presentations have stable
classifications, an invalid credential cannot hide beside a valid one, bounded
ordinary requests avoid authority lookup, and fresh checks resolve one active
principal without exposing credentials or provider objects.

The service-authentication suite issues a service API key, sends it through a
generated route that narrows `current_principal.service`, and verifies a user
principal receives the generated 403 for that service-only route. The same
suite covers verifier-only persistence, exchange, rotation, revocation,
disabled-service rejection, bounded expiry, and cross-process credential state.

The service-adapter suite builds a checked service-operation fixture behind a
generated HTTP route and directs its compiler-owned adapter to a real loopback
provider listener. It checks the emitted service audit contract, pinned
host/path, verified CA and hostname, bounded credentials and payloads, identical
body/header idempotency keys, typed success and declared failures, stateful
same-key deduplication, a provider acceptance stored before its acknowledgement
is dropped, shared per-operation elapsed/attempt budgets, cancellation, and
secret-safe logs. It is HTTP-adapter evidence only; durable intent, restart,
reconciliation and worker scheduling remain separate acceptance work.

The temporal suite proves strict instant normalisation, explicit London and
New York daylight-saving overlap/gap handling, 23/24/25-hour local-day bounds,
calendar-versus-elapsed arithmetic, explicit-reference friendly formatting,
closed policy options, and generated timezone/locale provenance.

The policy suite proves authoritative direct and membership role resolution,
same-scope and cross-scope concealment, field narrowing, policy predicates
before ordering/pagination and mutation, create preconditions, user and service
application roles, stable regular-user and administrator projections over one
entity, public invoke policy, and next-operation membership revocation against
an isolated SQLite database.

The authored fixture example runs through `jadpo test` itself. It proves a
fixed clock is stable within each callable operation, explicit clock advance is
visible only to the next operation, every test starts from a fresh fixture,
typed configuration is installed and restored, and `secret(...)` is accepted
only at the fixture boundary without becoming an authored runtime function.

The suite uses a real ephemeral TCP listener and covers valid input, semantic
boundary rejection, unknown-field rejection, automatic domain-failure mapping,
internal-context non-disclosure, and secret-safe structured startup failure when
the configured database cannot be reached.

The persistence suites prove the same behavior against a temporary SQLite
database and a caller-supplied fresh PostgreSQL database: invalid boundary
values write no rows, while valid `create Customer` responses match the rows
actually returned from storage. They exercise `query optional` for zero, one,
and duplicate rows; `query required` for typed not-found and successful lookup;
transactional required update/delete for zero, one, and multiple rows; declared
create/update constraint-to-conflict mapping; atomic fixed-shape multi-field
updates; and deliberate SQLite/PostgreSQL failures proving raw driver
exceptions become compiler-owned `PersistenceFault` values at the adapter
boundary. The Account pressure case separately maps a single-field unique
collision and a compound-unique collision to different domain failures while
proving the surrounding multi-field update rolls back. The SQLite suite also
inspects and executes the fresh
schema to prove generated identity, uniqueness, and lookup-index metadata.
It also proves the generated `User`–`Todo` foreign key rejects an orphan, is
indexed, and applies its declared cascade lifecycle. The PostgreSQL suite
contains the same assertions when a live `DATABASE_URL` is supplied; the P10
exit run passed all 22 PostgreSQL cases against Postgres 16.
The relationship route also proves `query many` returns zero-or-more validated
todos in explicit identity order rather than relying on database row order.

The first-party authentication suite exercises real signed/opaque cookie and
bearer adapters, protected HTTP routes, ownership-scoped persistence, expiry,
refresh, revocation, rotation, CSRF (including mutative GET), configuration
failures, request isolation, parser mutations, and audit/OpenAPI agreement.

The first-party suite's earlier 24-case checkpoint passed in both SQLite and
PostgreSQL modes, including generated
immediate-cookie and signed-bearer variants, separate-process restart/revocation,
identity replacement, initialization races and startup-schema failure. The two
startup-failure subprocess cases use SQLite in both runs. The PostgreSQL runner
requires PostgreSQL tools on `PATH`, creates a disposable localhost cluster and
removes it on exit. SQLite mode ignores ambient `DATABASE_URL`; no existing
application database is used. See the [example](../../examples/first-party-authentication/README.md).

The readiness-recovery suite starts a generated service with an unavailable
SQLite path, proves `/health/live` stays 200 while `/health/ready` reports only
the stable `database` status and ordinary traffic receives a generic 503, then
creates the missing path and verifies readiness recovers without restarting.
It also starts the generated first-party-auth application against an unavailable
SQLite path, verifies an authenticated route is gated, then creates the path,
provisions a user/session through a loopback-only test harness and authenticates
the protected route after recovery. A 32-request readiness burst returns ready
with one authentication-schema initialization. This proves startup-time SQLite
recovery for the auth storage path, not recovery from a later database loss or a
hard latency bound. The full verifier also runs the readiness suite against a
disposable PostgreSQL cluster to exercise generated startup and the normal
database readiness probe. A listener-free generated-handler case also exercises
database gating, a 32-request readiness recovery burst with one schema
initialization, authenticated identity reads, and recovery after the SQLite
auth-session table is restored. It provides focused evidence on hosts that
prohibit socket binding, but does not replace the live HTTP or PostgreSQL checks.

The golden migration authentication suite is a bounded compiler/runtime slice:
it builds the migrated target, exercises a protected user create and checks the
browser-origin CSRF boundary. Its service rejection on a user-only route does
not establish service lifecycle correctness. The separate golden protected-route
suite verifies stored owner identity, typed strength and refresh, strict adapter
enum normalization, and no writes on credential/CSRF/configuration rejection.
These two migration suites use SQLite. The separate
`golden-service-credentials.test.ts` suite runs on SQLite and through
`bash tests/runtime/postgres.sh golden-service-credentials`. It covers declared
credential expiry/status/revocation, service disablement, rotation, exchange and
refresh bounds, malformed metadata, identity substitution, non-disclosure and
atomic rollback when either issuance write fails. It also exercises generated
`POST /auth/exchange` on both databases, including real-listener duplicate-header
rejection and failure redaction. SQLite additionally counts principal versus
credential lookups and verifies a fresh generated-process follow-up. Both
trusted revocation APIs are exercised. These suites do not count the 44 frozen
golden cases as executed.

The `golden-auth-http-jwt.test.ts` suite runs the unchanged migrated target behind
an actual localhost HTTP listener. It proves signed-user and pinned-JOSE JWT
creation, stored ownership, typed `primary` identity and zero writes for missing,
malformed or bad-signature credentials. Only issuer discovery/JWKS responses are
controlled by the test; HTTP and signature verification execute normally.

The bounded `golden-todo-routes.test.ts` slice checks owner-scoped stable keyset
pages, composed filters, defaults/bounds and SQLite's composite-index plan. Its
PostgreSQL counterpart runs through `bash tests/runtime/postgres.sh
golden-todo-routes`, seeds 8,000 older rows plus equal-time, deleted and foreign
rows, exercises the route over multiple cursors, and uses `EXPLAIN ANALYZE` to
require the partial descending index without a sort and with at most the
page-plus-one rows emitted after a deep cursor. It verifies the route slice,
not execution of frozen LIST-001/002, which remains RM-109 work.

`delivery-hook-components.test.ts` derives the complete compiler-checked
reminder target, including `target/jobs.ts`, and runs its host-invoked worker
entrypoint. The host supplies a worker credential, separate activation and
delivery profiles, and the operation-time snapshot; generated output installs
no scheduler or profile defaults. SQLite and
`bash tests/runtime/postgres.sh delivery-hook-components` exercise actual create
and supplied/equal/cleared/omitted/ABA patch revisions, source rollback, caught
hook faults, ordinary owner-policy refusal, concurrency and fresh-process recovery.
An ordinary job-free public target additionally checks owning-reference Instant
patch encoding. Test-owned user principals and raw fixture seeds are not worker
authentication, admission, receipt/completion or whole-golden evidence. A separate
native authority component scope uses the actual generated authentication host,
selected direct service-key verifier and declared storage to mint an opaque proof,
then rechecks the selected live credential/service/application membership/role
inside the owning transaction. It covers forged/copied/foreign-storage proofs,
revocation/expiry/verifier/metadata changes, configuration invalidation, wrong role,
expiry during query/row-lock delays and caught native SQL failure savepoint recovery.
Current-issuer controls reject a separate real native factory on the exact same
storage before and after failed initialization/key rotation; they retain current
host success, withdrawal during the native metadata digest and overlapping
initialization where a stale completion cannot publish over a newer success/failure.
SQLite setup checks same-adapter mutation/rollback; PostgreSQL checks row locks
held through owning commit. These are authority components, not assembled worker
selection/fences/dispatch/completion or18 generated traces. Requires the existing
pinned JWT dependency. No public job or execution-profile default is enabled.
Selection/enrollment uses the finished binding and actual generated app validators/
configuration. It tests captured-time predicates, pre-limit owner visibility against
501 earlier ineligible/501 eligible rows, typed due/id continuation, immutable ID/
payload duplicates and equal/ABA revisions, caught native SQL rollback, concurrent
scans, missing revision/corrupt payload refusal and fresh-process recovery. Durable
scheduler ownership and complete worker execution remain open. Independent review
reproduced a PostgreSQL boundary lock-wait paging defect; bounded original tuple
capture plus locked-current revalidation now refuses a changed page before
enrollment. The observed-lock501-row regression covers later/equal-snapshot due
and done-status changes, no partial enrollment and fresh bounded continuation.
Its independent correction review approves the paging fix narrowly.

Native invocation components use an explicit test-owned finite profile and
disposable loopback reference HTTP provider, never real mail or public scheduling.
They check owning-COMMIT-only opaque one-use dispatch, current source/recipient/
role admission and claim/checkpoint, adapter-only outcomes, unknown blocking,
safe no-effect retry, original-revision completion and provider-clock separation.
Native deferred-COMMIT/sent-write faults exercise rollback; actual SQLite COMMIT
followed by injected lost acknowledgement verifies no token despite durable
possible dispatch. Post-cut configuration/role/owner/source withdrawal is not a
new authority cut. The invocation review requires CI-I01, now corrected: completion
captures its own operation time and updates checked generated-change field roles
only with the guarded sent mutation. Creation time, business schedule revision and
already-sent/missing/superseded/stale-fence rows are unchanged. A separate
`delivery-completion-representation.test.ts` uses a checked-source variant with a
renamed generated field and durable cache representation; native change-log failure
rolls receipt/state/sent/timestamp back, then the same genuine outcome completes
without a second mail. No generated code or frozen golden fixture is rewritten.
Run its PostgreSQL counterpart with `bash tests/runtime/postgres.sh
delivery-completion-representation`. Independent correction review approves CI-I01
on its exact pinned source.

Native singleton storage components additionally test UTC tick flooring, coalesced
downtime/overlap, one current activation, bounded immutable staged pages, durable
cursor progress only on fenced finish, rollback and a real fresh generated process
resuming a staged page with a new activation-time snapshot after lease expiry.
Persisted-state decoding refuses15 malformed/null/noncanonical/out-of-range or
incoherent states under the actual row lock, with claim/stage/finish each preserving
the exact durable row on refusal. Portable0001/9999 controls pass; year0000 refuses.
The independent final storage correction review
approves only this scoped correction, including real PostgreSQL late-lock controls;
the original required-changes reports/red probes are preserved.
These trusted native storage seams are not yet the assembled finished-binding
scheduler,18 generated traces or44 golden cases. Component profile values do not
approve the outstanding public profile; public jobs remain disabled.
All four database/variant modes are in the supported gate; inherited component URL/output/variant overrides are
removed by the verifier and direct PG mode accepts only a disposable localhost DB.

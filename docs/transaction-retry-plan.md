# TX-001 — bounded transaction retry contract

**Status:** frozen RM-401 contract, 2026-10-02, following independent
transaction review and correction re-review. Safe retries are enabled by default
as selected by the owner; production lowering and conformance belong to RM-402.

**Planning review, updated 2026-10-02:** the candidate now follows approved
[TIME-D05](time-testing-plan.md#2-approved-v01-decisions): every retry attempt
gets a new `clock.now` snapshot. Nested actions within that attempt share it.
The original fixed-across-attempt wording was inconsistent and is superseded
here; the accepted time contract has not changed. The adapter evidence and
independent transaction review below satisfy the contract freeze prerequisites.

The pre-freeze adapter evidence is an RM-401 bounded proof phase: inspect driver
phase signals and capture representative SQLite busy/rollback and PostgreSQL
abort/ambiguous-commit traces, preserving unknown when evidence is insufficient.
It does not enable production retries. RM-402 follows freeze with production
lowering and the complete conformance matrix. This separates proof preparation
from implementation without removing the evidence gate or making RM-401 wait
for the task that depends on it. See the [planning assessment](work-plans/roadmap-assessment.md).

The [bounded adapter probe](../tests/validation/rm401-phase-probe.ts) and
[saved result](../tests/validation/rm401-phase-probe-results.json) ran with Bun
1.2.20, SQLite via Bun and disposable PostgreSQL 16.3 on 2026-10-02. SQLite
reported structured `SQLITE_BUSY` at failed `BEGIN IMMEDIATE` with no active
transaction; a commit-time `SQLITE_BUSY` left a transaction that a successful
rollback cleared, with the stored value unchanged. PostgreSQL `SQL.begin()`
returned a server error after the callback returned for a deferred foreign-key
rejection (`errno` 23503) and a coordinated serializable write skew (`errno`
40001); the rejected transactions left no rows/updates. A coordinated deadlock
returned `errno` 40P01 with one victim's writes rolled back. Bun's outer
`code` was the generic `ERR_POSTGRES_SERVER_ERROR`, so classification must use
structured server `errno` and phase evidence, never message matching. A local
proxy then forwarded `COMMIT` but dropped the response: the client received
`ERR_POSTGRES_CONNECTION_CLOSED` while a direct query found the row committed.
That case must remain `unknown` to the client and cannot be replayed blindly.
The probe does not implement production retry lowering or prove all network
failure timings; the generated adapter still needs the RM-402 conformance suite.

## Boundary and current gap

The unit of replay is the **outer logical database transaction**, never an
individual statement, nested savepoint or provider call. The compiler must
prove the effect graph repeatable before enabling automatic replay. A database
mutation and its outbox intent may share the transaction; an outbound service
call may not. A failed attempt publishes no change revision or outbox entry.
Every attempt gets fresh transaction state and re-reads authority, policy and
lifecycle guards. The request/delivery identity stays fixed across attempts,
while each attempt gets a new operation-time snapshot under TIME-D05. All
application-visible time calculations and generated timestamp assignments are
recomputed from that snapshot. The elapsed deadline uses a monotonic clock.

### RM-402 implementation checkpoint — 2026-10-02

Generated code now records typed phase/outcome evidence, retries only proven
no-commit transient failures at the outer transaction boundary, uses a shared
three-attempt/one-second retry budget with full jitter, refreshes TIME-D05 per
attempt, and maps exhausted transient failures to a safe 503. Lost commit
acknowledgements remain `outcome_unknown`. SQLite and PostgreSQL tests cover
real contention, nested rollback, commit acknowledgement loss, operation-clock
refresh, retry exhaustion, and process-level PostgreSQL deadlock. The focused
validation suite passes 29 tests on each adapter; full-gate evidence is linked
from the [delivery checkpoint](work-plans/golden-delivery-planning.md).

Cancellation now reaches route transactions. Generated async queries check it
before and after each query; synchronous SQLite calls check before and after
each call. A PostgreSQL test aborts while a trigger is sleeping and proves the
active statement is allowed to finish, after which the transaction rolls back
before commit and does not retry. Bun 1.2.20 exposes `Query.cancel()`, but an
actual cancellation attempt against `pg_sleep` did not interrupt that query.
SQLite's `bun:sqlite` calls are synchronous. The adapter therefore cannot yet
interrupt an active query/call, and cancellation can only be acted on after it
returns, despite Bun's [documented query-cancellation API](https://bun.com/reference/bun/SQL/Query/cancel).
The retry-attempt continuation now propagates the shared monotonic retry-window
deadline into generated async and sync persistence clients. Before another
attempt's operations and after statement completion, the adapter checks the
deadline; SQLite and PostgreSQL tests prove that a statement which returns late
is rolled back and reported as unavailable without another retry. The initial
transaction is still allowed to run when the retry window has elapsed, so this
is a retry budget rather than a whole-request timeout. Bun 1.2.20 cannot
interrupt an active database call here; it may overrun the deadline until the
call returns. A separate request-level deadline is not currently supplied.
Tests now expire the retry window and abort the request after COMMIT starts on
both adapters. When the driver acknowledges that commit, the row remains,
evidence says `committed` with `commitAcknowledged=true`, and the attempt log
records `cancelled=true`. Lost COMMIT acknowledgement remains `outcome_unknown`
under the earlier fault-injection cases.

The frozen `nonrepeatable_effect` case requires compile rejection. The current
conservative effect walk disables replay for unproven call graphs. Jadpo has no
provider-dispatch callable declaration today, so the new compile fixture proves
that a provider call in a mutating action is rejected as `SEM_UNKNOWN_CALLEE`
before execution. Any future provider syntax must add explicit effect metadata
and retain this compile-rejection guarantee. Independent transaction review is
still pending.

### RM-402 retry-deadline continuation — 2026-10-02

The generated adapter preserves a retry deadline through policy, operation-time
and cancellation client views. PostgreSQL callback failure is recorded as a
proven rollback under Bun's documented `sql.begin()` contract; route handling
maps driver faults with proven no-commit evidence (and pre-BEGIN failures) to
the safe unavailable response. Runtime tests inject a transient first attempt,
then make the retry's SQLite synchronous statement or PostgreSQL trigger-backed
statement return after the one-second retry window. Both assert no persisted
row, a stopped retry sequence and safe unavailability. The 31-test persistence
suite passes on SQLite and PostgreSQL 16.3. The [full supported verifier report](../build/validation/20261002T214536-64903/report.json)
passes all 55 checks;
the separate golden and release gates remain open.

The added [compile-fail fixture](../tests/compile/fail/169_transaction_external_provider_dispatch.jadpo)
exercises provider-call rejection while Jadpo has no provider dispatch
declaration. It does not establish the effect semantics of a future declared
provider API. Keep RM-402 partial until operation-deadline semantics and active
call interruption are settled, provider-effect handling is reviewed for any
supported dispatch mechanism, and independent transaction review is recorded.

### RM-402 commit-race continuation — 2026-10-02

SQLite and PostgreSQL 16.3 both now have deterministic tests where a transient
first COMMIT is rolled back, then cancellation and retry-window expiry arrive
after the next COMMIT operation has started. The driver acknowledges the
committed retry, the row remains exactly once, and audit evidence records the
known commit plus cancellation rather than claiming rollback or replaying it.
The focused persistence suites now pass 33 tests on each adapter. This proves
the acknowledged-commit branch; it does not make active database calls
interruptible or supply a separate operation deadline. The fresh full supported
verifier run passes all 55 checks, including nine PostgreSQL modes:
[report](../build/validation/20261002T215540-67673/report.json).

### RM-402 request-deadline and cancellation recheck — 2026-10-03

The generated HTTP entry point passes `Request.signal` into its operation
context, but does not create a request-level monotonic deadline. The transaction
retry helper accepts an optional `deadlineAt`; no current route supplies it.
The enforced one-second bound is therefore the shared retry budget: it limits
retry attempts and waits, but does not bound the first attempt or total request
duration. Do not describe it as a request timeout or invent a default timeout
without an accepted configuration contract.

Cancellation is observed around asynchronous database work. If an active
PostgreSQL statement returns after cancellation, the callback fails and the
transaction rolls back before commit; synchronous SQLite work is observed only
after it returns. If cancellation arrives after COMMIT begins and the adapter
acknowledges the commit, the committed result remains authoritative and audit
records both `commitAcknowledged=true` and `cancelled=true`. The pinned Bun
1.2.20 path still has no demonstrated safe interruption of an active query. Bun
documents a `SQL.Query.cancel()` API, but the earlier `pg_sleep` probe did not
interrupt the live call; a fresh one-off probe on 2026-10-03 could not start a
disposable PostgreSQL server because the host denied `shmget`, so it adds no new
result.

The checked language has no provider-dispatch callable declaration; fixture 169
rejects the undeclared provider call before execution. This preserves the
current fail-closed boundary but does not define effects for future provider
syntax. The full supported verifier passes 58/58 checks
([report](../build/validation/20261003T213248-5311/report.json)). RM-402 remains
partial: request-deadline semantics, supported active-call interruption and
independent transaction review are still open.

### RM-402 cancellation-boundary continuation — 2026-10-04

The fresh supported verifier now passes 61/61 supported checks, including the
SQLite/PostgreSQL retry suites ([report](../build/validation/20261004T015807-88670/report.json)).
Its PostgreSQL cancellation case records 1020.33 ms from aborting an in-flight
statement until the generated request settles; the transaction then rolls back
before commit and does not retry. This is one observed run around the test's
one-second `pg_sleep`, not a service-level response bound. The acknowledged
COMMIT race remains separately covered: PostgreSQL returns the known committed
row after cancellation and deadline arrive during COMMIT.

A new standalone attempt to test `SQL.Query.cancel()` could not start its
disposable PostgreSQL cluster because this shell was denied the required
`shmget` operation. It supplies no new driver-cancellation result. Retain the
previous Bun 1.2.20 probe finding: an attempted `pg_sleep` cancellation did not
interrupt the active call. Do not claim that the generated adapter or Bun's
query cancellation has a proven hard response bound. The one-second retry
budget remains distinct from a request-level deadline, which no route currently
supplies. RM-402 stays partial pending a supported interruption result, any
owner decision needed for request-deadline scope, and independent transaction
review.

### RM-402 request-deadline contract decision — 2026-10-04

The owner selected an explicit deadline per route/operation and no universal
default. The implementation uses an optional `deadline: <duration>` route item;
omitting it leaves the request without a generated route deadline. A declared
deadline is a monotonic execution budget shared by nested calls, database
transactions, retry waits and checked service calls. It starts when the selected
route operation begins. It is distinct from TX-001's one-second retry budget:
the effective budget is the earlier of the route deadline and the retry limit.

Expiry prevents starting another operation or retry and prevents committing a
transaction whose database work returns after the deadline. The runtime must
preserve a commit already acknowledged by the adapter and keep ambiguous commit
outcomes as `OutcomeUnknown`; deadlines never authorize blind replay. No universal
route timeout is inferred. This cooperative budget does not promise a hard HTTP
response bound: Bun 1.2.20 has not demonstrated interruption of an in-flight
PostgreSQL call, and SQLite calls are synchronous, so the handler can settle only
after active work returns. A later supported interruption result may strengthen
that guarantee without changing the no-replay boundary.

## Classification contract

Adapters produce an internal result with `phase`, `database`, `server_code`,
`rollback_proven`, `commit_acknowledged`, and `outcome` (`no_commit`,
`committed`, `unknown`). The code and raw error remain internal. A retry
requires **both** a known `no_commit` outcome and a listed transient class;
an unrecognised driver error does not become retryable by default. Authentic
server errors must be identified from driver fields, not message substrings.

| Evidence | Outcome | Retry |
|---|---|---|
| SQLite `BUSY`/`LOCKED` before `BEGIN IMMEDIATE` succeeds | Known no transaction | Yes |
| SQLite busy/lock during a statement; outer `ROLLBACK` succeeds and proves no commit | Known no commit | Yes |
| SQLite busy/lock at `COMMIT`; transaction remains active and `ROLLBACK` succeeds with proof of no commit | Known no commit | Yes |
| PostgreSQL server `40001` serialization or `40P01` deadlock rejection with confirmed transaction abort, including commit-time server rejection | Known no commit | Yes |
| PostgreSQL connection failure before a transaction or before any mutation, with proof no commit was attempted | Known no commit | Yes, if classified transient |
| Constraint/cardinality, policy, lifecycle, domain or validation rejection | Known no commit if rollback confirmed | No |
| Cancellation or deadline exhaustion | Stop future attempts; preserve actual outcome | No |
| Lost connection/acknowledgement after `COMMIT` may have been sent, or rollback proof fails while an effect/commit remains possible | `OutcomeUnknown` | Never blind retry |
| Failed rollback with independent proof that BEGIN never succeeded or no effect/commit was possible | Known no effect/no commit; classify original operational cause | Only for a listed transient cause under the ordinary gate |
| Any unclassified driver failure after writes without proved rollback | `OutcomeUnknown` | Never blind retry |

For SQLite, `ROLLBACK` returning an error after a `COMMIT` failure is not proof
that commit failed: the commit may have succeeded already. For PostgreSQL,
`postgres.begin()` may hide the failing phase. If the driver cannot provide
the required evidence after a possible effect, classify as unknown and leave the operation available
for reconciliation. Independent no-effect evidence takes precedence over a
failed cleanup: pre-BEGIN, read-only and pre-dispatch failures cannot become
uncertain writes merely because rollback also failed. Never infer success or
no effect from a timeout alone.
The existing `OutcomeUnknownFault` and RM-207 Bun HTTP envelope carry uncertain
outcomes; a background caller must persist and expose them without replay.

## Default budget and observability

There are at most three total attempts, including the first. The retry window
is the smaller of one second and the operation's remaining deadline. For retry
index `k` starting at zero, wait a uniform full-jitter delay in
`[0, min(250 ms, 50 ms × 2^k)]`; never schedule a wait beyond the remaining
window. Release the failed connection/lock before waiting. A deadline checked
only between attempts is insufficient: begin, statements and waits must obey
the shared deadline. Cancellation prevents new attempts and interrupts an
in-flight operation where the adapter can do so safely. Nested calls share
the outer attempt and time budget, so nesting never multiplies retries.

Production workers use independent random sources; tests inject a seeded
random source and monotonic clock. Audit one logical operation with per-attempt
phase, safe failure class, elapsed time, selected delay, rollback proof and
terminal outcome. Do not log SQL values, raw driver messages or credentials.
The audit may record each attempt's operation-time value without claiming the
attempts used one wall-clock instant. A retry that crosses a due-date or expiry
boundary can validly change a domain outcome; the whole action reruns from its
start with fresh authority and time, while preserving its delivery identity.
Exhaustion with a proved no-commit outcome returns a safe operational failure
and internal attempt evidence; an unknown outcome retains its distinct safe
mapping. The numeric limits are starting defaults, not a performance claim.

## Acceptance cases and freeze

[`transaction-retry-v0.1.json`](../tests/assurance/transaction-retry-v0.1.json)
lists the canonical failure-injection cases. RM-402 must run them against both
SQLite and PostgreSQL, including separate processes, nested savepoint rollback,
commit acknowledgement loss, deadline/cancellation and jitter distribution.
It must prove contiguous committed revisions, one outbox intent per committed
logical operation, no duplicate mutation, and a single shared retry budget.
Do not claim conformance from same-process tests or a fake driver alone.

Review must verify that Bun's structured `errno` and the generated adapter's
phase record are enough for each proposed retryable server abort. The saved
probe establishes 40001 after the callback and 40P01 with a confirmed victim
abort on this pinned runtime. Its dropped response case shows that a transport
failure after possible `COMMIT` can hide a committed write; keep such cases
non-retryable. The [independent review and correction re-review](../tests/validation/rm401-rm207-independent-contract-review.json)
approve this contract freeze. Original findings remain in that record; the
review does not claim generated-adapter conformance.

### RM-402 route-deadline implementation checkpoint — 2026-10-04

The compiler now accepts an optional route `deadline:` in positive whole
milliseconds, seconds, minutes or hours. The generated monotonic budget follows
the operation into persistence transactions/retries and checked service calls.
The route budget and the internal one-second retry window remain distinct:
route expiry returns 504 only when the commit outcome is known, while retry-window
exhaustion remains a safe 503. An unknown commit is never translated into a
definite deadline response, and an acknowledged commit remains successful.

The SQLite runtime case deliberately blocks a synchronous insert for about
417 ms against a 200 ms route budget. It proves rollback and a 504 after the
active call returns, not preemption or a hard response bound. The generated
deadline fault uses a stable internal marker across the separate app and
persistence modules.

Current evidence: the core Rust library passes 87 tests; the Rust workspace
passes with only its localhost-bind test skipped; all 215 compile-fixture pairs
pass; the SQLite persistence runtime suite passes 37 tests, including explicit
deadline rollback, retry-window 503, acknowledged-commit success and
unknown-outcome mapping; the service fake suite passes 3 tests. The supported
verifier attempt is recorded at
[`20261004T041018-15968`](../build/validation/20261004T041018-15968/report.json)
and stopped because the sandbox denied localhost binding. The PostgreSQL
deadline suite could not start because this host denied `shmget`. PostgreSQL
deadline evidence, the independent transaction review and active-query
interruption evidence remain open; RM-402 is still partial.

### RM-402 reviewed adapter checkpoint — 2026-10-04

Approved disposable host verification supersedes the historical socket/shared-memory
restriction above; the fresh supported [gate](../build/validation/20261004T092753-91801/report.json)
passes 61/61, including the complete registered PostgreSQL persistence mode.
PostgreSQL transactional statements receive a server-side statement timeout
refreshed from the operation budget; setup/refresh/reset/savepoint faults are
classified, while rollback/release cleanup remains possible after expiry.
Expired known-no-commit inline operations and retry backoff return 504;
retry-window-only exhaustion remains 503, acknowledged COMMIT remains success,
and uncertain COMMIT is neither replayed nor converted into definite timeout.
The independent [correction review](../tests/validation/rm402-independent-correction-review.json)
approves those scoped corrections.

Native SQLite write contention is covered using two explicit test-only shared-cache
connections: exact `SQLITE_LOCKED_SHAREDCACHE` is classified as `sqlite_locked`,
then rollback/retry yields one committed revision. This does not change production
SQLite shared-cache mode or admit unknown extended codes by message matching.
Six deterministic jitter fractions across normal and clipped budgets check actual
unrounded timers separately from rounded telemetry; a fixed monotonic test clock
avoids fractional-clock equality artifacts. This tests mapping/caps, not random
distribution or progressive clock consumption. Injected BUSY/40001 rollback cases
produce contiguous revisions 1–4 with no holes/duplicates; the 40001 revision case
is structured injection, not a native PostgreSQL serialization event. See
[coverage review](../tests/validation/rm402-independent-coverage-review.json) and
[jitter correction approval](../tests/validation/rm402-independent-jitter-correction-review.json).

Whole-request hard bounds remain unproved for pool acquisition, BEGIN,
nontransactional reads, logs and COMMIT; SQLite synchronous calls are cooperative.
Policy-bearing, recursive, unproved and external effect graphs remain fail-closed
for replay eligibility rather than establishing broad safe-default support.
Durable outbox/compiler binding and whole-task conformance remain open. These
limits preserve the owner's cooperative deadline decision; no universal response
latency or exactly-once effect guarantee is claimed.


### RM-402 private authenticated HTTP prerequisite review — 2026-10-06

The [independent native review](../tests/validation/rm402-independent-private-http-review.json)
approves the inert candidate and private conformance slices `f273727`, `564270e`
and `fce6f5d`. Public protected-route admission remains literal false, and explicit
callable policy remains excluded. The ordinary repeatability path preserves its
prior behavior; the private path additionally excludes create/default identity
and unproved transitive effects.

Fresh exact generated bodies are exercised through real HTTP with only the private
admission bit changed: SQLite seven controls/121 assertions and PostgreSQL eight/154
pass. Native aborted transactions retry current valid PATCH/DELETE once; disabled,
revoked or rebound authority refuses subsequent mutation, and a soft-deleted row
stays concealed. Three-attempt exhaustion and the original cumulative one-second
retry budget retain source, schedule and immutable previous intent. The awaited
authority-lock elapsed-budget control is PostgreSQL-only. A checked worker cancels
stale prior intent before claim. All 105 core tests pass; original reviewer setup
errors remain retained. Native HTTP disconnect/unknown COMMIT and whole accepted
RM-402 gates remain open; this does not activate public authenticated retry.

### Current guarded user retry evidence — 2026-10-06

The [88-check integration](../tests/validation/evidence/rm402504601-reviewed-fusion/report.json) includes independently reviewed genuine PostgreSQL 40P01 deadlock and native credential-wait revocation controls, both service forms denied on user-only mutations, and signed/RSA-JWT self-disable lifecycle controls. The [corrected lifecycle review](../tests/validation/evidence/rm402-independent-lifecycle-retry-4de/review.json) passes SQLite 5/78 and PostgreSQL 5/93; deleting the native intent before baseline now refuses all five cases on both adapters. Original empty-baseline and cached-activity observer failures remain in their evidence packets.

The [static user guard review](../tests/validation/evidence/rm402-independent-inert-static-057/review.json) requires a checked positional user argument and selected signed/JWT user validators, retaining full transitive effect exclusions. It grants no public replay in this epoch. The [reviewed next admission plan](../tests/validation/evidence/rm402-protected-retry-admission-plan/actual-public-admission-parent-assessment.json) selects only runtime user authority with the compiler-private captured fresh-authority callback, and requires actual emitted HTTP proofs, unknown-COMMIT controls and separate security review before fusion. Accepted per-route deadlines remain cooperative; whole RM-402 is open.

### RM-402 actual protected HTTP admission review — 2026-10-06

The [independent public admission review](../tests/validation/evidence/rm402-independent-public-admission-9de/review.json) approves source 9de and native tests 008: checked protected PATCH/DELETE actions can replay only for an actual user with the compiler-private captured fresh-authority callback. Full transitive effect exclusions, current policy, original request credentials, shared budget, proven rollback and unknown-COMMIT refusal remain required. The maintained public HTTP proofs directly import unchanged emitted application bytes; private hook inspection remains a separate prerequisite. Native deadlock, revocation, signed/RSA JWT lifecycle, service denial and lost-COMMIT controls passed independently on both applicable adapters. The exact source/test deltas now pass the [92-check combined gate](../tests/validation/evidence/rm402504-query-schema-public-fusion/report.json) alongside the initial temporal schema and range-query gates.

The [accepted-case exit matrix](../tests/validation/evidence/rm402-protected-retry-admission-plan/exit-matrix/inventory.json) retains all 17 frozen IDs and 27 backend entries with native versus injected qualifications. It identifies remaining distinct SQLite statement-boundary cancellation, jitter distribution/progressive clock consumption and joint revision/outbox trace evidence. Its [first tests-only plan assessment](../tests/validation/evidence/rm402-protected-retry-admission-plan/exit-matrix/first-leaf-parent-assessment.json) permits a cooperative native SQLite boundary injection with causal controls. It claims no synchronous preemption, universal hard HTTP timeout or whole-task completion.

### Current required retry trace integration — 2026-10-06

The independently reviewed [seeded original-budget control](../tests/validation/evidence/rm402-protected-retry-admission-plan/exit-matrix/second-leaf-independent-native-review.json) and [joint source/change/outbox control](../tests/validation/evidence/rm402-protected-retry-admission-plan/exit-matrix/third-leaf-independent-native-review.json) are selected as exact additive tests. They retain native rollback acknowledgement before another BEGIN, the original monotonic budget, committed revisions and intent sequences 1–4, immutable prior payloads, and causal omission controls on both adapters. Their native faults are explicitly injected; private create composition grants no public-create replay authority. The fresh current-source [92-check gate](../tests/validation/evidence/rm402-seeded-joint-reviewed-fusion/report.json) passes all checks, with all88 migrated supporting HTTP entries;660 source/tooling pins and72 actual compiler input pins remain unchanged.

The [full accepted-plan assessment](../tests/validation/evidence/rm402-protected-retry-admission-plan/full-contract-exit/report.json) goes beyond the frozen 17-ID matrix. Root independently confirmed its declared nonrepeatable-service refusals using the current preserved compiler: a valid known-service baseline passes, and four direct/transitive atomic or writing forms refuse `SEM_SERVICE_ATOMIC_EFFECT`. The original frozen169 unknown-callee limitation stays unchanged.

The [parent correction](../tests/validation/evidence/rm402-protected-retry-admission-plan/full-contract-exit/parent-current-compiler-refusal-review.json) refuses the author's broader closure-ready verdict: the saved RM-402 plan explicitly requires a concurrent many-client native contention trace covering independent seeded jitter, bounds, cancellation and no retry amplification. Serial seeded samples and two-process contention are useful narrower evidence; together they do not yet establish that trace. A bounded tests-only plan is being prepared. RM-402 remains open even if the current fusion gate passes. Native recovery/serving lifetime limits remain RM-403 work under their existing contracts.

### RM-402 current four-client checkpoint — 2026-10-06

The [independent native contention review](../tests/validation/evidence/rm402-protected-retry-admission-plan/exit-matrix/many-client-contention-independent-review/review.json) closes the earlier required trace gap with four independent native processes, original budgets, cancellation and source/change/outbox conservation on both adapters. The [current96-check gate](../tests/validation/evidence/rm402-startup-duration-contention-reviewed-fusion/report.json) includes these exact controls. PostgreSQL40001 faults are native fixture injections; no naturalSSI/fairness claim. The [truthful audit-label plan](../tests/validation/evidence/rm402-protected-retry-admission-plan/full-contract-exit/retry-audit-correction-plan/parent-plan-assessment.json) remains mandatory before whole-task closure.

### RM-402 accepted implementation complete — 2026-10-06

The [full current closeout](../tests/validation/evidence/rm402-audit-host-ordinary-reviewed-fusion/closure.json)
satisfies the accepted runtime/control and truthful artifact agreement conditions.
All17 frozen case expectations and27 backend entries are preserved; the canonical
assurance status now links the implementation review. The required concurrent
four-client native trace, independently reviewed metadata correction and fresh98/98
source gate complete RM-402. The original cooperative active-statement contract,
conservative effects/fresh authority/original budget and unknown COMMIT refusal
remain unchanged. Native40001 injection is not spontaneous SSI; private create
composition is not public create/provider replay. Native hard lifetime work remains
RM-403. See [history](implementation-history.md#2026-10-06--rm-402-bounded-retry-implementation-and-native-conformance)
for evidence and timing; this closes neither the formal golden nor release gate.

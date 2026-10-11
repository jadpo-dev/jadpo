# DATA-007 — first entity lifecycle contract

**Status:** RM-205 contract frozen, 2026-10-02, after the
independent correction re-review
approved the exact candidate for freeze. The accepted semantic body and 27-case
catalog are byte-pinned by that review (candidate document SHA-256
`663d9bf8a60cc2d5e200c771d18c1205971b420a743a0b78fe5babb102da3215`).
This status/footer update records freeze without changing the reviewed semantics.
No compiler/runtime lowering is claimed; RM-206 must implement and execute the
positive and negative cases.

**Owner decision, 2026-10-02:** retain purge as compiler-owned, narrow
retention-maintenance authority. It may physically remove only rows already
soft-deleted and past their configured retention time, in deterministic batches
of at most 500, with an explicit audit record. It grants no general authored
delete or entity-mutation authority. Scheduling, delivery recovery and leases
remain RM-108 scope. The language/policy review must check that the eventual
lowering enforces these limits; this answer resolves the authority choice but
did not itself freeze the contract; the independent review above supplied that gate.

## Source and candidate declaration

The frozen `examples/golden-todo/app.jadpo` and policy digest remain the
pressure source pinned in [the service plan](service-plan.md#scope-and-source).
The source currently repeats `status == active` and `deleted_at == none` in
actions; these are lifecycle rules, not new permissions. The successor
first-class entity declaration is the following candidate spelling:

```jadpo
entity User {
    status: UserStatus
    disabled_at: Instant?

    lifecycle {
        initial: { status: User.status(UserStatus.active), disabled_at: none }
        visible when status == UserStatus.active
        transition disable {
            from: status == UserStatus.active
            set: { status: User.status(UserStatus.disabled), disabled_at: clock.now }
        }
    }
}

entity Todo {
    deleted_at: Instant?
    updated_at: Instant generated { on: create_or_change }

    lifecycle {
        initial: { deleted_at: none }
        visible when deleted_at == none
        transition delete {
            from: deleted_at == none
            set: { deleted_at: clock.now }
        }
        purge after config.soft_delete_retention from deleted_at
    }
}
```

The snippets show only lifecycle-relevant fields; identity, relationships,
policy and other fields stay in their dossiers. `initial` is compiler-owned
create state, not a client default. The compiler supplies it on every ordinary
create, including trusted provisioning, and rejects caller-supplied values for
owned fields. The field constructors above follow existing nominal conversion;
the candidate does not introduce implicit enum-to-field conversion.
`clock.now` is fixed within one transaction attempt; a proved-safe retry gets
a new snapshot under [TIME-D05](time-testing-plan.md#2-approved-v01-decisions)
and recomputes the full transition. Todo's generated `updated_at` changes with `delete`;
`Todo.status` remains an ordinary business field. User disable retains children.

For this first form, `initial` assigns each lifecycle-owned field exactly once.
`visible when` and `from` are pure same-row predicates using typed equality,
absence and conjunction; they cannot name principals, roles, other entities,
mutable configuration or operation inputs. A transition has a unique name and
a fixed, type-checked `set` over owned fields. Values are closed literals or
variants, `none`, or the logical operation clock; arbitrary effects and
client-supplied expressions are excluded. Every transition must preserve a
consistent state, and the first contract has no restore. Unsupported shapes
fail checking rather than becoming ordinary updates.

An owning action invokes a transition by name on an identity-bounded update:

```jadpo
attempt update required Todo {
    where: id == todo_id
    transition: delete
    missing: TodoNotFound { todo_id: todo_id }
    conflict: TodoMutationConflict
}
```

`transition:` is the sole write marker for lifecycle-owned fields. It excludes
`set:` and `patch:` for those fields; ordinary non-owned updates keep the
existing form. `User.disable` uses `transition: disable` on an identity-bounded
User update. The action retains its typed missing/conflict result and unchanged
HTTP shape. The transition name does not grant permission: `User.disable`
keeps its existing self-scoped named policy decision, while `Todo.delete`
keeps its owner `delete` permission even though SQL performs an update. The
compiler records the logical effect separately from its SQL verb. No application
role, principal or permission is added by this contract. The owner-selected
retention maintenance plane below has its own narrow compiler capability.
RM-106 must encode the
existing self-scoped `User.disable` rule in the migrated entity policy; its
current read-only policy is insufficient for that action. An ordinary hard
delete of Todo fails checking; only the privileged retention purge physically
removes soft-deleted rows.

## Enforcement and failure boundary

For ordinary named queries, direct reads, joins/inverse reads, list/reminder
selection and mutation selection, the compiler conjoins lifecycle visibility
with policy scope and author predicates **before** rows are exposed or limited.
This prevents a visible page from shrinking after hidden rows are discarded.
Nested actions and calls inherit the same graph obligations. Policy,
lifecycle, business predicates and generated-field ownership remain distinct
semantic nodes and audit facts even when SQL combines their guards.

An owning transition adds both visibility and `from` to its conditional
mutation against the authoritative target row. Guard and write share the same
transaction and row/locking plan on SQLite and PostgreSQL. A concurrent loser
receives the existing concealed missing result when its row is no longer
visible; genuine contention or constraint failure uses the action's declared
conflict result. Neither path commits a partial write or discloses another
principal's row. A second Todo delete is `404 todo_not_found` with zero writes.
A second User disable after successful self-disable reaches the existing
authentication `403 user_disabled` boundary; an internal caller already
authenticated for that operation receives its declared concealed missing
result.

Authentication resolution is a distinct compiler-owned authority lookup. It
may inspect only the subject, identity and lifecycle state of a hidden User so
that fresh and refresh checks map a disabled account to the existing
`UserDisabled` failure. This is not an authored query or a way to project
hidden fields. Fresh checks and refresh reject immediately after disablement;
already-issued bounded credentials cease to work within five minutes. Disable
does not cascade to Todo rows. Ordinary reads of a disabled User stay
concealed.

The purge clause sets the earliest eligible time from `deleted_at` plus the
positive configured duration. Only a compiler-owned retention-maintenance
authority may invoke a bounded physical purge. It selects by `(deleted_at asc, id asc)`,
at most 500 rows per batch, rechecks age and deletion state in its transaction,
emits an explicit audit record, and never restores a row. Scheduling and
crash/retry delivery remain RM-108's work; RM-205 fixes the data and authority
boundary. Hard-deleting a User with retained Todos remains restricted by the
existing relationship rule.

### Retention maintenance authority

This contract is the separately recorded [POLICY-D30-M1 addendum](policy-plan.md#19-retention-maintenance-addendum--policy-d30-m1), outside the unchanged historically approved policy section 2. Owner direction selected this authority; the independent correction review accepted its separate provenance and contract.

The owner's narrow purge decision defines a distinct **compiler maintenance
plane**, not an authenticated application job with an implicit administrator
role. POLICY-D30 still applies to every authored job, action, service and internal
application caller. The maintenance plane's sole admitted effect is
`retention_purge(Entity, checked_purge_clause)`; it cannot invoke application
callables or obtain their entity, field, service or external-effect permissions.

An explicit checked `purge after ... from ...` clause activates the generated
worker for that entity. Startup validates the positive retention duration before
constructing an internal capability tied to the entity, lifecycle timestamp,
retention binding and compiler-generated operation. The trusted host starts that
generated worker; no public route, service key, principal variant, source-level
value or authored callable can mint, receive, serialise or invoke its capability.
Removing the clause removes the authority. Tests may drive the generated worker
with the compiler's isolated harness, without exposing a callable to Jadpo code.
Worker scheduling/leases remain RM-108; this contract grants no general purge API.

The maintenance predicate is default-deny and independent of ordinary row
visibility: deletion timestamp is present, the soft-deleted lifecycle state is
consistent, and `deleted_at <= attempt_now - validated_retention`. It admits
only physical deletion of those already-hidden rows, in deterministic
`(deleted_at asc, id asc)` batches of at most 500. Selection and the final delete
recheck this predicate in the same authoritative transaction. Live rows, young
rows, invalid state and inconsistent timestamps cannot be selected or deleted.
Referential constraints still apply; this authority grants neither cascading
deletion nor updates, reads/projections of hidden application fields, external
dispatch or privilege delegation. No `TodoRole.owner` grant is widened.

The semantic/effect and approval artifacts must show this maintenance capability,
its entity/clause and exact predicate/configuration binding separately from
ordinary policy. Changing its scope or retention is a reviewable authority
change, not permission to bypass the approval protocol. Each batch's internal
audit records the operation/entity identity, source revision, retention binding,
attempt clock, selected/committed count, bound of 500 and outcome; row identities
use the existing internal redaction policy and hidden payloads are never emitted.
Failed/rolled-back batches cannot report committed removal. These are required
lowering/audit fixtures, not current runtime evidence.

Joined reminder selection must apply both Todo visibility and required User
visibility before ordering/limiting. A fixture with 501 earlier eligible-looking
Todos owned by disabled Users and a later eligible active-owner Todo must select
the active owner's row without projecting any hidden User or sending mail for
the disabled owners. A direct ordinary read of a disabled User remains concealed;
the narrowly scoped authentication resolver exception does not change this join.

## Fixture and review gate

The [case catalog](../tests/assurance/lifecycle-v0.1.json) is the
reviewer-pinned contract fixture. It names positive declarations, invocation
and output cases, and negative source, visibility, authority and race cases,
with links to frozen obligations. RM-206 is mapping those cases into
compile-pass, compile-fail, formatter, SQLite and PostgreSQL evidence while
checking source, storage, response and audit agreement. Its original bytes and
contract-only status remain unchanged; implementation progress is recorded
below. RM-106 applies the accepted form to the golden migration and proves HTTP
outcomes.

Independent language/policy review checked `initial`, `transition:` and
purge spelling, ownership/visibility, authentication authority, logical `delete`
permission, concealed outcomes, guarded writes and original policy provenance.
Its final disposition is `approved_for_contract_freeze`, resolving R1–R3.
RM-206 owns executable compiler/adapter/audit conformance; the author's
self-review and these 27 contract cases do not satisfy that implementation gate.

## RM-206 implementation checkpoint, 2026-10-03

The initial executable slice extends fixture 170 with lifecycle-owned User
fields and timestamp-only Todo soft deletion, owner-joined reads and a bounded
Todo page. Todo `status` is caller-supplied business data and remains writable
by ordinary updates. Negative fixtures 171–173 and 175–181 cover identity
bounds, restore, direct lifecycle writes, hard delete, caller-owned initial
values, invalid purge declarations, policy inside lifecycle predicates,
transition/manual-write conflicts, patch writes to lifecycle-owned fields, and
authored or forged calls to the compiler-only purge entry point (fixtures 182
and 183).
Formatter goldens cover lifecycle fixtures 170 and 174. The pinned 27-case
catalog has not been rewritten to imply full coverage.

The runtime suite drives User and Todo creation through generated HTTP routes,
checks compiler-owned User active/null and Todo `deleted_at: null` initial values
in storage, accepts a caller-supplied Todo status, and rejects caller attempts
to set lifecycle-owned values. It also checks guarded User disable and Todo delete,
repeated transition concealment, update/transition races, retained child Todos,
hidden User reads through a required Todo owner include, and 100 earlier
soft-deleted Todos before a 100-row visible page. A required-owner page now uses
one joined SQL query whose parent-lifecycle `EXISTS` check and Todo visibility
both occur before ordering and `LIMIT`: 501 earlier Todos owned by disabled
Users are skipped and the later active-owner Todo is returned. Purge tests check
positive retention, eligibility rechecks, deterministic batches of 500 and one,
rollback when a retained Todo is referenced, audit counts/outcomes and redacted
row identities. A competing database connection also changes retention
eligibility during purge; SQLite and PostgreSQL serialize the writers and the
audit counts match the committed outcome. All nine runtime cases pass on both
adapters.

Required includes now return concealed absence when the referenced lifecycle
owner is hidden, while ordinary entities without lifecycle visibility retain
the missing-reference cardinality fault. This fixes the relationship-read
boundary without projecting hidden owner fields. It does not establish the
golden reminder selection or email-delivery behaviour owned by RM-106/RM-108.

The full supported verifier passed 58 checks at
report 2026-10-03T05:12.
Its status still records the full golden application, behavioural acceptance
and other release gates as open. The case-by-case status below distinguishes
fixture/runtime evidence from the remaining application acceptance. Independent
RM-206 implementation review also remains open; RM-206 is not complete.

### Frozen-case evidence map, 2026-10-03

The catalog in [`lifecycle-v0.1.json`](../tests/assurance/lifecycle-v0.1.json)
remains byte-for-byte unchanged. **Covered** means the compiler/runtime slice
exercises the stated lifecycle rule; it does not mean the golden application
acceptance has passed. **Partial** means a concrete part is exercised but an
acceptance condition or integration surface remains. **Open** means no matching
executable evidence exists yet.

| Case | Status | Evidence or remaining condition |
|---|---|---|
| `user_initial` | Covered | Generated `/lifecycle/users` route test checks compiler-owned active/null values and rejects forged lifecycle fields on SQLite and PostgreSQL. |
| `todo_initial` | Covered | Generated `/lifecycle/todos` accepts business `status`, supplies `deleted_at: null`, and rejects a forged `deleted_at`. |
| `disable_user` | Covered | RM-106's authenticated `/users/{user_id}` route returns 204 for self-disable on SQLite/PostgreSQL, sets lifecycle state and retains child Todos. Cross-user disable is concealed. |
| `disable_twice` | Covered | RM-106's repeated fresh DELETE returns 403 `user_disabled`; refresh is denied and no second transition runs. |
| `fresh_auth_after_disable` | Covered | After route-driven disable, fresh access and refresh reject as `UserDisabled`; ordinary signed access expires within five minutes. SQLite and PostgreSQL route suites cover the accepted boundary. |
| `delete_todo` | Covered | RM-106's owner-scoped DELETE returns 204, retains the row, stamps `deleted_at` and `updated_at` together, and subsequent GET/PATCH/list requests conceal it on both adapters. |
| `delete_twice` | Covered | Repeated lifecycle-fixture HTTP delete returns 404 `todo_missing`; the golden migration returns its frozen 404 `todo_not_found`. Both retain the row and business state unchanged. |
| `visible_before_limit` | Covered | 100 earlier rows with non-null `deleted_at` do not consume the 100-row page; generated SQL places `deleted_at IS NULL` before `LIMIT`. |
| `purge_retention` | Covered | Positive retention, no caller cutoff, configured-environment binding, wall-clock-clamped operation time, ordered batches of 500 then 1, final-predicate recheck, FK rollback and audit outcome pass; a competing eligibility change serializes correctly on both adapters. |
| `disable_write_race` | Covered | SQLite/PostgreSQL transition-vs-update test proves the visible row ends disabled with no partial/stale result. |
| `delete_write_race` | Covered | SQLite/PostgreSQL transition-vs-title-update race leaves the row hidden and preserves the winning title and business status. |
| `direct_lifecycle_write` | Covered | Compile-fail cases reject ordinary Todo `deleted_at` set/patch and User status/disabled timestamp writes; route creation also rejects lifecycle fields. |
| `caller_initial_write` | Covered | Compile-fail Todo creation and generated-route attempts cover direct and boundary attempts to set lifecycle-owned state. |
| `restore_transition` | Covered | Compile-fail restore transition is rejected. |
| `unbounded_transition` | Covered | A transition selected by non-identity `name` is rejected; identity-bounded transition is required. |
| `policy_in_lifecycle` | Covered | Principal-dependent visibility is rejected by the lifecycle predicate fixture. |
| `transition_with_manual_set` | Covered | A transition cannot be combined with a manual write to `deleted_at`. |
| `ordinary_hard_delete` | Covered | Physical deletion through ordinary source is rejected for lifecycle-managed Todo. |
| `nested_bypass` | Covered | HTTP rename route calls a wrapper action, which calls the ordinary rename action; after deletion it returns concealed 404 and cannot change the stored row. |
| `policy_lifecycle_separation` | Covered | Generated lifecycle audit test keeps policy, transition, generated-field and guarded SQL nodes distinct with source ranges. |
| `hard_delete_user_restricted` | Partial | Raw deletion of a disabled User with a retained Todo fails on both adapters' FK constraint and preserves the child; public `relationship_conflict` mapping remains absent. |
| `disabled_owner_before_reminder_limit` | Partial | A joined page skips 501 disabled-owner Todos before limit; due-time ordering and no-mail assertions belong to the golden reminder path. |
| `disabled_user_direct_read` | Covered | Direct hidden User query and required Todo-owner include both return concealed absence without projecting the owner. |
| `maintenance_purge_authorized` | Covered | Generated maintenance artifact binds purge to Todo's checked clause, sets `source_callable: false`, and exposes no User worker. |
| `authored_purge_job` | Covered | Compile-fail fixture 182 gives an authored action the compiler-owned `Todo.retention_purge()` call; the source call is unavailable (`SEM_UNKNOWN_CALLEE`). |
| `forged_maintenance_capability` | Covered | Compile-fail fixture 183 constructs and passes a lookalike value to `retention_purge_Todo`; the compiler-owned entry point is unavailable to source (`SEM_UNKNOWN_CALLEE`). |
| `maintenance_guard_and_audit` | Partial | Live, young, expired and FK-restricted rows, 500-row rollback/commit accounting, redacted identities and a competing database connection changing eligibility during purge are tested on both adapters. Locking serializes the update and purge, and the audit matches the committed outcome. The catalog's “inconsistent” lifecycle state is not representable when Todo lifecycle state is only `deleted_at`. |

The current map classifies **24 cases as covered, 3 as partial and none as
open**; these are evidence classifications, not a completion score. The
remaining partial cases are hard-delete mapping, disabled-owner reminder
selection, and one unrepresentable inconsistent purge state. RM-106 route
evidence does not close the source-bound 44-case golden acceptance gate. The
timestamp-only Todo lifecycle contract remains unchanged.

### RM-106 golden lifecycle route checkpoint, 2026-10-03

The migrated User and Todo entities now declare the accepted lifecycle
initial-state, visibility and transition rules. User disable uses a named
operation exception in the User policy so the self role receives `update` only
for `disable_user`; Todo deletion retains the owner's existing `delete`
permission while lowering to a guarded timestamp update. The migration exposes
fresh-authenticated `DELETE /users/{user_id}` and owner-scoped
`DELETE /todos/{todo_id}` routes. Retention maintenance is declared, but job
scheduling remains RM-108.

The golden route suite proves 204/404/403 boundaries, cross-owner concealment,
retained Todo rows, hidden reads and mutations, list exclusion, refresh failure,
five-minute signed credential bounds, and a concurrent HTTP patch/delete
outcome. Both adapters pass the route suite; the full supported verifier passes
58/58 steps, including all SQLite and disposable PostgreSQL integration steps
(report).
The full verifier continues to report the broader golden behavioural and
release gates as open. The independent auth/lifecycle review
approves this scoped route integration with no blocking findings; its two
nonblocking test-strength notes remain recorded there. The implementation and
review runs record 238.45 active minutes total: 224.35 for implementation and
verification in run RM-106-b3d3a86b6de8,
and 14.1 for review in run RM-106-cad27e70d915.
The reviewer model and effort are unknown under the owner's scoped continuation
instruction. The original 1–4h estimate is unchanged.

### Independent implementation review correction checkpoint — 2026-10-03

The initial RM-206 implementation review found two host-import boundary gaps:
raw persistence creation could override compiler-owned lifecycle initial state,
and the exported purge method accepted a caller-selected cutoff. The generated
persistence target now rejects mismatched initial lifecycle fields on direct
create calls. The purge method accepts no cutoff argument; it derives eligibility
from retention captured from its configured binding and clamps the supplied
attempt time to the wall clock, so a future operation-time argument cannot age
rows prematurely. Focused boundary regressions pass on SQLite and PostgreSQL,
and `cargo test -p jadpo-core` passes 85 tests. The full supported verifier passes
58/58 checks at report 2026-10-03T08:33.
The changes and original findings are recorded in
`rm206-independent-implementation-review.json`.
Independent correction re-review was still pending at this checkpoint; it is
resolved in the completion note below.

### RM-206 retention-initialization correction, 2026-10-03

The correction re-review found that the purge retention map was still read from
`Bun.env` when `persistence.ts` loaded, although the trusted first-party
initializer accepts its own environment object. Purge retention is now bound
from the validated application configuration: `initializeApplication` binds
its loaded configuration after authentication initialization succeeds, and the
maintenance worker binds the same configuration it uses for its audit record.
The low-level purge accepts no cutoff, fails closed until retention is bound,
and still caps the operation instant at the wall clock. A regression sets
`Bun.env` to 30 days while binding the initialized five-day value and verifies
that a Todo deleted ten days earlier is purged. All 13 lifecycle runtime tests
pass on SQLite and PostgreSQL, including an uninitialized direct-purge rejection.
`cargo test -p jadpo-core` passes all 85 unit tests plus integration tests. The
generated first-party lifecycle test also checks that initialization binds the
validated configuration. The independent
final correction review approved all three boundary fixes; see
`rm206-independent-correction-review.json`.

### RM-206 completion, 2026-10-03

**Completed task:** RM-206

RM-206 delivered compiler/runtime/audit conformance for the frozen lifecycle
contract: checked transitions and initial state, source rejection of forged
maintenance calls, identity-bounded guarded writes, visibility-before-limit,
bounded transactional retention purge and redacted outcome audit. The compile
corpus verifies 212 fixture pairs; the Jadpo core suite passes 85 unit tests
plus its integration tests; all 13 lifecycle runtime tests pass on SQLite and
PostgreSQL. The independent correction review approved the host create and
purge boundaries plus configuration binding, and a second read-only spot check
found no residual issue under the frozen trusted-host boundary. See the
correction review,
compile corpus run and
[lifecycle runtime suite](../tests/runtime/entity-lifecycle.test.ts).

The remaining seven partial case classifications belong to golden HTTP route
integration in RM-106 and reminder delivery in RM-108. This closeout does not
claim that the golden application or release gates pass; those remain separate
work. The final correction-review timer
`RM-206-f76cdc3c1c2e` records an
unreliable 460.62 active minutes due to an unobserved interval and is excluded
from the reliable task-effort sum. The other 11 measured partial/completion runs
sum to 367.15 active minutes; the original 1–4h forecast is historical, not a
remaining-effort estimate.

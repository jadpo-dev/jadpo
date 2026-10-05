# Golden delivery — planning loop

Planning completed 2026-10-01. The owner subsequently selected implementation
loop option 1 on the same date; execute the original 28-task scope, expanded by
owner decision to include RM-403 (29 tasks). That choice
did not explicitly create a new persistent Goal or authorise delegation. Source basis: working tree at
`1da4f900b0e489796acf2c72651c5681468ccf95`, with substantial concurrent changes.
Revalidate affected source and evidence before implementation; this file is not a lock.

Goal/chat: `01a0f884-59ca-7d10-bcde-75084fd349e9`. The selected scope is RM-213,
RM-101–110, RM-204–207, RM-301–307, RM-401–403, RM-504–505 and RM-601/RM-603:
29 tasks. RM-213 has its own [plan](documentation-hygiene.md) and status row.
Conditional work, hosted CI (RM-604), review UI (RM-602), public documentation
and comparative trials are outside this Goal.

**2026-10-04 earlier continuation (superseded by the checkpoint below):** RM-303 is complete with its
[independent adapter review](../tests/validation/rm303-independent-implementation-review.json)
and [history entry](../implementation-history.md#2026-10-03--rm-303-checked-provider-adapter).
RM-304's typed service fakes now pass the registered authored/runtime coverage
and the full supported gate under the existing `gpt-6-luna/xhigh` batch pin.
Keep RM-304 in progress for the required independent review of the public fixture
syntax and runtime mapping. Continue with RM-305's bounded contract preparation;
the owner has approved its decisions, but do not start RM-306 until the
candidate has independent transaction/effect review.

## Checkpoint

**Current parallel delivery batch, 2026-10-06 (chat
`01a10e31-cf29-7bb1-b0f6-13828aa5256e`):** the owner explicitly resumed the
session's plan: public GitHub baseline, focused delivery queue refresh, one
compiler/runtime writer and an independent harness writer in an isolated
worktree. This supersedes the earlier stop for this selected window. Actual
`gpt-6.1-sol/high` is verified in this chat's turn metadata; coupled runtime and
consequential verifier review set the bounded batch pin. No new Goal is active.
The [roadmap queue](../implementation-roadmap.md#delivery-order--golden-todo-first)
owns remaining order and [current ownership](roadmap-assessment.md#current-parallel-delivery-window--2026-10-06).

RM-306's retained scheduler module is now wired into the actual renderer, with
the transaction-required database clock helper. Selection/enrollment and the
immutable page commit before provider admission/dispatch. Six new generated
boundary tests pass on SQLite and PostgreSQL, including deferred COMMIT refusal,
overlap, malformed retained cursor rejection and a fresh process resuming the
original page/intent. Independent review found ACT-ASSEMBLY-CURSOR-01: resumed
pages bypassed typed continuation validation. The correction validates both
retained cursor and page continuation inside the owning transaction before
commit/effects. [Scoped review](../../tests/validation/rm306-independent-scheduler-assembly-review.json)
approves the pinned correction; [unchanged raw evidence](../../tests/validation/evidence/rm306-scheduler-assembly/manifest.json)
retains the original provider red probe and root's initial fixture assertion
failure. Whole 18-trace worker recovery, renewable leases, public activation and
hard runtime qualification remain open. Component profiles are explicit test
inputs and do not approve the pending golden 60s/40s proposal.

RM-109's separate worktree produced a source-bound 44-ID/two-backend protocol.
It requires variants/follow-ups, rejects missing/foreign/stale evidence and
records all 88 entries as `not_run` while adapters are absent. The independent
[original review](../../tests/validation/rm109-independent-harness-protocol-review.json)
found fractional physical query counts could pass. The
[correction review](../../tests/validation/rm109-independent-harness-protocol-correction-review.json)
approves integer-only counts and generated-cache traversal pruning; 42 combined
Python checks pass. `tools/verify.py` now retains a separate supporting
`golden-cases.json` ledger while `--require-golden` remains blocked. Independent
[integration correction review](../../tests/validation/rm109-independent-verifier-integration-correction-review.json)
approves refusal of pre-existing reports, preserving the
[original stale-output finding](../../tests/validation/rm109-independent-verifier-integration-review.json). Real
application observations and route/OpenAPI/policy/query-audit agreement still
require RM-108/RM-402/RM-403 and the listed prerequisites. No task is closed.

The combined local [supported gate](../../build/validation/20261006T002555-52853/report.json)
passes 65/65: normal SQLite57/512 and PostgreSQL56/518. The first Python
contract stage ran 41 tests before the final helper correction; a fresh focused
42-test run passes the corrected helper. Golden remains compile_failed49, all
88 case/backend entries not_run and release_equivalent:false. Hosted CI on the
final committed source remains required before integration. The broad optional
format check also exposes pre-existing core formatting drift; no unrelated
formatter changes were folded into this reviewed source capture.
RM-306 timing: `RM-306-8123cd2ac864`; harness preparation/review:
`RM-109-196f3f85662f`, `RM-109-90b0c76b7ed8`; root supporting integration:
`RM-109-494554bfa465`. The last timer started late after a rejected category;
the preceding interval is unmeasured and was not backfilled. Original estimates
are retained. Next integrate reviewed branches through protected main, then
select the next bounded scheduler-recovery/application-adapter checkpoint;
retain the unanswered execution/lease decision before golden activation.

### Earlier checkpoints (historical)

**Golden closure batch entry, 2026-10-04 (chat
`01a10337-a83f-72e0-9d14-a52c626704fd`):** the owner selected RM-108, RM-109
and RM-110 with their direct prerequisites. The bounded working window first
resolves RM-304/RM-305 independent reviews and RM-402/RM-403 current-source
verification, then advances eligible durable-job integration toward RM-108.
Actual `gpt-6-sol/high` is user-confirmed by “done” after the model-picker
handoff; retain this batch pin through implementation, verification and
self-review. Coupled transaction/service/job and final-gate reasoning set the
pin. The owner explicitly authorised independent review subagents; reviewers
inherit the same settings and have separate evidence/timing ownership.
No new persistent Goal or publishing is requested. Approved host verification
cleared the former socket/PostgreSQL environment restriction and exposed real
regressions. The nonentity deadline catch is corrected (10/10 focused tests).
RM-304 is complete after independent correction review and a fresh 61/61
supported gate. RM-402's PostgreSQL server statement timeout, cooperative inline
deadline mapping and retry-backoff corrections are independently accepted within
scope. Native SQLite shared-cache write LOCKED, deterministic jitter bounds and
contiguous revision history now pass; the latest completed full supported gate is 61/61
([report](../../build/validation/20261004T095612-6271/report.json)), with the
68-test PostgreSQL persistence suite passing. RM-403's mixed-response test race
and fatal transient SQLite startup contention are corrected and independently
accepted. Its whole-initialization/probe hard bounds remain open. Earlier failed
reports remain historical evidence; host socket/shared-memory restrictions were
cleared by approved disposable verification, not by skipping those gates.
RM-305 is complete after exact-byte semantic review and metadata-only freeze
confirmation. Continue into RM-306's compiler/outbox slice; do not count its
18 contract traces as runtime passes. See [history](../implementation-history.md#2026-10-04--rm-305-durable-delivery-semantic-contract).

RM-306's storage foundation has independent correction approval: enqueue shares
the live source-attempt token and connection; expired clients/views/retry attempts
cannot enqueue, and unsupported sparse arrays reject before SQL. Six generated-host
storage cases pass on both adapters. These are private primitives, not authored
job execution or the 18-case catalog. Private database-time claim/fencing and
possible-dispatch primitives now have independent correction approval, including
fresh allocation time and conservative charges when allocation returns late.
Seven generated-host claim cases pass on SQLite and PostgreSQL. Public typed
bindings, worker execution and outcome persistence remain open. A PostgreSQL
post-startup auth-session outage/recovery regression passes in the seven-case
readiness suite; its separate independent review is approved, and the refreshed
full [gate](../../build/validation/20261004T092753-91801/report.json) passes 61/61.
Lease-renewal and reference-mail receipt/uncertainty storage have independent
approval. Cancellation review found a caught-error atomicity gap; its correction
uses the same nested savepoint binding and passes a real native trigger-failure
regression on both adapters. Independent [correction review](../../tests/validation/rm306-independent-cancellation-correction-review.json)
approves the fix; preserve the original finding. The latest full gate is 61/61 (report above), including
68/68 PostgreSQL persistence cases (616 assertions). Four focused SQLite cancellation
cases pass (49 assertions), and five focused SQLite mail
outcome cases pass (69 assertions). This is trusted-host state evidence, not
provider receipt provenance, public authored-worker authority or 18-case closure.

**Owner decision checkpoint, 2026-10-04:** the owner accepted the narrow
reminder-service role and 3 total scheduled invocations within 1 hour described
under RM-108. These inputs are settled, not implementation evidence. Do not broaden
human-owned policy on the strength of generic private storage approval. Next are
checked public job/event bindings and safe retry/dead-letter state, then the
authenticated reminder worker and golden conformance. Remaining generic work is
still open, not declared completed or universally blocked. All currently verified
storage slices have their required scoped independent review; whole-task/public
worker/provider-origin reviews remain separate future gates.
Retain the confirmed Sol/high pin and current source; no Goal or publisher is active.

**Earlier continuation, 2026-10-04 (before self-disable repair approval):** private no-effect retry history, persisted
retry eligibility and bounded dead-letter metadata pass 11 focused registered
cases (107 SQLite assertions; one PG-only case early-returns). Real PostgreSQL
persistence passes 79/79 (718 assertions); its SQLite-specific whole-abort case
early-returns, as do existing SQLite-only cases. These are host state tests, not
a checked worker or ASYNC conformance.
The first retry-only supported
[gate](../../build/validation/20261004T112820-24028/report.json) passed 61/61,
including 77 PostgreSQL persistence cases. Independent
[review](../../tests/validation/rm306-independent-retry-state-review.json)
then found a P1: failed savepoint recovery left a live outer capability after a
native whole-transaction rollback. The correction poisons/revokes the shared
attempt, guards subsequent authority/outbox/policy operations and prevents a
caught cleanup fault from allowing callback commit. Real SQLite whole-rollback
and real-PG injected cleanup-failure regressions pass; independent
[correction review](../../tests/validation/rm306-independent-retry-state-correction-review.json)
approves the scoped poisoning fix. Preserve the original finding and failed evidence.

The next full [gate](../../build/validation/20261004T113642-29821/report.json)
failed because concurrently added, explicitly unimplemented successor grammar
was counted as current formatter coverage. Its explicit `ebnf-candidate` fence
leaves implemented grammar/coverage unchanged; the targeted coverage test passes.
The following [gate](../../build/validation/20261004T113901-31108/report.json)
passes compiler/formatter checks but exposes a golden self-disable policy
inconsistency under corrected field denial: User's empty status and disabled_at
policies deny the transition despite its named self-update grant. Do not skip
this check or bypass field policy. The independent
[policy review](../../tests/validation/rm108-independent-policy-composition-review.json)
confirms the regression and requires the narrow explicit field-policy repair's
owner approval below. Three authenticated scoped-policy tests pass on SQLite
(23 assertions); PostgreSQL service coverage passes 18/18 (236 assertions).
No golden reminder grant or self-disable policy edit is made. RM-306 and RM-108
remain partial; no completed-task marker is warranted.
Main timing: decision capture `RM-108-a133172ff5db`, policy prerequisite
`RM-108-0f3380f34158`, retry/poison correction `RM-306-2e12ef7d48f1` and final
compiler verification `RM-306-420264ad2afa`; all partial, original forecasts and
confirmed Sol/high retained. Initial new fixture checks failed on nominal sibling
input types and a Rust fixture's prelude-name collision; corrected fixture-only
types/names pass without relaxing the type checker. Final compile verification
passes 215 fixture pairs and the full locked Rust workspace suite passes;
independent reviewer timing/evidence remains separate. These final source checks
do not replace the still-failing golden HTTP/full-gate obligation.

**Earlier continuation decision:** hand off the explicit self-disable field-policy
approval before changing human-owned golden permissions or claiming a passing
golden gate. Independent retry correction/review is complete within its private
scope, and the next checked job stage has a saved plan; whole RM-306/RM-108 and
golden closure are not complete. No Goal is activated or marked blocked/complete.

**Current continuation, 2026-10-04:** the owner's latest “yes” approves the exact
two-field self-disable repair described below. Only `UserRole.self: [update]`
was added to the migrated User's status/disabled_at policies; entity authority,
named operation, field reads and lifecycle ownership remain unchanged. SQLite
golden routes pass 10/10 (116 assertions), including foreign-user rejection,
successful fresh self-disable, retained children and closed authentication.
Disposable PostgreSQL golden integration passes 1/1 (56 assertions), including
self-disable. These are different registered adapter suites, not ten PostgreSQL
route cases. The fresh supported full [gate](../../build/validation/20261004T141712-52403/report.json)
passes 61/61 with the current private retry/poison guards and repaired self-disable
source. Release equivalence remains false. Independent
[correction review](../../tests/validation/rm108-independent-self-disable-correction-review.json)
approves the exact two grants, authenticated SQLite behavior and five negative
ordinary-write/read/unnamed-authority probes; no independent PostgreSQL/full-gate
rerun is claimed. Main repair timing: `RM-108-08c05446f4bd` (partial).
The earlier failed and passing reports remain historical. RM-108 and
RM-306 remain partial. Continue with the saved public scheduled-job stage after
its required language/effect plan review; no new Goal or publishing is requested.

Earlier full supported gate, 2026-10-04: **61/61 checks pass**, including all
registered runtime suites and PostgreSQL modes
([report](../../build/validation/20261004T030423-6299/report.json)).
RM-304's public-fixture/runtime path was source-audited: fixture 170 builds an
authored route plus declared service fake sequences; the registered runtime
suite runs all six callable tests twice, makes `Bun.connect` fail on attempted
egress, checks retry/outcome timing, and scans attempt logs for secret canaries.
Direct adapter probes cover invalid input before fake consumption, malformed
receipt shape and exhausted sequences. This was a self-audit only; RM-304 stays
open pending independent review of the public fixture syntax/runtime mapping.
Release equivalence remains false.
RM-601 now has source-derived v5 artifacts, checked registry/compiler-input provenance,
full-state baseline/intent comparison, lifecycle facts, and the pinned service
contract linked to transitive route-level external-effect evidence; its AP-11
omission-conformance case passes in the 16-test approval-subject suite, while
runtime service conformance stays unproved. Route-effect differences produce
may-call scenarios with execution/input feasibility explicitly unproved. Route
auth-mode changes retain public-caller scope candidates when the effect graph is
unchanged, and scenarios carry before/after route policy obligations with exact
actor subjects. v5 adds deterministic added/removed subject deltas, including
restricted field-read subjects, without claiming role membership or feasibility.
The approval-subject suite passes 16/16; a focused unit test also covers
restricted-field subject deltas. The full `cargo test -p jadpo-core` suite passes
(88 library tests and all integration suites), and the approval CLI pin/text
integration test passes. Role/principal and feasible
counterexample analysis remain open
([test](../../jadpo/crates/core/tests/validation_approval_subject.rs)).
RM-207 has explicit Bun mapping, artifact parity and generated committed-write
acknowledgement-loss injection on SQLite/PostgreSQL. Both remain partial for
provider/job integration and the other stated task gates. Full golden compilation
still reports 53 diagnostics; all 44 frozen behavioral obligations remain
unexecuted. RM-402's focused retry, cancellation/rollback, late-statement
deadline and acknowledged-commit cases pass against SQLite and PostgreSQL 16.3.
The one-second retry budget is not a request-wide timeout; active-call
cancellation and the missing request-deadline source remain explicit in its
latest checkpoint. The latest full run is green for all supported checks.

Previous continuation decision (retained for provenance): the artifact/shared-boundary implementation batch
has reached its current supported-language checkpoint. Next eligible work is
**RM-402**, implementing the independently frozen retry contract and generated
phase/outcome evidence. Retain its existing `gpt-6-luna` Extra High (`xhigh`)
recommendation: the contract and phase probe now bound this implementation batch.
Current actual settings at this checkpoint were verified `gpt-6-astra` High.
That was a new batch-entry mismatch toward unnecessary capability, so RM-402
was deferred until the owner switched models. No Goal was created.

**Resume, 2026-10-02:** the active setting now verifies as `gpt-6-luna/xhigh`,
matching the RM-402 batch pin; implementation began under that pin. The owner
selected compiler-owned narrow retention-maintenance authority for RM-205:
eligible already-soft-deleted rows only, deterministic batches of at most 500,
explicit audit, and no general authored deletion authority. Record this in the
[lifecycle candidate](../lifecycle-plan.md); independent language/policy review
and join-visibility fixtures still gate its freeze.

**RM-402 first partial checkpoint, 2026-10-02:** the verified
`gpt-6-luna/xhigh` batch implemented phase-aware outer retries, safe exhaustion
mapping, two-adapter contention/rollback/commit-uncertainty evidence, retry clock
refresh and cancellation checks. The focused persistence suite passes 29 tests
on SQLite and 29 on PostgreSQL 16.3. An active PostgreSQL statement is not
interrupted by Bun 1.2.20 `Query.cancel()`; runtime checks now prevent later
statements/commit after an abort once the current statement returns. The
accepted in-flight deadline guarantee, provider-effect compile rejection and
independent transaction review remain open, so RM-402 is partial. See the
[implementation checkpoint](../transaction-retry-plan.md#rm-402-implementation-checkpoint--2026-10-02)
and [timing run](../task-timing/runs/RM-402-5a74fbbf545b.jsonl).

**RM-402 retry-deadline continuation, 2026-10-02:** the generated adapter now
preserves deadlines through transaction client views, checks retry-attempt
deadlines around database work, proves rollback when PostgreSQL callback work
fails, and maps proven no-commit driver outcomes to a safe unavailable response.
Injected late statements roll back without another retry on both adapters; each
focused suite passes 31 tests. Compile fixture 169 confirms the current compiler
rejects undeclared provider dispatch inside a mutating action. This does not
define effects for future provider syntax. The full supported verifier passes
55/55 checks ([report](../../build/validation/20261002T214536-64903/report.json));
request-level deadline, active-call interruption, provider-effect design and
independent transaction review remain open. See the updated
[retry checkpoint](../transaction-retry-plan.md#rm-402-retry-deadline-continuation--2026-10-02)
and resumed [timing run](../task-timing/runs/RM-402-790472c2aed4.jsonl).

**RM-402 commit-race continuation, 2026-10-02:** both adapter suites now pass
33 cases. New SQLite and PostgreSQL tests expire the retry window and cancel
after COMMIT starts; an acknowledged commit remains stored, is not replayed, and
logs `cancelled: true`. The fresh full supported gate passes 55/55 checks
([report](../../build/validation/20261002T215540-67673/report.json)).
Operation-level deadline, active-call interruption, future provider-effect
metadata and independent transaction review remain open.

**RM-402 request-deadline recheck, 2026-10-03:** source review confirms that
the generated HTTP entry point propagates `Request.signal` but supplies no
request-level monotonic deadline. The one-second limit therefore remains a
retry budget, not a bound on the first attempt or total request duration.
Cancellation remains cooperative: an active PostgreSQL statement may return
before the abort is observed and rolled back; SQLite calls are synchronous; an
acknowledged commit is preserved if cancellation arrives after COMMIT starts.
The current compiler also rejects undeclared provider dispatch, so there is no
supported external call inside a mutating action to classify yet. The supported
gate passes 58/58 checks, including the two-adapter persistence suite
([report](../../build/validation/20261003T213248-5311/report.json)); the golden
and release-equivalence gates remain open. A one-off fresh PostgreSQL probe for
the documented query-cancel API could not start in this sandbox because the
host denied `shmget`; it produced no new adapter evidence. RM-402 stays partial
pending a settled request-deadline contract, any safe adapter interruption
evidence, and independent transaction review. See the
[transaction checkpoint](../transaction-retry-plan.md#rm-402-request-deadline-and-cancellation-recheck--2026-10-03)
and current [timing run](../task-timing/runs/RM-402-42054bbc2878.jsonl).

**2026-10-02 routing handoff:** RM-402 continuation `RM-402-790472c2aed4`
ended partial after 25.38 active minutes; both focused adapter suites pass 33
tests and the full supported gate passes 55/55. The next saved RM-601/RM-207
implementation batch pins `gpt-6-astra/high` for its coupled semantic/security
work. This chat remains on `gpt-6-luna/xhigh`, so that batch is deferred until
the active setting verifies as `gpt-6-astra/high` or the owner gives a
batch-scoped override. No RM-601/RM-207 timer has been opened.

**2026-10-02 entry recheck:** this chat's latest turn-context metadata now
reports `gpt-6-astra/xhigh`. The model matches, but Extra High effort exceeds
the saved `gpt-6-astra/high` batch pin. The batch remains model deferred until
High effort is verified or the owner explicitly overrides for this batch. No
substantive implementation or task timer started during this entry check.

**2026-10-02 verified resume:** latest current-chat metadata reports
`gpt-6-astra/high`, matching the saved RM-601/RM-207 implementation and
verification batch. Run `RM-601-2315b5a8f25f` resumes RM-601 with this pin.
The current slice binds the validated schema registry and build-time compiler
source-input manifest, adds a full-state baseline pin, and keeps unsupported
language effects and independent review explicit.

**RM-601 provenance checkpoint:** schema v3 now binds the validated registry's
captured bytes/facts, records build-time compiler source inputs, and exposes
`--expected-before-state` for a complete baseline pin. Registry-only mapping
changes invalidate the pin and appear as decisions. Authentication details are
structured JSON. Twelve focused approval tests and the CLI baseline test pass;
the full supported gate passes 55/55
([report](../../build/validation/20261002T225252-86027/report.json)).
This remains partial: service/lifecycle/job facts await their implementations,
external-effect AP-11 and independent artifact review remain pending, and the
compiler manifest is not trusted build attestation. Continue the same pinned
batch with RM-207's unclassified read-failure boundary.

**RM-207 read-boundary checkpoint:** the generated Bun boundary maps listed
structured availability causes to `read_unavailable` only for checked query-only
paths with simple arguments, after authentication. Runtime, route inventory,
failure audit and OpenAPI agree. Native SQLite lock and PostgreSQL dropped read
response cases preserve the row and do not replay. Negative cases retain
contained defects, cancellation ambiguity and unknown outcomes. Both adapter
suites pass 35 tests (208 SQLite / 211 PostgreSQL assertions); the full supported
gate passes 55/55
([report](../../build/validation/20261002T230353-90661/report.json)).
The first test attempt targeted a client view unused by public queries; correcting
the hook to the actual root query method made the intended injection executable.
An initial compile/import and query freshness fixture error were also corrected.
The same integration pass fixed RM-601's absent-file build watcher: an unchanged
Cargo build now reuses output (observed 0.06s), rather than recompiling core.
Run `RM-207-75da0cfc92a0` includes this small provenance integration correction.
No task-completion claim: deadline-aware timeout mapping, provider/job boundaries
and independent runtime/artifact review remain open.

**Current continuation / next batch:** the RM-601/RM-207 supported-language
implementation batch ends at the above verified checkpoint. Their remaining
provider, lifecycle and job integration depends on the contracts/implementations
still open in RM-205/206 and RM-301–307; independent reviews remain separately
pending. Select **RM-205 and RM-301 contract corrections and re-review through
freeze** as the next bounded batch, using the saved independent findings and
owner decisions. Pin `gpt-6-sol/high`, the existing recommendation for these
bounded language/policy contract corrections; do not start lifecycle/service
lowering as part of that batch. The current verified `gpt-6-astra/high` is an
unnecessary-capability mismatch at this new boundary, so the next batch is
model deferred until Sol High is verified or a batch-scoped override is given.
Keep that pin across both contracts and their verification/re-review handoff.
No Goal, new reviewer delegation or publishing has been started.

**Next-batch entry recheck:** latest current-chat metadata reports
`gpt-6.1-sol/high`, rather than the selected `gpt-6-sol/high`. The RM-205/RM-301
contract batch remains model deferred until the exact selected setting is
verified or the owner explicitly authorises `gpt-6.1-sol/high` for this batch.
Only entry/context inspection occurred; no task timer or contract edits started.

**Owner-scoped routing override:** the owner explicitly selected
`gpt-6.1-sol/high` for the RM-205/RM-301 contract batch. Retain that setting
through both corrections, consistency checks and review handoffs. Pricing was
the owner's stated reason; no pricing measurement is claimed. Run
`RM-205-090cf4e6fc8e` starts after entry inspection; a rejected `decision`
category invocation created no timer and was corrected to `cross-cutting`.


RM-401 is complete: phase evidence and independent correction re-review permit
contract freeze. RM-207’s shared boundary-contract milestone is approved; its
runtime integration remains open. RM-205 is complete: the narrow purge authority, joined visibility fixtures and
separate policy-addendum provenance passed independent correction review. RM-301 is complete: the independently reviewed successor freezes durable identity,
schedule guards, uncertainty, secret/import checks and receipt-time disposition.


Current planning assessment: [all 89 open tasks](roadmap-assessment.md) have been assessed using three authorised workers on Astra High. The refinements below supersede stale next-step/model/dependency statements in historical checkpoints. RM-102 is complete; this assessment starts no implementation loop.

Latest bounded delivery: RM-102, RM-103, RM-104, RM-204, RM-105 and RM-107 are complete, with
[RM-104 exchange evidence](../implementation-history.md#2026-10-02--rm-104-generated-service-credential-exchange)
and [RM-204 route-input evidence](../implementation-history.md#2026-10-02--rm-204-typed-route-query-and-header-bindings),
plus [RM-105 keyset-page evidence](../implementation-history.md#2026-10-02--rm-105-filtered-keyset-list)
and [RM-107 UserWithTodos evidence](../implementation-history.md#2026-10-02--rm-107-self-scoped-user-todo-route).
The owner resolved RM-107's email projection conflict; independent policy review
and the full golden acceptance gate remain open.

2026-10-02 Sol High batch checkpoint: user-confirmed `gpt-6-sol` High after the
model-switch handoff; exact host model/effort metadata was not exposed in this
turn. RM-205's lifecycle candidate now has 21 mapped contract cases and awaits
independent language/policy review. RM-301's source provenance was corrected
after the RM-107 email revision, and its rescheduled-reminder identity question
has been asked once. RM-401 now has a TIME-D05-aligned candidate and saved
SQLite/PostgreSQL phase evidence, pending independent transaction review.
RM-207 has a shared boundary-contract matrix, pending separate review and
runtime implementation. No task was closed by this batch. The next independent
implementation batch is RM-601's semantic approval artifact plus RM-207's
remaining cross-boundary work once its review gate is met; its hardest semantic
and authority work recommends `gpt-6-astra` High. Current Sol High therefore
defers that next batch until a verified switch or explicit scoped override.
If the owner authorises separate contract reviewers, their own contexts set
their review pins and can proceed independently of the implementation switch.

2026-10-02 resumed batch: current-chat turn metadata verifies `gpt-6-astra`
High for RM-601 and RM-207 implementation/verification. The owner answered
“yes and yes”: a changed reminder schedule gets a new durable delivery intent,
retained across retries, and independent contract sub-agents are authorised for
RM-205, RM-401/RM-207 and RM-301. Those bounded review contexts use Sol High.
The new reminder direction authorises a reviewed successor to the frozen
Todo.id contract; the predecessor and its evidence remain preserved. Current
implementation actor is `01a0f95e-c47a-78d0-b173-6ef78949a7e9`; older timer actors
are retained as recorded, not rewritten.

RM-213 is complete; its audit evidence is in the linked plan and history.
Reviewed windows: RM-101–104; RM-204/105/205/206; RM-106/107/301/302;
RM-303–306; RM-307/108/207/401; RM-402/601/603/504; RM-505/109/110.
The table distinguishes adequate plans from unfinished decision packages and
execution prerequisites. No task is completed by appearing here.

Owner decision, 2026-10-01: **plan a required `BROWSER_ORIGIN` setting**, bind it
to browser authentication and preserve the accepted origin/CSRF requirements.
Record an authorised contract revision during RM-103; preserve the prior frozen
contract and its digest. No production origin value is needed for planning.

Owner answers, 2026-10-01 (supersede the earlier pending options):

| Task | Accepted planning direction |
|---|---|
| RM-204 / RM-105 | URL-encoded JSON cursor; efficient pagination across supported databases. This is an HTTP representation, not a database layout. |
| RM-205 / RM-206 | Entity-owned lifecycle rules enforced automatically. |
| RM-207 | Simple explicit uncertain-write response; preserve uncertainty and prevent unsafe retries. |
| RM-301 / RM-303 | Local HTTP reference mail provider first; no production-provider compatibility claim. |
| RM-305–307 | Database-agnostic durable-job contract, with SQLite/PostgreSQL adapters first; no Cloudflare-specific or other hosted queue dependency. |
| RM-401 / RM-402 | Safe retries enabled by default, bounded exponential backoff plus jitter; supersedes the proposed opt-in default. |

Design principle: the compiler/runtime should own routine architectural choices
and safe resilience defaults so application-writing LLMs need fewer decisions.
Record an exception only where domain intent or unrepeatable effects make a
universal default unsafe. The owner answers settle the planning directions;
concrete syntax, adapter contracts and required review/freeze evidence remain
work within their named roadmap tasks, not completed implementation.

The original 28-task planning pass was completed before implementation exposed
the additional blockers below. RM-101 and RM-213 are complete. RM-102 now has a
refined adapter plan. The owner subsequently added RM-403 to cover deployment
readiness and settled the GF-032 boundary contract; implementation proceeds with
the expanded 29-task scope.
Original estimates and all execution/review gates remain intact. Ordinary checks
apply to mapping/configuration work; reassess verify-loop if runtime evidence
reveals a sustained reproduction or verifier problem.

Timing: `RM-213-eca999414c8d` (1.66m),
`META-GOLDEN-PLANNING-576f603330bb` (10.43m),
`META-GOLDEN-DECISIONS-842f8f97ca5e` (1.54m), and
`META-GOLDEN-DECISION-ANSWERS-4cf3c8970edb` (see recorder).
Actual model/effort unknown; selected settings unchanged. Shared META time is
planning overhead, not a completed implementation sample for every task.
No product tests or experiments have been run during this planning loop.

Completion audit, 2026-10-01: all original 28 scoped IDs have explicit planned status and
proportional approaches, acceptance checks, dependency/review gates, model guidance
and source context. Numeric estimates match the roadmap; RM-213 remains honestly
unestimated. Dependencies resolve within scope or completed history, local links
and new anchors resolve, and `git diff --check` passes. All seven owner decisions
(browser origin plus the six answers above) are recorded. No implementation task
completion marker was added. Remaining grammar/freeze, coding and evidence work
belongs to the planned roadmap tasks; the planning deliverable is complete.

Implementation findings, 2026-10-01: all 44 cases are mapped, with original
candidate diagnostics/digests retained. The owner included RM-403 for deployment
readiness (CONFIG-003) and selected preservation of the frozen 422 input-validation
expectations under GF-032; malformed JSON remains 400. Service subject is
UUID-backed while the current target requires Text, an additional RM-102 adapter
gap. No runtime case is closed.

Implementation validation: [full report](../../build/validation/20261001T210735-10167/report.json)
passes all 47 supported steps, including seven PostgreSQL modes. Status remains
`supported_checks_passed_with_open_gates`: the frozen source still has 57
diagnostics and all 44 behavioural cases are unexecuted. Independent review is
pending. Resume at RM-102's credential/service binding; RM-403 remains queued
behind RM-303.

Model-routing checkpoint, 2026-10-01: inspected the remaining 27 scoped tasks.
The roadmap column owns their next stage/settings and separate blocker notes.
RM-102's authority refinement is saved below. The current implementation slice
uses gpt-5.6-luna xhigh, which is suitable for this bounded work. Keep Sol High
for an independent architecture/security review; do not claim that review from
this chat. This is not task or scope completion. Recheck actual settings on
resume and do not start dependent work merely because its model matches. No new
Goal or delegation is authorised.

Implementation timing checkpoints on the resumed chat: RM-102
`RM-102-be6d34c34d86` (0.20m active, partial); RM-103
`RM-103-9cccb14075a8` (0.44m active, 0.37m verification, partial); RM-207
`RM-207-5d1dc542edd8` (6.08m active, partial); RM-601
`RM-601-0b7d7db3dbf2` (4.00m active, partial) and
`RM-601-9b37b75995c5` (0.82m active, partial). These are measured slices, not
whole-task completion times; remaining work stays visible in each task's gap.

Sol High checkpoint, 2026-10-01: actual selected settings were verified as
`gpt-6-sol` High in this chat's turn metadata. Prepared candidate SERVICE-001
([RM-301](../service-plan.md), pinned import and cases), TX-001
([RM-401](../transaction-retry-plan.md), phase-aware retry cases), route-input
RM-204 cases and DATA-007 [RM-205](../lifecycle-plan.md) lifecycle cases.
RM-207 self-review found that its passing unknown-outcome HTTP test is a
synthetic read-path injection, not an ambiguous commit; RM-601 self-review
found hard-coded empty decision/effect lists in the partial subject scaffold.
The protocol and work-plan wording now disclose these gaps. All six slices
remain partial: RM-301 `RM-301-dc3d36365b34`, RM-401
`RM-401-60951e3d329c`, RM-204 `RM-204-8c4ec42af98a`, RM-205
`RM-205-7921864e17c8`, RM-207 `RM-207-7e7ce3bb647e` and RM-601
`RM-601-d75f8a054700`. No completed-task marker was added.

The next dependency-ready implementation group is RM-102/RM-103 integration,
then RM-207 and RM-601 on Luna Extra High. Their source and checks must be
revalidated on resume. Contract and security reviews remain pending and cannot
be counted as independent work from this chat. RM-301/RM-401/RM-205 are
candidate packages, not approved prerequisites for their downstream tasks.

Parallel batch authorised in chat `01a0f95e-c47a-78d0-b173-6ef78949a7e9` after
this chat stopped: Astra High (verified turn metadata), one compiler/runtime
writer, an independent authentication reviewer and a separate test author.
This is a bounded RM-102/103 integration batch, not a new Goal or an expansion
to the full roadmap. The typed-strength fix and protected user/browser tests
are recorded under RM-102 below. A subsequent owner-authorised batch implements
the explicit credential binding with compiler, test and review agents on the
same verified Astra High settings; its evidence follows the original checkpoint.

## Task states

| Task ID | Planning depth and reason | Dependency evidence / blocker | Planning state | Execution state |
|---|---|---|---|---|
| RM-101 | Short; preserve 44 frozen obligations across migration | Mapping and provenance verified; see history | planned | done |
| RM-102 | Short; selected auth shapes exceeded target support | Selected adapters, HTTP/SQLite/PostgreSQL and independent auth/language review evidenced in history | planned | done |
| RM-103 | Direct closure audit | Origin revision, protected-browser positives/negatives, generated startup ordering and independent scoped review recorded in history | planned | complete |
| RM-104 | Short; runtime primitive exists, authored integration missing | Generated SQLite/PostgreSQL HTTP evidence and independent runtime/security review recorded in history | planned | complete |
| RM-105 | Short; checked keyset-page lowering and both-adapter plan evidence | RM-101, RM-204, RM-102; accepted URL-encoded JSON cursor and supported-database pagination | planned | complete |
| RM-107 | Short; bounded route implemented and both database adapters exercised | RM-101/RM-102 complete; owner resolved email output while preserving policy | planned | done |
| RM-108 | Decision-dependent composition; scoped owner choices accepted | RM-105–107 and RM-304 complete; RM-307 unfinished. Owner accepted 3 scheduled invocations / 1 hour, narrow reminder-service authority and exact self-disable repair. New input: golden-only worker execution/lease profile (60s/40s proposed below). Fresh full gate passes 61/61; self-disable correction independently approved. Checked bindings/admission/completion remain | needs input | in_progress |
| RM-109 | Short; executable golden gate must be built | Source-bound 44-ID/two-backend protocol and supporting verifier inventory independently reviewed; all 88 cases not_run. Real adapters and RM-103–108/RM-402/RM-403 acceptance gates remain | planned | in_progress |
| RM-110 | Short; integration evidence, not another feature | RM-109, RM-504, RM-505, RM-603 | planned | queued |
| RM-204 | Decision; public query/header decoding contract | Compiler/runtime/OpenAPI/formatter agreement and independent runtime/security review recorded in history | planned | complete |
| RM-205 | Decision; separate lifecycle from authorisation | [Frozen entity-owned contract and 27 cases](../lifecycle-plan.md); independent correction review approved R1–R3 | planned | complete |
| RM-206 | Short after representation decision | Frozen contract; 212 compile fixture pairs, 85 core unit tests plus integration tests, 13 lifecycle runtime tests on each adapter; independent correction review approved scoped fixes | planned | done |
| RM-207 | Decision; unknown effects cannot be blindly retried | Owner selected simple uncertainty mapping; database route/OpenAPI/audit parity and reference-provider accepted/lost-ack response are evidenced; durable job persistence/restart/reconciliation and independent runtime review remain | planned | in_progress |
| RM-301 | Decision; SERVICE-001 frozen | [Contract, pinned snapshot and 30-case successor catalog](../service-plan.md); independent correction review supports freeze | planned | complete |
| RM-302 | Checked declaration/import compiler slice | Closed request/receipt schemas including nominal intent identity, secret configuration enforcement, supported imported-schema validation and project-root lookup passed correction review and the full supported gate ([history](../implementation-history.md#2026-10-03--rm-302-checked-service-contracts)) | planned | complete |
| RM-303 | Decision-dependent provider adapter | [Generated HTTP adapter, 19 tests/90 assertions, 59/59 supported checks and approved independent review](../implementation-history.md#2026-10-03--rm-303-checked-provider-adapter) | planned | complete |
| RM-304 | Short after service contract | [History](../implementation-history.md#2026-10-04--rm-304-checked-authored-service-fakes); independent correction review and fresh supported gate pass; malformed host-map seams excluded | planned | complete |
| RM-305 | Decision; independently reviewed ASYNC-001 semantic freeze complete | [History](../implementation-history.md#2026-10-04--rm-305-durable-delivery-semantic-contract); 18 traces remain contract-only, with implementation in RM-306/RM-307 | planned | complete |
| RM-306 | Decision-dependent outbox/compiler subsystem | Static/read/hook/current-issuer/paging, CI-I01 completion and ACT-STORAGE-TIME-01 storage corrections independently approved. Final65/65 supported gate: normal SQLite51/442 and PG50/448; renamed representation52/458 and51/464. Owner resumed the parallel plan, actual6.1-sol/high; closed native scheduler assembly now passes6 focused tests per adapter with independent correction review. Combined supported gate65/65 passes;18 traces/profile and public execution remain open. [Resume point and limits](#rm-301307108--services-durable-jobs-and-reminders) | planned | in_progress |
| RM-307 | Decision-dependent worker subsystem | RM-306, RM-304 | planned | queued |
| RM-401 | Decision; bounded retries enabled by default | Contract frozen after phase probe and independent correction re-review; [completion evidence](../implementation-history.md#2026-10-02--rm-401-bounded-retry-contract) | planned | complete |
| RM-402 | Short after retry/failure decisions | RM-401, reviewed RM-207 boundary-contract milestone; scoped corrections/native LOCKED/jitter/revision review approved; fresh supported gate61/61; [current TX checkpoint and limits](../transaction-retry-plan.md#rm-402-reviewed-adapter-checkpoint--2026-10-04) | planned | in_progress |
| RM-403 | Short staged; CONFIG-P5 separates liveness and bounded dependency readiness | Scoped recovery/startup-lock, PostgreSQL auth-session, real connection-loss/32-way recovery and asynchronous response-deadline evidence independently approved. Response controller passes six deterministic groups, ten independent emitted-module probes and14-case live readiness. [CONFIG-P5](../configuration-plan.md#config-p5--liveness-and-readiness) retains synchronous/native/startup/platform gates; mail advisory/unprobed; actual6.1-sol/high | planned | in_progress |
| RM-504 | Short; extend live evidence to claimed behaviours | RM-501 complete; RM-102, RM-402 unfinished | planned | queued |
| RM-505 | Short; hostile cases from settled contracts | RM-501 complete; RM-106, RM-304, RM-307, RM-402 | planned | queued |
| RM-601 | Short; versioned review artifact with exact provenance | Scoped job, static role/member, principal, actor-route, field/output and corrected generated-byte provenance reviews approved. Field48/48+4 internal; byte pins7 public+5 helper and fresh supported61/61. [Canonical audit and checkpoint](#rm-601603--review-artifacts-and-local-attestation-validation) retain full field-flow, worker/emission, feasible counterexample/provenance and whole artifact-review gates; owner selected6.1-sol/high | planned | in_progress |
| RM-603 | Short; local verification boundary, release stays gated | RM-601; protected issuer belongs to RM-604 | planned | queued |

## Common delivery rules

A staged dependent plan can be adequate for handoff while execution is queued.
Replan its source shapes if an upstream accepted contract differs from the
assumptions below; these outlines never approve the missing contract.

Original estimates below are unchanged agent-session forecasts from the
[roadmap](../implementation-roadmap.md), not elapsed deadlines. M = 0.5–2h,
L = 1–4h, N = 2–8h provisional. N work must be split and re-estimated after its
first vertical slice; human decision/review waits are additional and unknown.
No new implementation timings have been measured here.

Use normal project checks. From the repository root, the full supported gate is
`python3 tools/verify.py`; full golden completion additionally requires
`python3 tools/verify.py --require-golden`. The latter currently fails because
the golden harness is not executable: planning must not call that a test failure
already observed in this session or weaken the gate to obtain success.
Focused commands below assume their generated fixtures have first been built,
as described in the [runtime guide](../../tests/runtime/README.md).

Public language, authentication, policy and transaction changes need the review
identified in the [workflow](../roadmap-workflow.md). Independent review is
pending: none is authorised or supplied by this planning chat. Self-review
does not replace it. Preserve frozen source, acceptance IDs, human permissions
and failed evidence. Revert only the current task's changes if a slice fails;
never reset concurrent work or narrow acceptance to fit the estimate.

The per-stage recommendations below are planning inputs; revalidate them against
adaptive-delivery's current model suitability gate on resume. The roadmap's
routing column owns the next-stage recommendation and any model deferral; it has
not yet been assessed under the new routing procedure. Preserve user-selected
settings while requesting switches as needed. Required reviewer independence is
separate from model choice.
No verify-loop campaign is launched: use its setup checks if sustained verifier
or reliability work becomes necessary during an authorised implementation run.

## RM-101–104 — frozen evidence and authentication

**RM-101 — M.** Extend the existing
[obligation map](../../tests/validation/golden-obligations.json), not a parallel
case list. For each of the 44 acceptance IDs link migrated declaration/route,
remaining compiler/runtime gap, RM dependency and required observation. Retain
original candidate diagnostics and the reviewed acceptance digest. Reconcile
the migration README's 11 files/47 declarations against current source before
updating claims. Check exact case-set equality and digest provenance using the
verifier inventory; source checking is not executed acceptance. Luna High is
sufficient for mapping; use Sol High review for ambiguous contract ownership.

**RM-102 — L; refined adapter plan, 2026-10-01.** The current bounded
implementation slice uses gpt-5.6-luna xhigh. Existing source checks were
insufficient: generation had to keep failing closed until the following checked
binding could be lowered. This refines implementation under AUTH-001, not the
frozen policy/acceptance or a new authentication strategy. The prior planning run
`RM-102-8beb69b271c7` and current implementation run `RM-102-6c08f4006008` are
partial task evidence.

### RM-102 credential and principal binding

Keep three distinct facts in a compiler-checked descriptor: the credential
record (identity, verifier, active predicate, expiry/revocation), its required
reference to the service identity, and the service record (identity, owner,
active predicate). Bind declared fields through semantic IDs, never the names
`Service`, `status` or `service_id`. Reject a nullable/non-authoritative reference,
a reference to a non-identity field, an ambiguous/missing mapping, and credential
fields presented as principal identity. The currently checked `owner` setting
means owner of the service authority; do not reinterpret it as the credential's
service reference. Keep the existing direct service-authority fixture supported.

Use an explicit credential binding on the API-key validator and a separate
service resolution, rather than inventing an implicit join from field names.
**Historical review correction:** the illustrative sketch below was not implementation-ready.
Its service resolution names credential fields absent on `Service` and maps a
verifier into the subject, contradicting the non-disclosure invariant below.
Retain it as the superseded proposal; the replacement must separate a nonsecret
service subject from the private credential lookup and lifecycle binding before
lowering. The implemented successor in
[authentication.jadpo](../../examples/golden-todo-migration/authentication.jadpo)
uses `verifier: ServiceCredential.verifier`, `authority: Service.name`,
`name -> Principal.service.subject` and `id -> Principal.service.service_id`.
Its subject is Text; verifier material remains in credential storage.
This does not reopen the accepted expiry/revocation/active-service rules.

Original proposed syntax to encode in positive/negative parser and formatter fixtures
before target lowering (a successor migration extension, not frozen-source edits):

```text
service_key {
    mode: api_key
    principal: service
    secret: config.session_signing_key
    audience: "todo-service-key"
    owner: Service.owner_id
    credentials {
        authority: ServiceCredential.verifier
        identity: ServiceCredential.id
        principal: ServiceCredential.service_id
        active: status == CredentialStatus.active
        expires: ServiceCredential.expires_at
        revoked: ServiceCredential.revoked_at
    }
}
resolution service {
    authority: Service.id
    active: status == ServiceStatus.active
    mappings {
        verifier -> Principal.service.subject
        service_id -> Principal.service.service_id
    }
    inactive: ServiceDisabled
}
```

The original spelling above is superseded, not the accepted compiler syntax.
In the successor, `principal` must
resolve by its existing reference to this resolution's identity. `active` runs
in the credential record's typed environment; expiry is an Instant and revoked
is a nullable Instant. The service subject uses the unique nonsecret name while
`service_id` remains the UUID-backed service identity; neither verifier material
nor private metadata may be exposed. The successor adds a service signed
validator in the same bearer slot with a distinct `todo-service-token` audience
for the already-required bounded exchange. Both use the existing secret sink;
no new credential location, provider algorithm or permission is introduced.

The existing runtime retains key/version/strategy metadata in a private record.
For a declared credential binding, that private row is metadata, not a competing
revocation authority: issuance writes it and the declared credential row in one
transaction in the same database; failed/uncertain commit returns no raw secret.
Only the declared row owns service reference, verifier, status, expiry and
revocation. Cross-check metadata against it; missing/inconsistent rows fail closed.
Generate lifecycle writes inside the trusted authentication host, never through
ordinary authored persistence or exposed secret values. Revocation updates the
declared record; rotation adds a separate credential and never overwrites another
credential's lifecycle. Reject unsupported cross-store layouts rather than dual
writing. Preserve existing internal-only storage for applications without a
declared credential binding.

Direct keys, exchange, refresh and fresh requests verify the originating
credential and current service state. Signed ordinary requests retain the existing
bounded lookup-free path and expiry cap. No principal subject, signed envelope,
error, audit or business return may contain the verifier or private metadata.
Credential and principal query counters stay separate; reuse within a request
only when identity and lifecycle checks remain equivalent. Do not add a disabled
service owner's check as a new permission rule: preserve the declared owner
relationship and approved service active predicate.

A further existing mismatch needs a negative fixture: the golden principal has
`AuthenticationStrength { primary, multi_factor }`, while adapters currently
produce mode strings (`signed`, `api_key`, `jwt`) and special principal-field
normalization only checks text. Do not cast those strings to the authored enum
or infer multi-factor strength from credential transport. Treat typed strength
mapping as an explicit binding with compiler-known conservative `primary` for
these adapters; admit `multi_factor` only with separately evidenced support.
Retain legacy Text strength for existing fixtures. If implementing that binding
requires a changed assurance meaning beyond this conservative mapping, return
with the exact proposed change before lowering it.

### RM-102 implementation slices and evidence

1. **Checked binding:** AST/parser/type validation, formatter and audit descriptor;
   preserve unsupported-target rejection until all invariants are supported. Add
   fixtures for valid renamed entities/fields, mismatched identity, missing active
   predicate, wrong expiry/revocation types, private-field leaks and invalid typed
   strength. Declare the conservative strength mapping explicitly in the successor
   and pin its positive/negative fixture spelling; do not broaden the enum.
2. **Storage/resolution adapter:** transactional issuance/revocation, metadata
   consistency and credential-to-service resolution for SQLite/PostgreSQL. Tests
   must directly mutate the declared credential's active/expiry/revocation fields
   and the service's status, proving none is shadowed by a private cached row.
   Cover rollback, orphan/mismatched metadata, rotation and owner reference checks.
3. **Protected route integration:** generate the unchanged Todo GET/create/patch
   routes and execute one real HTTP user path, service separation, disabled
   service/credential paths, duplicate credentials and startup failure. Keep
   internal host provisioning trusted; an authored issuance endpoint is outside
   this slice and needs its own authorisation contract. RM-104 owns the accepted
   exchange endpoint integration; generation alone cannot close that task.
4. **Gate/review:** existing first-party and service-auth suites, explicit JWT
   dependency path and full `python3 tools/verify.py`, with migration-specific
   HTTP/SQLite/PostgreSQL evidence registered in the manifest. Ordinary regression
   checks are appropriate; no verify-loop campaign is proposed. Independent auth
   and public-language review remains pending and cannot be supplied by changing
   this chat's model. Do not claim RM-102 complete without its protected-route
   acceptance and required review.

Original implementation run: gpt-5.6-luna xhigh for bounded fixture-led work. Sol High
remains the recommendation for independent architecture/security review if this
slice exposes a cross-component contract question. Original 1–4h forecast is
retained; the newly exposed binding/strength work is a forecast risk, not measured
effort.

RM-102 implementation checkpoint, 2026-10-01: the compiler now accepts the
credential-record reference to the separate service identity and lowers the
migrated package to 25 derived/target files. Route matcher identifiers are
allocated per generated route so files with the same source offset cannot collide.
`cargo build --manifest-path jadpo/Cargo.toml -p jadpo-cli`,
`jadpo check examples/golden-todo-migration`, and the focused first-party target
tests pass. A local Bun smoke harness initialises SQLite and observes healthy
`GET /health/live` (200), unauthenticated protected access (401), invalid bearer
(401), mixed credentials (401 ambiguous), service issuance/resolution, user Todo
creation (201), browser-cookie CSRF (201) and wrong-origin rejection (403). Full
CRUD, authored issuance/exchange integration and the 44 acceptance cases remain
unexecuted; this is a partial RM-102 slice, not task completion.
The registered `tests/runtime/golden-migration-authentication.test.ts` reproduces
the bounded SQLite checks under `bun --no-install`: one test and eight assertions
pass, including the credential-record bridge, user create, service denial and
browser-origin CSRF boundary.
The unified `python3 tools/verify.py` attempt at
`build/validation/20261001T214255-23214/report.json` passed its verification
contract step but stopped in an existing CLI watch test that requires permission
to bind a localhost socket; this is environment evidence, not a product failure.

Parallel-batch checkpoint, 2026-10-01: built-in proof modes now map to `primary`
for the declared `primary`/`multi_factor` enum, and principal normalization rejects
undeclared strength values. Legacy Text mappings remain compatible. The new
registered [protected-route suite](../../tests/runtime/golden-protected-route.test.ts)
proves ownership at storage, bounded/fresh/refreshed user/browser strength,
trusted-adapter enum validation, and no writes after credential/CSRF/configuration
rejection (6 tests / 104 assertions in the focused run). It does not execute the
44-case frozen suite. The [strength contract](../auth-runtime-extensions.md#typed-authentication-strength)
owns the supported mapping and limits.

The earlier independent review and [deliberately red service probe](../../tests/validation/rm102-service-authority-probe.test.ts)
confirmed the then-current golden adapter accepted a disabled service, an expired
declared credential and a present `revoked_at`; it also places the declared
verifier in principal subject. Revoked credential **status** does reject.
[Raw diagnostic output](../../tests/validation/rm102-service-authority-probe.log)
is retained as historical failure evidence; this probe targets the superseded
verifier-as-subject API and is not the current executable regression. The new
registered lifecycle suite below preserves its negative obligations.

Final batch evidence: [independent review and run summary](../../tests/validation/rm102-authentication-review.json);
the [full supported gate](../../build/validation/20261001T224512-61096/report.json)
passes 49 steps including seven PostgreSQL modes. The sandbox socket failure and
subsequent stale-assertion failure are retained in that summary. The repaired
assertions account for existing approval artifacts and check the HTTP
unknown-outcome envelope in the application target. Implementation run
`RM-102-ac73de1e14b1`, test-author run `RM-102-d63de817c00b`, and independent
review run `RM-102-b9d4b6df90e1` remain partial task records. No completion marker
or golden case result is added. The reviewer approved this strength slice, not
the missing credential/service descriptor. Runtime typed-enum coverage directly
exercises signed user/browser paths; service/JWT typed mapping is source-reviewed.

Declared-credential milestone, 2026-10-01: the successor binding is implemented
across parser, semantic checks, formatter, audit and target generation. Trusted
issuance atomically writes the declared credential and private identity/key
metadata; current declared status, expiry and revocation govern authority checks.
Verifier integrity metadata detects changes without exposing verifier material.
Service resolution uses the nonsecret name and stable UUID. The registered
[lifecycle suite](../../tests/runtime/golden-service-credentials.test.ts) passes
13 tests / 103 assertions on both SQLite and PostgreSQL, including negative
obligations from the historical probe, both revocation APIs, independent rotation,
metadata corruption and rollback of either issuance write. The first full gate
for this milestone passes [51 steps / eight PostgreSQL modes](../../build/validation/20261001T230533-73518/report.json).
Review found and fixed the revoke-by-token clock reference and rejected generated
lifecycle role overlap. No unknown-commit/restart or concurrent-disable injection
is claimed; the transactional lock/recheck is independently source-reviewed. The final
[52-step gate](../../build/validation/20261001T231148-76808/report.json) also includes
[real HTTP/JWT integration](../../tests/runtime/golden-auth-http-jwt.test.ts),
3 tests / 27 assertions. The intervening formatter-matrix failure is retained at
`build/validation/20261001T231014-75830/report.json`; the new grammar production
now has an exact-output fixture and all formatter checks pass. Independent
[review](../../tests/validation/rm102-declared-credential-review.json) approves
RM-102 closure; authored exchange and full golden cases remain RM-104/RM-109/RM-110.

RM-103 configuration slice, 2026-10-01: required origin and validator binding
added; owner revision/digests are in the candidate REVIEW log. Rebuilt compiler
source-checks the migration. Expanded missing-origin/missing-proof/wrong-proof
and missing-configuration checks in the existing authentication suite: 25 tests,
255 assertions pass with local socket access. The sandboxed run first passed 24
and failed at `listen` (EADDRINUSE); retained as environment evidence, not erased.
P10R structural checks pass (44 cases/694 links); no golden case executed.
Run `RM-103-005630d0ff9e` is partial pending RM-102 and independent auth review.

**RM-103 — M.** Add `browser_origin: Url { binding: "BROWSER_ORIGIN" }` to the
migrated typed configuration and `origin: config.browser_origin` to the signed
browser validator, following the existing first-party example. Keep the setting
required: missing/invalid configuration must fail before listen. Record the
owner-authorised candidate revision and old/new digests without silently rewriting
the original frozen source. Use a controlled origin in fixtures. Test valid
origin plus session-bound CSRF proof, missing/wrong origin, missing/wrong proof,
and bearer transport separation, including any mutative GET path. Full golden
proof waits for RM-102; first-party configuration/CSRF tests can be prepared first.
Luna High implementation; Sol High security review. This decision adds configuration,
not a relaxation of accepted CSRF or credential rules.

Delivery sequence for RM-102/103: RM-103 contract/configuration slice, RM-102
supported adapter and protected target, then RM-103 integrated CSRF evidence.
Do not create a cyclic whole-task dependency or close RM-103 after configuration alone.

**RM-104 — L.** Start from the existing
[service authentication suite](../../tests/runtime/service-authentication.test.ts)
and AUTH-P5 in [AUTH-001](../authentication-plan.md). Wire authored golden
issuance/exchange to compiler-owned operations; do not introduce an application
token-minting shortcut. Prove reveal-once/verifier-only storage, credential-to-service
ownership, rotation overlap, individual revoke, whole-service disable, expiry,
user/service route separation and bounded exchange/fresh checks across restart.
Count principal lookup separately from credential/session queries, per the
[accepted review](../../examples/golden-todo/REVIEW.md). Failures must not expose
secrets. Reuse generated adapters after RM-102 and retain golden case IDs from
RM-101. Luna Extra High implementation; Sol High independent auth review.

## RM-204/105 — typed route inputs and list query

**RM-204 — L, contract and implementation plan.** `RouteDeclaration` currently has path fields
and a body input, but no query/header bindings. Extend syntax, semantic checks,
formatter, generated decoder and OpenAPI together. Candidate: retain the pressure
source's `query: ListTodos`, with an analogous typed `headers: HeaderInput`;
permit scalar query/header fields and a JSON-encoded structured query value for
the cursor. Owner-selected `after` wire value: percent-encoded JSON of the existing
`TodoCursor { created_at, id }`; response `next` remains the frozen object/null.
Do not silently replace the response with an opaque token.

Use the selected structured-query encoding. Planned decoding rules: missing optional stays omitted;
defaults apply only when absent; an empty present value is validated, not defaulted;
duplicate declared keys and unknown query keys reject; malformed percent
encoding or JSON syntax is `InvalidRequest`, while a valid JSON value with the
wrong typed shape or a failed constraint is `InvalidValue`.
Headers use case-insensitive names, reject ambiguous duplicate declared values,
and ignore unrelated transport headers. Reject ordinary input declarations for
configured credential slots and compiler-owned auth/CSRF headers. No arbitrary
collection/header-object coercion. Freeze positive, negative and formatter
round-trip fixtures before lowering. Check 400/422, omission/default semantics
and OpenAPI agreement through real HTTP. Sol High design/review, Luna Extra High
compiler implementation. Finalise grammar/wire fixtures in this task and review
them against the selected boundary; no opaque-token redesign is authorised.

Contract checkpoint: `query: ListTodos` in the pinned golden source is the
positive declaration. A typed `headers: HeaderInput` field is the candidate
extension, with credential and CSRF fields excluded at check time. Negative
declarations bind a non-object, declare the same wire name twice, reuse a path
name, or claim `Authorization`, cookie/session or CSRF headers; each must fail
checking. The selected `after` request value is one URL query parameter whose
percent-decoded bytes are one JSON object with exactly `created_at` and `id`;
the JSON values are validated through the referenced `TodoCursor` fields.
Decode once, never accept a second percent-decoding pass. Unknown/duplicate
query keys and malformed JSON are `InvalidRequest` (400); a decoded value that
fails its typed constraint is `InvalidValue` (422). An absent optional field
stays absent and defaults only when absent. No cursor may be inferred from an
invalid value. The candidate [boundary cases](../../tests/assurance/route-input-v0.1.json)
fix these distinctions for RM-204 lowering and real-HTTP/OpenAPI comparison.
The grammar and security review remain open; this preparation does not complete
the compiler/runtime portion of RM-204.

**RM-105 — L, short plan conditional on RM-204.** Reuse the frozen ListTodos
fields, PageSize 1–100/default 25, optional status/due-before predicates, and
descending `(created_at, id)` cursor. Extend checked query representation/lowering
only where these shapes are absent; never concatenate unvalidated URL values
into SQL. Apply owner policy and nondeleted visibility before ordering/limit.
Use the existing TodoView projection and fetch sufficient rows to derive `next`
without N+1 reads. Verify LIST-001/002, equal timestamps, omitted filters, empty
pages, invalid cursor, page-size edges, foreign/deleted rows, and stable repeat
over the frozen dataset on SQLite/PostgreSQL. Preserve the one-query budget and
separate auth accounting. Concurrent-insert snapshot semantics are not promised
by the frozen cases. Lower continuation to indexed keyset predicates on
`(created_at, id)` rather than OFFSET or loading all rows. Keep cursor values typed
and database-neutral; use bound parameters and per-adapter timestamp/UUID ordering
that satisfies identical conformance fixtures. Inspect SQLite and PostgreSQL query
plans on a representative larger fixture, including deep pages, to establish that
pagination does not scan prior pages or fetch unbounded rows. Index design must
include scope/visibility/order as justified by each adapter; do not invent a
universal SQL dialect or claim all future databases are already supported.
Luna Extra High implementation; Sol High query/policy review.

**Outcome:** implemented the checked page AST, type rules, bounded generated
route and static first/continuation SQL. SQLite's larger-fixture plan uses the
partial descending owner/order index with no temporary B-tree; PostgreSQL 16.3
uses the matching partial descending index with no Sort and at most the page-plus-
one rows emitted after a 4,000-row cursor. PostgreSQL's real route test also
verified optional-filter parameter casts and tied-key continuation. The
[implementation history](../implementation-history.md#2026-10-02--rm-105-filtered-keyset-list)
records commands, assertions and limitations. Partial predicates participate
in index naming/de-duplication. The full supported [validation report](../../build/validation/20261002T162110-68433/report.json)
passes; LIST-001/002 remain unexecuted as frozen end-to-end obligations until
RM-109.

## RM-205/206/106/107 — lifecycle and synchronous routes

**RM-205 — M, decision package.** POLICY-D25/26 already separate lifecycle and
business rules from permissions. The owner selected entity-owned lifecycle
declarations with named transitions, compiler-enforced visibility and protected-field
ownership. Finalise the DATA-007 spelling before compiler work; application actions
must not have to repeat lifecycle guards.
Use User.disable (retain children) and Todo.delete/purge (soft delete, configured
retention, no restore) as the complete first pressure set. Specify canonical
source and negative fixtures for direct client writes, forbidden restore,
bypass through nested calls and guard/write races. The existing spec contains
illustrative lifecycle blocks, not an approved complete transition grammar.
Keep a semantic node for lifecycle separate from policy even when predicates
share SQL. Sol High preparation and independent language/policy review. The
architectural direction is settled; concrete grammar and positive/negative contract
fixtures are deliverables of RM-205, required before RM-206 lowering.

2026-10-02 review checkpoint: the [DATA-007 candidate](../lifecycle-plan.md)
now specifies compiler-owned initial values, a named transition invocation on
an identity-bounded update, logical versus physical effect mapping, trusted
authentication resolution of disabled users, and bounded privileged purge.
The [21 contract cases](../../tests/assurance/lifecycle-v0.1.json) cover
positive/negative source shapes, scope/visibility, concealed outcomes, and
both-adapter race obligations; they are not registered compiler or runtime
tests. Self-review checked case IDs and golden-obligation links. RM-205 remains
in progress pending a separate authorised language/policy review of spelling,
ownership, auth exception, delete permission and guard/write atomicity, then
contract freeze. RM-206 remains queued. The resumed run is
`RM-205-3a4d3c994093`; batch pin is user-confirmed `gpt-6-sol` High after the
model-switch handoff, while exact host settings are not independently exposed.

**RM-206 — L, staged plan after RM-205.** Implement accepted declarations in AST,
checking, semantic graph, write-ownership diagnostics and audits before runtime
lowering. Reuse transaction context for visibility/invariant guards and guarded
mutation; no check-then-write gap outside the transaction. Register compile-pass,
compile-fail, formatter and runtime fixtures. Prove concealed absence, rollback,
nested invocation and concurrent disable/delete/update traces on both databases.
Unsupported lifecycle shapes fail closed. Luna Extra High implementation and
Sol High independent semantic/transaction review; final fixtures depend on RM-205.

**RM-106 — L.** Apply the accepted lifecycle to migrated User/Todo entities and
the missing delete/disable actions. Preserve AUTH-006/USER-001: self-disable is
204, Todo rows remain, subsequent fresh check and refresh deny, ordinary bounded
credentials cease working within five minutes. Todo deletion conceals reads and
mutations; restore stays prohibited. Test create/patch/delete/disable interleavings
with guard and write in the same transaction; no client write to lifecycle fields.
Retention purge scheduling belongs to RM-108, not an accidental hard delete here.
Luna Extra High implementation; independent auth/lifecycle review pending.

**RM-107 — M.** Completed 2026-10-02; the implementation and evidence below
supersede this planned slice. RM-106 still owns lifecycle delete/disable and
their interleaving tests.

## RM-301–307/108 — services, durable jobs and reminders

**RM-301 — M, decision preparation.** Use D3 in the
[decision sprint](../decision-sprint.md), CONFIG-001 secret sinks and the frozen
ReminderMail operation as the boundary. Use the owner-selected local HTTP
reference provider, then prepare the requested SERVICE-001 artifact: pinned import digest/version,
canonical positive/negative declarations, allowed egress and redirects, secret
slots, closed request/response validation, safe error mapping, deadline and
retry/idempotency matrix, and fake contract. Keep raw HTTP/provider objects out
of source; calls only from admitted effectful contexts. Preserve the candidate's
5s timeout and maximum 3 attempts/30s unless an explicit revision is accepted.
An uncertain write outcome cannot become a blindly retried temporary failure.
No live provider call, account or deployment is needed for planning. Sol High
design; formal contract/freeze review remains a deliverable of RM-301. The provider
choice itself is resolved; do not ask it again or switch to a production vendor.

Historical pre-answer provenance/decision checkpoint, 2026-10-02: the [SERVICE-001 candidate](../service-plan.md)
now distinguishes the preserved app digest from the current RM-107 email
privacy successor and labels the frozen blanket timeout mapping as unsafe
after dispatch. The pinned OpenAPI snapshot digest and response set were
rechecked. The owner has been asked once whether a due-date change after an
accepted reminder creates a new reminder delivery; a new durable delivery ID
is recommended because the existing source clears `reminder_sent_at` on that
change, while `Todo.id` cannot safely identify changed payloads. The question
is pending. RM-301 also still needs an authorised independent effect/security
review and a frozen successor declaration before RM-302. Resumed timing run:
`RM-301-44b87659abc4`.

**RM-302 — N, staged decision-dependent plan.** After RM-301 acceptance, implement
one declared operation end-to-end through parser/checker/effect graph/audit before
generalising import support. Reject undeclared/dynamic egress, secret escapes,
service calls from queries/pure functions, incompatible atomic effects and stale
or broadened imports. Preserve exact typed failure sets. Add formatter/diagnostic
fixtures and register the runnable example in the verifier. Re-estimate after
this slice. Sol High architecture, Luna Extra High compiler work, independent
language/security review. No final grammar is implied by this outline.

**RM-303 — N.** Implement the selected adapter after RM-302 and the reviewed RM-207 boundary-contract milestone. The selected local
reference server must be an actual HTTP boundary with reviewed
request/response fixtures, distinct from an in-process fake. Verify typed payloads,
malformed responses, credential containment, timeouts, forbidden redirects/hosts,
failure mapping and stable idempotency key across permitted retries. Inject lost
response after provider commit to prove uncertain outcome handling. No real email
or credentials in tests. Split normal success/failure from retry/recovery and
re-estimate. Luna Extra High implementation; Sol High effect/security review.

**RM-304 — M.** Extend the existing authored fixture machinery with fakes exposing
only declared service operations/outcomes. Check unknown operations, wrong request
or response types, undeclared failures and secret leakage reject. Integrate callable
invocation, deterministic clock and per-test isolation; test success, malformed
response, deadline, retry and unknown outcome with zero real side effects. Fakes
must preserve runtime outcome semantics, not bypass them. Separate adapter HTTP
conformance (RM-303) from fake evidence. Luna High implementation and test review.

**RM-305 — M, decision preparation.** D4 proposes transactional outbox and
at-least-once delivery, not exactly-once external effects. The owner selected
backend-neutral durable-job semantics, using the existing authority database through
SQLite/PostgreSQL adapters first. Do not depend on Cloudflare or a hosted queue.
Database-specific locking/SQL belongs inside adapters; authored jobs use one contract. Prepare canonical enqueue/cancel/replay
source, durable states, claim/lease and crash traces, ordering/version identity,
singleton overlap behaviour, UTC scheduling, bounded selection/continuation,
retry budget/jitter, dead-letter and operator recovery. Clarify cancellation
before/after an external effect and replay with the same idempotency identity.
Freeze concrete semantics and acceptance traces before RM-306; Sol High design
and independent transaction/effect review. Prepare/freeze the concrete ASYNC-001
contract within RM-305 using the selected database-backed/coalesced-schedule baseline.

Planned adapter contract, to turn into canonical fixtures and review evidence:

- Store intent, attempts, leases and terminal results in the same authority
  database as the mutation; commit mutation+intent together. Support SQLite and
  PostgreSQL through compiler-owned adapters, with no external queue selected.
- Use durable `pending → running → succeeded` transitions, with `retry_wait`,
  `failed`, `cancelled` and `outcome_unknown` as explicit alternatives. A claim
  records worker, lease and fencing generation; stale workers cannot acknowledge
  or mutate job state. Lease expiry alone never proves an external call did not occur.
- Preserve at-least-once delivery identity across crash/reclaim. Singleton means
  one valid claim, not a proof that a paused old process cannot still contact a
  provider. Provider idempotency and the uncertain-outcome path protect that edge.
- For the golden interval jobs, coalesce missed/overlapping ticks into one pending
  scan of currently eligible rows; do not replay every missed interval. Keep
  declared 15m/1h intervals and 500-row bounds. Longer catch-up uses another bounded
  invocation, with its continuation represented durably.
- Preserve `retry: next_schedule` for known retryable golden job failures. Each
  provider call keeps its declared maximum 3 attempts/30s; a later scheduled scan
  is a distinct invocation, not an unbounded hidden retry loop. Persist the failed
  run. Other jobs inherit compiler-owned bounded safe-retry defaults and a terminal
  failure path; an application need not invent its own retry architecture.
- Cancel pending work without effects. Cancellation after dispatch records a
  request and waits for a known outcome; it does not assert rollback. Unknown
  delivery blocks automatic resend, including later scans of the same operation.
  Replay retains operation identity and requires resolved uncertainty or a
  reviewed provider guarantee. Safe inspection/recovery primitives are required;
  the broader RM-308 operator surface remains outside this scope.

Required traces: rollback produces no deliverable event; commit then crash retains
intent; crash after claim permits safe reclaim; stale acknowledgement is rejected;
duplicate delivery retains identity; downtime coalesces interval ticks; cancellation
before dispatch sends nothing; cancellation/timeout after dispatch preserves the
known or uncertain outcome; exhaustion records a terminal failed run; restart and
the next schedule do not bypass an unresolved delivery. Unknown-outcome rules are
shared with RM-207. ASYNC-001's independently approved freeze fixes semantic
lease/deadline constraints and reviewed digests. Exact checked parser spelling
and canonical enqueue/job fixtures belong to RM-306, consistent with SERVICE-001's
staged semantic-freeze then compiler-spelling approach; the historical outline's
"final grammar within RM-305" is superseded by that explicitly reviewed scope.
No numeric golden limits or executable conformance are implied. Validate lease claims using the database adapter's
authoritative time/fencing, never assuming separate workers have identical clocks.
Run identical logical traces against both adapters; a backend lacking required
atomicity must reject the capability rather than weaken the contract.

**RM-306 — N.** After ASYNC-001 acceptance, build declaration/effect checking and
transactional outbox insertion first, then durable claim/ack primitives. Prove a
rolled-back mutation publishes no event, committed intent survives restart, and
re-delivery retains identity/version/ordering metadata. Reject incompatible
transaction domains and effects before generating code. Add audit/formatter and
negative fixtures; re-estimate after commit/rollback/restart evidence. Sol High
design, Luna Extra High implementation, independent transaction review.

**Current first slice, 2026-10-04:** ASYNC-001 is independently frozen. Under the
confirmed Sol/high batch, establish the compiler-owned storage primitive before
binding public enqueue/event spelling to it. A delivery's ID is generated inside
the runtime, not supplied by an authored caller; source operation/entity/revision
uniqueness preserves it, while payload/version/order-key mismatches reject rather
than overwrite. Per-key sequence allocation is serialized in the same source
transaction, avoiding commit-order overtaking. Enqueue is allowed only on a
transactional persistence client and shares its connection with the mutation.
Use actual generated output in the existing SQLite/PostgreSQL persistence suite
to prove rollback, acknowledged commit/restart and duplicate identity. These are
storage-primitive checks, **not generated authored-job conformance** or closure of
the 18 trace catalog. Public typed declarations, effect/audit/formatter checks,
worker admission and independent implementation review remain RM-306/RM-307.

**Next private state slice:** add a separate compiler-owned claim table sharing
the existing source transaction. Serialize claim/reclaim/checkpoint on the same
job/key row, admit only the first nonterminal intent, use database UTC time,
persist immutable finite limits and first-claim lifetime, and increment a BIGINT
fence without JavaScript numeric coercion. Expired pre-dispatch claims may reclaim
within cumulative budgets; expired possible-dispatch claims become unknown and
block successors. A stale, expired or already-checkpointed handle cannot admit
another dispatch. Test rollback, concurrent claims, fencing, FIFO, budgets and
unknown recovery on both actual generated adapters, then independent transaction
review. This slice exposes no authored/public worker or operator authority and
does not dispatch externally. Ordinary invariant checks are sufficient for this
settled state slice; hard readiness I/O bounds remain a separate investigation.

**Next renewal slice:** a current running claim may renew its lease using a fresh
database-time SQL sample, capped by its immutable execution and lifetime deadlines.
Require its exact claim ID/fence and a still-live old lease in the atomic update;
an expired claim cannot revive itself. Renewal preserves invocation count, first
claim, absolute deadlines and possible-dispatch evidence, including during an
in-flight effect. A late renewal result returns no handle, never a fresh dispatch
permission. Check rollback, stale/expired fences, caps and delayed SQL on both
adapters, then independent review. This remains private storage, not heartbeat
scheduling or proof that a worker is permitted to dispatch after commit.

**Next cancellation slice:** serialize cancellation with claim/checkpoint using
the existing key lock. Pending/retry-wait or running-before-checkpoint work can
become cancelled; subsequent fenced checkpoint/renew calls then refuse it. Once
possible dispatch is durable, store only a cancellation-request marker and keep
running/unknown unchanged: a request is not rollback evidence and does not release
the FIFO block. Preserve a database-time request record atomically with state,
test rollback, both checkpoint/cancel orderings and restart/unknown containment.
No public operator endpoint or caller-supplied effect certainty is added.

**Next reference-mail outcome slice:** persist only SERVICE-001's closed
`accepted_at` receipt for the exact current intent/fence after committed possible
dispatch. Validate its exact UTC Instant shape; the provider timestamp is evidence,
not the compiler's observation time or a clock for retry/claim authority. Use
database time to condition the terminal state write on a live lease/execution/lifetime,
then persist the immutable receipt and database observation in the same owning
transaction. All operations/faults must be awaited and propagated by the future
compiler worker; callback results remain provisional until acknowledged COMMIT.
No late/stale receipt may complete a newer intent. Also persist explicit uncertainty
for a current possible-dispatch claim, without permitting resend. Test rollback,
current/stale/late fences, closed-receipt rejection, same-key release only after
known success and restart evidence. This is a **reference-mail-only private host
foundation**, not generic receipt typing, worker authority, trusted reconciliation
or authored ability to manufacture transport success.

**RM-307 — N.** Build scheduling and worker state transitions on RM-306 and
RM-304's deterministic boundary. Execute accepted concurrency, retry, timeout,
cancel, dead-letter and replay traces with explicit crash checkpoints before/after
claim, external effect and acknowledgement. Use fake time, separate workers and
restart evidence; a process-local lock cannot establish singleton behaviour.
Persist terminal state safely; full operator UI remains RM-308 outside scope.
Re-estimate after one recoverable job slice. Luna Extra High implementation;
Sol High independent recovery review.

**RM-108 — L.** Compose named Todo queries/actions with reviewed ReminderMail
for overdue reminders (15m). Retention purge (1h) activates the clause-bound
generated maintenance worker accepted in RM-205, never an authored Todo action
or a service-principal job that bypasses POLICY-D30. Both retain their reviewed bounds, singleton
behaviour and bounded batches from the frozen source. Run JOB-001–004 with injected
clock, excluded states, provider failures, duplicate delivery and crash after
external success before marking sent. Preserve the pressure source's `Todo.id`
key as comparison evidence; implement only the independently accepted SERVICE-001
successor with a persisted intent UUID per schedule revision. The owner's changed
due-date decision is recorded there, with fixed payload, admission and completion
guards; JOB-001's scoped successor and PATCH-005 must agree. Never let an old
receipt mark a newer schedule sent or create a new key to bypass an unknown outcome. Provider
timeout mapping must respect RM-207's uncertainty rule. Runtime composition
cannot claim an atomic database+mail transaction. Luna Extra High implementation;
Sol High review across job/service/policy boundaries. These interactions remain
decision inputs to RM-301/RM-305, not new acceptance claims.

**Accepted owner input, 2026-10-04:** the owner's “Yep, happy with that” accepts
both recommended choices in the preceding handoff. Known-no-effect mail failures
across the 15-minute schedules get at most **3 total scheduled invocations within
1 hour** of the first valid claim. Each invocation retains the frozen service
limit of 3 attempts / 30 seconds; unknown outcomes never auto-retry. The alternative
16 invocations / 4 hours was not selected. This settles concrete golden limits;
generic storage fixture limits are not the golden configuration.

**Accepted policy-authority input, 2026-10-04:** the migrated Todo currently declares only
`TodoRole.owner` for create/read/update/delete, with a User identity field binding
([current source](../../examples/golden-todo-migration/entities/todo.jadpo)).
POLICY-D30 requires authenticated service principals/qualified roles for scheduled
application callers; being a compiler worker is not authority. The preserved
pressure policy's job-specific sent-field/external-effect permission does not
provide a checked service-role binding in the migration. Do not silently add a
general service update or owner-impersonation path.

The owner approved a dedicated, revocable reminder-service
role scoped to currently reminder-eligible Todos and active owners' recipient
details, ReminderMail dispatch, and only the matching revision's `reminder_sent_at`
write, preserving ordinary owner-only CRUD and no general update/delete. The
alternative to defer background reminders was not selected.
Public worker binding must also select an existing checked service authentication
path, recheck live authority at admission, and preserve secret-only credential
handoff; do not construct a principal from a job name. No permission change is
implemented merely by recording approval. Use the existing database-backed
service membership/qualified-role mechanism; constrain reads by live eligibility
and recipient authority, and narrow the named completion action to the one sent
field and exact revision. Never grant ordinary patch/delete to that service.
Test revocation, inactive owners, stale revisions, credential failures and forbidden
fields/actions on both adapters; independently review new policy/semantic-graph
reachability before golden integration. Generic private storage evidence remains
separate. Ordinary regression checks fit this slice; no new verify-loop campaign.

**RM-306 next storage slice:** add a private reference-mail known-no-effect
outcome seam (only checked adapter classifications, bounded provider-attempt
count, selected nonnegative delay; no raw diagnostic text). Persist one outcome
per fencing generation and its absolute database-time retry eligibility; claims
must check that instant at allocation. Preserve immutable lifetime/count limits,
record permanent/exhausted failures with bounded dead-letter metadata, and end
at `cancelled` if a post-dispatch cancellation request is followed by trustworthy
no-effect proof. Make outcome/history/dead-letter changes savepoint-atomic even
when the host catches faults. Test stale/expired/duplicate outcomes, rollback,
native storage faults, delayed allocation, FIFO, cancellation and process restart
on both adapters. Independent transaction/effect review is required. This private
host seam does not prove adapter provenance; public worker integration and
bounded exponential/full-jitter selection remain later checks.

**RM-108 policy prerequisite, independent plan review:** the
[source review](../../tests/validation/rm108-independent-policy-plan-review.json)
accepts the authority direction but identifies unimplemented scope/field/guard
composition. First bounded correction: validate exception-only role bindings;
resolve application memberships independently of the retained resource scope in
both SQL and row checks; admit a field rule only where that same actor/effect has
a base or named-operation grant, still requiring the exact operation grant at
runtime; emit explicit denial for every ungranted field effect, including empty
or read-only field policies. Test owner/resource isolation, service/application
membership revocation, exact named operations and forbidden mixed/fixed writes
on both adapters. Do not add reminder grants yet. This does not add arbitrary
multi-resource scope composition, live eligibility predicates or schedule-revision
CAS; those remain later implementation prerequisites with scoped reviews.

**Owner-approved policy repair, 2026-10-04:** explicit-denial enforcement
correctly rejects User.disable_user: status and disabled_at declare `policy {}`,
even though the self-scoped named operation has update authority. The golden HTTP
test expected 204 but got 404. The owner explicitly approved adding only `UserRole.self: [update]` to
these two lifecycle-owned field policies. Ordinary set/patch remains compiler-
forbidden, no reads are granted, and the existing self-disable named operation
remains the sole checked update grant. Alternative: retain denial and explicitly
defer self-disable, leaving its acceptance obligation open. That alternative was
not selected. The exact two grants are now applied; no lifecycle/generated bypass
was added. Independent review had correctly distinguished the existing intent
from explicit field-grant approval; the latest owner “yes” supplies that approval.

**Public binding staging:** independent compatibility inspection finds no
semantic conflict with a narrow typed scheduled-job binding on ASYNC-001. Keep
scan activation/clock separate from per-Todo durable delivery identity; resolve
run to a checked action with closed inputs, exact failures and transitive effects;
derive source operation/revision/version and envelope internally, reject unsupported
stored versions, and bind authority through checked service identity. Do not
expose host claim/outcome methods or adopt generic events/subscribers, fan-out,
graph ownership or publication semantics from conditional RM-309. RM-306's event-
binding acceptance remains pending, not waived by a job-only milestone. Exact
compiler/worker lowering still needs its short plan and scoped independent
public-language/effect review before integration.

**RM-306 public scheduled-job short plan (original, corrected below):** preserve
`job overdue_reminders every 15m { concurrency: singleton run:
send_due_reminders(clock.now) retry: next_schedule }`. Use a typed JobDeclaration
and route-style invocation/duration AST, exactly one of each closed body clause;
reject unknown/duplicate clauses, callable/export use, dynamic callees and
authored principal/UUID/version/fence/receipt/outcome properties. Resolve run to
an action with exactly one Instant parameter, exact `clock.now` argument and Unit
result for this first slice; install a job clock typing environment. Derive
transitive effects/failures and require reviewed durable dispositions. Parser/
type fixtures are a separate milestone; build must fail closed until a finite
execution profile and checked worker lowering exist, not emit a no-op job.

Keep activation identity, Todo schedule identity and delivery fence separate.
Create/supplied successful due_at patch establishes/advances the private schedule
revision; the scan admits the revision's stable intent. Equal supplied due dates
still advance; omitted/failed patches do not. Do not use general row-change
revision, a due timestamp or source digest as the business schedule revision.
Before reminder lowering require an explicit internal CheckedReminderDeliveryBinding
with resolved scan/selection/admission plan, identity/revision hooks, closed
request construction, per-intent service handler/completion, contract version
and Todo ordering-key derivation. Never infer admission from an action's name or
silently turn an ordinary mail call into enqueue. Scan/admission transactions
and provider execution are distinct; an ordinary mutative action's existing
transaction wrapper cannot span provider HTTP. Public language/security review,
positive/negative AST/type/audit/formatter fixtures and real generated worker
cases on both adapters remain gates. The independent read-only recipe informed
this stage; it did not approve runtime lowering or waive event acceptance.

**Scheduled-job plan correction, 2026-10-04:** the first focused implementation
checks exposed `TYPE_PRIMITIVE_SIGNATURE`: a bare Instant action parameter is
forbidden by the existing callable rules, so the original positive signature
could never pass. Preserve that failed evidence and the original
[plan review](../../tests/validation/rm306-independent-job-binding-plan-review.json).
The independent [correction plan review](../../tests/validation/rm306-independent-job-binding-plan-correction-review.json)
approves composing the existing explicit nominal-constructor contract instead:
`type JobRunAt = Instant {}`, `action scan(at: JobRunAt) -> Unit`, and
`run: scan(JobRunAt(clock.now))`. JobRunAt is illustrative, not a reserved type.
The first slice permits only a direct unconstrained nonnullable named scalar
Instant wrapper; the constructor must resolve to that exact parameter type and
wrap only the intrinsic snapshot. Check ordinary construction/call compatibility
and record the trusted-Instant/unconstrained-wrapper no-additional-rejection
proof. Bare clocks, implicit casts, constrained/indirect/nullable/sibling wrappers,
arbitrary nested calls and unknown receivers reject. No primitive-signature rule
was relaxed, and historical pressure/frozen contracts are not rewritten.
All other clause, authority, fail-closed target and future worker/event gates from
the original plan remain. This correction requires no new owner semantic choice.

**Scheduled-job frontend checkpoint, 2026-10-04:** typed Declaration::Job,
closed singleton/run/next_schedule clauses, representable positive intervals,
exact action/nominal-constructor binding, dedicated clock typing and non-callable/
non-exported identity are implemented. The job graph retains constructor import
visibility and exact action identity (no receiver suffix fallback). Static failure
and external-service reachability are separate from route HTTP failure mappings;
`audit/jobs.json` records constructor proof and explicitly pending runtime
profile/dispositions. Target rejects every job until checked lowering exists.
Ten registered job groups, including parameterized hostile cases and transitive
service-effect evidence, pass. New compile/formatter fixture: 173; current corpus:
216 pairs. Existing formatter layout limitations under RM-203 remain.

The fresh supported [gate](../../build/validation/20261004T144930-62012/report.json)
passes 61/61 after this frontend slice. It covers the current private transaction
guards and repaired self-disable too, but remains non-release-equivalent and does
not execute a worker or any of the 18 ASYNC traces. The earlier
[failed gate](../../build/validation/20261004T144820-61488/report.json) retained
the exact fixture-count mismatch (215 versus the new 216), now corrected without
weakening checks. Earlier focused failures also retain the real primitive-plan
conflict, unavailable Rust API, and invalid temporary service-fixture spellings
(raw UUID construction, unsupported loop and raw action query). The final fixture
uses an existing named authoritative query and nominal identity construction;
no checker, query, loop or UUID rule was relaxed. The separate
[implementation review](../../tests/validation/rm306-independent-job-frontend-review.json)
requires an exact-decimal interval correction: f64 rounding admitted three
nonintegral authored values despite the passing supported gate. This is a real
scoped blocker; separate plan reviews and green broad tests do not approve it.
Main timing: `RM-306-711d34834931`, partial task; confirmed Sol/high retained.

**Interval correction checkpoint:** the shared duration helper now scales the
authored decimal coefficient using exact digit/integer arithmetic, tests every
fractional millisecond digit, then checks positive safe-integer bounds. Audit uses
the checked binding interval rather than reparsing a rounded value. All three
review reproductions reject with no checked job binding; exact fractional-unit,
safe-boundary and long trailing-zero positives retain their exact audit values.
Ten focused job groups pass after correction. The first correction test run
retains a 9/10 harness failure: calling artifact generation on an invalid project
correctly panicked; the regression now checks rejection/no binding and leaves
the CLI artifact-rejection check to independent probes. The fresh supported
[correction gate](../../build/validation/20261004T152556-66945/report.json)
passes 61/61, including both adapters and all 216 compile pairs; it remains
non-release-equivalent with full golden/worker/18-trace gates open. The scoped
independent [correction review](../../tests/validation/rm306-independent-job-interval-correction-implementation-review.json)
now approves the exact interval correction with no findings. It rebuilt a private
CLI/test target, reran 10/10 focused groups, and independently checked job
rejection/audit/build and the shared route-deadline helper, including exact
emitted integer deadlines. A legacy diagnostic-spelling assertion error in its
probe harness is retained and corrected against captured results; no product
check was weakened. Current narrow source hashes match its reviewed inputs.
No worker/profile/event/authority/trace or unrelated approval-artifact review
is supplied by that disposition.
Correction timing: `RM-306-7d39d4ca97b6`; task remains partial.

**Resumed checkpoint, 2026-10-05:** the interruption left the last timer's
unclosed interval unknown; it is not counted as overnight active work. The prior
reviewer is unavailable and produced no correction artifact. A fresh independent
reviewer is checking the exact correction under the existing owner-authorised
delegation. Retain the confirmed Sol/high batch and the original blocked review;
no product change or duplicate full-suite run is needed merely to resume.
Current continuation timing: `RM-306-8dc7ef0b7d81`, partial task. Its start copied
the prior Sol/high pin, but subsequent own-turn metadata positively established
`gpt-6.1-sol/high`; the finish evidence corrects that attribution without rewriting
the original event. The fresh reviewer independently found the same mismatch and
performed no substantive probes. Its
[model-deferred checkpoint](../../tests/validation/rm306-independent-job-interval-correction-review.json)
is not correction approval. The batch remains pinned to `gpt-6-sol/high` unless
the owner explicitly overrides for this continuation and its independent review;
otherwise switch back and verify before resuming. No additional product work or
test rerun took place after the positive mismatch was established.

**Next checked-worker integration stage — short plan, not execution-ready:**
the current migration has neither a mail declaration nor an eligible reminder
query/job/service grant. The canonical empty `scan` frontend fixture is not the
golden worker. Continue the previously required explicit binding approach, not
name matching or rewriting an ordinary provider call into enqueue:

- Resolve the selected job/action, bounded selection, source mutation hooks,
  exact imported service operation/request/receipt, private intent version/key,
  authentication validator and named completion operation into one checked
  `CheckedReminderDeliveryBinding`. Unsupported/incomplete combinations keep
  build closed. Public spelling/effect/policy review is required before lowering.
- Use the existing checked `api_bearer.service_key` authentication path with a
  secret-only credential handoff and live service/credential/membership checks;
  no synthetic principal or owner impersonation. Apply the owner-approved narrow
  reminder authority only, and correlated active-owner/recipient eligibility
  before ordering/500-row selection and again at admission.
- Add the private schedule-revision hooks to successful create/supplied-due-date
  patch transactions; retain omission, equal-value and rollback semantics from
  [the service contract](../service-plan.md#durable-identity-and-schedule-guard).
  Enqueue and later admission remain distinct short transactions. A committed
  possible-dispatch checkpoint releases all database locks before provider I/O.
- Complete the exact immutable intent/fence using the checked adapter receipt;
  atomically guard the sent-only write by current schedule revision. Preserve
  FIFO/unknown containment and the accepted cumulative retry limits. Keep
  singleton activation identity, delivery claim and business revision separate.
- Bind finite scan/continuation and per-delivery execution limits explicitly.
  The pending 60s/40s proposal concerns delivery claim execution/renewal; it is
  not permission to treat 500 sequential provider calls as one 60s activation.
  Persist one coalesced follow-up and bounded keyset continuation per ASYNC-001.
- Verify generated boundaries on both adapters with credential/role revocation,
  inactive owners, equal/omitted/failed/ABA patches, revision races, duplicate
  workers, crash checkpoints, unknown outcomes, exhaustion and restart. Ordinary
  regression checks fit the settled invariants; claim the 18 traces or full
  golden only after their separate executable gates pass.

This preparation does not activate a profile, grant a role, settle new public
syntax or implement the worker. It makes the next scoped language/policy review
and numeric-profile handoff concrete; RM-109/RM-110 still depend on integration.

**Worker-binding revalidation, 2026-10-05:** the migration currently provides
the checked service credential strategy, Todo mutation/lifecycle operations and
message/receipt value types, but no mail declaration, eligible scan query, narrow
reminder grants or explicit delivery binding. Its existing message key is still
`Todo.id`, not the reviewed persisted nominal intent identity. Thus there is no
execution-ready source binding to lower. Before parser/runtime changes, settle
one closed, explicit binding contract that identifies selection, create/revise
hooks, imported operation, exact payload/receipt, current authentication validator
and sent-only revision-guarded completion; require exact semantic identities,
not spelling or callback heuristics. Reject ordinary invocation of the private
completion or caller-supplied intent/fence/receipt authority. The service and
ASYNC contracts above already determine behavior; this is a compiler-spelling/
policy-composition prerequisite, not approval to enlarge them or adopt RM-309.
Numeric-profile input remains separate and unset. Canonical prerequisite audit
below identifies checked principal/route facts that can be implemented while
these gates remain open. Revalidation timer: `RM-306-e61621faed83`, partial,
actual6.1-sol/high.

**Delivery-binding next static slice — source-grounded preparation, 2026-10-05:**
the migration's User lifecycle already declares `status == UserStatus.active`;
required-owner lowering has an EXISTS visibility/policy predicate before LIMIT.
Reuse these checked clauses, not a post-limit owner test. Ordinary QueryExpression
still carries one equality predicate and one order field. QueryPage carries a
predicate list/ordered cursor tuple, but supports equality, optional equality and
optional `<=`, not the golden's required strict `due_at < operation_time`; its
page parser is not a required-owner include surface. Thus an existing query must
not be advertised as the fully checked golden selector. Selection needs a bounded
checked successor for compound predicates, strict comparison, ordered `(due_at,id)`
keyset continuation and required-owner visibility before limit. These are necessary
golden semantics, not authority to turn on a generic query/event/workflow extension.

Prepare one closed **candidate** job `delivery:` clause, optional only on a
reviewed delivery job, rather than an action-name convention or ordinary provider
call rewrite. Existing non-delivery jobs retain their checked three-clause contract
and remain fail-closed until supported lowering exists. This clause is a proposed
compiler spelling, not accepted language or an activated profile. Its first static
milestone resolves into `CheckedReminderDeliveryBinding` containing:

1. Exact job/run action and source entity/identity; explicit selected query with
   all checked predicates, ordered keyset, bound and required-owner relationship.
   Preserve strict-overdue, open, visible and unsent conditions, typed active-owner
   visibility and policy-scoped recipient selection at both scan and admission.
2. Exact successful-create and supplied-due-date patch operation/field hooks;
   private schedule revision and equal-value/omission/rollback/ABA rules from
   SERVICE-001. A general row revision is not the business schedule revision.
3. Exact imported service operation, nominal immutable intent type, closed
   request/receipt/version and payload mappings. Private intent/fence/time slots
   are compiler-owned; ordinary source never constructs or mutates their authority.
   Reject mismatched identity/header, different recipient, arbitrary mapping calls,
   unknown versions, additional service effects and unchecked source aliases.
4. Exact `api_bearer.service_key` validator and narrow service-role source, with
   current credential/service/membership and entity/field/operation obligations.
   Do not broaden general entity grants or bypass output restrictions merely to
   make a selector compile. The credential stays in a secret-only host handoff;
   no ordinary source credential, simulated principal or owner impersonation.
5. A named, private sent-only completion operation bound to the admitted intent,
   current fencing generation and schedule revision, using compiler receipt
   observation time. Ordinary routes/actions/jobs cannot invoke it or supply
   receipt/fence authority. The service `accepted_at` remains provider evidence.

The static milestone must emit precise audit/source contracts and reject missing,
ambiguous or forged links; it does not run a no-op worker. A missing numeric profile
keeps build/execution closed. Required public-language/effect/policy review must
challenge this candidate before parser/checker/lowering changes: especially the
selection successor, sealed request/completion context and non-broadening policy
composition. Then implement AST/type/formatter/audit positives and negatives with
explicit unresolved runtime labels before lowering. Runtime completion still
requires short transactional enqueue/admission, committed possible-dispatch before
HTTP, private version/fence/revision completion, singleton coalescing/continuation,
all18 ASYNC traces and golden acceptance on both adapters. Main timer:
`RM-306-4eae95d58b42`, actual6.1-sol/high; field review continues separately.

**Delivery-binding static contract refinement, 2026-10-05:** the independent
[public/effect/policy plan review](../../tests/validation/rm306-independent-delivery-binding-static-plan-review.json)
requires DBSP-01–04 before implementation; it does not approve the preparation
above. Reconcile them with this closed recipe. No business scope or execution
profile changes. This candidate still needs the scoped refinement disposition
before parser/type/policy changes.

Exact token example (newlines/whitespace separate clauses; **no semicolons**):

```jadpo
job overdue_reminders every 15m {
    concurrency: singleton
    run: reminder_tick(ReminderRunAt(clock.now))
    retry: next_schedule
    delivery: reminder_v1 {
        selection: {
            name: reminder_candidates
            entity: Todo
            identity: Todo.id
            due: Todo.due_at
            before: operation_time
            open: Todo.status(TodoStatus.open)
            visible: Todo
            unsent: Todo.reminder_sent_at
            required_owner: Todo.owner
            owner_visible: User
            order_by: Todo.due_at asc, Todo.id asc
            limit: 500
            continuation: due_identity
        }
        hooks: {
            create: Todo.create_todo
            patch: Todo.patch_todo
            supplied: PatchTodo.due_at
        }
        service: {
            operation: ReminderMail.send_overdue_reminder
            intent: ReminderIntentId
            input: ReminderMessage
            output: ReminderReceipt
            payload_version: "reminder.v1"
            payload: {
                idempotency_key: generated_intent
                from: config.mail_sender
                to: Todo.owner.email
                todo_title: Todo.title
                due_at: Todo.due_at
            }
        }
        authority: {
            validator: api_bearer.service_key
            membership: ReminderServiceMembership
            role: ReminderRole.sender
            permit: reminder_only
        }
        completion: {
            name: reminder_sent
            field: Todo.reminder_sent_at
            time: compiler_receipt_observation
        }
    }
}
```

Grammar is closed, not an arbitrary record/expression/config map: one optional
`delivery: reminder_v1 { ... }` job clause; all five sections and every displayed
key exactly once, in any source order. Required single-declaration names and
dotted semantic references use the existing Name/name-reference token rules.
`open` alone is field-reference `(` enum-variant-reference `)`; `order_by` is
exactly two field references, each `asc`, separated by one comma; `limit` is the
literal integer `500`. `payload_version` is the exact string `"reminder.v1"`.
Closed descriptor terms are only `operation_time`, `due_identity`,
`generated_intent`, `reminder_only`, `compiler_receipt_observation`. They are not
ordinary source intrinsics or values. The payload keys are exactly those shown;
only idempotency_key accepts generated_intent, and other values are direct checked
source/config references, not invocations, aliases or caller-provided records.
Missing/doubled/unknown keys, modes, terms, operators, directions and values have
distinct recoverable delivery diagnostics; no partial descriptor becomes checked.
Canonical formatter emits the displayed section/key order and round-trips the
AST. Original three-clause jobs round-trip without a delivery clause.

DBSP-01: resolve field/reference/identity/lifecycle types, not matching identifier
strings. The `before` AST operator means strict non-null `<` against this job's
activation operation-time snapshot (never refreshed clock/scheduled_for). Require
open equality, visible/unsent Todo, required owner reference to checked User
identity, active-owner lifecycle visibility and recipient authorization **before**
ordering/limit. Cursor is the ordered typed `(due,id)` ascending tuple, privately
owned continuation, not OFFSET. The same eligibility/recipient obligations and
current business schedule revision bind admission. Reject altered/omitted
conjuncts, optional/wrong owner, post-limit filtering, dynamic/501/unbounded limits,
single/reversed cursor or other time source. This closed descriptor is not a
generic QueryExpression/Page enhancement or a generated SQL proof.

DBSP-02: binding-local selection/completion are new nonordinary semantic kinds,
never callable declarations. Resolve the intent to a nominal UUID brand and mark
its delivery origin **by exact checked identity**, not a name prefix. Seal the
bound immutable request and receipt-observation/completion context. Recursively
reject ordinary constructors, aliases/base-type conversions, signature/route/input
escape, source service invocation and raw object/fence/time/receipt construction
that could forge or expose these bound capabilities. Unbound ordinary services
retain ordinary semantics; no ordinary call is rewritten to enqueue. Bind exact
import/schema/request-body/header/receipt identity and immutable payload version.
Completion retains only exact receipt evidence and writes only the selected sent
field at compiler receipt-observation time, guarded by current fence, original
schedule revision and unsent state; accepted_at remains provider evidence.

DBSP-03: `reminder_only` is a deliberately new **delivery-phase-qualified** narrow
contract, not a generic read/update/invoke grant. Its key includes job, binding,
phase, exact service role/member source and targets. Resolve checked service-key
credentials plus service resolution and application-scoped authoritative membership
whose member is the checked Service identity and role is the declared enum variant.
Live credential/service/membership active/expiry/revocation obligations are distinct
from active recipient User visibility. Scan/enqueue may read only the eligible
Todo identity/due/status/sent/owner/title and active owner's recipient email;
admission must recheck these eligibility/recipient/role requirements against
current state and authorize only the bound service operation for the immutable
intent. Changed sender/title cannot mutate its payload; a now-unauthorized recipient
or disabled owner denies admission, per SERVICE-001. Admitted completion authority
is limited to that intent's fenced/revision-guarded sent write and retained receipt,
not a new generic privilege. No whole-User read, email output, other service,
arbitrary update, delete/purge or owner impersonation. Ordinary entity/field grants
remain unchanged, including User.email's empty policy and existing output shapes.
Worker credential secret handoff is separate from provider config.mail_api_key;
audit records slots/identities, never values, payloads or recipients.

DBSP-04: hooks resolve exactly one compatible checked create/update structural
site in the already-qualified owners and the patch's optional supplied due field.
Reject extra/incompatible mutation sites, helper/callback aliases and alternative
schedule/sent mutations that bypass the hook. Successful create establishes private
revision; supplied patch (including equal value) advances it and clears sent state
in the same transaction; omitted/failed/rolled-back patch does neither; A→B→A is
distinct. These hooks do not dispatch or blindly enqueue ineligible rows. Only
explicit job delivery AST owns orchestration; ordinary calls to run do not activate
it. First descriptor requires an effect-free nominal-Instant-to-Unit run body and
rejects ordinary provider calls/extra effects. Existing supported source clear:none
is preserved at these exact hooks; ordinary sent writes remain forbidden.

Implementation sequence after refinement review: (1) closed AST/parser/formatter
and recovery tests; (2) typed sealed links, hook-site and phase-policy analysis with
fully grounded positive fixture and each negative; (3) nonexecuting versioned audit,
semantic graph/source facts/decisions/digests and independently expected/rehashed
omission tests; (4) required consequential implementation review. Parser-only
acceptance is not a CheckedReminderDeliveryBinding: until the typed/hook/policy
stage exists, analysis rejects a parsed delivery clause with an explicit unsupported
binding diagnostic rather than exporting ordinary checked-job facts that omit it.
Runtime obligations are each
unimplemented/unproved; numeric fields remain unset, execution_profile:null and
runtime_lowering_supported:false; build stays JADPO_TARGET_JOB_NOT_IMPLEMENTED.
Accepted scheduled3/1h and provider3/30s are separate contract facts, not a complete
execution profile. Both-adapter pre-limit >500/continuation, hooks/admission/races,
all18 ASYNC traces and full golden gates remain future executable requirements.

**Closed descriptor stage checkpoint, 2026-10-05:** the independent
[refinement disposition](../../tests/validation/rm306-independent-delivery-binding-static-refinement-review.json)
approves only bounded AST/parser/formatter/recovery plus the explicit unsupported
binding guard. It reconciles DBSP-01–04 at recipe level, not typed/runtime proof.
`JobReminderDelivery` now has dedicated closed section/term/order/bound/version
data, and `parser/delivery.rs` enforces the five-section/key catalogue without
expression/config-map authority. Formatter canonicalizes section/key sets while
preserving comments/tokens and ordered cursor values. Malformed/unknown values
recover to sibling keys and later jobs/declarations, including truncated nested
descriptor recovery. Normal analysis emits
TYPE_JOB_DELIVERY_BINDING_NOT_IMPLEMENTED and adds no checked job binding; checked
export fails and target remains JOB_NOT_IMPLEMENTED. Original three-clause jobs
remain compatible. Six focused groups cover complete AST expectations, fail-closed
analysis/export/target, each required/duplicate/unknown leaf and section, closed
negative values/shapes, formatter permutations/comments/compact/idempotence, and
recovery. Ten original job tests also pass. Consequential stage-one implementation
review is [scoped approved](../../tests/validation/rm306-independent-delivery-descriptor-syntax-review.json):
19 focused groups and 77 independent probes, including actual CLI export refusal
and private build-time source/binary binding. All reviewed product/test pins match
at integration; document hashes are inspection snapshots, not unrelated document
approval. Typed binding/runtime remain outside this disposition. The final formatted-source shared
[supported gate](../../build/validation/20261005T040504-41228/report.json) passes61/61,
including both adapters, with full golden compile_failed49/all44 not_run. Three
existing formatter corpus suites pass. Self-review fixed extra generated blank
lines and a recovery boundary that mistook qualified config/input values for
declaration starts; both failures and their regressions are retained. Typed hook/role/sealed contracts
and execution profile remain absent. Main timer RM-306-0c82ac0c854a, actual6.1/high,
includes the following independent provenance-correction work while reviews ran;
no task-completion marker or worker implementation claim.

**Next typed binding stage — source-grounded preparation:** retain the guard until
all typed-origin, hook and phase-policy links are complete; deleting it alone would
silently discard delivery semantics. Current `analyze_sources` builds graph, types,
failure/entity models and then policy, so the completion boundary must see all of
them before exposing a CheckedReminderDeliveryBinding. Ordinary nominal job-run
validation must be reused rather than skipped when that guard is eventually
replaced. The first typed milestone is one unambiguous binding per selected
entity/store/hook contract; multiple competing intent/request/hook owners are an
explicit unsupported ambiguity, not a generic workflow expansion.

Type resolution must use effective inherited nullability and exact reference/
identity/catalogue relationships. The migration's due_at is Instant?; the frozen
provider schema also permits nullable due_at, but strict-before selection excludes
null at scan/admission. Do not narrow or rewrite either source schema to pretend
that runtime selection is a type proof. The owner relationship is the checked
`references owner_id: User.id as: owner`, not an invented `owner` field. Resolve
actual TodoStatus/open and active User lifecycle predicates, exact primary
authority stores, imported operation/schema parity and nominal Uuid intent.

The positive fixture must supply missing service/config/intent/member/role/job
declarations explicitly, initially in test-owned analyzed source rather than
breaking current working migration feature targets with an unsupported job.
Reuse the migration's checked authentication source and exact create/patch hooks;
replace only the fixture's historical message key with the selected nominal intent.
Authoritative application membership must reference actual Service.id (not just
the existing policy model's `principal: Service` string, which alone does not prove
identity). Its chosen member/role/store is the only source for the private phase
resolver; no global role-union or caller claim may substitute. Do not broaden
ordinary entity/field policies or copy provider fixtures that construct intents:
they are ordinary unbound service tests, not sealed delivery-origin positives.

Implementation must enforce recursive sealed-use restrictions across aliases,
record fields, callable/route/input/output boundaries, constructors, invocations
and writes; the sole permitted source type occurrences are the exact bound
service/request contract declarations. Preserve ordinary unbound service semantics.
Create/patch sites are explicit qualified structural addresses, with exact clear
and supplied semantics, not inferred name suffixes or broad row-revision hooks.
Closed phase contracts stay separate from generic PolicyEffect grants. Before
changing this boundary, refine its concrete analysis/data ownership and obtain
the required proportional public/effect/policy disposition; then implement positive
and origin/role/hook/selector negatives plus independently expected audit/graph/
decision/export conformance. This preparation is not typed implementation approval,
runtime authority or an answer to the numeric question.

**Typed binding implementation recipe, 2026-10-05:** the next bounded stage owns
the following data/analysis boundaries, not another configurable workflow language.

- `semantic/typecheck.rs` factors the existing nominal schedule check into one
  helper returning `Option<CheckedJobBinding>`. Plain jobs retain their existing
  checked rows. Delivery jobs are validated by that same helper but do not publish
  an ordinary row, and the unsupported diagnostic remains until the complete
  boundary below exists. This preparatory refactor alone grants nothing.
- Core `delivery.rs` owns `DeliveryModel` and an opaque
  `CheckedReminderDeliveryBinding` with private fields/constructor and read-only
  accessors. It runs after graph, typing, failures, entity model and policy. It
  consumes an explicitly named *candidate* (never a checked binding) from semantic
  type analysis for resolved nominal/field/reference facts, reuses the checked
  schedule, and validates exact structural hooks, sealed uses and phase authority.
  No binding is inserted if any prerequisite/source diagnostic exists. Core adds
  delivery diagnostics to the normal semantic diagnostic stream before export.
- Type-level candidate facts use the existing effective type catalogue: exact
  nominal Uuid intent, exact service input/output signatures and record fields,
  inherited nullable Instant due/sent fields, identity and reference aliases,
  enum/open variant, configuration source, lifecycle predicates and authoritative
  store. String display/prefix matching is not proof. No new authored intrinsics.
- Sealed declaration occurrences are explicitly allowlisted at the bound intent,
  request, receipt and imported service contract; aliases, recursive containment,
  constructors/casts, other callable/route/input/output types and ordinary selected
  provider invocations are rejected. An effect-free direct nominal run action is
  required; its ordinary invocation remains an ordinary empty action, not a worker
  activation. Unbound service fixtures keep their existing semantics.
  Grounded migration reconnaissance exposed the existing effectless-action invoke
  requirement (POLICY_EFFECT_UNGRANTED without a rule): the positive run action
  has an explicit `ReminderRole.sender: [invoke]` policy, no entity/provider effect.
  This ordinary invoke rule is not the private phase resolver or activation right.
- Exact create and patch structural sites are resolved by callable identity and
  parameter/field identity, not local spelling. Require one compatible create and
  one patch site: open/unsent initial creation with the supplied due source;
  optional patch due field plus `none when <that parameter>.due_at supplied`.
  Compiler hooks will operate in the owning mutation transaction. Reject competing
  sites, helper indirection and all other authored due/sent writes; unrelated
  status/delete mutations remain ordinary. Omission, explicit clear, equality,
  rollback and ABA are separate later executable obligations, not syntax proofs.
- Private phase facts are a closed structure indexed by exact job/binding,
  selection/admission/dispatch/completion phase, Service principal, selected
  membership/role, exact entity/fields and service operation. Validate actual
  `Service.id` member reference, authoritative same-store membership, role enum,
  api-key credential verifier/active/expiry/revocation and Service resolution.
  This does not add generic `PolicyEffect` grants or borrow a global role union.
  User visibility remains a recipient check, not the service actor's authority.
  Completion uses the admitted authority and current intent revision/unsent fence;
  do not invent extra completion-time live-role/visibility rules.
- Checked private graph nodes are added only after all static gates succeed,
  with dedicated selection/completion node kinds and typed binding-owned edges
  stored in `DeliveryModel`, not ordinary `CallEdge`s. Stable identities derive
  from qualified job/binding/phase names, not byte offsets or allocation IDs.
  Source cannot call these nodes; ordinary resolution runs before their insertion.
- Existing job audit/approval facet gains versioned optional delivery facts from
  the opaque model, retaining every schedule and ordered cursor plus exact hooks,
  payload, authority and completion facts. Each is included in source facts,
  decisions/digests and expected/rehashed-omission tests. No opaque source JSON,
  self-authored broad capability or silently omitted delivery row is acceptable.

Implementation gates: grounded test-owned migration fixture first; common nominal
schedule validation while retaining refusal; resolved type candidates and every
sealed/identity negative; exact hook/phase validation; complete nonexecuting model
and audit/graph conformance; consequential implementation review. Until the complete
static boundary passes, keep the guard. Even after it passes, every runtime/live
authority/transaction/trace obligation remains individually unproved, profile is
unset and target stays JOB_NOT_IMPLEMENTED. This recipe needs independent
public/effect/policy disposition before the unsupported boundary changes. Main
timer RM-306-92ce2827332b includes source preparation and syntax-review integration.

The independent [typed plan review](../../tests/validation/rm306-independent-typed-binding-plan-review.json)
scopes architecture approval to the candidate/opaque-core direction and requires
TBP-01–03 before guard replacement. Its source pins describe the inspected common
schedule-helper epoch, not later selector implementation or a compiled binary.
The following refinements settle those routine integration details without new
owner decisions or frozen-contract changes:

**TBP-01 — atomic finish/export:** `finish_delivery_candidates` is core-private,
runs once after all prerequisite Error diagnostics, and is the only constructor
of completed delivery rows and private nodes. The eventual integrated semantic
checker emits incomplete candidates and ordinary schedule errors, not a permanent
unsupported error that core would need to suppress. The unsupported diagnostic
then belongs to the incomplete core finish branch; do not clear arbitrary errors
or publish candidate schedules through `typing.jobs`. Until the entire replacement
exists, the current type guard remains unchanged. Finish validates all descriptors
before committing any checked delivery nodes/rows; a failure publishes diagnostics,
not a partial checked model. Checked export verifies each authored descriptor has
exactly one completed binding or an Error. Audit explicitly unions ordinary rows
from `typing.jobs` with completed delivery rows from `DeliveryModel`; mixed projects
retain both without duplicates or filtering delivery through the ordinary row
predicate. Ordinary run failures/effects remain separate from private service
outcomes and unproved durable dispositions. Tests require full row/source/phase/
edge expectations, mixed projects, failure-with-no-checked-node and fresh rehashed
omissions. Semantic-only candidates cannot issue a checked core export.

**TBP-02 — complete write census:** traverse every root/owned callable and inline
route body, nested block/expression/branch, entity create, all explicit and
conditional update fields, whole-patch effective record fields, lifecycle initial
and transition set fields, and generated-field declarations. Resolve exact logical
entity/field identities before comparing the two-site structural allowlist; reject
all other creates of the bound entity and all other due/sent writes, including
unreachable branches, helpers, inherited patch fields and transitions. Unknown
patch provenance/target is an error, not an empty footprint. The create/patch
allowlist permits only its exact compatible site, not every mutation inside the
named callable. Status/delete mutations are allowed only with a proven disjoint
footprint. Cycle-safe type/AST traversal rejects unsupported aliases instead of
claiming arbitrary alias/control-flow solving. Local parameter renames preserve
identity; different supplied parameter/field, additional patch/create/conditional
set or transition must reject. Static census is not atomicity/rollback/ABA proof.

**TBP-03 — closed phase obligation table:** every row is indexed by the exact
job/binding and selected Service principal, validator, application membership and
role identities. These are nonexecuting source obligations, not implemented live
permissions. Resolve explicit reference metadata before primitive ancestry:
membership effective member must target the selected entity's actual Service.id
identity, not another Uuid field; api-key credential principal and Principal.service
resolution id mapping compose to that same identity and authoritative store.
Do not accept the ordinary role-type union as selected membership evidence.

| Phase | Exact scoped effects / requirements | Evidence still required |
|---|---|---|
| Selection / intent enrollment | Read selected Todo id/due/status/visibility/sent/title/owner relation and active visible owner's authorized recipient email; strict non-null overdue/open/unsent and owner/policy filtering before due/id order and500 limit. Persist private unique revision intent and immutable mapped payload, not a general Todo write or blind mutation-hook enqueue. | Generated both-adapter scan, pre-limit filtering, keyset continuation, transactional identity/payload uniqueness and recipient authorization. |
| Admission | Recheck current revision, overdue/open/unsent/visible Todo, active authorized immutable recipient, selected service credential/service resolution/application membership/role, external operation and remaining shared budget. Commit current fence and possible_dispatch before releasing the transaction. | Actual live revocation/expiry/member/store checks, immutable payload comparison, budget/ordering/fence races and committed admission cut. |
| Dispatch | Consume exactly the admitted intent/operation/authority under the frozen provider attempts/timeout/egress/unknown contract. No independent generic invoke grant, recipient substitution or additional live-policy cut; no database transaction spans HTTP. | Generated adapter/worker integration, crash/uncertainty/cancellation/retry evidence; no automatic unknown replay. |
| Completion | Retain exact-key validated receipt against its original immutable intent. Under current fencing generation, write only selected sent field at compiler observation time when original intent schedule_revision equals current Todo schedule_revision and Todo is unsent. Superseded receipt remains on A; it cannot mark B. | Same completion transaction, receipt-origin/fence/revision/unsent race proof on both adapters. No invented post-admission role/visibility reauthentication. |

Audit records each exact allowed target set, validator/member/role identity and
individual unproved live requirement without credential/recipient/payload values.
Wrong scope, nonidentity member, principal mapping, validator, competing binding
or cross-job reuse reject. The empty ordinary run invoke rule proves none of these
phase permissions; ordinary email-read/output denials and protected policy bytes
remain unchanged. Consequential implementation review still governs the complete
static model; this refinement is not guard removal or worker approval.

**Guarded typed prerequisite checkpoint:** the independent
[refinement disposition](../../tests/validation/rm306-independent-typed-binding-refinement-review.json)
approves TBP-01–03 at recipe level, with no remaining plan findings or additional
owner decision for faithful comprehensive static implementation. All18 recorded
context/contract pins matched before this checkpoint integration; the reviewed
refinement region remains unchanged. This is not implementation/guard/runtime
approval. Common nominal schedule checks now run
for delivery jobs too, but every descriptor still reports unsupported and has no
ordinary checked-job row. `typecheck/delivery_selector.rs` adds necessary exact
identity, mutable nullable Instant fields, enum/variant, ordered cursor, completion
field, authoritative same-store visibility contracts and required identity-owner
alias checks. Effective inherited nullable types are used rather than the raw
owning-reference nullable flag. No checked candidate/model, sealed-use proof,
mutation census, phase resolver, graph node or runtime is exported. Four new
grounded groups plus6 syntax+10 ordinary jobs+3 formatter groups pass23/23.
The test-owned fixture uses actual migration/auth/hooks/config, pinned service and
selected persistent application membership; only the historical message key is
replaced in memory. Initial missing invoke permission was corrected with its
narrow effectless action rule, and an assertion using source validator spelling
was corrected to the actual authentication graph identity. The first full gate
[failed](../../build/validation/20261005T043522-49823/report.json) on diagnostic
conformance indexing: the new selector code was asserted in a non-test helper.
An explicit assertion in the executable negative test now supplies that evidence;
the failed report remains historical, not a claimed passing gate. The focused
diagnostic conformance recheck passes. The corrected fresh full
[supported gate](../../build/validation/20261005T043813-51236/report.json) passes61/61
including both adapters, with full golden still compile_failed49/all44 not_run and
release_equivalent:false. Scoped selector/common-schedule implementation review
initially remained pending. The subsequent
[selector review](../../tests/validation/rm306-independent-selector-prerequisite-review.json)
requires SEL-PRESENCE-01: the selected status field also must have required presence.
Its private41 probes passed40 and cleanly reproduced optional status slipping
through the necessary selector check (the unconditional guard still refused all
checked delivery). Main added the grounded regression and reproduced red, then
added the one-line `status.optional` rejection. Syntax-clean optional due/sent/owner
regressions were also added. Corrected4+6+10+3 focused groups pass23/23, and a scoped
correction review is pending. All26 diagnostic unit and5 discovery tests also pass.
The61-step report above pins the pre-presence-correction
source; it is not claimed fresh for these new bytes. No runtime lowering or ordinary
policy changed, so focused frontend/corpus/catalogue checks are proportionate for
this correction; the full golden finish still requires a new full verifier.
Optional strict Clippy exposed an untouched core/build.rs
needless-borrow and five semantic/lib.rs boolean-comparison warnings; no unrelated
source was rewritten. Semantic-library Clippy passes with only the five existing
boolean warnings explicitly allowed; full strict Clippy is not claimed. Touched
source formatting/diff checks pass. Main timer
RM-306-92ce2827332b and correction RM-306-0b1430099715, actual6.1/high; whole task
remains open.

**Guarded prerequisite evidence (before complete static finish):** the
[selector correction review](../../tests/validation/rm306-independent-selector-presence-correction-review.json)
resolves SEL-PRESENCE-01 with41/41 independent probes. Necessary service/type
candidates now resolve the exact checked nominal intent, operation and closed
request/receipt/payload fields; they remain explicitly incomplete and cannot
publish an ordinary job or delivery authority. Recursive typed containment and
source-origin sealing reject ordinary intent/request/receipt use and selected
provider calls. Independent [initial review](../../tests/validation/rm306-independent-service-seal-prerequisite-review.json)
found unused route paths and skipped invalid route/job/fixture expressions.
Main reproduced red, added seven syntax-clean regressions and completed the root
walk, including lifecycle and authentication expressions. The
[correction review](../../tests/validation/rm306-independent-service-seal-correction-review.json)
approves this bounded prerequisite with46/46 matrix,7 boundary and8 additional
root probes; its final semantic/test/fixture pins match this checkpoint. Prior
failure reports remain unchanged. Neither review approves hook/phase/runtime work.

Core `delivery_hooks.rs` adds exact direct create/patch allowlisting plus complete
nested expression, lifecycle, generated-field and whole-patch footprint census.
Parameter renames and proven disjoint status/delete changes remain allowed;
competing creates/due/sent writes, wrong supplied/reset origins and unknown patch
provenance reject. Core `delivery_phases.rs` currently checks necessary *identity
composition*, not a completed phase table or live grant: exact API-key credential
reference, active Service resolution and identity mapping, selected same-store
authoritative application membership/role and competing binding rejection. Its
grounded positive exposed a test-fixture issue: legacy `id: Uuid identity` did
not retain an EntityDossier/store. The fixture now uses explicit `identity: id`;
the new authority test verifies retained primary persistence and exact reference.
No parser widening, ordinary policy change or real migration edit was used.
The [census review](../../tests/validation/rm306-independent-hook-census-prerequisite-review.json)
found HC-I01: an outcome success binding shadowed an outer status-only patch
parameter, but the census retained that outer origin and missed a due-field write.
The clean regression reproduced only the unsupported guard before correction.
Arm-local origin traversal now removes the shadowed parameter without changing
subject, sibling or post-match scopes. The
[correction review](../../tests/validation/rm306-independent-hook-census-correction-review.json)
approves this bounded correction:16 focused groups and12 adversarial/control probes
pass; helper/test/fixture hashes pin that correction epoch, before later captured
hook facts and positive-finish test changes. The success-arm Reject case explicitly
retains FAIL_OUTCOME_SUCCESS_VALUE_REQUIRED and is traversal evidence, not clean
source acceptance. Original findings and red evidence remain preserved. This is
not a new whole-census, phase/model or runtime approval.

Core now captures four closed **incomplete nonexecuting phase candidates**, not
checked authority. Private construction stores actual validator, credential,
Service identity/principal mapping, selected application membership/member/role
and same-store identities, active-owner visibility and exact payload sources.
Selection/enrollment has no ordinary Todo write; admission lists live eligibility,
recipient and authority rechecks plus budget/fence/possible-dispatch obligations;
dispatch consumes admitted authority without another live cut or transaction over
HTTP; completion retains the original receipt and limits its authored-field write
to sent, guarded by fence/revision/unsent and compiler observation time. All live
requirements remain individually unproved. Private target identities are binding-
qualified; no allocation IDs, offsets, credential values or generic grants enter
these facts. A read-only explicitly candidate inspection API cannot export them
as a checked job. Competing bindings clear the prerequisite set. Exact whole-table,
renamed-job, source-offset/node-allocation and inactive-recipient negatives join
the existing tests:7 binding+5 hooks+7 authority+6 syntax+10 ordinary jobs+3 formatter
groups pass38/38 at that guarded epoch. The initial phase review subsequently
found PHASE-BUDGET-01 (dispatch omitted the shared admitted budget) and
PHASE-LIVE-02 (admission lacked individual resolved live-check slots). Main
reproduced red, then bound dispatch to the same invocation budget and added eight
individual admission predicates and its dispatch predicate, all unproved. The
[correction review](../../tests/validation/rm306-independent-phase-candidate-correction-review.json)
approves those two bounded fixes:8 authority groups,39 identity/ordinary probes,
4 read-only boundary controls and4 slot/rename/allocation probes pass. Its pins
precede later graph/source-fact factoring; it does not approve the full opaque
model, captured hook metadata, checked graph, audit or runtime.

The earlier guarded integration added core-private `finish_delivery_candidates` and
opaque binding/model types with private construction and read-only access. Finish
runs after every syntax/graph/type/failure/entity/policy gate, retains all errors
(including the unchanged semantic guard), checks exact descriptor/source identity,
candidate cardinality and job/callee ownership, and commits no partial rows.
Error-free incomplete sets get explicit unsupported diagnostics rather than silent
omission. The model remains empty for every actual delivery source while the guard
is retained; this is not a usable checked boundary. A synthetic negative unit test
checks each existing Error stream independently and the incomplete/no-error branch;
current7 authority+7 binding+5 hooks also pass. The subsequent whole
`cargo test -p jadpo-core` passes, including98 unit tests, all integration suites
and doc tests on these additions. Positive completed-model construction,
typed private nodes/edges, complete mixed audit/export and consequential full-model
review are still required before replacing the guard. The61-step report below
precedes these atomic-finish additions; current focused evidence does not refresh
the entire supported/runtime gate.

**Current complete static implementation checkpoint:** the sole opaque finish now
joins exact typed descriptor/schedule, captured validated hook-site parameter and
field identities, closed phase obligations and checked imported service metadata.
It runs after all syntax/graph/type/failure/entity/policy Error streams, preserves
every original error and publishes no partial binding/node set. The permanent
semantic unsupported guard has moved to the incomplete Core finish branch, as
TBP-01 requires; delivery still never enters ordinary `typing.jobs`. Dedicated
selection/completion nodes and canonically sorted typed binding-owned edges are
committed only after all rows are ready. They add no ordinary call edge or policy
grant, and source resolution precedes their insertion.

The checked model feeds an explicit ordinary/delivery union in schema2 job audits,
normalized metadata, approval source facts/semantic graph/decisions/digests.
Ordinary run failures and may-call service effects stay separate from private
service outcomes and pending durable dispositions. Runtime/live authority remains
false, execution profile null and target JOB_NOT_IMPLEMENTED. The grounded new
`validation_job_delivery_model.rs` tests independently expect the entire private
graph, exact mixed rows, stable facts under changed offsets/allocation, changed
resolved membership digests, source-error atomicity and ten rehashed omissions.
The initial four positive cases reproduced the old guard before replacement;
the mixed test's mistaken `name` key was corrected to the existing `job` schema.
All42 focused groups subsequently passed (8 authority+7 binding+6 hooks+5 model+
6 syntax+10 ordinary jobs). Additional candidate-corruption and incomplete-export
checks are being verified; their first descriptor-mutation test used a nonexistent
field on the closed payload-version enum and was corrected to an actual identity
path. Two atomic unit groups pass, preserving each original error stream and
rejecting eight missing/duplicate candidate/failure mutations without node changes.
Consequential [whole-static implementation review](../../tests/validation/rm306-independent-static-binding-implementation-review.json)
passes the bounded source/type/seal/hook/authority/atomic/private-graph and positive
audit/omission controls, including independent46 service/seal and39 authority
cases,11 rehashed substitutions and source-call/opaque-constructor refusals. It
withholds approval for STATIC-EXPORT-01: mutable delivery failure-row identity
was not checked against the opaque schedule, allowing an orphan panic or a
contradictory run callee. Main independently reproduced the orphan panic. The
correction retains the exact ordinary-run `JobFailureSet` in the opaque binding,
feeds audit from that owned contract, and rejects all identity/callee/failure/
may-suspend drift before export. Core finish also rejects orphan failure sets.
Two atomic unit groups (now10 corruption mutations),6 model groups (now11 rehashed
omissions plus delivery-only/mixed failure mutations) and10 ordinary-job groups
pass after correction. The bounded independent
[static correction review](../../tests/validation/rm306-independent-static-binding-correction-review.json)
approves the corrected static boundary:18 focused groups and21 independent
positive/export-mutation/rehashed-omission checks pass at its pinned source.
The original red review remains intact. This is scoped static approval, not
runtime traces or task closure. Main timer RM-306-e1fbcb623590 retains actual6.1-sol/high
and the original estimate; measured partial-session effort is not remaining effort.

**Fresh supported integration and independent runtime continuation:** the
[no-incremental supported rerun](../../build/validation/20261005T180247-50212/report.json)
passes61/61 on SQLite/PostgreSQL at the complete-static epoch before STATIC-EXPORT-01
correction and schedule storage. The prior
[attempt](../../build/validation/20261005T105203-48246/report.json) stopped before
tests because Rust's incremental dependency-graph path disappeared; no product
assertion failed. The newer61 result still records compile_failed49,44 unexecuted
golden cases and release_equivalent:false. A newer
[61-step supported gate](../../build/validation/20261005T204518-63732/report.json)
covers the static export and schedule-read corrections, before generated hook
lowering. It passes61/61; full golden remains compile_failed49/all44 unexecuted.

Private business schedule storage now establishes revision1, atomically increments
an existing revision and reads canonical full-int64 revision text in the actual
source transaction/adapter. Its counter is separate from ordinary change-log,
compiler-source, clock and claim revisions; duplicate creation cannot reset retained
history and missing/exhausted advancement refuses. Active transaction lifetime and
nested savepoints remain enforced. Four initial focused groups pass26 assertions
on SQLite; the original persistence suites passed83/83 on SQLite (753 assertions)
and fresh PostgreSQL (744 assertions), including commit/rollback/client expiry, equal/ABA
caller-controlled advancement, no-change omission/failed-transaction controls,
concurrent increments, fresh-process recovery and int64 exhaustion. These are
explicitly **private host/storage tests**, not checked generated Todo hook,
live admission, service dispatch or worker evidence. The initial independent
[storage review](../../tests/validation/rm306-independent-schedule-revision-storage-review.json)
found SR-I01: a caught native PostgreSQL revision-read failure could report source
success while the aborted server transaction rolled back prior writes. Main's
valid-ID native SELECT1/0 regression reproduced83 pass/1 fail (returned source_kept,
stored source row absent). Read schema/SELECT/validation now use the existing
nested savepoint. Corrected complete suites pass84/84 on SQLite (758 assertions)
and PostgreSQL (749 assertions). The
[correction review](../../tests/validation/rm306-independent-schedule-revision-correction-review.json)
approves SR-I01 narrowly:5 focused groups,4 independent probes per adapter,
native read recovery, validation and cleanup-poisoning checks; writers unchanged.
Original red evidence is preserved. No profile default, authored grant or mail
activation was added.

**Persistence-only generated hook continuation:** `target/delivery_hooks.rs`
selects only the finished opaque binding's checked entity/identity and exact
qualified patch callable. The production persistence renderer encloses create
and the bound patch in source savepoints, establishes revision1 after successful
INSERT, and advances after successful due-supplied patch (including equal/clear)
in the same transaction as its authored sent reset. Omission, missing rows,
ordinary policy refusal, source rollback and caught revision-helper failure do
not leave half-mutations. The public target still refuses every job; no worker
or profile is emitted/enabled. The component test uses complete checked fixture
sources, clears no source errors and rewrites no generated code. Its explicit
internal persistence sink is not public full-target or authored-action evidence.
Test-owned user principals/raw setup seeds are not worker authentication/grants.
The original ten actual hook component cases pass on each adapter (50 assertions each), including
ABA, concurrent updates and fresh-process recovery. Independent hook review is
recorded below; runtime metadata remains unproved/nonexecuting.

Initial component runs exposed an existing destination encoding defect: owning
reference PatchTodo.due_at reached SQLite as raw text rather than Instant's
numeric storage. Initial SQLite4 pass/5 fail and PostgreSQL8 pass/1 fail are not
green evidence. Patch argument lowering now uses the checked destination field's
representation root rather than unresolved input owning-reference spelling.
A separate ordinary job-free public-target regression now passes with the
10 hook cases on both adapters (11 cases/54 assertions each). Its first
test fixture incorrectly used reserved callable `patch`, then a disallowed
field-selection patch binding. Source-error assertions refuse both; fix the fixture
to `var changes: AlarmPatch = input.changes; patch: changes`, without widening
syntax or clearing diagnostics. All six source Error streams are asserted empty
before emitting this ordinary target. The initial control also failed because
invalid source had produced no patch method; that is not product success.
The registered supported [expanded gate](../../build/validation/20261005T205845-69223/report.json)
passes63/63 including both11-case component modes. Its Rust workspace began before
the final ordinary-control test-fixture corrections; later CLI/runtime component
builds compile the final source. Product hook/encoding code was unchanged during
that gate. This is not a single immutable test/source snapshot or protected
executable attestation; record the epoch limit and obtain a fresh full gate after
review corrections and before whole-golden completion. Full golden still has49
compile diagnostics/all44 unexecuted; release_equivalent:false. The pending
independent hook review cannot be replaced by this supported-check result.
The later stable-source [gate](../../build/validation/20261005T210538-72399/report.json)
passes63/63 at the final original hook/control epoch without that fixture split;
it predates the newer native authority implementation. Dirac's
[hook review](../../tests/validation/rm306-independent-generated-schedule-hooks-review.json)
found no scoped hook/encoding product defect but required GH-ENV-01: remove the
new component DB URL and output directory from inherited verifier environment.
The production sanitizer now removes both before every subprocess; a harmless
canary checks the actual helper without connecting/writing. All22 Python tests
pass. The [independent correction](../../tests/validation/rm306-independent-generated-schedule-hooks-correction-review.json)
approves that fix narrowly (10 focused Python tests and5 control/mutation probes).
Original red evidence is unchanged; concurrent authority work is not covered by
this approval.
Main timer RM-306-e1fbcb623590 is a partial checkpoint (871.56 active minutes,
618.09 verification minutes already included), not a completed task or forecast
of remaining work. Continuation RM-306-ebb0f80e9c22 retains actual6.1-sol/high and
the original1–4h slice forecast. The stable-source hook gate and review correction
are integrated; the newer authority gate passes and review remains pending. No required
source/runtime gate has been weakened and the technical golden Goal remains active.

**Native worker-credential / admission-authority continuation:** the actual
generated authentication host verifies the selected direct service key through
its configured HMAC, declared credential storage and service resolution, returning
an empty frozen WeakMap-branded proof. It retains no random secret and cannot be
substituted by a service-shaped principal, serialized/copy/prototype object or
another storage host. Reinitialization (including failure) withdraws old proofs.
The opaque checked binding supplies borrowed typed resolved identities to
`target/delivery_authority.rs`; no JSON metadata is deserialized into authority.
Its private `check_delivery_authority` requires the owning active transaction,
uses a nested source savepoint, re-reads actual credential verifier/status/expiry/
revocation, selected service identity/name/active state and the selected application
membership/member/role, and validates the matching private credential metadata.
PostgreSQL locks those live rows through the owning commit; database time is sampled
after lock waits and metadata checks so expiry while blocked cannot slip through.
This is the admission phase only, not an extra post-admission cut or ordinary grant.

The unmodified production app/auth/persistence component renderers consume the
complete checked source; public job derivation still refuses execution and facts
remain runtime:false/profile:null. The test-only membership enum's ungranted
`observer` supplies a real wrong-role control. Both adapters pass21 component
groups/106 assertions (11 original hook/control groups plus10 authority groups):
crypto/proof negatives, credential/service/membership changes, verifier/metadata
tamper, configuration invalidation, expiry across waits and native SQL failure
recovery. The fresh stable-source [supported gate](../../build/validation/20261005T215123-82165/report.json)
passes63/63 including both21-case/106-assertion component modes and existing
ordinary authentication/credential/lifecycle/runtime controls. All product/test
pins stayed unchanged through the run. Full golden remains compile_failed49,
all44 cases unexecuted, release_equivalent:false; this is not protected executable
attestation or full worker proof. SQLite additionally observes test-owned mutation/rollback in its actual
adapter; PostgreSQL observes concurrent disable waiting on the admission locks
until owning commit. Those setup mutations are not granted source actions.
The first authority test fixture attempted an ungranted ordinary Service update
and correctly got no change; a subsequent adapter probe incorrectly assumed Bun
SQL.prototype existed. These red fixtures were corrected without adding policy
or rewriting generated code. Initial Rust closure lifetime errors were corrected
before component success. Independent authority review remains pending;
actual selection/intent/fence/dispatch/receipt completion, all18
generated traces and44 complete-golden cases remain open. Continue those eligible
integration pieces under the same6.1-sol/high pin; no numeric profile is assumed.

**Next eligible integration slice (short plan, settled DBSP/TBP/SERVICE semantics):**
use the finished binding's exact selected entity, identity, owner reference and
phase-owned active/visibility facts to lower the private candidate query. Apply
strict non-null overdue/open/visible/unsent and active visible authorized owner
predicates before due/id ordering and the literal500 limit. Reuse checked lifecycle
predicate lowering with qualified columns; do not guess `owner_id`, append policy
after LIMIT, impersonate an owner or turn a source-facts JSON blob into authority.
Use captured activation time and typed due/id keyset state, not OFFSET/refreshed
time. Enroll each selected business revision under the existing same-key lock in
the owning adapter transaction. The current generic storage enqueue allocates ID
after canonicalizing its caller payload; it cannot alone construct the bound
request's generated idempotency key. Factor a compiler-owned immutable request
builder inside that allocation boundary: existing unique revision returns its
retained original ID/payload, new revision gets one ID and canonical request atomically.
Changed title/sender must not rewrite the retained request. Preserve the general
host/storage identity-conflict controls; do not pass an authored callback or key.

Then combine current source revision/eligibility/immutable recipient, selected
live authority, remaining budget and fence/possible-dispatch checkpoint in one
admission transaction. Release the transaction before the exact provider adapter;
dispatch and completion consume admitted authority without another live-role cut.
Before assembling that worker, resolve the self-review issuer concern with the
authorised reviewer: storage branding alone does not demonstrate that a same-store
separately constructed native authentication factory is the currently generated
host. Pin the actual compiler-initialized issuer if required, without accepting an
arbitrary proof-reader callback as authority. This is a routine implementation
boundary, not a new owner profile question or assumed whole-worker approval.
Acceptance for the next slice includes >500 unauthorized-prefix/continuation,
duplicate/rollback/ABA/concurrent enrollment and immutable request/ID controls on
both adapters, followed by proportional independent integration review. Public
job/profile refusal stays until complete runtime conformance exists.
Main continuation timer `RM-306-ebb0f80e9c22` closes **partial** at52.02 active
minutes (41.30 verification minutes already included), actual6.1-sol/high with
the original1–4h slice estimate preserved. This is a completed work session, not
RM-306 or Goal completion. Reselect the short-plan integration slice above and
the pending independent authority disposition on continuation; no terminal blocker
is asserted while those independent in-scope pieces remain eligible.

**Current native issuer correction, 2026-10-05:** the independent
[authority review](../../tests/validation/rm306-independent-worker-authority-review.json)
returned required correction **WA-I01**: an unpublished native factory using the
exact real storage and old key could survive failed initialization/key withdrawal.
This is a selected native issuer/configuration gap, not a remote forgery finding
or new owner decision. The red evidence and original review remain intact.
The correction uses a private native host WeakMap brand and a read-only generated
application bridge that closes over its currently published authentication host;
the persistence authority reader cannot supply a substitute issuer, principal,
callback or setter. Both proof checks still bracket the awaited owning-transaction
SQL/crypto/time work. Failed initialization clears the current host before config
loading; stale completion cannot publish and explicitly invalidates its discarded
instance. Finished bindings use live import aliases in the app/persistence cycle,
not an eager read of an uninitialized persistence export. No ordinary policy grant
or public job/profile activation is added.

Main unmodified component tests now pass **24 groups /130 assertions per adapter**
on SQLite and native PostgreSQL, adding real same-storage unpublished-issuer refusal
before/failure/key rotation, current rotated-host success, withdrawal during a native
metadata digest, and overlapping initialization with newer successful/failed
generations. The independent [correction review](../../tests/validation/rm306-independent-worker-authority-correction-review.json)
approves **WA-I01 only**, with13 authority groups /76 assertions per adapter and
four fresh native import-order probes. It verifies genuine unpublished-issuer
rejection, current-host positives and generation withdrawal without a callback or
setter; it does not approve the complete worker or profile.
The fresh full [supported gate](../../build/validation/20261005T220739-88864/report.json)
passes63/63 against this stable correction epoch, including24 component groups /
130 assertions per adapter. The previous63/63 report is preserved as the earlier
epoch. Full golden still fails with49 diagnostics, all44 cases unexecuted and
release-equivalent:false. Main timer
`RM-306-11283d39d519` remains open, no task completion marker. Continue the selected
selection/immutable-enrollment integration after integrating this correction's
review and gate evidence. Full worker,18 generated traces and44 golden cases remain
open; the original60s/40s owner proposal is still unanswered and not activated.

**Current selection/enrollment implementation, 2026-10-05:**
`target/delivery_selection.rs` consumes the finished descriptor and borrowed
phase-owned active-owner identity. It finds the actual required owner FK/identity
from checked reference metadata, projects only permitted source/recipient fields
in visibility-qualified subqueries, and applies strict non-null captured-time
overdue/open/unsent/active-owner conditions before due/id order and the literal500
limit. PostgreSQL SHARE locks cover source/owner rows through owning commit; the
business revision is read **after** those locks, not from an earlier joined
snapshot. The owning client needs an operation-time snapshot and actual current
service-key proof; selected live-role authority is checked in the same transaction.
Typed cursor validation binds due/id to this binding/snapshot. This native component
accepts a checked cursor value; durable scheduler ownership/coalescing/persistence
of continuation remain to be assembled, not certified by this component API.

The private allocator wraps key/sequence/identity/request insertion in a savepoint
and allocates the UUID **before** invoking a lexically private compiler request
constructor. Host enqueue accepts neither a callback nor an idempotency key /
retain-original flag and preserves strict canonical caller-payload equality.
Bound enrollment validates and returns original canonical request bytes for an
existing revision; changed title, recipient or configured sender cannot rewrite/
rekey it. New revisions create a new UUID and exact validated request atomically.
Read-only configuration and pure checked input/cursor validators use the actual
generated app; no ordinary User.email grant, impersonation, provider call or
profile default is introduced. A retained changed recipient is **not** dispatch
permission: admission must compare it to the currently authorized recipient.

Main components pass **33 groups /185 assertions per adapter**, including9 new
groups:501 earlier unauthorized /501 eligible rows over two bounded pages; strict
snapshot/time and typed-cursor negatives; immutable duplicates/title/email/sender;
supplied equal/ABA identities; outer rollback/missing revision/concurrent scans;
genuine caught INSERT fault rolling back the page while disjoint source work
commits; fresh-process actual-host/ID/payload recovery; corrupt source/retained
request rejection. Public target refusal and all6 source Error streams remain
checked; these are unactivated components, not18 complete worker traces.
Independent review is assigned to the authorized6.1-sol/high reviewer, with actual
FK/identity renaming and post-lock revision probes requested. Fresh full
[supported gate](../../build/validation/20261005T222609-94119/report.json) passes63/63
on this stable selection epoch, including33 component groups /185 assertions per
adapter and complete84-case persistence suites (758 SQLite /749 PG assertions).
The earlier63/63 correction report proves only its earlier source epoch.

Focused generic regression limits are retained: filtering persistence tests to
`durable` gave13 pass /1 fail because a raw cancellation-table probe skipped earlier
setup; the first rolled-back cancellation also rolled back its lazy schema. This
is subset fixture dependency evidence, not a product failure or passing full suite.
Filtering `outbox|enqueue` gave6 pass /67 assertions. These direct runs used the
existing CLI, not an independently rebuilt selection-epoch binary; the fresh gate's
rebuild and complete both-adapter suites above provide source-bound generic
allocator evidence. Main timer `RM-306-11283d39d519` closes partial below.

**Selection correction / committed invocation continuation, 2026-10-05:**
The independent [selection review](../../tests/validation/rm306-independent-selection-enrollment-review.json)
requires SE-PAGE-LOCK-01: an observed PostgreSQL row-lock wait changed the500th
tuple's due date, and publishing that new boundary skipped unchanged eligible
row501. The earlier63/63 gate does not refute this reproduced defect. Selection
now captures the original bounded eligible tuples, locks the same bounded query,
and compares exact ordered identity/due/recipient and cardinality before any
enrollment. A changed page faults `delivery.selection_changed` inside the whole
page savepoint; no partial intent or cursor is published. Captured operation time,
prelimit visibility, literal500 and post-lock revision remain. The observed-lock
regression covers later due, due equal to snapshot and done status; fresh bounded
retry covers all remaining eligible IDs. The
[independent correction review](../../tests/validation/rm306-independent-selection-enrollment-correction-review.json)
approves SE-PAGE-LOCK-01 narrowly; original red probe/report stays unchanged.

`target/delivery_invocation.rs` emits unactivated native admission/dispatch/
completion components from finished bindings. One root owning transaction combines
current revision/eligibility/original recipient, current native credential/live
role, immutable cumulative3-invocation/1-hour limits, claim/fence and possible
dispatch. Only acknowledged **owning COMMIT** mints an empty WeakMap-bound one-use
invocation; released savepoint facts cannot dispatch. Execution/lease values are
explicit test-owned component inputs, **not** approved public profiles/defaults.
Database-time remaining lease/execution/lifetime is conservatively translated to
a monotonic send deadline by subtracting the entire admission elapsed time.
Dispatch uses the exact compiled service outside the database transaction and
privately captured credential: post-cut role/configuration withdrawal is not a
second live cut. Adapter-only outcome tokens retain typed success, pinned
known-no-effect failure or conservative unknown. Receipt observation is captured
at validated adapter return; provider `accepted_at` never supplies sent time.
Completion locks source before ordering key, retains the original intent receipt,
and guards sent by current business revision/still-unsent. No authored receipt,
fence, request callback, secret reader or extra entity grant is introduced.

Checks before three added tests: SQLite admission6/72; full PostgreSQL40/271,
including corrected501-row race. Added tests cover native sent-write failure/
same-outcome completion retry without HTTP, owner/source visibility withdrawal
after admission and SQLite actual COMMIT/lost acknowledgement/no token/unknown
recovery. Fresh [supported gate](../../build/validation/20261005T225347-1292/report.json)
passes63/63: current SQLite43/275 and PostgreSQL42/281 component assertions,
including the explicitly SQLite-only actual-COMMIT/acknowledgement-loss injection.
Both complete persistence suites pass84 (758 SQLite /749 PG assertions).
Full golden remains compile_failed49, all44 cases unexecuted, not release-equivalent.
This earlier gate predates the following correction. The completed
[invocation review](../../tests/validation/rm306-independent-committed-invocation-review.json)
requires CI-I01: accepted sent completion leaves the generated modification field
stale on both adapters. Other bounded examined invariants passed. The governing
rules are **TIME-D19/D21**, not the earlier mistaken TIME-D26 shorthand (D26 owns
the temporal namespace). Completion now captures its own stable operation time,
updates all checked `CreateOrChange` fields in the guarded source mutation and,
where applicable, records the returned changed row through existing derived
representation machinery in the same transaction. Sent remains the validated
adapter observation, not completion/provider time. No ordinary touch helper,
policy widening or extra post-cut lifecycle/owner check is added.

New checks retain creation time, business revision and unchanged timestamps on
already-sent/missing/superseded/stale-fence cases; sent/timestamp/receipt/state
rollback together, with the genuine outcome retryable without another HTTP call.
A separate checked-source variant renames the generated field `modified_at` and
adds a supported durable cache representation. Its native change-record INSERT
fault rolls back every completion write; retry records exact returned row/time.
SQLite11/127 and PostgreSQL49/366 pass this variant. It is registered separately
in the supported gate; neither frozen source nor emitted code is rewritten.
Initial correction exposed a real SQLite placeholder-order defect (12 passed,
3 failed); parameter occurrence/order was fixed, after which focused SQLite15/158
passed. The failed check is preserved, not relabelled green.

`runtime/delivery_scheduler.ts` adds native, unactivated singleton storage to the
actual persistence renderer: UTC interval floor, one latest pending occurrence,
one fenced active activation, bounded immutable staged page, durable cursor and
fresh operation snapshot on resume. Source selection/enrollment and staged page
will share the owning transaction; provider effects never share it. Page progress
does not advance the durable cursor until fenced finish. Five storage cases pass
both adapters, including a real fresh generated process recovering a staged page
after fixture-controlled lease expiry. These are storage components, **not** the
assembled schedule/worker18 traces; raw binding/interval/profile/cursor methods
remain trusted native seams until closed finished-binding orchestration. No
public job/default is enabled. The fresh
[supported gate](../../build/validation/20261005T232209-9778/report.json)
passes65/65: normal SQLite49/344 and PG48/350, renamed-representation SQLite50/360
and PG49/366, complete persistence84 on each. All44 golden cases are still
unexecuted, compile_failed49 and release_equivalent:false. The completed
[CI-I01 correction review](../../tests/validation/rm306-independent-committed-invocation-correction-review.json)
approves the pinned correction narrowly with no additional required finding;
original required-changes report is unchanged. The completed
[singleton storage review](../../tests/validation/rm306-independent-singleton-activation-storage-review.json)
requires ACT-STORAGE-TIME-01: malformed persisted deadline TEXT sorted after valid
DB time and authorized staging/finish. The independent original red reproduces on
both adapters and is preserved. Corrected native row decoding now validates all
persisted nullable UTC instants, finite interval/bounded canonical generation,
cursor/continuation coherence and active/idle null-state/deadline coherence under
the same row lock, before live/reclaim/write decisions. Adapter-time sampling and
final fenced write guards remain. Fourteen actual corruption states invoke claim,
stage and finish and leave every row byte unchanged on refusal; SQLite6/131 and
full PG49/434 pass. The next
[supported gate](../../build/validation/20261005T233529-18514/report.json)
passes65/65 on f98113/04b0ba plus unchanged400e35/913492: normal SQLite50/428
and PG49/434, renamed representation51/444 and50/450. This decoder epoch is
not the final portable-range correction below. No whole-worker/public/golden
approval is inherited; the earlier65/65 report is the pre-decoder epoch.

The independent [decoder correction review](../../tests/validation/rm306-independent-singleton-activation-storage-correction-review.json)
verifies the original14 states but reproduces portable year0000 acceptance on both
adapters. Its required-changes report/red probes stay unchanged. The shared native
Instant decoder now rejects year0000, matching the source field's portable0001–9999
range without normalizing corruption. The matrix adds actual year0000 operation
time (15 states × claim/stage/finish, with full durable row unchanged), plus
positive0001/9999 tick/claim/stage/finish controls. The independent
[portable-range recheck](../../tests/validation/rm306-independent-singleton-activation-storage-portable-range-review.json)
approves ACT-STORAGE-TIME-01 narrowly: both actual adapters pass7/145 focused
assertions, all45 corrupted-state refusals per adapter and retained real
PostgreSQL late-lock controls. Final pins are a9f023/126fcf plus unchanged
400e35/913492. Its capture is released, own timer closed and disposable PG stopped;
the reviewer is closed. The fresh final
[supported gate](../../build/validation/20261005T234310-22992/report.json)
passes65/65, terminal0: normal SQLite51/442 and PostgreSQL50/448, renamed
representation52/458 and51/464, complete persistence84/758 SQLite and84/749 PG.
Reviewed source pins still match after the gate; no product source changed during
capture/verification. Full golden remains compile_failed49/all44 unexecuted,
behavioural_evidence:not_run and release_equivalent:false. These checks finish the
bounded current work item, not RM-306 or the whole golden application.

Main continuation `RM-306-0965db355677` retains the verified/owner-selected
6.1-sol/high batch. New nonoverlap `target/delivery_scheduler.rs` contains a
**not yet wired or compiler-tested** closed finished-binding activation candidate:
selection/enrollment and immutable staged page share one owning COMMIT; only after
acknowledgement can the private invocation bridge call the provider. It derives
interval from checked schedule metadata, privately rebinds persisted typed cursor
coordinates to a fresh activation snapshot, conservatively clamps send deadline
to the database activation lease and leaves staged work on interruption/late
completion. This draft postdates the pre-decoder65/65 gate; inclusion in a later
source manifest does not compile or execute its unregistered module. On explicit
resumption, wire its module/bridge/method insertion and missing clock helper, then
add actual scheduler-boundary checks before any assembly claim. Do not start this
next work item under the current stop request. Storage capture is released;
handle01a10948-bc5b-72d3-a73c-a3bfb8dcf73f completed the narrow same-handle rechecks
and was closed, not restarted as another original review.
CI reviewer01a10959-550b-7f72-ab55-3165b3a611dc completed and was closed.

**Human stop request:** finish the current work item, then stop without starting
the next. The owner asked whether this is safe to resume and how long the broader
task would take. Main recommended the bounded verified completion/storage item:
its source/evidence are saved, whereas whole scheduler assembly has no reliable
short remaining-time forecast. Stop after its final gate/checkpoint; preserve the
unwired draft and do not close RM-306 or activate profiles/jobs. The host Goal
read returned `goal:null`, so there is no existing Goal to pause or complete and
none is recreated. This explicit stop supersedes earlier continuation instructions.
Delivery is now stopped; no next work item has begun. Main timing
`RM-306-0965db355677` closes partial against this checkpoint/final supported gate;
the original estimate and all failed/interrupted evidence remain intact. No
completed-task marker is added. Both authorized reviewers are completed and closed.

Failed checks retained: initial SQLite36/4 socket-denied run was environmental,
recovered with supported loopback permissions. Permitted SQLite39/1 exposed a
test JSON-property-order comparison, repaired with exact deep payload/key checks.
Initial PG regression timed out because Bun's rejection matcher waited before
the held writer could be released. That specific session was interrupted (130,
disposable cluster EXIT cleanup), then rejection was captured as data so the
observed-lock loop releases the writer. Fresh PG40/0 passes; no observation
timeout alone was treated as a stopped process. Prior main timer closes partial
25.85 active/17.93 unknown tail after the status interruption; current
`RM-306-f3e0dabc6415` retains the established6.1-sol/high pin.
It closes **partial** after the stable supported gate and same-handle verified
review waits; no roadmap task is complete. Both reviewer handles remain live at
the final10-second observation timeout, not restarted or considered terminal.

Resume point (only after an explicit request to continue): assemble durable
singleton UTC activation, coalesced follow-up and scheduler-owned bounded
continuation, retaining an immutable page across crash. Resume gets a new TIME-D05
snapshot: privately persisted due/id cursor coordinates must be rebound to that
snapshot, never substituted as scheduled_for or refreshed within a page. Recheck
saved source and independent review pins before wiring the retained draft.
Then prove full worker recovery/18 traces. Numeric profile and44 golden cases
remain open. No task marker, profile/public activation or Goal completion is claimed.

The newer full [supported gate](../../build/validation/20261005T054747-18283/report.json)
passes61/61 on SQLite/PostgreSQL at the pre-seal-root-correction/pre-phase epoch.
It is not a fresh full gate for the later corrections; full golden still has49
compile diagnostics and all44 cases not executed, release_equivalent:false.
Initial Rust header/callable-name integration errors and malformed negative helper
fixtures were corrected before focused success, not treated as product evidence.
Phase integration's incorrect record lifecycle access was corrected to the actual
dossier field. Exact-table tests exposed noncanonical private-target ordering and
a graph-allocation probe that did not move the selected node; canonical order and
an actually earlier unrelated type corrected those tests. The first newer broad
[attempt](../../build/validation/20261005T062221-32822/report.json) failed only on
the existing dev rollback test's sandbox localhost socket permission. The supported
socket/disposable-PostgreSQL [rerun](../../build/validation/20261005T062320-33344/report.json)
passes61/61, including the new38 focused groups in the Rust workspace and both
adapters. Core integration formatting changed after the Rust workspace snapshot;
later compiler-build/fixture/runtime steps pass the formatted source. No semantic
source change occurred during the rerun. Full golden remains compile_failed49,
all44 cases unexecuted and release_equivalent:false. This is supported integration
evidence, not whole golden acceptance or protected executable attestation.
Main interrupted timer RM-306-d352d9c785c4 is partial with its unobserved tail
preserved; continuation RM-306-4d666d7c8b28 records actual6.1-sol/high.

**Continuation decision:** continue the same authorised RM-306 milestone: integrate
native authority review and fresh supported evidence, then assemble checked
selection/intent/admission/fenced dispatch/completion and worker conformance under
the existing recipe. The hook/control and sanitation correction are scoped approved.
No execution activation without the required profile and runtime gates. Continue
eligible work after partial checkpoints; the Goal remains active. Numeric input
remains outstanding but does not block independent implementation work. Earlier
RM-306-4d666d7c8b28 is a partial static checkpoint, not a completed task; same
6.1-sol/high pin continues in RM-306-e1fbcb623590.

**Pending golden worker profile input:** asked once asynchronously whether to
use a **60-second execution deadline / 40-second renewable claim lease** for the
golden reminder worker only. This proposal leaves headroom around SERVICE-001's
30-second provider budget while retaining database-time fencing, finite deadlines
and no resend after uncertainty. It is not a universal default or measured hard
runtime bound; implementation must verify the selected profile. Existing 3 total
scheduled invocations / 1 hour remains accepted and unchanged. Alternative:
leave the profile unset and keep worker execution disabled. No answer assumed
and no proposed numeric profile is activated.

**Earlier continuation decision (superseded by the overnight Goal below):** model-switch/override handoff first, followed
by the concrete golden-only worker profile above; do not enable job execution with an unset profile or missing
checked binding/review. RM-109/RM-110 remain dependent on actual worker/golden
behavior. Frontend implementation review found the interval blocker above;
its correction requires scoped independent re-review, currently model-deferred,
before milestone approval. Existing
generic event acceptance and other recorded prerequisite gates remain open, not
waived or silently borrowed from conditional successor contracts. The narrow
self-disable repair is complete within scope, not whole RM-108 completion.
No new Goal, publishing, task closure or unattended runner is claimed.

**Overnight delivery Goal, 2026-10-05:** the owner explicitly selected
`gpt-6.1-sol/high` for everything and then approved starting a persistent Goal.
Own turn metadata verifies those actual settings. This supersedes the model
deferral above for implementation and authorised independent reviews; the
original deferred evidence remains historical. The Goal now covers technical
RM-108–110 completion and necessary prerequisite milestones, all original 44
golden cases on the applicable SQLite/PostgreSQL boundaries, required independent
reviews and a fresh passing `--require-golden` verifier. Preserve all evidence
gates; it does not activate conditional work, deployment, real mail or publishing,
and does not certify external assurance studies. Overnight completion is an
objective, not a promised deadline.

The independent interval-correction reviewer resumed under the direct human
model override and approved that narrow correction with no findings. Golden 60s/40s
execution/lease limits remain the original unanswered question, not silently
approved by Goal creation. Continue eligible work while awaiting that answer.
Next: accept/correct the independent frontend result, then implement the explicit
checked-worker stage when its language/policy and numeric-profile gates permit;
select independent transaction/readiness/evidence work if reminder admission is
blocked. Timing: `RM-306-e5430a80632d`; actual `gpt-6.1-sol/high`.

**Current independent continuation, 2026-10-05:** exact interval correction and
nonexecuting approval job-impact reviews are approved within their narrow scopes.
Golden worker profile remains unanswered. Continue RM-403's missing real
PostgreSQL connection-loss/concurrent-recovery evidence on a separate disposable
database: refuse connections, terminate its backends, observe safe not-ready and
route gating with liveness preserved, restore connections, then require 32 ready
responses and the same credential's original identity without restart. The initial
focused wrapper passes 8/8 with 4,655 assertions; the final explicit local URL-pair
guard and verifier environment isolation pass the fresh supported
[gate](../../build/validation/20261005T010805-27481/report.json), 61/61.
The [independent review](../../tests/validation/rm403-independent-connection-recovery-review.json)
approves the narrow connection-loss evidence with no findings, fresh isolated
compiler/targets and an independent 8/8 wrapper run. This does not prove hard response/initialization bounds or
count PostgreSQL schema initialization. Main remains the sole implementation
writer, actual6.1-sol/high; timing `RM-403-097f5d94f1f4`.

**Next RM-403 response-bound slice — short plan, review before implementation:**
the existing PostgreSQL probe requests cancellation at one second but still
awaits the native promise. Preserve that finding rather than assuming cancellation
works. Separate a finite caller-visible response from the lifetime of one active
dependency attempt: after the existing one-second monotonic budget, return safe
not-ready, retain the single-flight slot until the native attempt settles, and
never launch overlapping attempts merely because the response timed out. Late
success must not publish ready or replace the active client; both probe and
recovery initialization must pass the same deadline/attempt fence. Recovery
initialization therefore cannot unconditionally publish its ready flag before
the outer controller validates its result. Initial startup behavior and fatal
schema/configuration rejection remain unchanged in this slice.

Use compiler-private code with deterministic clock/timer seams in host tests,
not authored capabilities or configuration bypass. Ordinary regression tests
must cover cancellation that never settles, 32 concurrent waiters, bounded
response and native-attempt counts, late success/rejection, no late client
publication, retry cutoff/backoff, fatal state and eventual genuine recovery.
Pair controlled asynchronous native-promise tests with live PostgreSQL recovery
above and current generated-handler correspondence. A JS response timer is not
hard interruption of synchronous SQLite work, event-loop starvation, native
cleanup or top-level schema initialization; keep those limits open. No additional
worker profile or mail health behavior is implied. The accepted CONFIG-P5
timeout/concurrency semantics govern this implementation; independent review
must check stale publication and hidden attempt accumulation before integration.

**Response-bound implementation checkpoint, 2026-10-05:** the
[independent plan review](../../tests/validation/rm403-independent-response-bound-plan-review.json)
approves the slice with RB-01–06 refinements. The compiler-private bundled
`readiness_gate.ts` now owns one shared response, deadline timer and handled
observer per aggregate native check. Expiry returns false, marks not-ready and
advances backoff once, but retains the native slot; later requests cannot launch
another attempt while it remains unsettled. Exact deadline equality, fatal state
and newer availability-invalidation generations fence publication. Candidate
preparation is fenced again before synchronous client/ready publication.
Recovery initialization uses a per-call `publish=false`, suppressing its entire
successful publication block; genuine late fatal classification remains active.
Initial startup behavior is unchanged. Cancellation is best-effort after settling
the response, not a prerequisite to returning it.

Six deterministic groups pass (115 assertions), including 32 initial and 128 later
waiters with one native observer/timer, late/equal success, preparation overrun,
new invalidation, capped once-only backoff, throwing cancellation, late rejection
and fatal containment. The generated persistence includes the exact helper bytes
and checked recovery/publication wiring. The current disposable readiness wrapper
passes14/14 (4,418 assertions in that run), retaining live same-credential recovery,
SQLite initial/recovery locks and prior startup behavior. The fresh supported
[gate](../../build/validation/20261005T012226-44058/report.json) passes61/61,
including the14-case live readiness suite (4,419 assertions in that run).
The [independent implementation review](../../tests/validation/rm403-independent-response-bound-implementation-review.json)
approves this narrow slice with no blocking findings. Its six controlled groups
and ten in-memory complete emitted-module probes pass, including first-party
initialization, false outcomes, late fatal failures and availability invalidation.
Synthetic SQL/logical time is not live hard-latency evidence. Scope remains
async scheduler-governed response behavior, not synchronous SQLite/event-loop
interruption, native cleanup, startup hard bounds or whole RM-403/golden closure.
An initial `cargo fmt` check on the virtual manifest could not find targets;
direct rustfmt check exposed broad preexisting formatting drift. No format rewrite
or unrelated source change was made. Timing: `RM-403-0e9f34d7177f`, actual6.1-sol/high.

## RM-207/401/402 — operational failures and transaction retry

2026-10-02 reviewed implementation checkpoint: the shared semantic model now
has no HTTP default for OutcomeUnknown; Bun explicitly selects 500. Route inventory,
OpenAPI and failure-audit v2 expose the same safe operational envelope. The former
synthetic unknown-read test is replaced by a committed-write acknowledgement-loss
injection, proving a durable row, one attempt, safe response and artifact parity.
The injection tests the generated boundary, while RM-401's independent network
probe demonstrates the actual lost-ack driver behavior. Combining these is not
claimed as generated network-phase classification; RM-402 and service/job
integration still own that evidence. Core tests and focused SQLite suites pass;
full supported verification passed all 55 checks, including this case on PostgreSQL
([report](../../build/validation/20261002T194433-31665/report.json)). Timing:
`RM-207-6e2b6e07b491`, retained Astra High batch pin.


**RM-207 — L, decision package.** Keep the
[failure model](../failure-model.md)'s distinction between domain rejection,
temporary unavailability, timeout, uncertain write and defect. Build a matrix of
read failure, pre-effect write failure, known rollback and unknown commit outcome
across DB/provider/HTTP/job boundaries. Apply the owner-selected simple Bun HTTP
uncertainty mapping below. Other unmapped boundaries remain rejected; never turn
uncertainty into a retryable timeout. Background execution should retain an inspectable uncertain
state without automatic reissue. Reconcile the golden/provider mappings with
this decision, then implement the missing cases and route/OpenAPI/audit
parity. Test no secret/raw driver/provider detail and contained defects. Sol High
design/review; Luna Extra High implementation. Keep the public envelope small.

Owner-selected Bun HTTP mapping: HTTP 500 with safe code `outcome_unknown`,
request ID and static message "The operation may have completed." Emit no retry
instruction; do not claim rollback or automatically reissue. This explicit
adapter-contract addition is now recorded in the failure model; RM-207 must supply
the implementation and conformance evidence. Keep it compiler-owned,
without a per-route numeric status or application architecture decision. Other target
adapters need an explicit mapping before support can be claimed.

| Situation | Required observation / planned disposition |
|---|---|
| Read unavailable, no write attempted | Existing safe unavailable/timeout kind; no claim of uncertain mutation |
| Write rejected before start | No effects; safe operational failure; retry only under the accepted RM-401 policy |
| Transaction aborted and rollback confirmed | No committed changes/revisions; only a proven replay-safe effect graph may retry |
| Commit acknowledgement lost | Preserve uncertainty; selected explicit HTTP mapping above; no blind retry |
| Provider response lost after possible acceptance | Persist uncertain delivery identity; no success marker and no automatic new send |
| Defect or provider authentication failure | Safe envelope and internal diagnosis; never expose raw cause or classify as recipient rejection |

Planning traces to turn into executable fixtures: (1) inject failure before
commit and observe zero writes; (2) commit, drop acknowledgement, then observe
stored state plus the uncertain response; (3) accept mail and drop response,
restart the worker and run the next schedule—no second send while unresolved;
(4) reconcile a proven provider receipt and mark sent without resending.
Reconciliation requires trustworthy adapter evidence; absence of evidence must
not be treated as proof the effect did not happen. These are planned fixture
expectations under the owner-selected mapping, not executed evidence.

Implementation checkpoint 2026-10-01: the generated persistence boundary now
has a compiler-owned `OutcomeUnknownFault` marker, preserves it as the
`PersistenceFault` `unknown` kind through adapter wrappers, and maps it at the
Bun HTTP boundary to `500`, `outcome_unknown`, request ID and the static
message above. A generated SQLite runtime regression exercises the safe
envelope and confirms raw operation details are not returned. The registered
Rust generator check and `tests/runtime/validation-persistence.test.ts` pass;
provider delivery, background reconciliation and the full failure matrix remain
open for a later RM-207 slice.

Sol High self-review checkpoint: the current runtime test injects an `unknown`
fault into a read query to check envelope containment; it does **not** prove a
real ambiguous mutation or commit. The generic failure checker also assigns
`OutcomeUnknown` a 500 status in shared semantic artifacts, while the accepted
decision is specifically a Bun HTTP boundary mapping. RM-207 must confine that
transport decision, add phase-aware unknown-commit injection from RM-401/402,
and prove route/OpenAPI/audit parity before closure. The existing focused
SQLite runtime suite still passes (11 tests, 69 assertions); no task completion
is claimed from the synthetic envelope test.

2026-10-02 boundary-contract candidate: the [shared case matrix](../../tests/assurance/operational-boundary-v0.1.json)
uses proved `no_effect`, proved `no_commit`, and `unknown_to_caller` outcomes
across database and provider boundaries. The [retained RM-401 probe](../../tests/validation/rm401-phase-probe-results.json)
supports structured SQLite busy, PostgreSQL 40001/40P01 abort, and a committed
write with lost acknowledgement. The [failure model](../failure-model.md)
now states that a read with no possible side effect cannot yield
`OutcomeUnknown`, and that HTTP 500 is Bun-specific; the existing generic
failure-checker mapping and synthetic read test are explicit implementation
gaps. This is a review candidate, not the reviewed milestone. RM-303/RM-402
remain gated until separate boundary-contract review. Provider/job runtime
traces, route/OpenAPI/audit parity, and full RM-207 closure remain later work.
Run `RM-207-deb0afa4ac86` records this preparation.

**RM-401 — M, retry contract plan.** Safe transient-failure retries are enabled
by default, per the owner. The compiler derives replay safety from effects and
adapter outcome certainty; application-writing LLMs do not need to opt in or
choose a retry architecture. Never retry a domain rejection, unknown commit or
non-repeatable external effect. Retry the outer transaction after confirmed
rollback; do not independently replay nested fragments.

Starting versioned runtime policy for contract fixtures: at most 3 total attempts,
within the smaller of the operation's remaining deadline or a 1s retry budget.
For retry index k starting at 0, choose full jitter uniformly between zero and
`min(250ms, 50ms * 2^k)`. Production workers use independent randomness; tests inject
a seeded random source and monotonic clock. Exponential delay without jitter is
insufficient to prevent synchronised retries. Release locks/connections before
waiting, propagate cancellation/deadline, and share one attempt/time budget across
nested calls and adapter waits so retries cannot multiply. These numeric starting
values are engineering defaults to verify in RM-402, not measured optimal settings.

Map SQLite busy/lock and PostgreSQL serialization/deadlock failures through adapter
classification into the same semantic retry policy. Re-read authority/policy and
lifecycle guards each attempt; take a new `clock.now` snapshot per retry under
TIME-D05, while elapsed retry deadlines use monotonic time. Generate fresh attempt state;
only committed changes publish revisions/outbox effects. On exhaustion surface a
safe operational failure with internal attempt evidence. Freeze the matrix and
audit fields before RM-402. Sol High design and independent transaction review.

2026-10-02 proof checkpoint: the [TX-001 candidate](../transaction-retry-plan.md)
and [case catalog](../../tests/assurance/transaction-retry-v0.1.json) now follow
approved TIME-D05 with a new stable `clock.now` per retry attempt. The
[saved SQLite/PostgreSQL phase probe](../../tests/validation/rm401-phase-probe-results.json)
shows structured SQLite busy and rollback evidence plus a PostgreSQL commit-time
40001 abort after both callbacks returned, a 40P01 deadlock victim, and a
dropped commit response where the client saw connection loss despite a
committed row. No retry implementation is claimed. Independent transaction
review remains before freeze. Run `RM-401-fb913791a10d` records this slice.

**RM-402 — L.** Implement accepted policy in generated SQLite/PostgreSQL adapters,
then extend `entity-dossier.test.ts` and its multi-process writer with deterministic
contention, nested savepoint rollback, attempt/deadline exhaustion and injected
connection loss before/after commit. Add a many-client contention trace to verify
independent jitter, bounded attempts/deadlines, cancellation and no retry amplification;
use seeded randomness for repeatable assertions rather than flaky timing thresholds.
Compare query/transaction plans and observations on both databases. Assert isolated
operation clocks, committed
revision contiguity, no duplicate writes and correct uncertainty classification.
Run `bun --no-install test tests/runtime/entity-dossier.test.ts`,
`bash tests/runtime/postgres.sh entity-dossier` and the full gate. A passed
same-process test does not replace contention from separate processes. Luna
Extra High implementation; Sol High independent transaction review.

## RM-601/603 — review artifacts and local attestation validation

**Current actor-binding checkpoint, 2026-10-05:** v5 now exposes the existing
checked role-binding and membership contracts as sorted `impact.actors` facets
and individual `role_binding:Entity.field` / `membership:Entity` decisions.
Inventory status/facts shape is preserved. Explicit absence, changed/removed
bindings, exact entity/application scope and principal reference entity, changed
membership field and rehashed-omission rejection pass in the focused23/23 suite.
The first removal fixture rejected correctly with `POLICY_SCOPE_MISSING` because
it removed the only valid scope source; the corrected fixture retains a separate
owner binding. No checker or policy relaxation was made. The intermediate full
[failed gate](../../build/validation/20261005T013207-54043/report.json) remains;
the corrected fresh supported
[gate](../../build/validation/20261005T013626-57310/report.json) passes61/61.
The [independent actor-binding review](../../tests/validation/rm601-independent-actor-binding-review.json)
approves that narrow static slice with no findings: independent23/23 focused
tests and read-only CLI facts/digest/removal/invalid-source checks pass. It pins
the exact actor extraction/test regions while the separate principal helper was
being added; that helper is not approved by the actor review. The canonical
audit was pending at that gate. The now-complete
[independent prerequisite audit](../../tests/validation/rm601-independent-canonical-prerequisite-audit.json)
does not clear RM-601 or RM-603: it identifies omitted supported principal
projection, actor-to-route and field/output facts, plus remaining counterexample,
provenance and complete-review requirements. It explicitly does not approve the
concurrent binding implementation. These are static checked source contracts, previously included
in aggregate policy, not live assignments or principal-specific authorization.
Whole RM-601/603 and golden gates remain open. Timer: `RM-601-e0017270d55a`,
actual6.1-sol/high. Continue the explicit worker-binding preparation while the
original golden execution/lease question remains unanswered.

**Principal-contract slice — saved short plan:** derive application principal/
revocation, declared principal variant fields, validator principal/settings,
claim mappings and resolution principal/authority/active predicate/mappings/
inactive-failure contracts from already checked AST/model data. Use sorted
semantic identities and expression tokens, excluding source offsets and runtime
credential/configuration values; declarations and supplied literal spellings
remain exact. Give contracts individual before/after decisions. Keep generated
authentication opaque and live authority/runtime/counterexample feasibility
unproved. Paired valid same-type projection and Bool active-predicate changes,
absent/invalid source, canonical rebuild/order and rehashed-omission checks must
have independently expected facts. Ordinary checks plus scoped consequential
artifact review fit this settled extraction slice. Actor-source→route links and
field/output extraction are the following slices, not silently claimed here.
The owner-selected6.1-sol/high pin is retained.

**Principal implementation checkpoint, 2026-10-05:** `principal_contracts`
now emits those checked AST facts as sorted facets and individual stable
application/variant/transport/validator/claim/resolution/projection decisions.
Expressions retain nontrivia tokens; field types and exact nominal paths come
from the checked AST. Target-keyed claim/projection identities preserve source
changes and treat target renames as removal/addition. Four new groups pass in
the27/27 focused suite, including the actual migration's secret-only config
references and credential-slot fields, semantic field ordering, exact
mapping/predicate/settings changes, invalid/duplicate/type-incompatible mappings,
explicit absence and rehashed omissions. The initial26-test run had two fixture
syntax failures: audience was placed before the parser-required principal and
an unsupported block-comment spelling was used in a whitespace test. Fixtures
were corrected; no parser/checker change or false passing gate is claimed.
The fresh supported
[gate](../../build/validation/20261005T014747-67437/report.json) passes61/61;
the [independent principal implementation review](../../tests/validation/rm601-independent-principal-contract-review.json)
approves this narrow extraction with no findings. It independently passes27/27,
rebuilds the CLI and checks two valid subjects plus three expected invalid-source
rejections, with exact source/binary/compiler-input pins. No runtime, complete
canonical milestone or attestation authority is inferred.
Actor-to-route links, field/output flows and all other canonical/golden
requirements remain separate. Timer: `RM-601-48902f1e12a0`, actual6.1-sol/high.

**Following actor-to-route slice — short plan, independent challenge first:**
derive per-operation actor-source links from checked policy obligations and
restricted-field reads, retaining exact entity/effect/field/subject/scope and
source-contract identities. Built-in public/authenticated subjects retain their
fixed contract meanings. For qualified roles, reference exact direct-role
bindings and exact membership enum contracts; expose resource/application scope
conditions rather than inventing live assignments. Attach checked principal
resolution candidates only by exact authority-reference entity, not entity-name
heuristics or an inferred authenticated principal kind. If composition or route
validator selection cannot be proved statically, preserve that edge as explicitly
unresolved. A binding grants no effect by itself. Join operations to HTTP entry
points through checked call edges, retaining a deterministic may-call witness,
including direct route policy obligations. Use these contracts in before/after
scenario comparison so a binding/projection change remains visible even when
grant strings and entity effect topology do not change. Add a distinct admission
scenario for added/removed/auth-changed routes with no terminal data/service
effect; never invent an effect to force them into an existing witness category.
Paired policy-runtime binding/membership changes and pure public-health auth
changes must assert independently expected affected routes, exact conditions,
deterministic identities/order and rehashed link-omission rejection. Preserve
live access/feasibility/runtime as unproved. Review this mapping/label boundary
before implementation; it does not grant worker or attestation authority.

**Actor-to-route plan challenge disposition, 2026-10-05:** the
[independent review](../../tests/validation/rm601-independent-actor-route-plan-review.json)
requires AR-P01–03 refinements before implementation as originally written;
its constrained recipe is approved and needs no new owner semantic choice.
Apply all refinements: retain explicit nominal `Entity.identity` principal fields
as distinct candidates alongside authority resolutions, and distinguish identity
from same-entity nonidentity mappings; walk every reachable checked policy-bearing
operation through semantic call endpoints, preserving original obligation AND
restricted-field provenance separately from copied route summaries; produce
effectless admission scenarios for actor-link changes as well as add/remove/auth
changes. AR-C01–05 preserve OR subject groups/conjunctive independent surfaces,
exact resource/application scopes and symbolic principal/key/currentness
conditions, granular unresolved edges, tuple-based stable identity and distinct
admission evidence. Existing effect scenario meanings/IDs stay intact. Acceptance
must include nominal versus plain-UUID identities, nested invoke/field reads,
mixed scopes, effectless membership/projection changes and exact omission
rejection, not merely nonempty arrays. This refines the saved plan; implementation
and its consequential review are the next eligible stage, not yet approved.

**Actor-to-route implementation checkpoint, 2026-10-05:** the refined extraction
now adds sorted operation/surface/source facts and route may-call witnesses through
semantic call endpoints. Checked record/dossier/identity declarations supply the
identity index: the policy-runtime entity audit has an empty entity-contract list
despite those checked declarations, so neither EntityModel availability nor an
`id`/`*_id` fallback is assumed. Exact direct/member scope conditions, nominal
versus authority-resolution identity candidates, original field/invoke provenance
and separate copied-route surfaces implement AR-P01–03/AR-C01–05. Authenticated
effectless entries retain checked auth candidate contracts without inventing a
selected validator. Effectless add/remove/auth/source changes get a separate
admission row with null effect and static route-contract certainty; existing
effect row meanings/IDs remain intact. Full contracts, not only link IDs, are
compared and bound into before/after decisions and state/behavior digests.

Eight added regression groups initially pass35/35 focused tests: resource direct
versus membership sources, both application principal kinds, nested invoke and
restricted-field provenance, nominal versus plain-UUID/nonidentity resolution,
effectless membership/projection/auth/add/remove, inline operations, cycles and
alternate deterministic witnesses, semantic ordering and rehashed omissions.
Final strengthening adds mixed resource/application sources and a checked identity
named `key`; the final35/35 focused suite passes and independent implementation
review is pending at this checkpoint. Failed intermediate fixtures are retained in timing/evidence:
an administrative output wrapper lacked a valid field-policy proof; bare Unit
calls were unsupported statements; an unused identity-query fixture acquired an
ungranted read when policy was enabled; and a test incorrectly expected empty
policy contracts for effectless wrappers. Corrected fixtures do not change any
checker. Graph inspection confirmed the wrappers are traversed even though only
the reachable original policy operation is emitted. The inline fixture now also
has an independently expected `route_inline` User read. A same role enum declared
with conflicting resource/application scopes was also rejected with
`POLICY_BINDING_INVALID`; the positive mixed-scope case uses distinct checked
role enums, and that invalid case is now an export-rejection regression. The fresh
supported [gate](../../build/validation/20261005T021605-89918/report.json) binds
this final candidate and passes61/61, including both adapters. It records full
golden `compile_failed` with49 diagnostics and all44 cases unexecuted. Independent
actor-route implementation review remains pending; this gate is not release-equivalent.
Timer: `RM-601-0a8760752da5`, actual6.1-sol/high. No worker profile, whole
RM-601/RM-603 or Goal completion is claimed.

**Continuation selection:** finish focused checks, independent actor-route review
and a fresh supported gate, then continue the field/output slice below. Existing
principal/binding reviews remain historical scoped evidence, not approval of this
new link extraction. Keep the original execution/lease owner question pending.

**Actor-route independent findings and correction, 2026-10-05:** the
[implementation review](../../tests/validation/rm601-independent-actor-route-implementation-review.json)
does not approve the first candidate despite its35/35 tests and61/61 supported
gate. AR-I01 shows distinct copied route obligation groups can share a surface
key and overwrite one another. AR-I02 shows root-only checked binding/member
contracts omit the exact source reference, hiding same-entity nonidentity changes.
Both are bounded extraction fixes, not new owner semantic choices or policy fixes.

The corrected candidate includes sorted distinct OR-subject groups in each stable
surface/link tuple and retains separate conjunctive requirements. Exact checked
source-value declared-type tokens/effective reference paths use explicit reference
precedence, nominal type otherwise and direct checked self identity when applicable.
Source-value composition is separate from principal candidates: identity-reference,
nonidentity-reference and unresolved absence are distinct. Resource membership
scope references are explicit; application scope has no value reference. Equality
conditions are required and unproved. No checker or runtime authority change is made.

Three additional regression groups pass in the final38/38 focused suite: a fully
checked inline route retains both PrivateNote read subject groups and original
witnesses, reordered calls remain equivalent, paired group edits bind and rehashed
group omission rejects; direct and application member User.id→User.alternate_id
or User.display_name changes bind affected routes despite identical root-only
policy contracts; scope-reference changes, absent checked identities, explicit
references and a dossier self-identity named `key` retain exact composition. Invalid
explicit-reference type mismatch still rejects. An intermediate test compilation
failed on Value versus &Value comparison and was corrected; no failing product
check was suppressed. The proposed primitive explicit-reference fixture was
corrected to the checker's required exact nominal type before actual fixture
execution, with its incompatible spelling retained as a negative regression.
The38-test correction passes a fresh supported
[gate](../../build/validation/20261005T023701-14095/report.json),61/61 including
both adapters. The [independent correction review](../../tests/validation/rm601-independent-actor-route-correction-review.json)
approves the corrected narrow actor-source route slice with no remaining findings:
independent38/38 focused tests, group/link tuple and complete45-file compiler-input
freshness checks, exact digest/baseline/negative/export checks pass. It pins the
current helper, parent and test source without a checker change; original rejected
review bytes are preserved. This is not full canonical or runtime authority. Golden remains
`compile_failed` with49 diagnostics/all44 cases unexecuted. The earlier61-step
report pins the superseded candidate, not these fixes. Continue
the field/output stage under the unchanged6.1-sol/high pin; whole gates remain open.

**Field/output continuation — source-grounded preparation:** the existing
scanner visits return/construction/mutation children but emits only entity-level
effects. Checked output field declarations, policy restricted-field reads, query
predicate fields, literal mutation field sets and page projection/cursor/include
contracts are available and must be represented explicitly. Start with those
exact facts plus constructor target-field/source-expression tokens and guarded
return paths, then join their owning operation through checked call witnesses.
Do not infer a database source field from a local variable's spelling or a shared
primitive type: use an exact checked nominal field reference/resolved expression
or mark the alias edge unresolved. Preserve branch/predicate/supplied-field
conditions and whole-record boundaries; these are may-flow contracts, not taint
or satisfiability proofs. Pair PrivateNote.summary versus private-label output,
policy-runtime administrative output and a same-entity mutation-field change.
Expected field/condition/output paths must be asserted independently of the
emitter, then rehashed omission rejected. Nested outputs and include/page
projection need their own exact contract checks before claiming complete flows.
No implementation or complete canonical acceptance is claimed by this preparation.

**Field/output first slice — short implementation recipe, challenge pending:**
add explicit checked field and output-expression contracts without claiming a
complete alias/taint analysis. Preserve the existing `impact.fields` inventory
shape and add a separate source-contract facet. Index checked entity/output
fields, declared nominal references and policy restricted reads. Walk root and
entity-owned callable bodies plus inline HTTP bodies with the exact owning
operation identity. Emit query predicate/order fields, page predicate operators,
cursor/order/projection/result contracts, bounded include declarations, create
field initializers, update literal and supplied-conditional field sets, lifecycle
transition name, and delete/key predicates. Include exact nontrivia expression
tokens and inferred expression types matched by source/range, but exclude those
offsets and build-local NodeIds from stable contract identity. Keep branch/arm,
failure and supplied-field conditions explicit and symbolic, never satisfied.

For constructed output fields retain the exact target field, its checked declared
type/reference, value-expression tokens and inferred type. Returns of a whole
record/call result/local alias are explicit whole-value boundaries, not missing
fields or proven safe projections. A nominal field type is a contract reference,
not proof that a local value came from a database row: retain aliases as unresolved
unless a separate checked resolution proves the origin. Never turn `note.title`,
an input typed `PrivateNote.title`, or a shared primitive into a database-flow
claim solely from spelling/type. Nested constructors retain their full target-field
path; page/include declarations describe checked projections, not executed joins.
Unsupported origin edges remain present with an exact reason. This slice records
supported source facts; the canonical field/output may-flow acceptance stays open
until necessary alias, nested output and runtime/feasibility coverage is evidenced.

Use stable tuple identities with owner, structural statement/expression address,
contract kind and checked target/field; statement ordinals are build-local syntax
addresses, not rename/insertion-stable semantic identity. Before/after values and
individual source-contract decisions must include changed literal values,
predicates, target fields and output expressions even with identical entity/call
topology. Join owning operations to entry points with semantic call endpoints,
deterministic witnesses and explicit route output contracts. Do not mislabel pure
response changes as actor admission: first expose exact route field contracts and
decisions, then review any new scenario discriminator before adding it. Preserve
existing actor/effect/admission meanings and IDs.

Acceptance: paired PrivateNote summary/private-label output, administrative nested
output, same-entity mutation-field/value and supplied-patch changes; exact branch
and missing/failure conditions; nested constructor/whole-record boundaries;
page/cursor/include projection contracts; root/entity/inline route witnesses;
changed aliases not claimed as data origins; semantic set sorting, absent/invalid
source and rehashed field/route omission rejection. Independent expected paths,
fields and conditions must be asserted, not generated by the extraction under
test. Ordinary focused compiler checks, supported integration gate and independent
consequential artifact review fit this additive slice. Retain6.1-sol/high. No
worker or issuer dependency is waived, and no new owner choice is assumed.

**Field/output recipe challenge disposition and refined constraints, 2026-10-05:**
the [independent challenge](../../tests/validation/rm601-independent-field-output-plan-review.json)
requires FOP-01–03 before this first slice. It approves the response/admission
distinction and does not require a new alias solver, counterexample campaign,
worker profile or owner semantic decision. Apply these source-grounded constraints:

- **Total type evidence (FOP-01):** match observations internally by exact source
  and expression range, coalesce identical rows and classify missing or conflicting
  display observations explicitly. Never omit a checked source fact or reject a
  valid page for lacking its outer inference row: the checker returns early there.
  Declared page/result/field/output/header contracts remain separate checked AST
  evidence; an outer attempt observation is not a child page observation. Display
  strings do not prove resolved data origins or secret-flow classification.
- **No synthetic-name leakage:** build a normalization registry from actual checked
  route context and parser-generated inline-record declarations, not a prefix
  heuristic. Route path/header display names are matched internally against the
  exact compiler-created shape for that route, then represented by method/path
  and path/header surface with checked fields. Inline Object lowering is identified
  by its checked Value-record name range pointing to the authored `Object` token,
  with the generated name differing from that token; map references to the owning
  declaration/field structural path and checked shape. Named authored records and
  aliases keep their exact names. Missing structural association gets a precise
  unresolved synthetic-type row, not a raw offset/counter value or guessed rename.
  Normalize contract values as well as IDs, including generic/nested references;
  retain original authored type/expression tokens without trivia. Exact source/
  compiler provenance still changes when source bytes change, as intended.
- **Plain patch mappings (FOP-02):** retain the exact `patch` occurrence/binding and
  uniquely associated checked patch record (Value or Input, exactly as the checker's
  `is_patch_object` accepts), then map every declared optional patch-record field to the same checked entity field
  with its own symbolic supplied condition. `patch: input` must not look like an
  empty write set merely because literal/derived change arrays are empty. Nullable
  present-null remains distinct from omission; empty-patch, missing and conflict
  triggers are separate. Derived fields retain exact value and supplied-path
  contracts. If an alias cannot be associated with a checked input declaration,
  preserve the whole-patch boundary and exact reason. No payload-origin proof is
  inferred from a compatible patch-record type or alias; Entity/Output are not
  accepted patch kinds. Association uses unique exact occurrence type evidence or
  demonstrably matching checked binding, never variable spelling/matching field sets.
- **Catalogue and headers (FOP-03):** cover checked Entity/Value/Input/Output record
  fields and enum payload constructor fields; retain scalar aliases/constraints
  separately. Use flattened root/owned callable entries and already-qualified
  `name.text`, not owner-prefix reconstruction. Carry parameters/return/failures,
  receiver, mutation guard, freshness and consistency, including revision/reload
  differences even with unchanged bodies. Query cardinality/key/predicate and
  return/missing/empty/conflict/outcome trigger kinds stay distinct. A lifecycle
  transition points to the existing checked lifecycle contract, not inferred SQL
  writes solely from its name. Standalone/unreachable owners still emit contracts
  and explicit no-route links.

Extend independent expected coverage with fixtures40/44 plain and derived patches,
120 qualified receiver/guard callables, 56 enum payload/match arms, 165 Value outputs
and route whole headers/path, migration TodoPage/TodoView/TodoCursor and ordered
created_at/id tuples, 49 optional nested include plus a separate bounded-many
include, and 68 created/no-content response boundaries. Whitespace or an unrelated
earlier inline Object must not change contract shape solely through generated
offset/counter names. Preserve statement/evaluation/page/cursor tuple order while
sorting semantic sets. Producer completeness tests remain necessary in addition
to fresh-export equality. These refine the saved recipe; no field implementation
or complete canonical acceptance is claimed yet.

The [refined review](../../tests/validation/rm601-independent-field-output-refined-plan-review.json)
resolves FOP-01/03 at plan level and identifies this final FOP-R01 patch-kind
correction. Current source confirms `is_patch_object` accepts exactly Value|Input;
the paragraph above incorporates it. Add paired named `type PatchCustomer = Object`
and Input patch fixtures, including nullable optional fields and the derived Todo
supplied-field case, while keeping existing invalid shapes rejected. Generated
type normalization is source/provenance scoped, exact-component and recursive;
derive nested owner paths from checked declaration/type-reference structure, not
sanitized `field_owners` strings. Ambiguous shape or patch association remains
explicit. No further owner choice or repeat plan audit is required for this bounded
correction; proceed to first-slice implementation, focused completeness checks and
independent implementation review. Prior plan reviews remain historical constraint
evidence, not field implementation approval.

**Field/output implementation checkpoint, 2026-10-05:**
`artifacts/approval/field_contracts.rs` implements the constrained first slice as
an additive `impact.field_source_contracts` facet, leaving actor/effect/admission
semantics and the field inventory intact. Catalogue, qualified operation surfaces,
recursive source expressions, constructor/patch/predicate mappings and route
may-call contracts each have explicit evidence/absence labels and bound individual
decisions. Exact source/range observations are internal lookup keys; generated
Object/header/path names are normalized in values as well as IDs. Nullable patch
assignments use checked inherited nullability. Whole aliases/call returns retain
unresolved origins; branch/outcome/supplied/missing/empty/conflict conditions remain
symbolic. Optional includes do not acquire the parser's synthetic pagination, and
the page child's missing inference does not hide its checked projection/cursor.

Nine producer-completeness groups extend the focused suite to47/47: nominal safe
versus private projections and changed literals with unchanged graph, Input|Value
patches/null/omission/derived assignments, qualified receivers and guard changes,
inline/root route witnesses, migration page order/operators/cursor and missing
child observation, nested optional versus bounded-many includes, enum payload and
match/outcome triggers, pure response versus created/no-content boundaries, nested
constructors/whole aliases/admin projection, invalid patch exports and rehashed
source/route omissions. Four internal tests cover stable inline shapes, authored
synthetic-looking names, exact duplicate/missing/conflicting inference and whole
headers normalized across an unrelated preceding Object. Existing38 tests still
pass. Failures were repaired without a checker change: private API/nonexistent
helper compile mismatches; a real inherited-nullability extraction defect; an
inline fixture missing declared failures; a wrongly expected synthetic graph hop;
and an invalid unwrapped Instant in a nominal-field test. Self-review also corrected
the lifecycle pointer to the exact checked transition path and retained explicit
success/failure outcome pattern kinds. Timer: `RM-601-4d484c159e07`, actual6.1-sol/high.
The final formatted-source supported
[gate](../../build/validation/20261005T031701-27483/report.json) passes61/61,
including both adapters, with golden `compile_failed`49/all44 cases unexecuted.
The earlier [gate](../../build/validation/20261005T031335-24773/report.json) also
passes61/61 but pins the pre-format parent source. Formatting and diff checks
pass; independent consequential implementation review remains pending. Full
canonical field-flow, worker/emission, feasibility/provenance,
whole RM-601/RM-603 and all44 golden acceptance remain open. Continue from review
findings or the checked delivery-binding stage; never activate the unanswered
60s/40s worker profile. Historical scoped approvals and gates are not replaced.

**Field/output independent review correction, 2026-10-05:** the immutable
[original implementation review](../../tests/validation/rm601-independent-field-output-implementation-review.json)
found FO-I01: declarative plain-patch mappings followed source field order rather
than semantic-set order. Sort only these mappings by checked patch-field identity;
do not reorder initializer/evaluation/cursor/witness sequences. The new Input and
Value declaration-reversal regression brings the focused suite to48/48; all4
internal field tests remain passing. The independent
[correction review](../../tests/validation/rm601-independent-field-output-correction-review.json)
approves this narrow repair, with no findings. Its field helper/tests are unchanged
by the following additive provenance work; historical parent/source pins and gates
remain historical, not blanket approval of the evolving parent artifact. Whole
field-flow/canonical/worker/golden gates remain open.

**Generated-artifact provenance continuation — short plan, 2026-10-05:**
canonical audit `RM601-CANON-DEPLOYMENT-PROVENANCE` requires available output pins,
not a production issuer claim. Refactor the existing metadata/audit generator into
one shared no-approval producer. Hash its freshly generated exact bytes and the
existing `derive_target` output; never read a pre-existing `build` directory.
Exclude approval JSON/text explicitly to avoid self-reference, and preserve the
existing artifact order and contents. Sorted descriptors retain path, producer,
byte length and SHA-256; duplicate output paths reject instead of overwriting.
Target rejection is explicit diagnostic/status with no target files, not successful
generation or empty supported output. Metadata/audits remain separately available.

First implement this disjoint helper/shared producer with tests comparing against
actual generator bytes, supported/unsupported targets, changes, ordering and stale
disk decoys. Then bind its facts into each checked snapshot, individual decisions
and full state/behavior digest, compare actual built output and reject rehashed
pin omissions. Source contracts, compiler inputs and expected generated bytes are
distinct provenance. Toolchain/executable/protected build and written-file runtime
conformance remain unproved; deployment qualification remains explicitly unavailable.
Required independent consequential artifact review and fresh supported gate follow
integration. Current field helper/parent/test pins stay unchanged during its review;
new compiler-input hashes require fresh build evidence. Ordinary deterministic
checks fit this settled local pinning contract. Timer `RM-601-f23d23010ac1`, actual
6.1-sol/high. This advances a canonical prerequisite, not whole RM-601 or RM-603.

**Generated-byte implementation checkpoint, 2026-10-05:** the shared no-approval
producer and `artifacts/artifact_provenance.rs` now derive exact fresh available
metadata/audit and target byte sets. Each path pin, target status and whole facet
is bound into checked snapshot facts/digests/comparison decisions. Approval outputs
remain explicitly excluded, target rejection remains diagnostic rather than a
successful empty build, and stale disk decoys are ignored. Tests compare actual
generator bytes/lengths and available SQL/auth/dependency surfaces, output ordering,
set duplicates/self-reference, supported/unsupported output, changed literal
decisions, rehashed pin omissions and unchanged comparisons across roots.

A real pinned-service relocation regression found that audit/services.json and
app.meta.json retained absolute external-effect source paths. Normalize only those
known metadata source fields in the actual producer, not just the descriptors or
authored values/imports. Identical imported bytes/source in two real isolated
project roots now yield identical pins and no comparison decisions. The first
attempt used nonexistent roots and correctly failed import checking; it is a
fixture failure, not evidence of a checker defect. Current focused results are
48 approval tests,6 public provenance tests and5 helper tests passing. The fresh
[supported gate](../../build/validation/20261005T034158-34720/report.json) passes61/61
including both adapters, with full golden compile_failed49 and all44 unexecuted.
Independent consequential implementation review found two required corrections;
the earlier61/61 reports above do not pin this new source. Written-file, runtime,
executable/toolchain and release authority remain explicitly unestablished.

**Generated-byte required correction checkpoint, 2026-10-05:** preserve the
[original independent implementation review](../../tests/validation/rm601-independent-artifact-provenance-implementation-review.json).
GP-I01 found unnormalized checked clock-read sources in actual metadata;
GP-I02 found raw lifecycle provenance paths in actual target-owned
audit/lifecycles.json. Correct those exact producer fields with the shared
project-relative helper; no hash-only shadow normalization. Separately normalize
the same known lifecycle metadata source positions in the older impact.lifecycles
facet, with no change to lifecycle names/ranges/node lookup/effect semantics.
The full pinned service fixture now retains its checked clock reads in the real
two-root test; a seventh public group compares actual metadata/all target output
bytes and no comparison decisions/scenarios for relative versus absolute service
and lifecycle-project invocations, with nonnull equal source digests. Focused
7 provenance and48 approval tests pass. Its first relative-root construction
wrongly produced two identical absolute paths under Cargo's core-crate CWD; the
test correctly failed, then a true relative path was constructed and passed.
Scoped independent
[correction review](../../tests/validation/rm601-independent-artifact-provenance-correction-review.json)
approves GP-I01/02 and the exact older lifecycle metadata normalization with no
findings. All five reviewed candidate source hashes still match after integration;
the reviewer independently ran7+48+5 tests and reproduced both original CLI
comparisons with zero decisions/scenarios. Its actual binary/compiler snapshot is
preserved; later concurrent descriptor edits/shared binary replacement are not
silently treated as the same snapshot or worker approval. The fresh final-formatted
[shared gate](../../build/validation/20261005T040504-41228/report.json) passes61/61;
preceding reports remain historical. Also read-only checked171 freshly
written descriptors across10 examples from that preceding gate: all exact byte
length/digests matched. A whole-directory assertion correctly found runtime
local.sqlite beyond the generated set; it is not an attested artifact. No stale
migration build or written-file claim was substituted for fresh generator evidence.

**Current job-impact checkpoint, 2026-10-05:** eligible independent work under
the active technical golden Goal and owner-selected `gpt-6.1-sol/high`.
The v5 reserved `impact.jobs` slot now derives checked nonexecuting schedule
facts, sorted by semantic identity, from the existing job audit. Interval,
action/constructor, failure and reachable-service changes bind into the full
state/behavior digests and individual decisions. Worker/delivery authority,
runtime conformance and feasible job scenarios remain explicitly unestablished;
emissions/deployment stay unsupported with null facts. Four added regression
groups cover exact absence and limits, interval/callee/failure deltas (including
unchanged graph), semantic ordering, transitive mail paths and rehashed omission.
The focused approval suite passes 20/20. The fresh full supported
[gate](../../build/validation/20261005T005656-21515/report.json) passes61/61,
including both adapters; it is not full golden or release-equivalent evidence.
The [independent scoped artifact review](../../tests/validation/rm601-independent-job-impact-review.json)
approves this nonexecuting slice with no blocking findings and independently
passes 20/20 focused tests. Its nonblocking stale schema label is corrected in
the generated-artifact owner. This is partial RM-601,
not issuer authority, complete job analysis or attestation verification. Existing
canonical slots/serialization are unchanged; compiler-input provenance binds
the implementation change. Timing: `RM-601-9bbb48d8ba84`.

**RM-601 — L.** Extend compiler-owned artifact generation from checked policy and
semantic graph facts; use the [approval protocol](../approval-protocol.md)'s
ApprovalSubject fields and evidence categories. Define versioned canonical bytes
and stable decision IDs, then before/after behaviour and a non-graphical export
from the same subject. Include intent/provenance, actors, transitive data/effects,
alternatives, unknowns and exact policy/graph digests; the renderer must not infer
meaning. Test stable canonicalisation, meaningful changes without graph topology
changes, explicit absence, and AP-11 effect completeness. Wire schema/CLI and
artifact fixtures into the full gate. Luna Extra High implementation; Sol High
independent policy/public-artifact review. This does not implement the web UI or
claim the candidate protocol has passed external assurance.

Implementation checkpoint 2026-10-01: `build/approval/subject.json` now emits a
versioned canonical local subject with explicit null before digests, checked
after policy/semantic-graph digests, stable route/entity impact identities,
evidence unknowns and provenance. A stable `fnv1a64` subject identifier is
provided for correlation only; protected issuer validation and release
authority remain RM-603/RM-604. Artifact generation tests cover byte stability,
explicit absence and digest change across checked graphs.

Sol High self-review checkpoint: this is a partial scaffold, not yet the
`ApprovalSubject` required by AP-11. The generated decision list and most
impact/effect arrays are hard-coded empty, even when semantic effects exist;
The non-graphical export must preserve the same facts; a heading or different presentation bytes is not itself a contract failure. Exact canonical JSON bytes remain the approval-binding input.
`fnv1a64` is correlation-only. Complete graph-derived content, individual
decision IDs, before/after behaviour and executable AP-11 omission tests in
the Luna Extra High RM-601 implementation slice before attempting RM-603.

**RM-603 — L.** Define a verifier boundary accepting authenticated issuer evidence
and canonical RM-601 subject; untrusted repository JSON can never become an issuer.
Validate exact subject/policy/graph/version binding, each decision, reviewer role
and separation from implementer, issue/expiry, revoke/supersede and replay policy.
Use a clearly test-only issuer fixture for local positive cases; production
release remains non-releasable without RM-604's protected authority. Reuse
[AP-01–12](../../tests/assurance/approval-protocol-v0.1.json), adding explicit
consumption replay and malformed envelope cases. Test missing issuer/decision
and stale/forged/mismatched subjects fail closed. No invented cryptography or
local environment bypass. Sol High boundary design/review, Luna Extra High
implementation. Hosted provider wiring and release authority remain outside scope.

## RM-504/505/109/110 — integrated evidence

**RM-504 — L.** Extend registered live PostgreSQL suites for the actual auth,
policy, query and time behaviours claimed after RM-102/RM-402. Reuse disposable
cluster runners; verify the evidence records database/runtime versions, exact
JWT dependency and timezone provenance where claimed, query counters and known
instrumentation limits. Compare SQLite/PostgreSQL observations rather than
assuming shared generated code proves parity. Missing tools are a blocked gate,
not a skip/pass. Run the full verifier, not `--profile quick`. Luna High fixture
work; Luna Extra High for adapter differences; Sol High security review as needed.

**RM-505 — L.** Derive named hostile cases from the accepted lifecycle, auth,
service/job and transaction obligations after its prerequisites. Cover revocation
during fresh access, cross-owner/service confusion, soft-delete races, secret
escapes, duplicate/crashed delivery and uncertain commit. For each name record
expected observable response plus storage/effect/audit result, contract and proof
limit. Reuse RM-501's obligation inventory and existing mutation findings rather
than duplicating campaigns. Normal regression checks first; if a sustained
mutation/verifier campaign is needed, prepare verify-loop's target, actual model
check, budgets and evidence ledger before running it. Sol High case design/review,
Luna Extra High implementation. No independent reviewer/campaign is active here.

**RM-109 — L.** Build the missing golden harness and verifier integration against
all original case IDs, including SQLite/PostgreSQL, HTTP, jobs, source rejection
and artifacts. The current `tools/verify.py` inventory only accepts unexecuted gap
statuses and the golden flag deliberately fails: changing the obligation file
alone cannot close this task. Define versioned executed evidence linked to exact
source/compiler/contract digests; require every applicable case/result and reject
missing, stale, duplicate or foreign evidence. Compare route, OpenAPI, policy,
query audit and runtime outcomes, preserving query budgets and credential lookup
accounting. Register suites/builds in the manifest; prove a deliberately failing
case makes the golden gate fail. Luna Extra High integration, Sol High verifier review.

**RM-110 — M.** Revalidate prerequisite evidence, execute
`python3 tools/verify.py --require-golden` on the final exact source and preserve
the fresh report, raw case failures and named proof/runtime evidence. Keep frozen
toolchain results separate from repaired-toolchain results. All 44 cases need
explicit dispositions supported by the accepted contract; a narrowed local gate
must not be advertised as full behaviour. Local RM-603 evidence does not establish
protected hosted approval (RM-604) or E07 assurance. Close the roadmap task only
after its technical gate and required review are evidenced. Luna High evidence
assembly; Sol High final review, independent review still pending.

## Assessment refinements — 2026-10-01

These source-grounded refinements govern the next selection. See the [shared gates](roadmap-assessment.md#shared-contract-gates-and-sequencing) for RM-207 milestone staging, retry clock reconciliation, adapter proof before retry freeze, reminder identity and mandatory RM-403 acceptance evidence. RM-303/RM-402 wait for the reviewed RM-207 boundary-contract milestone; full RM-207 closure still requires their integrated evidence. This is a plan, not evidence that the milestone passed.

### RM-103 strengthened handoff

**Depth / next stage:** direct closure audit. Prefer Sol High review; current Astra High suitable. Original estimate and open state unchanged.

**Source:** `examples/golden-todo/REVIEW.md` browser-origin revision records owner authorization and prior/new successor digests; `examples/golden-todo-migration/config.jadpo` and `authentication.jadpo` implement required origin binding. `tests/runtime/golden-protected-route.test.ts` tests valid origin+proof and absent/wrong origin/proof, bearer separation and configuration failure against the generated migrated target. `tests/validation/rm102-authentication-review.json`, `rm102-declared-credential-review.json` and `docs/implementation-history.md` RM-102 entry provide prior review and final supported-gate evidence.

**Steps:**

- Resolve the recorded hashes as historical checkpoints; authentication source changed again legitimately for RM-102, so its current hash need not equal the origin-only historical hash. Verify original frozen candidate/acceptance/policy were preserved and link the successor chain.
- Map each RM-103 acceptance clause to a concrete test/review observation. Separate invocation through generated handler from an actual listener and startup process: `initializeApplication` rejecting configuration alone does not by itself prove listener absence; inspect generator startup ordering and existing startup tests before deciding whether additional process evidence is needed.
- Confirm mutative browser route requires exact configured origin AND session-bound proof; a bearer request must not be misclassified as browser. Existing broad first-party suite contains mutative GET coverage; confirm it applies to the same generic transport logic and record its scope rather than adding an unnecessary golden GET mutation.
- Check review findings against exact changed boundaries; retain independent review gaps if any, rather than inferring that any RM-102 review covers all future changes.
- During an authorized execution pass, run only missing/fresh evidence needed: generated fixture build per `tests/runtime/README.md`, `bun --no-install test tests/runtime/golden-protected-route.test.ts`, relevant first-party/startup suites, then required gate if code changes. A documentation-only audit can cite the existing fresh 52-step report without rerunning all tests.
- Close only RM-103 after all clauses have evidence. Update current-state pending-origin/CSRF wording in migration README and obligation mapping without changing original44 case execution status. Historical logs remain historical.

**Boundaries:** no new production origin, CSRF relaxation, login mechanism, full44 golden claim or automatic RM-104 execution.

### RM-104 strengthened handoff

**Depth / next stage:** short plan beginning with a checked transport fixture/security review, then implementation. Luna Extra High suitable for the bounded implementation after the binding is settled; Sol High/Astra High suitable for design/review. RM-101/RM-102 complete.

**Existing foundation:** `jadpo/crates/core/src/runtime/first_party_authentication.ts` owns issuance/exchange/revoke; `runtime/declared_credential_storage.ts` owns transactional declared/private credential linkage; `target/first_party.rs` emits checked bindings. `docs/auth-runtime-extensions.md` lists trusted host operations. `tests/runtime/golden-service-credentials.test.ts` already covers declared expiry/status/revoked state, service disablement, rotation, stable service identity, metadata integrity, atomic issuance and user/service separation on SQLite/PostgreSQL.

**Missing behavior:** frozen `AUTH-009` and `AUTH-011` explicitly invoke `POST /auth/exchange`, but the migrated authored routes have no exchange endpoint. Host API tests do not supply that HTTP proof. The owner has since selected trusted-host one-time provisioning for issuance, so a generic minting action or inferred administrator route is outside RM-104.

**Steps:**

1. Add canonical positive/negative checked source fixtures for a compiler-owned exchange transport binding to the existing selected API-key/signed strategies. The transport consumes the already selected raw credential privately; business actions receive only typed principal values. Resolve exact source spelling with public-language/auth review within RM-104. Do not add arbitrary host callbacks, raw token parameters or generalized minting expressions. Trusted host provisioning prepares the golden credential and reveals it once to its authorised sink; there is no authored issuance endpoint.
2. Keep `Service.name` nonsecret subject and stable `Service.id`; private verifier/metadata must not enter principal, ordinary business returns, errors or audit. Issuance remains atomic across declared record and private metadata. Reuse RM-102 storage; do not design another credential store.
3. Lower the checked transport to the existing host exchange operation. Enforce exact credential inventory before validation, user/service separation, active service/credential, expiry/revocation, signed lifetime cap, audience and safe failure mapping. Route artifact/OpenAPI/audit must agree. Clarify exchange output as the intended new bounded bearer, never original raw service key or verifier; `secretInOutput:false` in AUTH-009 cannot be satisfied by exposing originating key material.
4. Add real HTTP tests for AUTH-009 and AUTH-011 using current generated migrated source. Add negative user credential, competing credential, expired/revoked key, service disable, wrong audience and restart cases; assert response, stored credential authority, issuance count, safe audits and distinct principal/credential SQL counters. Reuse existing lifecycle tests rather than copying their whole matrix into a second suite.
5. Register new mode/fixture in `tests/validation/manifest.json` and `tests/runtime/postgres.sh` where necessary; test SQLite and disposable PostgreSQL, existing service suites plus new real HTTP suite, then `python3 tools/verify.py`. Record independent security/language review. Bind evidence to source/contract/compiler; preserve original golden IDs and leave aggregate44 execution gate to RM-109.

**Review focus:** no authored raw-key escape, no user-to-service confusion, no service-owner-active permission invented, one credential selector, no signed lifetime above credential expiry, no secret returned after failed/uncertain issuance, no new admin surface inferred.

### RM-104 checked exchange decision packet — 2026-10-01

**Current fact:** the golden `api_bearer` strategy already has a checked API-key validator, a signed-service validator and a trusted host `exchangeServiceCredential(strategy, credential, now)`. The migration has no `POST /auth/exchange`; existing host tests cannot satisfy AUTH-009/011. Route actions see a typed principal, not the originating key, so a normal `run:` expression cannot safely call the host exchange API without exposing raw credentials to application code.

**Recommended candidate for public-language/security review:** an optional compiler-owned exchange endpoint declaration on the *existing authentication strategy*. The authored source supplies the path `/auth/exchange`, while the compiler fixes POST, exactly one bearer credential, key-validator selection, fresh active credential/service checks, status/error mapping, response envelope, audit and OpenAPI. The authentication boundary performs the exactly-one credential inventory once and privately passes the selected raw key into `exchangeServiceCredential`. It must not first authenticate into a typed principal and then parse the headers again. The key is never bound as a route input or principal field. The compiler refuses path/method collisions, missing key/signed validator pair and cross-strategy references. A mixed bearer strategy is eligible only when it contains a checked service `api_key` and service `signed` pair; user-only and browser-only strategies are not. The endpoint rejects a user credential rather than rejecting the mixed strategy itself. This is a proposed binding shape, **not accepted syntax**. It needs a named grammar/AST/semantic fixture and independent public-language/security review before implementation.

The alternative is a special route item such as a compiler-recognised exchange operation. It would keep route inventory colocated with business routes, but risks making credential material appear to be ordinary `input` or `current_principal` data and adds a second authentication dispatch path. Compare the alternatives with compile-positive and compile-negative fixtures, then select one reviewed form. Do not mint a generic business `issueCredential` callable or infer an administrator endpoint. The owner selected trusted host provisioning and one-time reveal, resolving the earlier “authored issuance” wording without adding an administration surface.

For AUTH-009, specify the exact reviewed 200 response envelope and prove a POST with one current service key returns a bounded new bearer with service identity/strength, while the original key and stored verifier never appear in the response, principal, audit, error or generated artifacts. The `secretInOutput:false` obligation applies to original credential material; the intentional new bearer response must be explicit in the reviewed wire contract. For AUTH-011, map the host's `principal_inactive` plus declared failure through the generated route error boundary so disabling the service returns its named `service_disabled` 403 and issues no token even if the key is cryptographically valid. Negative fixtures and real HTTP tests must also cover user bearer/cookie, absent/competing credentials, revoked/expired keys, wrong audience, restart, safe failure and distinct credential/principal SQL budgets. Reuse the existing SQLite/PostgreSQL declared-credential suites; the new HTTP suite must exercise generated migrated source and source-bound artifacts. Review failure paths for secret redaction and unexpected token issuance before claiming either case.

**Independent read-only review:** a separate Sol High reviewer found the boundary suitable for a grammar/fixture stage after correcting mixed-strategy eligibility and requiring one private credential-inventory pass. The reviewer did not approve implementation or final syntax. The 200 wire envelope and 403 disabled-service mapping need exact positive/negative fixtures and review before contract freeze. The current selector uses `Headers.get`, which may merge repeated raw Authorization lines; the implementation proof must send duplicate raw headers through a real Bun listener and reject them before validation. Synthetic `Request` tests cannot establish wire-level multiplicity; if the transport cannot observe it, use a proven fail-closed adapter.

The contract packet is captured in [`tests/validation/rm104-service-exchange-contract.json`](../../tests/validation/rm104-service-exchange-contract.json). An independent read-only review found it useful as a design sketch but not ready for public-language/security review: it needs a full positive `.jadpo` source plus negative compiler fixtures, expiry tests proving `min(key expiry, now + maximum delay)`, and a principal proof path that does not add an unbudgeted lookup. The output was narrowed to the new bearer, token type and expiry; `secretInOutput:false` now explicitly excludes the original key and verifier while allowing the newly issued bearer. The private selection flow and duplicate-header transport gate are explicit. The JSON remains proposed, not accepted grammar or runtime evidence.

**2026-10-02 fixture and review result:** The proposed `exchange { path: "/auth/exchange" key: service_key signed: service_signed }` declaration now has an AST node, compiler checks for a same-strategy declared service key plus service signed validator, a fixed absolute path, and collisions with literal or parameterised authored POST routes. Full positive service-only and [mixed user/JWT/service source](../../tests/compile/pass/164_mixed_service_exchange.jadpo), isolated [negative binding/path fixtures](../../tests/compile/fail/165_service_exchange_user_only.jadpo) through fixture 168, and the [route-overlap fixture](../../tests/compile/fail/164_service_exchange_route_collision.jadpo) pass. Target generation rejects the declaration until lowering exists. The [contract packet](../../tests/validation/rm104-service-exchange-contract.json) gives two fixed-clock expiry vectors, same-call principal/audit proof for AUTH-009, explicit failure mapping and a [reproducible Bun 1.2.20 raw-header probe](../../tests/validation/rm104-raw-header-probe.ts) with its saved [result](../../tests/validation/rm104-raw-header-probe.json). The existing inventory splits coalesced duplicate values, but generated exchange rejection is still unproved. The independent public-language/security re-review found this sufficient for bounded runtime lowering. The supported `python3 tools/verify.py` gate passed on 2026-10-02 with localhost access at `build/validation/20261002T061347-51575/report.json`; its aggregate golden application gate remains open.

**2026-10-02 generated-runtime checkpoint:** The compiler now lowers the reviewed strategy declaration to a generated `POST` route. It inventories credentials once, rejects absent/malformed/competing presentations before host validation, and passes the sole selected API key directly to the host exchange. The host reuses its already checked private credential record, avoiding a second credential-authority lookup, then resolves the active service once before signing and auditing. The response contains only the new bounded bearer, `Bearer` and the exact UTC host expiry; the route inventory, OpenAPI and authentication audit artifact describe the endpoint. The migration source now declares the path. `golden-service-credentials.test.ts` passes 18 tests / 166 assertions on SQLite and 18 / 160 on disposable PostgreSQL, including AUTH-009's `Service.name=reporting`/audit proof, AUTH-011 disabled mapping, real-listener duplicate raw headers, user/competing/expired/revoked/wrong-audience rejection, audit-sink failure and a fresh generated-process follow-up. SQLite query tracing sees one private metadata query, one declared credential query and one service-principal authority query; PostgreSQL parity passes, but this test does not claim PostgreSQL query-count instrumentation. `golden-migration-authentication.test.ts` passes 1 / 8. The final supported [validation report](../../build/validation/20261002T075300-73475/report.json) passes; the complete frozen golden case gate remains open for RM-109/RM-110.

**Closure:** the [independent runtime/security review](../../tests/validation/rm104-generated-runtime-review.json) approves the scoped generated exchange. Two preliminary cross-strategy findings were withdrawn after compiler evidence confirmed that project-wide credential-slot uniqueness rejects a second Authorization-header strategy before generation. RM-104 is complete; RM-109/RM-110 retain the full frozen golden gate.

### RM-107 bounded synchronous-route completion

**Depth / completed stage:** short integration plan. The owner resolved the email-policy conflict; the bounded `TodoView` child projection and route now pass both adapter suites. Current golden-delivery batch pin is gpt-6-luna/xhigh, established for the cross-component E01 implementation and retained through RM-107. Independent policy review remains a separate gate. RM-101/RM-102 complete.

**Source:** `entities/todo.jadpo` contains by_id/get/create_todo/patch_todo. GET by ID is in `routes/liveness.jadpo`, POST/PATCH and UserWithTodos are in `routes/todos.jadpo`. Frozen `acceptance.json`, `golden-obligations.json` and `values/contracts.jadpo` own required output shapes and original IDs.

**Steps:**

- Build migrated fixture and enumerate existing GET/POST/PATCH cases against frozen IDs before adding features. Use generated HTTP boundary with injected clock/configuration and actual isolated database; collect expected response and post-operation storage.
- Prove owner concealment for absent/foreign/deleted row; input cannot set owner, UUID or generated timestamps. Create with omitted due_at materializes none, past/equal-now rejects, future accepts. Verify rejected input performs no write.
- PATCH: empty rejects, unknown/client-owned/protected fields reject, title/status-only patch retains reminder state, any supplied due_at (including none if contract permits) clears reminder_sent_at, and omitted due_at does not clear it. Compare updated_at semantics on actual persisted change/no-op. Future-only validation uses one operation clock, not ambient test wall time.
- The owner decided on 2026-10-02 to keep `User.email` provisioning-only and omit it from the self-service response. The acceptance successor changes AUTH-007/REL-001; the predecessor and hashes are recorded in `examples/golden-todo/REVIEW.md`. Do not change `policy.jadpo` or copy JWT email into output.
- Add UserWithTodos via a named query and the existing self/owner policy paths. The public shape remains `user_id` plus `List<TodoView>`. Use one required self-scoped User lookup followed by the existing indexed keyset-page query for Todo; it already projects declared `TodoView` fields and applies owner/deleted predicates before the bounded limit. This avoids widening include lowering, which returns full persisted child rows and has no child predicate. Keep the two-data-query ceiling and assert email/other private fields are absent.
- Test SQLite/PostgreSQL parity and physical query counts separately from authentication queries. Reuse generated-identity/entity-dossier coverage for generic foundations while providing golden-boundary evidence. Register focused runtime mode, run focused suites then `python3 tools/verify.py` after source/runtime changes; retain independent policy review.

**Evidence recorded 2026-10-02:** the migrated target builds. `tests/runtime/golden-todo-routes.test.ts` passes eight SQLite HTTP tests / 78 assertions, including the existing create/read/patch obligations and UserWithTodos zero-, one-, and over-cap cases. The observed application SQL contains one self-scoped User lookup and one Todo page query; deleted and foreign rows do not consume the 100-row cap, child order is stable by `(created_at, id)`, and the response contains only declared fields. `tests/runtime/golden-auth-http-jwt.test.ts` passes four real-HTTP tests / 30 assertions, including a JWT with an attacker-controlled email claim and a response with no email. `bash tests/runtime/postgres.sh golden-todo-routes` passes on PostgreSQL 16.3; it checks the same capped ordered projection, deleted/foreign filtering, exact TodoView shape, indexed keyset plan and generated two-read path. All checks use the current gpt-6-luna/xhigh batch pin.

The synchronous UserWithTodos route is implemented. The frozen AUTH-007 and REL-001 entries remain `not_executed` in the repository's case map until the source-bound golden harness is run; focused evidence is linked in RM-107 history. REL-002 remains a separate persistence-integrity obligation. The aggregate 44-case gate and independent policy review remain open.

**Boundaries:** list/pagination RM-105; automatic soft-delete/disable/races RM-106; jobs RM-108. Existing manual deleted_at checks are not a claim of completed lifecycle enforcement. No 44-case completion claim.

### Next batch routing checkpoint — RM-205

The RM-107 batch is closed at its recorded gpt-6-luna/xhigh pin. The roadmap
recommends gpt-6-sol/high for RM-205's lifecycle contract and independent
language/policy review. No RM-205 work has started. Pause at this batch boundary
until the user changes the active model and the new setting is verified; the
workflow requires stopping for mismatches in either capability or effort.

### RM-204 additional boundary checks

Check raw percent encoding before a forgiving URL parser, decode exactly once, distinguish malformed syntax (400) from typed-shape rejection (422), and apply defaults only when absent. Fetch Headers can merge repeated lines: prove what the pinned Bun real HTTP boundary actually exposes before claiming duplicate-header rejection. In-memory Headers tests alone cannot establish that. Update parser, AST, semantics, runtime, OpenAPI and formatter as one coherent change; preserve credential-header exclusion. The implemented `tests/runtime/route-inputs.test.ts` suite owns the query boundary evidence. Use the supported Bun invocation `bun --no-install --env-file=/dev/null test` with generated fixtures prepared as required.

**2026-10-02 implementation checkpoint:** `query: ListTodos` and the reviewed
`headers: { trace_id: Text from "X-Trace" optional }` grammar now parse and
type-check. Query lowering validates the raw URL before one form-decoding pass,
rejects unknown/duplicate keys, parses structured values as JSON, materialises
source defaults only for absent keys, and separates 400 syntax failures from
422 typed-value failures. The real-listener `route-inputs.test.ts` suite passes
5 groups / 29 assertions and checks OpenAPI parity. Header checking rejects
invalid/duplicate/reserved wire names, unsupported types and cross-boundary
binding collisions. A Bun 1.2.20 loopback probe proved that repeated ordinary
`X-Trace` lines reach Fetch as only the last value and `getAll` is unavailable;
the built-in `node:http` compatibility layer instead preserves both lines in
`IncomingMessage.rawHeaders`. Generated applications with `headers:` now count
those raw names before constructing a Fetch Request, bind only one declared
value, ignore unrelated headers, and reject duplicate or missing required
values with 400. Request bodies cross the adapter as a stream, so authentication
and CSRF checks still run before body consumption; unconsumed input is drained
after the response. Int query decoding rejects values outside JavaScript's safe
integer range rather than rounding them.

**Closure:** the query/header suite and protected-route listener suite pass 13
tests / 152 assertions, including safe-integer boundaries, bearer authentication,
browser Origin/CSRF, duplicate raw credentials, POST body conversion and response
delivery. The independent Sol High re-review approves the corrected runtime and
security boundary. The fresh 54-step [supported gate](../../build/validation/20261002T083246-98017/report.json)
passes; complete golden and release gates remain RM-109/RM-110. RM-204 is complete.

### RM-601 implementation checkpoint, 2026-10-02

`artifacts/approval.rs` now emits schema v2 local subjects with SHA-256 binding,
checked before/after sources, supplied intent, exact source hashes, declaration
body tokens, individual grant decisions, policy/data/failure/transaction facts,
and source-derived transitive data-effect witnesses. Unsupported categories are
explicit. `jadpo approval` adds baseline pinning and JSON/text inspection without
replacing build output. Nine source-based tests pass, including deliberate
omission of a transitive read; this is not external-service AP-11 evidence.
CLI baseline coverage is included in the Rust workspace gate. The first full
verification attempt stopped at a localhost socket permission in watch_protocol
(`build/validation/20261002T193429-29613/report.json`), an environment failure;
The later full run with approved local integration permissions passed 55/55
checks: `build/validation/20261002T194433-31665/report.json`.
RM-601 remains open for the gaps listed in [the protocol](../approval-protocol.md#2-canonical-approval-subject),
including complete compiler/registry provenance and independent review. Timing:
`RM-601-4f37f6034cf2`, Astra High. Continue the same batch at RM-207's transport
mapping now that its boundary contract is independently reviewed.

### Strengthened near-term plan: RM-601 complete behavioral artifact

Existing approval protocol and AP-11 are the acceptance authority. The emitter
now lives in `artifacts/approval.rs`; prior references to placeholder arrays and
FNV described the superseded scaffold. Source-derived effects and SHA-256 are
implemented. The remaining semantic categories still need their language
implementations and independent consequential review.

Sequence:

1. Map every ApprovalSubject category to checked semantic graph/policy/entity/effect facts, transitive-call summaries, or explicitly unsupported/unknown evidence. A category with a supported effect cannot be encoded as empty merely because extraction was omitted. Separate no effects, unavailable analysis and unknowns. Human request/rationale/provenance are supplied inputs, never invented compiler facts; source annotation grammar RM-214 need not be implemented first.
2. Specify versioned canonical bytes, absent-before semantics and explicit baseline input with exact checked source/compiler/schema provenance. Compare before/after facts and decisions, not only graph topology. Keep plain-text/non-graphical export fact-equivalent to canonical JSON; text headings are permitted and must not be hashed as canonical bytes. Record source edit/rename and prior artifact mismatches explicitly.
3. Retain RustCrypto SHA-256 for canonical approval binding; FNV elsewhere remains correlation-only. Keep verification independent of UI and do not adopt generated JSON or a compiler-input manifest as issuer authority.
4. Produce individual decision IDs and direct/transitive actor→route→action/query→data/output/effect impacts with exact evidence classifications. Stable schema registry IDs are available; general non-schema rename-stable IDs remain RM-219. Define versioned current declaration identity and conservative rename/unknown handling rather than pretending graph ordinals solve durable identity. The selected artifact contract must expose this limitation.
5. Build paired source fixtures: permission weakening, newly reachable unchanged declaration, external effect through nested call, changed body/value with unchanged graph topology, absent baseline, harmless rebuild/rebase, stale baseline and unsupported analysis. Assert exact expected semantic deltas from fixture contracts. Deliberately omit a transitive effect and show AP-11 conformance fails; ordinary snapshot equality is insufficient. Register artifact and CLI baseline flows in the common gate.

Acceptance: before/after behavior, individual decisions, supplied intent/provenance, complete supported transitive impacts, exact canonical binding and usable non-graphical export. No release approval, review UI comprehension or protected provider success is claimed; RM-603/604/602/704 own those gates. Required independent consequential policy/artifact review must examine missing effect classes and digest trust boundary.

Verification:

```sh
cargo test --manifest-path jadpo/Cargo.toml -p jadpo-core --lib artifacts::tests
cargo test --manifest-path jadpo/Cargo.toml -p jadpo-core --lib
cargo test --manifest-path jadpo/Cargo.toml -p jadpo-cli
python3 tools/verify.py
```

Add proposed integration test `jadpo/crates/core/tests/validation_approval_subject.rs` and run `cargo test --manifest-path jadpo/Cargo.toml -p jadpo-core --test validation_approval_subject` once implemented. The test must exercise source-derived effects and AP-11, not just serialization. Astra High preferred for this coupled semantic/security design; Sol High suitable with bounded resolved extraction/digest contracts; Luna Extra High suitable for then-explicit compiler edits. Artifact emitter conflicts with RM-204; assign one writer or serialize landing.

## RM-205/RM-301 contract batch checkpoint — 2026-10-02

Actual pin remains the owner-selected `gpt-6.1-sol/high`. RM-205 is frozen after
independent correction re-review resolved authority, visibility-before-limit
and original policy digest provenance. The original D01–D43 section hashes
unchanged to `66e7a8f586b62ed92c3a7220f524e25aee5808ca60b8504fb3c2d225ef2d68bd`;
D30-M1 has its separate row pin. The 27 cases are contract-only; RM-206 owns
implementation. Closure changes status/references only, preserving reviewed
semantic body and candidate hash in the owning document/history.

RM-301 corrections now have a separate service source, 30 contract-only cases
and a scoped JOB-001 acceptance successor pinned to the existing 44-case
contract. The pressure app, human-owned policy and current acceptance bytes
remain comparison evidence. The already-authorised independent reviewer resolved all six initial findings
and one retry-catalog ambiguity; final disposition supports semantic freeze.
RM-302 can implement the accepted semantics and settle exact spelling.
Runs: RM-205-090cf4e6fc8e, RM-205-8dff420ad28a and RM-205-fca941d693bd;
RM-301-c0b69a48d6e2. Interrupted RM-301-50be866d3066 retains a 5.34-minute
unknown interval and is excluded from precise effort ratios. Next action is
resolve any service re-review findings, freeze that contract if accepted, then
reassess the next bounded implementation batch. No new model handoff inside this
contract batch, no persistent Goal and no new delegation scope.

**Contract batch complete:** RM-205 and RM-301 are evidenced completed decision
tasks; RM-206 and RM-302 are now ready to start their first implementation
slices. The semantic service successor remains separate from the historically
preserved app/policy/current acceptance; the JOB-001 overlay and its 43 unchanged
IDs are the accepted review input for future application/harness migration.
No compiler, HTTP or job runtime completion follows from these freezes.
RM-301 closure run: RM-301-1f3f6ad95b7b; review pins the 30-case catalog as
`f07d92b18bd4ae66644a56e4942304ce0bb34872023cfe1bfc2abaaf6b68d236`.

## Next implementation batch entry — RM-206/RM-106

The RM-205/RM-301 contract batch has finished its agreed freeze boundary. Select
**RM-206 lifecycle lowering and RM-106 delete/disable integration** through
focused two-adapter/full-supported verification and self-review. Saved plans
are adequate now that DATA-007 is frozen; the hardest expected work is guarded
mutation, visibility and write-ownership checking across compiler/runtime.
Use the existing **gpt-6-luna/xhigh** implementation recommendation for this
bounded settled-contract batch. Retain that pin through both tasks and normal
stage transitions; required independent implementation review remains separately
pending until authorised. No reviewer is spawned by this selection.

The owner selected **gpt-6-luna/xhigh** for the full RM-206/RM-106 batch (“use
it, it's cheaper than 6”). The partial run below confirms this pin; retain it
through RM-206 and its verification, then RM-106 only after the dependency gate.
Run
[`RM-206-601deb5ffab3`](../task-timing/runs/RM-206-601deb5ffab3.jsonl) records
the current partial implementation slice.

**RM-206 partial checkpoint, 2026-10-03:** Todo soft deletion now follows the
frozen timestamp-only contract: `status` remains ordinary caller-controlled
business data, while `deleted_at` owns visibility and deletion. Compiler and
runtime cases cover lifecycle-owned initial values, direct and patch-write
rejection, nested post-delete action concealment, 100-row visibility-before-
limit, required-owner join filtering before limit, transition/update races,
foreign-key restriction of retained rows, and clause-bound 500-row purge with
rollback/audit checks. The eight runtime cases pass on SQLite and PostgreSQL;
the compile corpus has 209 pairs. The [full supported report](../../build/validation/20261003T044513-41532/report.json)
passes all 58 steps. The frozen 27-case catalog is mapped in the
[lifecycle plan](../lifecycle-plan.md#frozen-case-evidence-map-2026-10-03):
17 cases covered, 7 partial and 3 open. Golden HTTP/auth/job integration,
three open source-authority/authentication cases and independent implementation
review remain open; RM-206 and dependent RM-106 are not complete.

The timer [`RM-206-601deb5ffab3`](../task-timing/runs/RM-206-601deb5ffab3.jsonl)
records 280.79 active minutes, including 89.15 minutes in verification/review,
against the original 1–4h forecast. Active effort exceeded the 4h upper bound
by 40.79 minutes; keep the original estimate as historical and do not describe
it as remaining effort. Continue RM-206 only within the selected
`gpt-6-luna/xhigh` pin, and start RM-106 only after the
lifecycle dependency and review gate are met. Do not record either task complete
from this slice.

**RM-206 resumed partial checkpoint, 2026-10-03:** The two source-level
maintenance-authority attacks now fail with `SEM_UNKNOWN_CALLEE` (fixtures 182
and 183), and a competing database connection races a retention-eligibility
update against purge on SQLite and PostgreSQL; both suites pass, with locking
serializing the outcome and the audit matching committed removals. The compile
corpus has 211 pairs, the lifecycle suite has nine passing cases on both
adapters, and the [latest supported report](../../build/validation/20261003T051204-47840/report.json)
passes 58/58 checks while keeping the golden application gates open. The frozen
case map now has 19 covered, 7 partial and 1 open; auth freshness and golden
HTTP integration remain on their owning path. The resumed timer
[`RM-206-a52601d5d5b4`](../task-timing/runs/RM-206-a52601d5d5b4.jsonl) adds
21.77 active minutes, and the checkpoint timer
[`RM-206-1e49ac0edc14`](../task-timing/runs/RM-206-1e49ac0edc14.jsonl) adds
1.10 minutes. The three partial runs total 303.66 active minutes, including
102.19 minutes in verification/review; this exceeds the original 4h upper bound
by 63.66 minutes. Independent implementation review remains pending until
separately authorised. RM-106 is still ineligible at this dependency boundary;
next, obtain that review, resolve any findings, then continue this same batch
at RM-106 under the existing `gpt-6-luna/xhigh` pin.

**RM-206 authentication-freshness checkpoint, 2026-10-03:** Added a generated
golden-migration auth-host test that changes the authoritative User lifecycle
state to disabled, then verifies signed ordinary access remains inside its
bounded window, fresh authority and refresh reject immediately as
`UserDisabled`, and the signed credential expires within five minutes. The
focused golden protected-route suite passes **8/8 tests and 121 expectations**;
the full supported verifier passes **58/58 checks** with the golden application
and release gates still open ([report](../../build/validation/20261003T054854-64386/report.json)).
The frozen lifecycle map is now **19 covered, 8 partial, 0 open**. This test sets
the status directly; RM-106 still needs to exercise the actual disable route and
its HTTP behavior. The independent RM-206 implementation review remains pending.
Run [`RM-206-3c05cc7263f3`](../task-timing/runs/RM-206-3c05cc7263f3.jsonl)
records 7.86 active minutes (4.15 in verification), actual `gpt-6-luna/xhigh`;
outcome partial. RM-206 remains open, so RM-106 stays queued at its dependency
and review gates.

**2026-10-03 build-session entry checkpoint:** The owner asked to start the
unattended build session. Resume the existing RM-206/RM-106 batch through
RM-206's remaining auth-freshness/golden HTTP evidence and, only after its
dependency and review gates, RM-106. The saved pin remains
`gpt-6-luna/xhigh`. A host notice indicates the active model changed since its
last verification, but current-chat metadata does not expose the exact model or
effort. The batch was **model deferred** before implementation. The owner then
confirmed the picker was ready; this is recorded as confirmation that the
current setting matches the existing `gpt-6-luna/xhigh` pin. RM-206 resumed as
partial run `RM-206-3c05cc7263f3`; see the authentication-freshness checkpoint
above for code, verification and remaining gates.

**RM-302 implementation checkpoint, 2026-10-03:** Added a bounded `service`
declaration parser and a checked single-operation `ReminderMail` effect. The
semantic graph validates the exact contract and project-local pinned snapshot
bytes, request/receipt types, secret slot, fixed authority, retry policy and
outcome mappings; it adds the operation to call, type and failure checks and
emits `audit/services.json` with runtime conformance explicitly unestablished.
Dynamic authority, import-pin/snapshot changes, missing imports/configuration,
secret-slot changes, unstable identity, broadened operation shape, pure callers,
atomic actions, and direct or transitive persistence-write/service combinations
have rejection evidence. The compile-pass example has an exact formatter
expectation and is included in the verifier. Compile-fixture inventory is now
212 pairs. The full supported gate passes **58/58 checks** ([report](../../build/validation/20261003T071455-23519/report.json)); the full golden application and its release gates remain open.

This is a partial implementation checkpoint, not a task closure. Independent
language/security review remains required, and actual HTTP adapter behavior and
provider conformance belong to RM-303. Run
[`RM-302-6b717ae9cae8`](../task-timing/runs/RM-302-6b717ae9cae8.jsonl)
records the original 2–8h forecast and `gpt-6-luna/xhigh`; the numeric estimate
is unchanged. Do not close RM-302 or start dependent RM-303 until its review gate
is met.

**RM-206 continuation boundary, 2026-10-03:** Re-entry assessment confirmed
that the current frozen-case map remains 19 covered and 8 partial, with no new
case evidence in this 4.16-minute review-preparation run. The remaining golden
disable/delete HTTP integrations are RM-106 scope; disabled-owner reminder
selection is RM-108 scope. RM-106 remains queued until RM-206's required
independent implementation review and dependency gate are satisfied. Run
[`RM-206-e0950fbb8542`](../task-timing/runs/RM-206-e0950fbb8542.jsonl)
records the partial assessment under the existing `gpt-6-luna/xhigh` batch pin.

**RM-206 independent implementation review correction checkpoint, 2026-10-03:**
The initial review found that raw host persistence calls could set lifecycle-owned
initial state and choose a retention cutoff. Generated `create_User` and
`create_Todo` now reject mismatched lifecycle initial values; retention purge no
longer accepts a caller cutoff and calculates its bound from the import-time
captured configuration and an attempt time clamped to the wall clock. Direct
host-boundary regression cases pass on SQLite and PostgreSQL. `cargo test -p
jadpo-core` passes 85 unit tests, and the supported verifier passes 58/58 checks
([report](../../build/validation/20261003T083341-45067/report.json)); release and
golden gates remain open. Independent correction re-review is pending, so RM-206
stays in progress and RM-106 remains queued. Runs
[`RM-206-743d25b5ed43`](../task-timing/runs/RM-206-743d25b5ed43.jsonl) and
[`RM-206-edc3313f4952`](../task-timing/runs/RM-206-edc3313f4952.jsonl) record the
fix/verification and correction-review work under the existing batch pin.

**RM-206 retention-initialization correction, 2026-10-03:** The correction
review found that persistence captured its retention duration from `Bun.env` at
module import, which could differ from the environment passed to the first-party
initializer. The generated persistence target now binds purge retention from the
validated application configuration; the first-party initializer binds after
successful authentication setup, while the maintenance worker binds the config
it loaded for its run. Purge remains cutoff-free and fails closed until a
configuration is bound. A five-day initialized value purges a ten-day-old Todo
even while `Bun.env` says 30 days; direct purge before binding fails closed. All
13 lifecycle runtime tests pass on SQLite and PostgreSQL. `cargo test -p jadpo-core`
passes 85 unit tests plus integration tests. The generated
first-party lifecycle case asserts the initialization binding. The independent
correction review approved the scoped fixes
([report](../../tests/validation/rm206-independent-correction-review.json)); a
second read-only spot check found no residual issue. RM-206 is complete within
its compiler/runtime/audit scope and RM-106 has started. Runs
[`RM-206-fab3228ab24e`](../task-timing/runs/RM-206-fab3228ab24e.jsonl)
and [`RM-206-eb168cb23348`](../task-timing/runs/RM-206-eb168cb23348.jsonl)
record the correction and final fail-closed verification under the existing
`gpt-6-luna/xhigh` pin. Compile corpus validation is recorded in
[`RM-206-d840e261433b`](../task-timing/runs/RM-206-d840e261433b.jsonl). A later
correction-review timer has unreliable active minutes due to an unobserved
interval and is excluded from the reliable sum; the other 11 recorded runs total
367.15 active minutes, 127.15 minutes beyond the original 4h upper bound. Keep
the estimate historical, not remaining effort.

**RM-302 independent implementation review, 2026-10-03:** The review found that
the checker accepts request/receipt types without validating their closed
schemas, permits a non-secret configuration field in the credential slot, and
does not parse/compare the pinned operation schema or enforce project-root-only
import resolution. The report is
[`rm302-independent-implementation-review.json`](../../tests/validation/rm302-independent-implementation-review.json).
At the time of this review these issues remained open; see the correction checkpoint
below for the later resolution.
The review timer is
[`RM-302-0a05e99f86ab`](../task-timing/runs/RM-302-0a05e99f86ab.jsonl).

**RM-302 correction and independent re-review, 2026-10-03:** The checker now
requires closed request/receipt schemas with exact imported wire shapes and the
nominal `ReminderIntentId` field, verifies the `mail_api_key` secret marker, parses
the supported pinned operation subset, and keeps canonical import resolution
inside the project root. The independent reviewer reproduced the valid positive
case and rejected the one-field `Uuid` mutation; all three original findings are
resolved in the [correction review](../../tests/validation/rm302-independent-correction-review.json).
The final [full supported report](../../build/validation/20261003T213248-5311/report.json)
passes 58/58 steps. Golden behavioural and release gates remain open. Recorded
effort is 130.89 active minutes across timed implementation and initial-review
runs, all recorded as `gpt-6-luna/xhigh`; the follow-up review's model/effort/time
are unknown. Original 2–8h estimate unchanged; runs [`RM-302-6b717ae9cae8`](../task-timing/runs/RM-302-6b717ae9cae8.jsonl),
[`RM-302-0a05e99f86ab`](../task-timing/runs/RM-302-0a05e99f86ab.jsonl), and
[`RM-302-5f0a10056c41`](../task-timing/runs/RM-302-5f0a10056c41.jsonl).

**RM-106 lifecycle route checkpoint, 2026-10-03:** The migrated User and Todo
entities now apply the accepted initial values, visibility and guarded
transitions. User self-disable is a fresh-authenticated 204 and retains child
Todos; the User policy grants the self role `update` only for the named
`disable_user` operation. Todo owner deletion is a 204 soft-delete; later GET,
PATCH, list and repeated delete requests conceal the row. SQLite and PostgreSQL
route evidence also covers cross-owner denial, refresh rejection and the
five-minute signed credential bound. SQLite races HTTP PATCH against DELETE
and verifies one hidden final state; the RM-206 lifecycle suite already proves
disable/update and delete/update guarded-write races on both adapters. The full
supported verifier passes 58/58 steps, including the disposable PostgreSQL
route run ([report](../../build/validation/20261003T203150-84421/report.json));
the broader golden behavioural and release gates remain open. The [independent
auth/lifecycle review](../../tests/validation/rm106-independent-route-review.json)
approves this scoped integration with no blocking findings; two nonblocking
test-strength notes remain recorded there. The implementation and review runs
record 238.45 active minutes total: 224.35 for implementation and verification
in [`RM-106-b3d3a86b6de8`](../task-timing/runs/RM-106-b3d3a86b6de8.jsonl), and
14.1 for review in [`RM-106-cad27e70d915`](../task-timing/runs/RM-106-cad27e70d915.jsonl).
The reviewer model and effort are unknown under the owner's scoped continuation
instruction. The original 1–4h estimate is unchanged.

**RM-402 route-deadline checkpoint, 2026-10-04:** The optional route
`deadline:` now carries a monotonic budget into persistence, retries and service
calls, with retry-window expiry separated from an owner-declared deadline.
The SQLite failure-injection case exceeds its 200ms route budget during a
synchronous insert, proves rollback and returns 504 after the statement returns;
the full SQLite persistence suite passes 37/37, including 503 for retry-window
exhaustion, acknowledged-commit preservation and unknown-outcome mapping. The
core Rust library passes 87/87, the workspace passes with the socket-bind test
skipped, the 215 compile-fixture pairs pass, and the supported gate's other
reached steps pass. The full verifier stopped at the sandbox localhost-bind
test ([report](../../build/validation/20261004T041018-15968/report.json)); the
PostgreSQL cluster could not start because `shmget` is denied, so no PostgreSQL
deadline result is claimed. RM-402 remains partial pending PostgreSQL evidence,
independent transaction review, and supported active-query interruption. The
[TX-001 checkpoint](../transaction-retry-plan.md#rm-402-route-deadline-implementation-checkpoint--2026-10-04)
and [timing run](../task-timing/runs/RM-402-7b7a1d83e32e.jsonl) retain details.

**RM-403 advisory readiness checkpoint, 2026-10-04:** Generated `/health/ready`
responses now keep required database state in `checks` and expose every
declared service under a stable snake-case ID in `advisories` with status
`unprobed`. An in-process request to the generated mail app returns
`advisories.reminder_mail: unprobed` without `Bun.connect` or `fetch`; the
service-fake suite passes 4 tests/25 assertions. This remains advisory and
cannot change the ready decision. RM-403 is partial: startup-time recovery is
covered by the prior supported suite, but this turn cannot refresh the HTTP and
PostgreSQL readiness evidence due sandbox binding/shared-memory failures;
post-startup auth-store recovery, broader active-call deadlines and independent
review remain. See the [CONFIG-P5 record](../configuration-plan.md#config-p5--liveness-and-readiness).

**RM-403 listener-free recovery continuation, 2026-10-04:** Added a direct
generated-handler test to preserve progress when this host disallows socket
binding. It gates identity while SQLite is unavailable, recovers readiness when
the missing directory returns, provisions and authenticates a user, then proves
the same credential receives `authentication_unavailable` during an auth-session
table outage and works again after restoration. The focused test passes 13
assertions, including the 32-request single-initialization check. This is
handler-level evidence only; it does not replace the prior
live HTTP suite or PostgreSQL readiness/auth evidence, and broader active-call
deadline/concurrency and independent review remain. See the
[CONFIG-P5 record](../configuration-plan.md#config-p5--liveness-and-readiness)
and [runtime test](../../tests/runtime/readiness-recovery.test.ts).

**RM-601 actor-scope continuation, 2026-10-04:** The canonical v4 route scenario
now retains both sides of the checked route-policy contract, including the
exact actor subjects for each operation obligation, when authentication changes
without changing the effect paths. The regression confirms an unchanged read
graph still shows newly admitted unauthenticated callers, preserves
`Access.public` in the after-policy facts, and labels the actor as a candidate,
not a feasible counterexample. The full `cargo test -p jadpo-core` suite passes,
including 87 library tests and the 15-test approval-subject integration suite;
the changed approval implementation and regression file pass `rustfmt --check`,
and `git diff --check` is clean. The latest broad verifier report remains
historical: a fresh `tools/verify.py` run stopped at the host-denied localhost
bind test, so no new full-gate result is claimed. RM-601 remains partial for
principal/role resolution, feasible input/data counterexamples, durable job and
emission integration, trusted build attestation, and independent artifact
review. Run [`RM-601-ff5348289dfe`](../task-timing/runs/RM-601-ff5348289dfe.jsonl)
records this bounded continuation under the existing `gpt-6-luna/xhigh` pin.

**RM-601 actor-subject delta continuation, 2026-10-04:** Canonical approval
subjects are now v5. Route scenarios emit sorted added/removed actor-subject
deltas for compiler-derived operation obligations and restricted field reads,
keyed by their entity/effect/source or entity/field surface. The added regression
changes a checked customer read grant from `Access.authenticated` to
`Access.public` with an unchanged effect graph and verifies both sides of the
delta. Each delta is labelled as a compiler-checked subject change, not proof of
principal membership, input/data feasibility or executed behavior. This schema
increment passes the 16-test approval-subject suite, 88 core library tests,
all core integration suites, and the CLI baseline/text test. The updated files pass formatting checks and
`git diff --check`; a fresh broad
verification remains host-blocked by localhost binding, and the prior 61/61
report is historical. Independent artifact review and the remaining runtime,
job/emission, principal-resolution and feasible-counterexample gates still
prevent closure. Runs [`RM-601-f5dc81ff0545`](../task-timing/runs/RM-601-f5dc81ff0545.jsonl)
and [`RM-601-5904d999cb93`](../task-timing/runs/RM-601-5904d999cb93.jsonl)
record this continuation under the existing `gpt-6-luna/xhigh` pin.

**RM-207 provider-boundary re-entry, 2026-10-04:** Source inspection found the
reference HTTP adapter already has cases for provider acceptance followed by a
lost acknowledgement, post-dispatch timeout/cancellation, malformed replies,
and pre-dispatch connection refusal. Its committed-acceptance fake verifies one
dispatch and the safe `outcome_unknown` response; the separate approved service
adapter task owns that transport implementation. No duplicate test or source
change was made. RM-207 remains partial because durable worker state across
restart, no-reissue and reconciliation belong to the gated job path, while
deadline-aware operational mapping and independent runtime review remain. Run
[`RM-207-aee77ee77b62`](../task-timing/runs/RM-207-aee77ee77b62.jsonl) records
the 3.18-minute source/evidence review under the current batch pin.

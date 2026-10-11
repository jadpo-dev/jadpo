# ASYNC-001 durable jobs and events

**Status:** semantic contract frozen, 2026-10-04, after the authorised
independent transaction/effect review
approved the owner-selected candidate without blocking findings.  
**Task:** RM-305 · **Version:** 0.1 · **Updated:** 2026-10-04

This document records the durable job/event contract required before RM-306
implements compiler declarations and a transactional outbox. It follows D4 in the
[decision sprint](archive/decision-sprint.md#d4--jobs-and-events-in-async-001), the
[transaction retry contract](transaction-retry-plan.md), the accepted
[service uncertainty rules](service-plan.md), and TIME-D05 in the
[time/testing plan](time-testing-plan.md). The
[candidate trace set](../tests/assurance/async-contract-v0.1-candidate.json) is
planning evidence only; none of its cases has runtime evidence yet.

## Proposed contract

The first implementation uses the authority database already governing the
mutation, through the compiler-owned SQLite and PostgreSQL adapters. It does not
select a hosted queue or claim exactly-once external effects. A committed source
mutation and its durable event/job intent commit in one transaction; a rollback
publishes neither. Workers deliver at least once. Stable compiler-owned identity,
payload version, ordering key and effect idempotency make repeats safe only where
the checked operation proves them safe.

The owner has selected database-backed jobs, backend-neutral authored semantics,
and coalescing missed interval schedules. For v0.1, the recommended scope is UTC
fixed intervals, singleton schedule execution, one coalesced pending follow-up,
and bounded continuations. Cron and additional concurrency modes can be added by
a reviewed successor when a concrete application requires them. The golden app's
15-minute reminder and 1-hour purge intervals retain their 500-row bound.

### Durable identity and transaction boundary

Each logical delivery has a compiler-generated immutable ID, operation and payload
version, immutable typed payload, ordering key, enqueue revision/time, attempt
history and terminal evidence. Source code cannot forge or mutate the delivery
record. Repeated enqueue of the same source operation/revision resolves to the
existing ID. A changed business schedule creates a new intent only when the owning
contract says it is a new operation; the accepted reminder successor already
requires a new ID per due-date schedule and retains that ID across retries.

The authority mutation and outbox intent use the same database transaction and
adapter. A failed transaction exposes no committed intent or change revision.
The worker claims an intent in a short transaction and releases database locks
before running user code or an external provider. Database-specific locking and
claim SQL remain inside the adapter; the authored job contract is identical on
SQLite and PostgreSQL.

Delivery order is FIFO within an explicit ordering key, not global. A pending,
running or safely retrying earlier intent prevents a later intent with the same
key from overtaking it. A succeeded, cancelled or known-no-effect failed intent
is terminal and releases the key; `outcome_unknown` blocks later intents until
trusted resolution. Different keys may progress independently. The initial
golden reminder ordering key is the Todo identity; a new schedule revision has a
new intent but cannot bypass an earlier `outcome_unknown` intent for that Todo.
Every stored item carries a payload/contract version. A worker with no supported
handler for that version refuses it without silently decoding it as a newer
shape.

### State and effects

The durable state vocabulary is `pending`, `running`, `retry_wait`, `succeeded`,
`failed`, `cancelled`, and `outcome_unknown`. A separate cancellation-request
marker may be recorded for a running job. State transitions and attempt records
are durable and conditional on the current fencing generation.

| State | Meaning and permitted next action |
|---|---|
| `pending` | Committed intent is eligible for a claim; it may be cancelled before any effect. |
| `running` | One current worker claim holds a bounded lease and fencing generation. A worker may continue only while its generation remains current. |
| `retry_wait` | The last attempt proved no external effect occurred and policy permits a bounded retry after its selected delay. Cancellation may end it. |
| `succeeded` | The checked operation returned a valid typed receipt and the state transition committed. |
| `failed` | A permanent failure or exhausted safe retry budget is recorded; no automatic retry occurs. |
| `cancelled` | Work ended before any effect could have occurred, or a running worker cooperatively stopped at a checked safe point. |
| `outcome_unknown` | An effect may have occurred without trustworthy acknowledgement. No worker, lease expiry, restart or later schedule may resend it automatically. |

Before an external effect, the runtime commits a possible-dispatch checkpoint.
If the process then loses its outcome, the intent remains `outcome_unknown`. A
timeout, connection loss, malformed acknowledgement, expired lease or worker
restart is not evidence of no effect. This reuses the accepted
[SERVICE-001 uncertainty contract](service-plan.md#reminder-identity-reconciliation)
and RM-207 boundary mapping. Database transaction retries use only the proven
phase rules in [TX-001](transaction-retry-plan.md#boundary-and-current-gap); an
outbox write shares its source transaction, but an external call never does.

Known-no-effect failures may retry only within finite cumulative invocation-count
and lifetime budgets for the logical intent, with bounded exponential backoff
and full jitter. The lifetime begins at the first valid claim, is persisted as an
absolute UTC deadline, and never resets on reclaim or a later schedule. A schedule
activation never resets either budget. The mail
operation retains its accepted maximum of three provider attempts over thirty
seconds within each invocation and one stable intent ID; the durable job's own
budget caps how many later scheduled invocations can occur. A job's
`retry: next_schedule` is a wake-up choice, not a budget: the next scan may resume
a known-safe intent only while its cumulative budgets remain. The golden job
integration must declare these finite limits; RM-108 owns their concrete values.
Exhausted or permanent failures become `failed` with a dead-letter record that
retains the intent ID, version, safe failure class, invocation/attempt counts and
bounded diagnostic evidence. Logs must not contain credentials, raw provider
responses or unbounded payloads.

### Claims, time and scheduling

Claims have a finite lease and monotonically increasing fencing generation.
Lease expiry and renewal use the adapter's authoritative database time, never an
assumption that workers' host clocks agree. A reclaim increments the generation;
stale workers cannot acknowledge, update state or checkpoint progress. Lease
expiry alone never proves an external call did not occur. If an old worker has
already crossed the possible-dispatch checkpoint, later delivery remains blocked
until the outcome is safely resolved.

Each activation receives a stable TIME-D05 operation-time snapshot. The nominal
UTC schedule occurrence is separate data (`scheduled_for`); it is not substituted
for actual start time or refreshed within that activation. In-flight timeout uses
a monotonic clock. A deadline that must survive restart is stored as an absolute
UTC instant and rechecked against the next activation's wall snapshot.

Missed or overlapping fixed-interval ticks coalesce rather than creating one
queued run per elapsed interval. The proposed scheduler keeps at most one active
run and one pending follow-up per singleton schedule; further ticks update that
pending occurrence instead of increasing backlog. Each activation selects at
most 500 rows. If more work remains, it persists a bounded keyset continuation
and returns; it does not drain an unbounded queue in one activation. A later
scheduled scan rechecks current eligibility, so due-date, visibility and lifecycle
changes are not trusted from stale page payloads.

### Cancellation, recovery and replay

Cancelling `pending` or `retry_wait` work prevents a future effect. Cancellation
of a running job is cooperative at checked safe points. Once possible dispatch is
committed, cancellation records a request and awaits a known outcome; it cannot
label the effect rolled back. Late receipts update only their original immutable
intent and cannot mark a newer schedule complete.

`outcome_unknown` remains inspectable and blocks automatic retry, later scans and
same-key replay. Only trusted evidence that resolves the exact intent may move it
to a known terminal outcome or permit the already-declared safe retry policy.
Operator replay is an explicit privileged operation, never an authored job
capability. It preserves original intent ID, payload version and ordering key,
consumes the remaining cumulative budget, and cannot reset an exhausted budget.
A new business operation receives a new ID through its normal transaction.
Automatic status lookup, arbitrary payload editing and exactly-once claims are
outside this contract.

Terminal outcomes and attempt evidence are retained until an explicit reviewed
retention policy permits deletion. v0.1 defines no silent age-based purge and
does not let job authority invoke entity-maintenance retention. Broader inspect,
retry and export surfaces remain RM-308; RM-305 fixes their authority and state
transition semantics, not the operator UI.

## Candidate acceptance traces

The candidate catalog has positive, negative and boundary traces for atomic
enqueue, duplicate identity, claim/fence races, ordering, uncertain effects,
coalescing, bounded continuation, cancellation, retries, exhaustion, replay and
version mismatch. RM-306/RM-307 must implement these at the actual generated job
boundary with injected clocks and crashes, then run the same logical traces on
SQLite and PostgreSQL. A direct action call is not job-scheduler evidence.

## Owner decisions before freeze

The owner approved the following choices on 2026-10-04:

1. Keep v0.1 to fixed UTC intervals, singleton schedules, one coalesced pending
   follow-up and per-key FIFO; defer cron, global ordering and additional
   concurrency modes until a concrete use case requires them.
2. Require finite per-job execution deadlines and leases constrained by their
   operation budgets, without inventing a universal numeric lease/timeout default
   before representative runs are measured. Use database time and fencing for
   claim authority.
3. Require cumulative finite invocation-count and lifetime budgets per logical
   intent; later schedule activations do not reset either. Keep the accepted
   per-invocation provider limit of three attempts over thirty seconds, and settle
   the golden job's later-schedule limits in RM-108.
4. Retain immutable terminal/idempotency evidence until a separately reviewed
   retention policy exists; define no automatic purge in v0.1.
5. Accept the cancellation, no-blind-replay, at-least-once and dead-letter rules
   above, including the frozen service and transaction uncertainty boundaries.

The independent review approved the exact pre-freeze document SHA-256
`c67d1b73ef685ea5fb569016364189ee52364fe12184489766af4b2046c627df`
and trace-catalog SHA-256
`508bdf6a2b04b803cf67147a8c21d271cf55f40b91bf07a9ebbf622f3a76b7d5`.
This status/footer update records that freeze without changing the reviewed
behaviour or owner decisions. The catalog's status metadata is updated only;
its 18 trace definitions remain unchanged and `contract_only_not_executable`.
RM-306 owns declarations, transactional outbox and restart-safe state; RM-307
owns generated worker/scheduler conformance; RM-108 owns concrete golden limits
and reminder integration. No runtime or golden completion is certified here.

# Pre-implementation decision sprint

**Status:** candidate planning control plane

**Prepared:** 2026-09-27

**Purpose:** clear owner-controlled design blockers before the next unattended
implementation run. This document schedules decisions; it does not authorise
compiler or runtime work by itself.

## 1. What is actually blocked

The parked work falls into distinct classes. Treating all of it as one backlog
would mix decisions that can be made now with evidence that still requires
people or external systems.

| Class | Items | Planning treatment |
| --- | --- | --- |
| Near-term owner decisions | none in D1/D2 | `TIME-001`, `TEST-001`, and `POLICY-001` are approved; continue with the service/async decision package. |
| Declared-effect contracts | `SERVICE-001`, service-facing `ASYNC-001` | Freeze together so provider calls, secrets, retry, and failure mapping agree. |
| Distributed execution | jobs/events `ASYNC-001`, `WORKFLOW-001`, remaining physical `CONSISTENCY-001` | Decide from explicit failure and recovery pressure, not from a preferred runtime library. |
| Approved, unblocked work | `AUTH-P1` through `AUTH-P3` and `NAME-P0` through `P2` implemented; Temporal/testing and policy compiler/runtime cores implemented to their adapter/external stop conditions | Keep the passing core; do not count service/job fakes, protected approval, live PostgreSQL, deployment hooks, or full golden integration as complete without their owning contract or environment. |
| External evidence | P10R review and five sessions, DX2 trials, live Postgres evidence, final P12 trials, JWT provenance, deployment hooks | Preserve as named gates. Planning cannot mark them complete. |
| Evidence-triggered extensions | streaming, advanced modules, transparent aliases, representation escape hatch, general message localisation beyond date/time formatting, live index advice, broader migrations, persistent tagged sums | Keep deferred until a frozen application or trial demonstrates pressure. |

## 2. Required decision order

Each decision package must be accepted before its implementation enters the
unattended queue. Later packages may be drafted in parallel, but they may not
silently choose an earlier package's semantics.

### D1 — `TIME-001` and `TEST-001`

Freeze time and testability first because authentication expiry, configuration
readiness, retries, schedules, and workflow deadlines all depend on them.

**Planning status:** approved on 2026-09-27. The implementation contract is the
[TIME-001 and TEST-001 decision plan](time-testing-plan.md), bound to section-2
digest `sha256:17b8ac38c645f6e3f42abf9a7644b9c6f67451d0c3407f7bf108ec057a931f01`.

Approved contract summary:

- the compiler owns a UTC wall-clock source and a monotonic deadline source;
- generated code does not read the host clock through ambient globals;
- actions, jobs, workflows, and adapters may read operation time through a
  declared runtime capability; pure functions receive time as data;
- named queries with a time cutoff take that cutoff as an explicit
  parameter rather than reading ambient time;
- one operation observes one stable captured wall time; the next route request,
  job activation, retry attempt, or workflow resumption gets a new snapshot;
- the bounded arithmetic core uses `Instant` for exact moments and `Duration`
  for elapsed time; a compiler-owned `temporal` library handles date-only facts,
  resolved zone-aware `Time`, a generated IANA `Zone` enum, local-period query
  bounds, and checked absolute/friendly human formatting;
- the test runtime supplies fixed and advanceable clocks deterministically;
- external fakes implement only declared capability contracts;
- route and job tests distinguish direct callable evidence from black-box
  boundary evidence; and
- generated tests are scaffolding, not assurance proof. General property-test
  syntax and arbitrary dependency injection remain deferred.

**Decision artifact:** `time-testing-plan.md` with source shapes, diagnostics,
runtime contracts, crash cases, and deterministic acceptance evidence.

The cross-cutting callable decision discovered during D1 is separately frozen
in the [language-wide naming and qualification
contract](naming-and-qualification.md), section-2 digest
`sha256:914e6c32c3d349cccfc9184dfc1dc40eb9671b247b4664d41d964c338901c0b4`.
NAME-P0–P2 applies that same rule to Temporal, collections, authored modules,
entity operations, constructors, variants, capabilities, diagnostics, and all
future standard-library families.

### D2 — `POLICY-001`

Freeze the policy language and proof boundary before protected target
generation or final golden-todo integration.

**Planning status:** approved on 2026-09-27. The implementation contract is the
[POLICY-001 decision plan](policy-plan.md), bound to section-2 digest
`sha256:66e7a8f586b62ed92c3a7220f524e25aee5808ca60b8504fb3c2d225ef2d68bd`.

Approved contract summary:

- authorization is default-deny and separate from authentication;
- qualified scoped roles come only from authoritative direct relationship or
  membership bindings, never credentials or request data;
- explicit `Access.public` and `Access.authenticated` cover the two cases that
  deliberately require no stored role without creating a second permission
  vocabulary;
- one role-first entity matrix governs compiler-derived
  `create`/`read`/`update`/`delete` effects across every named query and action;
- field policy is exception-only and may narrow but never widen entity access;
- routine reads and mutations receive automatic policy scope before data is
  released or changed, without hand-authored tenant checks;
- input validation, supplied patch fields, generated/lifecycle ownership,
  database integrity/decoding, projections, and exact output validation compose
  as separate fail-closed gates without silent stripping;
- lifecycle and business predicates remain distinct from authorization even
  when one database statement implements them;
- routes retain authenticated-by-default transport semantics and inherit
  entity policy through their checked call graph; and
- protected human approval binds both policy and semantic-graph changes so an
  input, output, route, field, binding, or call edge cannot widen access
  unnoticed.

**Decision artifact:** `policy-plan.md` with canonical multi-company source,
validation/trust-boundary composition, database lowering, diagnostics,
approval records, adversarial evidence, and POLICY-P0–P6.

### D3 — `SERVICE-001` and the service side of `ASYNC-001`

Freeze outbound provider calls, secret flow, and retry semantics as one effect
contract.

**2026-10-02 disposition:** [SERVICE-001](service-plan.md) is independently
reviewed and semantically frozen through RM-301. Its successor declaration,
30-case catalog and scoped JOB-001 overlay are pinned by the review. RM-302
owns checked grammar/import/effect implementation and RM-303 owns real HTTP
conformance; the remainder of ASYNC-001 is still RM-305 work. The baseline
below is historical motivation; the owning service contract supplies the exact
accepted no-effect/unknown rules.

Candidate baseline:

- a checked service declaration owns endpoint provenance, egress bounds,
  credential slots, operations, request/response types, timeouts, retry,
  idempotency, and failure mappings;
- ordinary source cannot issue arbitrary HTTP requests or choose dynamic hosts;
- secrets flow only from typed configuration into declared credential slots;
- service operations are callable only from actions, jobs, or workflows, not
  pure functions or queries;
- an OpenAPI snapshot import produces a digest-pinned review artifact and does
  not grant semantic authority directly;
- imported contracts may be narrowed explicitly but not silently broadened;
- automatic retry requires a listed transient cause, proved no effect, a
  repeatable operation, stable durable identity and remaining shared budget;
  an idempotency key alone is insufficient;
- provider objects and raw provider errors never cross the service boundary;
  and
- test fakes expose only declared operations and outcomes.

**Decision artifact:** `service-plan.md` with import/versioning rules, secret
sinks, failure normalisation, retry cases, and provider-fake evidence.

### D4 — jobs and events in `ASYNC-001`

Freeze durable delivery before selecting queue infrastructure.

Candidate baseline:

- authority mutation and event intent use a transactional outbox;
- delivery is at-least-once, with compiler-owned identity, version, ordering
  key, and idempotency metadata;
- jobs declare concurrency, timeout, retry, and idempotency behaviour;
- retries use bounded exponential backoff with deterministic jitter and a
  dead-letter terminal path;
- schedules use UTC intervals or reviewed cron forms, explicit concurrency
  policy, bounded selection, and continuation;
- enqueue, cancel, and replay are explicit named operations; and
- deterministic state-machine tests cover duplicates, crashes, late delivery,
  exhaustion, cancellation, and replay.

**Decision artifact:** `async-plan.md` with delivery state, scheduling semantics,
operator controls, and acceptance traces.

### D5 — order/payment pressure and `WORKFLOW-001`

The difficult P12 application must define the pressure before the workflow
runtime is frozen. Its dossier must include order and inventory authority,
idempotent payment/refund operations, webhooks, declines, timeouts, duplicates,
late success, partial progress, cancellation, refund failure, outcome
uncertainty, reconciliation, fallible compensation, and manual intervention.

Candidate workflow baseline:

- the authority database persists instance identity, definition version,
  input, state, completed steps, attempts, deadlines, idempotency keys, and last
  known outcome;
- intent is checkpointed before an external effect and outcome after it;
- execution is at-least-once and combines provider idempotency with explicit
  reconciliation for unknown outcomes;
- local database work uses normal transactions without claiming a distributed
  transaction;
- compensation is explicit, durable, and allowed to fail;
- cancellation is a state transition, not process termination;
- terminal states distinguish `succeeded`, `compensated`, `failed`,
  `outcome_unknown`, and `intervention_required`; and
- operators can inspect history and perform bounded retry, reconciliation,
  compensation, cancellation, or manual resolution.

**Decision artifacts:** `order-payment-pressure.md` followed by
`durable-workflow-plan.md`. Approve them together so the runtime answers the
actual application rather than a hypothetical one.

### D6 — physical `CONSISTENCY-001` adapters and readiness

Freeze projection delivery only after an application names a required physical
derived store.

Candidate baseline:

- authority mutation and outbox record commit atomically;
- delivery is at-least-once per entity and revision, not globally ordered;
- adapters apply changes idempotently and expose watermarks, lag, failures,
  rebuild, and reconciliation;
- rebuild uses named generation, catch-up, verification, and switch stages;
- authoritative and read-your-writes paths never depend on a stale derived
  store; bounded-staleness paths must prove their bound;
- unavailable or stale projections follow an explicit fallback or return a
  declared unavailable/stale outcome; and
- readiness requires only dependencies needed by currently routable behaviour.

**Decision artifact:** `consistency-adapter-plan.md`. Select a physical adapter
only if a frozen application requires one; otherwise keep generation
fail-closed through P12.

## 3. Gates that planning must not erase

The following remain external completion conditions even after every design
package above is accepted:

- independent P10R contract review and five first-user sessions;
- fresh-agent and first-user DX2 repair-cycle trials;
- live PostgreSQL evidence for the remaining database-specific behaviour;
- final P12 comparison trials against the frozen protocol;
- provenance verification for the exact JWT dependency version, if JWT is
  selected; and
- platform-specific deployment/readiness hooks, if a deployment platform is
  selected.

Unattended work may prepare fixtures, scripts, and evidence capture for these
gates. It may not manufacture their results.

## 4. Deliberate non-decisions

Do not expand the decision sprint to settle these without new pressure:

- streaming protocols;
- advanced module aliases, re-exports, or multi-file namespaces;
- a general raw-representation escape hatch;
- general message localisation and automatic domain display labels beyond the
  bounded date/time-formatting contract;
- live index recommendations;
- broad migration policy beyond the accepted bounded identity work; or
- persistent representation of data-carrying tagged sums.

Keeping these open is a scope decision, not missing planning.

## 5. Definition of decision-ready

A decision artifact is ready for owner acceptance only when it contains:

1. accepted invariants and explicit non-goals;
2. canonical positive and negative source shapes;
3. semantic, effect, failure, and authority rules;
4. runtime state and recovery behaviour;
5. diagnostics and which layer owns each rejection;
6. deterministic evidence, including crash and duplicate cases where relevant;
7. audit and operator surfaces;
8. dependencies, fail-closed behaviour, and stop conditions;
9. an implementation sequence with independently verifiable slices; and
10. a dated approval record and digest before implementation begins.

## 6. Unattended build queue after acceptance

Implementation status: the compiler/runtime cores in steps 1, 2, and 4 are
complete. Step 3 now includes the completed scoped browser/API authentication milestone,
with SQLite/PostgreSQL and restart evidence;
service/JWT and broader principal mappings remain open; steps
5–9 remain gated by their recorded capability contracts, applications, or
external environments. The original dependency order is retained below so the
remaining work cannot bypass those gates.

The owner scheduled
[WASM-EXP1](implementation-history.md#wasm-exp1--bounded-wasm-runtime-experiment)
on 2026-09-29. The owner subsequently clarified that the agreed authentication
scope must finish first, followed by a comprehensive validation phase, with
Wasm after both. The first-party checkpoint's local implementation and evidence
are recorded in the authentication plan; that checkpoint alone does not start
the experiment. Contract decisions can proceed independently.
The experiment retains Bun as the working target and must record its conclusion
before a broader target change; it does not reopen accepted language semantics
or authorise distributed infrastructure work.

For capability implementation, retain the dependency order below. Complete the
agreed authentication scope before the broader validation phase and Wasm;
neither requires waiting for every remaining capability in this list:

1. finish approved authentication work from `AUTH-P1` through `AUTH-P3`;
2. enforce the language-wide naming/qualification contract while implementing
   the bounded time and testing contract;
3. complete authentication adapters and configuration readiness using that
   contract;
4. implement policy semantics, proof output, and golden-todo integration;
5. implement the reminder service boundary and its secret/readiness sinks;
6. implement overdue-reminder jobs/events;
7. finish the golden-todo technical exit evidence;
8. implement the order/payment service and durable workflow from the jointly
   approved pressure and workflow plans; and
9. add a physical derived-store adapter only if the frozen applications require
   it.

Every slice remains subject to the roadmap's unattended triage rule. A build
must fail closed when it reaches an unaccepted decision, and external gates are
never auto-completed.

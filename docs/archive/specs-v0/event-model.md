# Typed events and enforced subscribers

**Status:** candidate design, 2026-10-04; not frozen or implemented. Independent candidate review identified corrections;
see the review record. **Owners:** RM-309 contract, RM-310 compiler, RM-311 runtime, RM-312
successor golden evidence. Syntax review RM-222 precedes adoption. Existing
DATA-007, ASYNC-001, SERVICE-001 and authentication/policy contracts remain the
implemented/frozen baseline until explicit reviewed amendments land.

This document owns the successor event semantics. The
[session context and plan](archive/work-plans/roadmap-assessment.md#syntax-component-messaging-and-graph-intake--2026-10-04)
preserves motivation, decisions, alternatives and scheduling. Its older command/
fact API examples are superseded by the one-event authoring direction here.
There is no claim that planning has removed every implementation risk.

> **Superseded in part (2026-10-12):** ownership now follows domain folders rather than module names, and public actions may change another domain inside the caller's transaction. See the [decision register](decision-register.md#project-layout-domains-and-call-naming--2026-10-12).

## 1. Small authoring model

Use a specialised nominal event type, typed variants, one `emit_event(...)`
operation and automatically discovered subscriber handlers. Keep prefix
`attempt`, exact `fails` and exhaustive outcome handling. Components are logical
effect owners, not processes; a monolith requires no broker or event-sourcing
architecture. Database-backed obligations reuse the existing durable worker.

All examples in this document are **successor sketches**, not compile fixtures.
They demonstrate proposed syntax and semantics without claiming parser support.

```text
event TodoEvent {
    completion_requested {
        todo_id: Todo.id
    }

    completed {
        todo_id: Todo.id
        completed_at: Instant
    }

    reopened {
        todo_id: Todo.id
    }
}

attempt emit_event(TodoEvent.completion_requested {
    todo_id: input.id
})
```

Only an `event`-declared nominal type is eligible. Ordinary records, ordinary
enums, strings, structurally matching data and foreign values are rejected.
Payload construction follows existing closed tagged-enum rules: exact fields,
nominal compatibility and variant narrowing. Runtime validation still applies
at untrusted/storage/version boundaries. Variant access does not grant emitter
authority. `emit_event` is a reserved compiler intrinsic; imports cannot replace
or shadow it. It is an effect primitive, not an interchangeable standard-library
helper; RM-222 must record this distinction against NAME-D06.

```text
subscriber complete_todo_request {
    on TodoEvent.completion_requested(payload) {
        var completed = attempt Todo.complete(payload.todo_id)
    }
}

subscriber notify_todo_change {
    on TodoEvent.completed(payload) {
        attempt notify_completed(payload)
    }

    on TodoEvent.reopened(payload) {
        attempt notify_reopened(payload)
    }
}
```

Handlers are runtime entry points and are not callable values. Duplicate selectors
inside one subscriber are errors. Separate subscribers observing the same variant
each get their own obligation. Multiple handlers mean **either event**, never an
implicit join. The event family is not one application-wide enum; applications
can define small domain families and the compiler generates the full catalogue.
Adding a variant does not force every selective subscriber to consume it, but
does invalidate any ordinary exhaustive match made incomplete by that addition.

## 2. Emission means admission

`attempt emit_event(value)` means that the enclosing operation must handle any
admission failure at its checked boundary. Prefix `attempt` is mandatory for
this intrinsic, like current persistence expressions, even if only operational
admission faults are possible. It never retries a business operation by itself. On successful
transaction commit, the immutable event and all enrolled obligations are durable.
Within a transaction the call stages intent; success becomes externally true
only when that transaction commits. Rolled-back intent is never dispatched.

First-slice recommendation: the expression returns `Unit`; envelope/delivery IDs
are generated and available to inspection without authored receipt plumbing.
No general `submit_command`, `send`, `result(receipt)` or workflow API is added.
If a public asynchronous request needs a pollable operation resource, that is a
separately declared business/API contract, not an inferred return type of every
event. Admission failure may be an operational boundary fault; do not invent
universal business failures or hide the existing uncertainty vocabulary. Exact
`fails` accounts for typed escaping domain failures; operational faults retain
the current generated boundary handling and are not invented domain variants.
Direct `match emit_event(...)` is unsupported in this bounded form. Ordinary
callable outcome matching remains available under its existing rules.

An emitted `completion_requested` records a request, not a completed Todo.
The compiler does not infer completion or receiver cardinality from English
names. Required business outcomes and authorised producers come from contracts.
An emitted variant with no subscriber is visible as an unconsumed-event diagnostic;
it must be an error when a declared completion obligation depends on a receiver.
Otherwise a durable recorded fact may legitimately have no current reaction.

Local synchronous work stays useful. A route admitted to the Todo owner may
invoke its own completion action and return the completed Todo atomically;
independent reactions run through the generated event. Cross-owner side effects
cannot use that local-call path. Read-only named query contracts remain a
separate checked read surface, with freshness and policy; neither importing an
implementation nor emitting a fire-and-forget event substitutes for a query.

## 3. Component and module boundary

Current modules are single-file and duplicate module declarations are rejected
(`MOD_DUPLICATE_MODULE`). Equating a component with one module would break the
owner's cross-file local-action requirement. Keep modules for resolution and
introduce one logical ownership mapping, without duplicating subscriber wiring.

Candidate contract data (the concrete surface below still requires RM-222/RM-309 review):

| Logical owner | Explicit module namespace subtree | Owned authority/effects |
|---|---|---|
| TodoManagement | `todo` | Todo transitions and Todo event contracts |
| Notifications | `notifications` | Reviewed mail capability and its private helpers |
| TodoStatistics | `todo_statistics` | Statistics mutations on received Todo facts |

The mapping uses explicit module names, not filesystem paths. Namespace matching
uses whole dot-separated segments. Overlap, duplicate claims or an effectful
module with no unique owner is a compile error; do not silently pick the longest
prefix. A small application may use a compiler-provided single owner, while
subscriber-only entry rules still apply inside it. Component identity is stable
and the mapping is included in semantic/policy review. There are no authored
subscriber lists or startup registration calls.

Owning actions can call private owning actions in another module/file. Imports
across owners may expose event/data contracts, pure functions and approved query
contracts; they cannot expose mutable implementation authority. Foreign code is
available only through effect-aware reviewed adapters.

## 4. Authority follows the caller

Compute each callable's transitive effects over resolved calls, strongly
connected call groups and runtime effect leaves. An imported helper runs under
its caller's entry context; it cannot lend its own privilege. Admission checks
include the logical owner, entry role, current event/transition contract and
runtime tenant/policy checks. Recursion/resource bounds remain RM-218 concerns.

| Entry / operation | Candidate permitted work |
|---|---|
| Pure function | Pure computations only; no mutable reads, emits, writes or provider calls |
| Named query | Policy-checked reads under its freshness contract; no emits/writes/providers |
| Route/local action in its owner | Authorised owning transitions and reads; event emission where authorised |
| Subscriber handler and its helpers | Its approved transitions/emissions and reviewed service effects, under the incoming event context |
| Transition engine | Required fact construction/publication for the actual committed transition |
| Compiler transport/authentication adapter | Existing narrowly reviewed protocol operations; no source-level general bypass |

Independent reactions can have required trigger contracts even within one owner.
For example, the statistics transition is admitted only from a handler of
`TodoEvent.completed`. Calling its helper from the originating completion action
must fail even if both are in one component. These restrictions must be bound
to the mutation/capability leaf, not inferred from whichever callers happen to
exist. Otherwise adding a caller would silently expand the permitted set.

Prefer reviewed application provider dispatch from subscriber contexts by
default. Synchronous auth/transport adapters keep their narrow existing authority;
moving a mail helper into an auth module does not grant it that authority.
Copying or renaming code cannot change the checked effect leaf. Copying an entire
service or rewriting ownership is a visible new authority/policy change, subject
to the existing approval boundary, not an invisible way to obtain permission.
The compiler enforces declared rules; it cannot infer omitted business intent or
stop an authorised owner from deliberately changing a contract.

### Concrete ownership and trigger candidate

The following spelling is a **review candidate**, not a frozen extension of the
current entity/service grammar. Put restrictions on the protected declaration,
not on a helper that an author could bypass:

```text
component TodoManagement {
    modules: todo
}

entity TodoStatistics {
    triggers: [TodoEvent.completed, TodoEvent.reopened]
    // Fields, persistence and existing entity/field policy remain here.
}

service ReminderMail {
    triggers: [TodoEvent.completed]
    // The pinned service operation, egress and outcome contract remain here.
}
```

`triggers` is an optional declaration clause, containing a nonempty list of
qualified event variants. It restricts **all source-reachable mutation effects**
on that entity, or all dispatch effects on that service. Reads keep their existing
policy/freshness rules. An optional transition/operation-level list can only
narrow the containing list; it cannot reopen a prohibited entry path. Unknown,
duplicate, empty or widening lists fail checking. No wildcard or runtime-produced
selector is supported. A trigger is the runtime handler’s current verified input,
not an arbitrary event value or a previously emitted event in the same action.

Missing `triggers` does not itself add authority: an ordinary entity retains
same-owner mutation admission and normal policy. Application provider dispatch
still requires a subscriber context plus its service/policy contract by default.
Use the entity-wide clause when mutations are independent reactions. The compiler
cannot guess which newly declared entities are reactions from their names; adding
or removing this clause is an explicit effect/approval change. A transition list
on an otherwise ordinary entity protects that transition’s governed fields, but
cannot pretend to protect unrelated writes. Compiler maintenance/authentication
protocols retain their separately reviewed capabilities; they are not arbitrary
source effects exempted by a module name.

An action/helper import, copy or relocation cannot remove the target entity or
service’s trigger restriction. A handler may call a local action from another
file; it carries the same owner/verified input context. Emitting an allowed event
and then making a direct call does not turn a route into its subscriber. Adding
a new subscriber does not bypass emitter, payload-access, effect or resource
policy. The compiler-generated catalogue derives the actual subscriptions from
`on`; the trigger clause is an allowed-effect restriction, not a second list of
handlers to register.

For the first slice each explicit module belongs to one namespace subtree owner.
If any component declarations exist, every effectful module must resolve to one
unique declared owner; there is no implicit catch-all around partial mappings.
With no component declarations, one generated application owner applies. Pure
shared modules acquire no effect authority, even when imported. Existing loader
roles continue to constrain declaration placement; namespace ownership does not
make a route file a legal location for raw entity writes. Current unmoduled legacy
sources require an explicit migration/default-owner disposition, not an inferred
owner based on their filesystem path. No cross-owner atomic-call exception is
introduced in the first slice: regroup the invariant into one logical owner, or
leave that application migration unsupported until a reviewed compatibility
contract exists. The old baseline remains usable while this gate is unresolved.

### Producer and payload policy candidate

Reuse the current role-to-permission policy concept on event variants, with
`invoke` for admitting authored emission and `read` for receiving the payload.
These are proposed event-policy applicability rules, not a modification to frozen
POLICY-001. Application authors do not get a parallel command declaration:

```text
event TodoEvent {
    completion_requested {
        todo_id: Todo.id
        policy {
            scope: todo_id
            TodoRole.owner: [invoke, read]
            ApplicationRole.todo_worker: [read]
        }
    }
}
```

`scope` in an event policy is a checked reference to one non-nullable payload
identity field; it resolves the same authoritative resource/role scope used by
existing entity policy. It does not load the whole row into the payload or grant
row disclosure. The example assumes those roles and their normal membership
bindings exist; it does not create a service role merely by naming it. A variant
without resource-scoped roles may use existing application-scope roles without
`scope`. Mixed/multiple resource scopes require an explicit later composition
contract and are rejected in this bounded form. `Access.authenticated` can admit
requests where appropriate, but cannot grant subsequent resource effects.

Omitted permissions deny. A variant bound to transition publication cannot grant
authored `invoke`: only its checked successful transition produces it. Its `read`
policy is still explicit. Emission from a direct authenticated entry resolves that actor’s current
scope/role and validates the payload. During delegated work it requires both the
current worker’s `invoke` permission and the current original actor’s `invoke`
permission on the downstream variant. Receiving separately validates the worker’s
current role, tenant and approved payload projection, plus the origin obligations
of the delivery contract. The compiler checks
potential access at enrollment; runtime checks current access before
exposing the protected payload to a handler. The trusted decoder may validate
and inspect the minimum scope metadata needed for that check; decoding alone
never grants application access. A field projection cannot silently
drop data that the typed handler requires: use an explicitly permitted narrower
payload contract or reject the subscription. Missing/deleted/revoked scope has a
concealed denial disposition, never an automatic broader role fallback.

### Delivery authority candidate

Default delivery is delegated work: the runtime authenticates the worker under
existing job/service policy and retains the original authorisation origin as
separate, unforgeable evidence. Every protected read/disclosure, mutation and authored emission must satisfy
its current applicable permission for the worker **and** the current original
authority. For emissions, this includes `invoke` on the emitted variant for both.
This conjunction is the unconditional ordinary delegated default, not an optional
per-handler check. A trigger match cannot replace either check. Missing, invalid
or ambiguous origin evidence fails closed. Public/originless entries need an
explicitly qualified protocol; absence of an origin cannot select worker-only
execution. This may deny
a queued request after the user loses access; denial does not mean retrying until
privilege appears. Its reviewed terminal/intervention disposition retains safe
evidence without exposing the payload.

Delegated downstream events preserve the original authorisation origin; they do
not substitute the worker’s broader principal. Emitting an intermediate event or
calling a helper therefore cannot launder resource authority. `current_principal`
continues to denote the authenticated execution principal; the internal origin
record is not a source-constructible principal or another caller-supplied argument.
A single configured worker identity can be reused where policy permits. Missing
or ambiguous worker configuration fails readiness, rather than generating roles
or credentials. Execution keys remain optional.

Some system reactions, such as a maintained statistics projection or reminder,
need authority independent of the original user’s right to mutate that derived
entity. Support them only through a separately reviewed, narrowly scoped existing
service/transition protocol that binds the verified input resource, permitted
output/effect, current eligibility and the exact replacement origin obligation.
Only that named protocol may vary the default conjunction; an omitted clause or
broad service role cannot select it. Do not infer this elevation from `on`,
`triggers`, an entity name or a service role. The first successor slice may reuse
such a qualified protocol; otherwise that reaction stays unsupported pending its
policy contract. No generic `system: true` escape or automatic blanket subscriber
role is proposed. Multi-origin joins and changes of authorisation origin remain
E08 contract work before execution is admitted.

## 5. Publication is part of a transition

Every managed successful state change produces a minimal internal change record.
That is not a public whole-row payload, notification or requirement to use event
sourcing. Named business facts bind to the owned transition contract; application
code does not remember a separate `emit_event` call for them.

Concrete Todo transition candidate (the idempotent no-op behaviour below is a
successor recommendation, not a change to the frozen golden PATCH contract):

| Rule | Complete Todo |
|---|---|
| Authority | TodoManagement, under existing Todo policy |
| Input | Nominal Todo identity and authorised execution context |
| Preconditions | Visible, permitted Todo; status open |
| Change | Set status completed and compiler-supplied completion time atomically |
| Required fact | `TodoEvent.completed { todo_id: after.id, completed_at: after.completed_at }` |
| Already complete | Return unchanged Todo, without a new completion fact |
| Rejected / rolled back | No completion change, fact or consumer obligation commits |

Transition payload expressions may read typed transition inputs and validated
before/after snapshots under disclosure rules. Compiler-owned metadata cannot
be overwritten. Required fields must be provable on every successful path;
nullable before/after states on create/delete must be handled. No arbitrary
user callback constructs a required payload after commit.

All writes to transition-governed fields pass the transition engine, including
another action, a nested call and bulk operations. A raw update capable of
crossing the transition is rejected or lowered through the same guard; it cannot
silently bypass facts. The first implementation should restrict mutation forms
to decidable checked assignments/guards rather than claim to prove arbitrary
programs. Unsupported forms fail with a transition-preserving repair.

The change, its actual transition revision, logical event ID, pinned enrollment
generation and consumer obligations commit in the same authority transaction.
Concurrent completions therefore produce one transition fact, not one fact per
attempt. A compiler-declared transition fact cannot also be manually emitted;
otherwise callers could forge state or double-publish. Other application-emitted
variants need explicit producer authority under policy. Import visibility and
knowing the payload type do not constitute that authority.

Entity actions can maintain invariants atomically across entities inside a
reviewed local ownership/transaction domain. Existing cross-owner atomic
composition requires an explicit DATA-007/TX compatibility disposition: regroup
the invariant into one owner or retain a narrowly reviewed atomic contract.
Never silently weaken atomicity into eventual delivery to enforce the new rule.

### Bounded publication proof candidate

Extend the existing checked lifecycle transition rather than introduce another
callable declaration. This fragment deliberately proposes governing completion
fields in the successor; it does not change today’s ordinary `Todo.status` field:

```text
transition complete {
    from: status == TodoStatus.open
    set: {
        status: Todo.status(TodoStatus.completed)
        completed_at: clock.now
    }
    publish: TodoEvent.completed {
        todo_id: after.id
        completed_at: after.completed_at
    }
}
```

The fragment sits inside the entity’s lifecycle declaration, with its required
field/initial-state/policy definitions supplied there. `before` and `after` are
compiler-owned snapshot bindings available only in the publication expression.
The first slice accepts one statically resolved publication per transition and
one authoritative transition binding per produced variant. Extra/ambiguous
bindings fail; no implicit choice of producer is made.

Payload evaluation is inside the same authority transaction and precedes commit.
Allow checked snapshot fields, closed literals/variants and already validated
nominal constructions; reject database reads, service calls, emission, dynamic
configuration and arbitrary helper execution in payload construction. The
compiler derives `after` from the checked transition assignment. A non-nullable
required payload field needs a static proof from an already non-nullable field,
a non-nullable assignment or an accepted guard refinement. Otherwise reject the
binding; do not insert an unchecked unwrap or wait for a subscriber failure.
Nominal compatibility remains mandatory even when representations match.

Payload construction/validation, obligation enrollment or capacity failure rolls
back the transition. Evaluation after a successful commit is prohibited. A safe
transaction retry recomputes tentative snapshots under the existing retry/clock
contract; only the committed transition revision yields durable fact identity.
No-op classification is based on the declared semantic transition, not merely
whether the SQL adapter reports an affected row. The owning action’s unchanged
case must still pass applicable current policy and disclosure checks.

The initial public-fact binding covers declared update transitions. Generic
create/hard-delete publication or arbitrary conditional callbacks are unsupported
until their own nullable snapshot/authority contract is qualified. The minimal
internal change record still covers supported managed mutations. If an application
declares a required named fact that this bounded form cannot produce, compilation
fails; it does not silently downgrade that requirement to the internal record.

## 6. Payloads, identity and security

Every producer is checked against the variant schema and every handler against
its selected schema. A new consumer needing unavailable data requires a reviewed
schema change or an authorised query. Subscription/enrollment must itself be
authorised for that event and payload projection; permission to emit an event
or execute a handler does not grant permission to receive every payload. Recheck
the contract’s current disclosure/consumption authority before delivery, including
queued snapshots. A denied receiver receives no protected payload; record an
explicit blocked/terminal disposition under the reviewed policy, not success. Do not auto-expand payloads or expose full
rows. A current read and an event-time snapshot have different meanings.

Generated envelope fields include event/variant contract identity and version,
logical event ID, originating operation/revision, causation, aggregate identity,
enrollment generation and origin policy/tenant evidence. Each obligation adds
subscriber/handler contract identity and execution/attempt records. Retain the
producer’s immutable manifest/site reference in the event, and the actual
executing manifest/site reference per attempt. The selected handler contract
version is separate from both. A build-A event processed by compatible build B
keeps A for the emission and B for its processing frames, joined by causal links.
The source
payload cannot forge any of these. A retried delivery preserves the logical
event ID; an actual later transition creates a new event.

Preserve origin principal evidence separately from execution identity. Delivery
checks the current authority and tenant boundary required for the effect; an
old event does not preserve an indefinitely valid permission or reactivate a
disabled user. The exact system-versus-delegated identity follows existing
policy contracts; do not invent a blanket subscriber service role. Revocation,
retention/erasure and secret disclosure need explicit accepted handling. Check
resource-scoped authority within a tenant as well as tenant isolation: an
authorised publisher cannot name another user’s Todo and gain the receiver’s
broader service permissions. Any reviewed system reaction authority must be
explicitly scoped to the verified originating transition/resource contract.

### First system-reaction protocol: completion projection

**2026-10-05 qualification candidate; narrow revocation direction accepted by the
owner, concrete policy grant not yet qualified.** Use one
local completion projection to test the exception promised above. It is a derived
view of committed Todo state, not an authoritative counter, permission source or
new command surface. This is the local statistics receiver in the initial slice;
it does not add completion email to the golden product. Notification/provider
integration remains bound to its separately reviewed reminder contract.

The proposed protocol is identified in review evidence as `completion_projection`.
This is a compiler/approval contract identifier, not a new authored keyword,
callable capability, generic `system: true` switch or second subscription pattern.
The subscriber identifies the reaction; a checked projection declaration and IR
mapping select a compiler-owned replacement effect. Ordinary authored entity
updates to a declared projection remain forbidden, including inside subscribers,
as required by [DATA-007 and the durable projection protocol](entity-query-model.md#92-durable-projection-protocol).
Before implementation, the compiler must check and bind the complete tuple:
logical subscriber/handlers, sealed input variants and producer transitions,
source entity/scope, target projection, exact field mapping, permitted policy
and protocol version. A caller cannot activate it by copying its name or code.
No generic source-supplied protocol registration escape is proposed. Activation
requires a protected approval bound to that exact checked tuple, including the
mapping representation and version; until declaration-to-IR binding and its
enforcement are qualified, this path is unsupported. No Todo-specific name check
or generated-code status grants permission. This reuses CONSISTENCY-001 and
[RM-405–RM-407](archive/implementation-roadmap-2026-10-11.md#e04--qualify-transactions-and-deployment-readiness)
for projection selection, delivery and recovery qualification. It does not activate
their conditional scope or claim their implementation. Logical declaration identity
and the runtime incarnation of an individual source row are separate identities.

| Dimension | Bounded candidate |
|---|---|
| Inputs | Compiler-published `TodoEvent.completed` and `TodoEvent.reopened`; never an authored lookalike or request variant |
| Source | One Todo authority in the same database/transaction domain as the derived rows and delivery markers |
| Target | At most one derived row per observed eligible application/tenant, nominal Todo identity and source incarnation within a projection generation; fields are identity, completion state/time and applied authority revision |
| Field mapping | Completed sets completion state and the validated completion timestamp; reopened sets open and clears completion time to `none`; source identity/scope/revision come from verified metadata; no arbitrary expression or helper can select a different target |
| Worker | Authenticated configured service identity with the exact reviewed projection permissions; missing configuration or revoked worker authority denies execution |
| Allowed writes | Compiler-owned replacement of that exact derived row and delivery/progress records in one local transaction; no authored projection writes, source Todo mutation, role/membership write, network dispatch or arbitrary event emission |
| Source lookup | Minimum authoritative identity/incarnation, scope and lifecycle eligibility; any additional source data needs normal permission and is outside this protocol |
| Disclosure | Querying the projection still uses current source row/field policy and lifecycle scope; hidden rows cannot contribute to exposed lists or aggregates |
| Revocation choice | Owner-selected: an accepted committed fact may finish this narrowly checked maintenance even if its original actor later loses access; no new request/effect gains that exception |

The owner selected this narrow revocation direction on 2026-10-05; see the
[decision record](decision-register.md#completion-projection-after-origin-revocation--2026-10-05).
It varies the default worker-and-origin conjunction only for the
verified maintenance effect and its minimum payload/metadata consumption. Worker
permissions, source scope/visibility, field minimisation and current query
permissions still apply. An original actor’s disable/revocation does not by itself
assert that the underlying Todo is erased or hidden; those remain independent
current lifecycle/retention facts. Until the concrete grant and checked mapping
are qualified, the ordinary conjunction remains active and this protocol is
unsupported. A durable record of prior authorisation is evidence of the accepted
transition, not a general reusable permission token.

The unselected alternative was to retain origin checks for this maintenance too. It requires
an explicit blocked/terminal disposition and a separately authorised rebuild to
restore the projection after revocation; silently treating the missed update as
an applied view is not acceptable. Neither option changes the authority of a
queued user request or an external provider effect.

### Projection consistency, retirement and disclosure

Source mutation/fact/enrollment commit together. The receiver validates the
sealed producer and pinned contract, authenticates its worker, and checks target
scope and source incarnation. Event IDs and matching nominal types alone do not
prove the requested row value equals the permitted row: the target key is derived
from verified input metadata, never a freely chosen action argument.

Apply a relevant source revision only once. The derived mutation, applied revision,
processed marker and acknowledgement-ready state share one transaction and fence.
An already applied duplicate cannot count twice. Earlier relevant revisions cannot
overwrite a later one. Unrelated source edits may create revision gaps; do not wait
for nonexistent completion events or claim the view covers every source field.
Subscriber lanes and predecessor rules remain those in section 7. Multiple
source incarnations cannot share dedup or ordering progress accidentally.

A currently hidden or removed source cannot be newly exposed by a delayed event.
Retire/suppress its derived visibility and record an explicit obsolete/superseded
delivery disposition under the qualified protocol. That disposition is not an
`applied` claim. Compiler-generated projection reads and aggregates first bind
application/tenant, nominal source identity, source incarnation and compatible
active projection generation, then recheck source lifecycle/row/field policy
before exposure. Missing or mismatching bindings mean unavailable enrichment.
An ID-only join cannot attach an old derived row to a newly created source
incarnation. Do not require equality with the current source revision: legitimate
eventual reads can lag. These checks close the interval before asynchronous cleanup.
Purge removes the derived association through a checked same-domain cascade or
reviewed maintenance step. Never recreate the source to satisfy a late delivery.
A later source row reusing an old external ID must have a distinct incarnation;
without enforceable incarnation identity, such reuse is unsupported. Restore or
ownership/tenant transfer requires additional publication/migration handling and
is unsupported by this initial mapping.

Reads explicitly retain eventual freshness and expose lag/missing enrichment;
this projection cannot answer an authoritative permission or completion decision.
An authoritative answer comes from the Todo authority. Initial coverage is partial,
starting at an explicit subscriber enrollment/projection generation: only eligible
completion/reopen facts observed since that boundary establish rows. Existing
Todos and new Todos which have never completed may be absent. Absence means
**not observed**, never open, false or zero. Expose coverage separately from
processing lag in checked query/inspection results; a caught-up consumer does not
prove a complete view. Do not claim whole-Todo counts or complete lists from this
slice. A complete-view claim requires a separately authorised consistent snapshot
bootstrap and catch-up barrier over its recorded source boundary; wall-clock
guesses or replaying old enrollment generations alone are insufficient. Bootstrap,
incarnation allocation/non-reuse, purge maintenance and the exact approved mapping
input remain qualification obligations, not authority granted by this proposal.

Rebuild writes go through
the same checked mapping under separately reviewed maintenance authority, not an
ordinary caller bypass. Version/field mapping changes require a reviewed migration
and drain/generation barrier; do not reinterpret pending old events. No arbitrary
retention number or blanket backfill permission is selected here.

Required pressure cases: actor loses permission after source commit; worker loses
permission; completion then reopen; duplicate apply; crash before/after local commit;
source hidden/purged before delivery; same external ID with another incarnation;
forged input/target; out-of-order relevant revisions with unrelated gaps; query
while cleanup is pending; an old derived row joined against a new source incarnation;
target owner/tenant transfer; old/new mapping overlap; absence and coverage before
bootstrap, including existing and never-completed Todos.
Run these on both database adapters after the implementation gate. They are planned
contract cases, not evidence that a new maintenance plane already exists.

## 7. Durable fan-out and defaults

Freeze membership and handler contract versions through an enrollment generation
at source commit. Store obligations atomically or a bounded durable enrollment
reference that provably reconstructs the identical set after a crash. The first
slice should materialise per-subscriber rows for a bounded fan-out. A deploy must
not race commit to drop or double-enrol a subscriber.

Use [ASYNC-001](async-plan.md) for states, claims, database clock authority, fencing,
backoff/jitter, finite cumulative budgets, cancellation and outcome uncertainty.
Add per-consumer progress; do not create another retry engine. A successful peer
is not retried because another peer fails.

Within one database domain, subscriber changes, its processed marker and new
facts commit together. This proves one committed local effect under duplicate
delivery, not globally exactly-once execution. Across databases use receiver
dedup/inbox plus outbox, without a fictitious shared transaction. External effects
retain before-dispatch checkpoints and no blind resend after uncertainty; a
stable provider key alone is insufficient safety evidence.

### Bounded handler recovery

The first slice admits only two checked execution shapes, inferred from the
handler’s transitive control/effect graph rather than another authoring keyword:

- One local atomic unit: all application mutations, output events and processed
  marker commit together. It may durably request another subscriber’s work;
  admission cannot be reported as that external work’s completion.
- One reviewed external-dispatch protocol, with its existing compiler-owned
  intent, possible-dispatch checkpoint, receipt and completion mechanics.
  Required post-dispatch writes are allowed only where that reviewed protocol
  already persists the outcome and resumes completion without resending.

Reject multiple provider calls or independently committed application steps on
one reachable path, including through helpers, branches and loops, until E08
specifies durable step identity, recorded outcomes and resumption. Pure work and
policy-checked reads remain subject to the shape’s reviewed authority rules.
Mutually exclusive branches with at most one admitted dispatch do not imply two
completed effects. Unknown/dynamic effect multiplicity is rejected, not guessed.
A later call’s proved no-effect failure never proves that the whole handler had
no effect. In particular, send A successfully then fail B safely, commit an
application write before dispatch, or dispatch successfully then fail a required
application write cannot trigger an ordinary whole-handler retry. The compiler
must reject these shapes or lower them through an already reviewed resumable
protocol; a processed marker alone cannot make them safe.

### Defaults and ordering

Normal handlers need no execution key, registration, ack, lease or retry loop.
Derive dedup identity from event and enrolled logical handler identity. A handler
contract version is immutable obligation data, not a new dedup opportunity.
Ordering is a separate stable lane: application, tenant and logical subscriber
identity plus typed aggregate identity, or the subscriber’s one serial fallback
lane. All `on` handlers for that subscriber use the same lane strategy. Infer an
aggregate strategy only when all relevant selectors have a compatible unambiguous
originating aggregate; otherwise use the serial fallback for the entire subscriber.
Do not choose a different fallback per event and accidentally split predecessors.

Handler names/versions, worker builds and enrollment generations cannot partition
that lane. Commit-time predecessor order and unresolved/uncertain blocking survive
rolling workers and explicit renames. A changed lane strategy/key needs a checked
drain or mapping barrier; a new key cannot bypass an old uncertain predecessor.
The existing runtime’s `job + ordering_key` partition must map to this logical
subscriber lane rather than to each handler/version, while retaining the selected
handler separately for dispatch. Different subscribers progress independently.
A declared join still needs a business correlation key, not a delivery ID.
Preserve ASYNC-001 predecessor dispositions; serial fallback intentionally trades
concurrency for safe default ordering, with pressure measured before overrides.

Numeric budgets must come from reviewed policy profiles and representative
evidence. Do not invent a universal timeout/lease or copy one provider's limits
onto every handler. Never default to unlimited attempts or silently multiply
nested budgets. Capacity/fan-out/queue-byte limits are checked before admission
promises durable work; telemetry limits cannot drop delivery obligations.

## 8. Multi-event coordination

One subscriber authoring model may acquire persisted state when coordinating
events. No separate workflow DSL is introduced. Simple multi-handler subscribers
remain stateless; OR does not imply ALL. RM-802 owns the definitive coordinated
state contract and RM-803/RM-804 its implementation.

Order pressure trace (no money/rounding semantics are invented here):

| Input / state | Required behaviour |
|---|---|
| New authorised order request | Persist correlation/order identity, required inventory/payment outcomes and deadline before effects |
| Inventory reserved only | Record one input and remain pending |
| Same reservation arrives twice | Deduplicate; it does not satisfy the payment input |
| Payment accepted after reservation | Complete once only if all required business conditions still hold |
| Payment may have happened but receipt is lost | Persist uncertain outcome; no blind charge retry or successful order claim |
| Cancellation/deadline followed by late payment success | Follow reviewed reconciliation/compensation; never discard evidence or label cancellation rollback |
| Compensation fails | Persist unresolved compensation and expose bounded intervention |

Correlated input updates, completion marker and emitted output facts share the
local transaction where possible. Optional observers do not decide order success.
Loop/fan-out budgets stop runaway event chains with an inspectable disposition;
a static graph cannot prove termination for arbitrary business data.

### Coordination qualification without another DSL

Use the existing entity/action/transaction model for persisted coordination state,
with `on` handlers as its event entries. There is no parallel workflow block to
learn. This is an RM-802 semantic pressure model, not a claim that current bounded
lifecycle assignments or source grammar already express every transition below.
The compiler must reject unsupported coordination until that contract is qualified.

Persist a record per stable subscriber, business correlation and generation.
Each required input slot retains event/producer identity, typed result, contract
version and provenance; two copies of one input fill only one slot. Correlation
comes from a checked business identity such as `Order.id`, not delivery ID,
subscriber execution key, matching field spelling or mere co-occurrence. Infer
only a proven single originating aggregate; otherwise an explicit correlation
binding is necessary. Do not guess a join key from shared customer identity.

| State / incoming evidence | Required local transaction result |
|---|---|
| First permitted order request | Record immutable command origin, correlation, generation, required inputs and deadline; stage only authorised request events |
| One valid inventory/payment result | Check producer and correlation against the outstanding intent, store the input once; remain waiting if another is absent |
| All required positive inputs | Guard current state and policy, commit completion once with its output fact; concurrent final inputs cannot both complete |
| Known refusal before completion | Persist a business failure and any explicitly required release/refund obligation; failure does not erase already successful steps |
| Unknown external outcome | Preserve uncertainty and block the affected effect lane; lack of a response cannot be converted to a negative input |
| Deadline/cancellation | Commit a conditional state change; do not erase outstanding effects or claim rollback |
| Late positive result | Record reality, then follow the declared late-result reconciliation/compensation rule; do not silently discard or reopen a completed generation |
| Failed compensation | Persist unresolved obligation and bounded intervention; do not report the overall operation as safely undone |

Every handler remains one local atomic unit or one qualified dispatch protocol.
A sequence of arbitrary calls is not converted into durable steps merely because
it sits inside a subscriber. Persisted state/processed input/output enrollment
commit together; provider I/O is driven by a separate durable obligation. Timers
are durable obligations linked to the same state generation, using existing clock
and claim authority. A timeout and a final input race through one conditional
state transition; retries cannot reset the operation’s deadline or budget.

Separate evidence provenance from business authority at joins. A provider receipt
proves an outcome only after its adapter verifies and binds it to the recorded
outstanding intent/resource/generation. The provider’s principal cannot replace
the command origin and grant access to its order; conversely, loss of the command
origin’s permission cannot justify denying that a recorded external effect really
occurred. Minimal outcome recording/reconciliation requires its own narrow
qualified protocol. Initiating further business effects keeps current delegated
permissions unless a named accepted contract specifies a different obligation.
Do not simply select the most privileged input actor or union permissions from
all contributors. Fan-in telemetry keeps every causal input reference.

A cancelled/expired correlation cannot be reused as a fresh operation by changing
an event ID. A genuine new business operation needs a new generation and explicit
admission; retries and late receipts retain the old one. Upgrade preflight must
identify the exact interpreter/handler for open records or drain/migrate them
with evidence. Numeric deadlines, compensation policy and operator grants must
come from the accepted order/payment pressure application, not universal defaults.
RM-801/RM-802 remain open for that business contract; RM-803/RM-804 own executable
restart, late-result, compensation and intervention evidence.

## 9. Upgrades, removal, replay and retention

New subscribers default to future enrollment generations. Backfill is explicit.
Removal drains old obligations, retains an executable compatible handler or
records a privileged disposition; deleting source is not an acknowledgement.
Deploy preflight checks pending contract versions and handler availability. Keep
compatible decoders/handlers or drain before cutover; a saved source map alone
cannot execute an old handler. Admission/worker fencing must cover rolling builds.

Projection reconstruction is distinct from replaying external side effects.
Replay preserves original intent evidence, scope and remaining budget, and
requires existing operator authority. Dedup retention must cover the supported
retry/replay horizon; no automatic deletion until a reviewed policy exists.
Preserve explicit migration decisions when a variant/payload changes. Compiler
closed-world checking cannot certify unknown external consumers or queued old
versions without their manifests.

Process/host crashes are covered only when the authority store survives under
its declared durability guarantees. Permanent disk/store loss needs backup or
replication/recovery policy; a monolith outbox does not manufacture that guarantee.

## 10. Generated graph and catalogue

Generate event families/variants, schemas, authorised emitters, transitions,
subscribers, transitive effects, owners and required outcomes from checked IR.
No second authored registry or connection diagram exists. Include documentation
and rationale references plus their provenance; comments do not grant authority.

Static edges distinguish emit, automatic transition publication, subscription,
read, local call and coordination input. Runtime edges distinguish actual causal
delivery/attempts and all fan-in contributors. Consume the
[identity/artifact successor](generated-artifacts.md#semantic-identity-and-incident-artifact-candidate--2026-10-04)
and [E11 graph plan](archive/work-plans/developer-console-mcp.md#hierarchical-application-graph-plan--2026-10-04).
Logs inherit compiler/runtime context, preserve disclosure controls and remain
useful without a retained graph. Stable logical identity never implies unchanged
behaviour after a fix.

## 11. Acceptance matrix before implementation closure

These are planned tests, not executed evidence. Runtime cases need separate
process/connection crash injection and both supported database adapters.

| ID | Evidence required |
|---|---|
| EV-01 | Event variants construct/typecheck; ordinary enum/record emission and wrong payloads fail |
| EV-02 | Multi-event OR handlers are checked independently; duplicate selector and direct handler calls fail |
| EV-03 | Missing/ambiguous ownership and cross-owner action imports/calls fail; approved pure/query contracts and local cross-file helpers pass |
| EV-04 | Direct, wrapped, relocated and copied reaction effects fail in disallowed entry contexts, including one-component cases |
| EV-05 | Every supported mutation path publishes the required fact; forbidden raw/bulk path fails before execution |
| EV-06 | No-op/rejection/rollback/concurrent completion produce exactly the contract's committed changes/facts |
| EV-07 | Crash after source commit before dispatch preserves exact consumer enrollment and payload versions |
| EV-08 | Crash after receiver commit before ack cannot duplicate local mutation, marker or output fact |
| EV-09 | One peer succeeds and another fails; only the failed peer is retried |
| EV-10 | Stale claims/fences, failed predecessors, capacity exhaustion and cumulative nested budgets preserve ASYNC rules |
| EV-11 | Possible external dispatch then lost outcome stays uncertain without automatic resend |
| EV-12 | Subscriber add/remove, rolling enrollment, old payload and missing compatible handler cannot silently lose or reinterpret work |
| EV-13 | Revoked actor, forged/foreign-tenant payload, same-tenant foreign resource, unauthorised emitter/subscription and denied queued-payload consumption cannot gain authority or expose data |
| EV-14 | Order join duplicates, late inputs, deadlines and fallible compensation satisfy RM-802 traces |
| EV-15 | Build-A emission processed by build B, then two branch fixes, preserves each endpoint’s exact artifact and agrees in UI/MCP/log exports |
| EV-16 | Two dispatches, separately committed writes before/after dispatch and helper-hidden mixed effects are rejected absent a reviewed resumable protocol; first-effect-success/second-no-effect cannot justify whole-handler retry |
| EV-17 | Completed/reopened handlers across versions and overlapping workers retain subscriber-lane order; uncertain predecessors block, explicit rename preserves lane, key migration requires a barrier, and unrelated subscribers progress |
| EV-18 | `payload` bindings succeed; declaration keywords including `event` fail as bindings/references with no contextual exception |
| EV-19 | Entity/service trigger restrictions survive raw supported mutation paths, helper/copy/relocation and emit-then-direct-call attempts; narrowing succeeds, widening/empty/duplicate lists fail |
| EV-20 | Missing event permissions, forged scope, same-tenant foreign resource, denied payload read and intermediate-event origin laundering fail; origin-allowed/worker-denied and worker-allowed/origin-denied paths (including downstream emission) both fail; missing/ambiguous origin cannot select worker-only execution |
| EV-21 | No components uses the single owner; partial/overlapping mappings fail; loader roles persist and cross-owner atomic composition cannot silently become eventual work |
| EV-22 | Publication is typed against checked snapshots before commit; nullable/nominal mismatch, effectful payload, duplicate producer and unsupported required-fact binding fail; enrollment/payload failure rolls back mutation |
| EV-23 | `attempt emit_event(...)` is a `Unit` effect statement; bare emission, direct intrinsic outcome matching and discarded non-`Unit` receipts fail; ordinary callable matching/exact failures retain existing rules |
| EV-24 | Qualified compiler-owned projection applies committed facts under the selected revocation rule; ordinary authored writes and unapproved name-only activation fail; revoked worker/forged target is denied; retired sources remain hidden; incarnation/generation read bindings reject an old derived row against a new source with the same external ID; changes cannot reuse old progress; missing/unbootstrapped rows mean not observed and cannot imply complete counts |
| EV-25 | Join slots/correlation/generation prevent duplicates and cross-order inputs; final-input/timeout races settle once; late outcomes persist without privilege union or blind provider resend |

## 12. Review and remaining decisions

Independent candidate review recommended adaptation; the saved review record
tracks corrections and re-review separately. Final public-language, policy and
transaction/effect qualification remain pending; self-review is not a substitute. Before freeze, settle the component
mapping, trigger and producer-policy candidates above against their owning
contracts; finish transition payload proof rules and E08 coordination format.
Cross-owner atomic migration is explicitly unsupported in the first slice.
RM-222 owns query/constructor/principal grammar coherence. This candidate chooses
minimal emission (`Unit`, no receipt API), local synchronous owner work and
explicit read contracts as recommendations to pressure-test, not hidden accepted
language amendments. Detailed plans must account for IR/storage/tooling changes.

Concrete first slice: one synchronous Todo completion, automatic completed fact,
one local statistics subscriber and one reviewed notification subscriber. Then
exercise explicit typed request emission through a receiver without claiming its
admission is business completion. Keep the original golden baseline and compare
the separately versioned successor. No implementation starts while the other
delivery session and its RM-110 gate remain unfinished.

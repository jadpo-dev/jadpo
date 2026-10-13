# Entity, query, transaction, and consistency model

**Status:** bounded compiler/runtime foundation implemented; physical derived-store adapters and durable-workflow execution pending
**Decisions:** DATA-007, TX-001, CONSISTENCY-001, WORKFLOW-001
**Accepted:** 2026-09-26
**Related:** [semantic model](semantic-model.md),
[project structure](project-structure.md),
[POLICY-001 decision plan](policy-plan.md),
[compiler/runtime architecture](compiler-runtime.md), and
[implementation roadmap](roadmap.md)

This document fixes the semantic boundary between identity-bearing entities,
ordinary values, persistence, queries, entity-owned operations, multi-entity
workflows, and multi-store consistency. The implemented `type` plus top-level
`persist` compiler path remains comparison and compatibility evidence until
this design has fixtures and executable support; it is no longer the intended
final source model.

## 1. Design rule

The accepted model is deliberately asymmetric:

- entities own identity, invariants, lifecycle, policy, and writes;
- named queries own reads and state their freshness requirement;
- application actions own multi-entity orchestration and consistency intent;
- every mutable fact has one declared authority, while caches, indexes, and
  read models are explicitly derived representations; and
- persistence is an optional explicit entity capability, not the definition of
  entityhood.

This takes operation locality and dot-call discoverability from active-record
systems without accepting hidden loading, mutation, saving, transactions, or
dynamic dispatch. It takes write ownership from aggregate-oriented design
without introducing authored repository objects. It uses a bounded read-model
split without requiring event sourcing or separate CQRS infrastructure. It
also removes hand-written dual writes: the compiler coordinates declared
representations without pretending that unrelated stores share an ACID
transaction.

## 2. Entity and value semantics

An `entity` is a nominal domain subject with stable identity. It may be
persistent, externally backed, graph-backed, cached, request-scoped, or not
stored at all. Entityhood intrinsically supplies only:

- a declared stable identity;
- nominal equality and references based on that identity;
- a stable semantic target for policy, lifecycle, capabilities, operations,
  audit, and tooling; and
- an authoritative source home.

Entityhood does not automatically supply persistence, CRUD, routes,
authentication, caching, graph participation, mutability, methods, public
serialization, or a distributed transaction.

An ordinary object `type` is complete value data without enduring identity.
Equal values are interchangeable. Snapshots, projections, request/response
shapes, money, addresses, ranges, and summaries remain values even when they
describe or contain entity data. Generated storage records are target artifacts
and are not authored entity types.

A complete entity value is an immutable validated snapshot. It is not an ORM
proxy or live handle. Field access never performs I/O, and an operation never
mutates an existing binding invisibly.

## 3. Authoritative source home

Every entity has exactly one authoritative declaration under `entities/`:

```text
entities/
  customer.jadpo
  account.jadpo
  authenticated_actor.jadpo
```

The initial language has no partial entities, extension methods, inheritance,
overrides, or declarations that add fields or operations to an entity from
another file. A file contains exactly one authoritative entity; small private
supporting value declarations may be permitted when they do not create another
identity-bearing concept.

The path is validated project organisation, not semantic or schema identity.
Moving or renaming the file does not replace the entity's compiler-owned stable
identity. If an entity eventually outgrows one file, a future entity-directory
form may add compiler-recognised companion files without permitting a second
entity declaration; that extension is not part of the initial grammar.

## 4. Entity capabilities and illustrative source shape

The entity file is a structured dossier. Fields and identity are followed by
explicit optional capability and operation sections. The exact punctuation
remains fixture-first grammar work, but the semantic shape is fixed:

```text
entity Customer {
    id: CustomerId
    email: Email
    status: CustomerStatus

    identity: id

    persistence {
        store: primary
        role: authority
        unique: email
    }

    cache hot {
        store: redis
        from: primary
        strategy: invalidate
        delivery: durable
    }

    projection relationships {
        store: graph
        from: primary
        delivery: durable
    }

    policy {
        // Qualified scoped roles mapped to create/read/update/delete.
    }

    lifecycle {
        // Permitted state transitions.
    }

    query by_email(email: Email)
        fails Unavailable
        -> Customer?
    {
        // Declarative, bounded read.
    }

    function display_name(self: value) -> Text {
        // Pure computation over a complete snapshot.
    }

    action change_email(self: ref, email: Email)
        fails CustomerNotFound, EmailAlreadyUsed
        -> Customer
    {
        // Explicit mutation of Customer.
    }
}
```

The cache and projection blocks illustrate accepted semantics, not final
punctuation. Application-wide authentication, route exposure, cross-entity
rules, external effects, secrets, jobs, and deployment behaviour do not become
entity powers.
Entity-specific policy is semantically colocated with the entity, but any
weakening remains subject to the human-approval and semantic-digest protocol.

## 5. References and operation receivers

Every entity exposes a compiler-owned nominal reference type, written
provisionally as `Customer.Ref`. Its representation is derived from the
declared identity and may initially be a single field. The reference contract
does not depend on that representation remaining scalar, so a future compound
identity does not force every action signature to change.

An entity-owned function or action declares one of two receiver requirements:

- `self: ref` requires only the entity reference; or
- `self: value` requires a complete entity snapshot.

The spelling above is provisional; the distinction is accepted.

A complete entity value may satisfy a reference receiver through a pure,
compiler-visible identity projection. A reference may never satisfy a value
receiver through an implicit load. The author must call a named query or action
that loads the entity.

Dot syntax is checked call syntax over the receiver, not dynamic dispatch:

```text
customer.change_email(email)
```

is semantically the qualified operation `Customer.change_email(customer,
email)`. The complete signature, effect, success type, failure set, suspension
state, and receiver requirement remain visible in IDE and agent output.

No operation call implies local mutation, automatic saving, lazy loading,
exceptions, an implicit transaction, detached work, inheritance, or prototype
lookup. A state-changing operation returns its declared result and callers
explicitly rebind when they need the new snapshot:

```text
customer = attempt customer.change_email(email)
```

The initial language has no receiver overloads. Each entity operation name has
one receiver contract. Only the entity value and its declared reference are
instance receivers. Arbitrary fields, including unique alternate keys such as
email addresses, do not acquire entity operations or trigger implicit lookup.
Alternate-key access uses an explicit named query such as
`Customer.by_email(email)`.

An entity-owned `function` is pure and uses a complete value receiver when it
needs entity fields. An entity-owned `action` has the same failure, effect, and
completion semantics as every other action; ownership changes its namespace
and mutation authority, not its runtime model. Static entity actions such as
`Customer.register(input)` have no instance receiver.

A state-changing action normally takes `self: ref`. A `self: value` receiver is
an immutable observation, not permission to overwrite newer state. A mutating
action that accepts one must reload or lock authoritative state inside its
transaction, or use a compiler-checked revision/conditional-write precondition.
The compiler rejects an unguarded stale-snapshot write.

## 6. Query declarations

Database reads are named compiler-understood query declarations rather than
anonymous I/O scattered through routes and workflows. A `query` is a restricted
read-only runtime operation:

- it may suspend and expose normalized operational problems;
- fallibility requires `attempt` or exhaustive outcome matching;
- it may call pure functions and other queries;
- it may not create, update, delete, call an action, invoke an external service,
  emit an event, or access a secret;
- its source entities, selected fields, predicates, ordering, cardinality,
  bounds, policy, result projection, chosen representation, and freshness
  requirement are part of the semantic graph; and
- when called inside a transaction, it uses that transaction's consistent
  context.

This is a restricted read-only form in the existing runtime effect model, not
permission to add target-level `async`, promises, or ambient exceptions.

Entity-centred queries live with the entity, for example
`Customer.by_email(email)`, `Order.for_customer(customer)`, or
`Task.overdue(page)`. The entity that is selected and returned is normally the
owner; referring to another entity does not make that entity own every reverse
lookup.

Reads that genuinely have no single owning entity are named top-level queries
under `queries/` and return complete value projections:

```text
queries/
  customer_account_summary.jadpo
  monthly_revenue.jadpo
```

They cover reports, dashboards, search results, exports, and other
cross-entity read models. This is a source-organisation and semantic boundary,
not a requirement for separate databases or asynchronously maintained views.

Raw query expressions are not permitted in routes, pure functions, ordinary
top-level actions, workflows, jobs, policy declarations, or configuration.
Those constructs call named entity or top-level queries. A future reviewed raw
SQL escape hatch, if any, remains explicitly audited and outside this decision.

A query that can use a derived representation declares the weakest freshness
it permits. The accepted semantic levels are authoritative, read-your-writes,
bounded staleness, and eventual consistency; exact spelling remains fixture
work. The compiler may choose a stronger source, never a weaker one. For
read-your-writes inside one operation graph, it propagates the authority commit
revision automatically and waits, falls back to an equivalent authoritative
plan, or reports a typed availability failure. It never silently returns an
older projection. Carrying that guarantee across a later request requires a
typed revision token at the application boundary; it is not inferred from
process-local state.

Freshness is not authorisation. Policy, lifecycle, ownership, and invariant
decisions default to authoritative state. A derived representation may
participate only when the compiler proves its propagation/revocation contract
satisfies the policy; otherwise candidate identities are revalidated against
authority before data is released or changed. Declaring a query `eventual`
never weakens policy.

## 7. Mutation ownership

Only operations owned by an entity may directly create, update, or delete that
entity. A `Customer` action may mutate `Customer`; it may not directly mutate
`Account`. Cross-entity code calls the owning entity's action instead.

This rule gives the compiler one complete inventory of every mutation path and
prevents a workflow or route from bypassing entity invariants, lifecycle, or
policy. Generated migrations remain compiler-owned schema artifacts rather
than ordinary application mutations.

Entity actions may contain the persistence operations needed to enforce their
own mutation atomically. Common identity loading may receive compiler-owned
support, but field access and method dispatch never load implicitly.

Entity actions write authoritative state. Application code cannot write a
declared cache or projection directly. When an authoritative change feeds
derived representations, the compiler records that change durably at the same
commit point and owns its delivery contract.

## 8. Multi-entity workflows and transactions

An operation coordinating multiple peer entities is an ordinary top-level
application action, normally stored under `workflows/`. It does not acquire raw
query or mutation authority; it composes named queries and entity actions:

```text
action transfer_funds(
    source: Account.Ref,
    destination: Account.Ref,
    amount: Money
)
    fails AccountNotFound, InsufficientFunds
    -> TransferReceipt
{
    var source = attempt source.debit(amount)
    var destination = attempt destination.credit(amount)
    return build_receipt(source, destination, amount)
}
```

Atomic intent is authored; transaction mechanics are compiler-managed. The
illustrative header `consistency: atomic` records the intent without fixing its
final spelling:

```text
action transfer_funds(...)
    consistency: atomic
    fails AccountNotFound, InsufficientFunds
    -> TransferReceipt
{
    // Compose entity actions.
}
```

The transaction contract is:

1. An entity action is independently failure-atomic when invoked without an
   enclosing atomic action. Its successful database changes commit together or
   none do.
2. Merely calling two mutative entity actions does not silently merge their
   transactions. An enclosing action that reaches multiple mutation scopes must
   explicitly choose an atomic boundary or durable-workflow disposition;
   omission is a compile error.
3. An explicitly atomic action owns the transaction. Nested entity actions and
   queries receive and reuse its transaction context and never commit
   independently. A nested success is provisional until the enclosing boundary
   commits.
4. Successful completion commits. A propagated failure rolls back the complete
   declared atomic boundary.
5. Each nested action remains failure-atomic. If its failure is handled locally
   and the caller continues, its partial writes are rolled back to a
   compiler-owned savepoint before recovery proceeds.
6. All participants in an atomic boundary must resolve to one compatible
   transaction domain. The compiler rejects an atomicity claim spanning
   different databases or non-transactional adapters unless every participant
   explicitly supports one compiler-verified prepare/commit and recovery
   protocol.
7. Invariant, lifecycle, and policy reads that guard a write execute in the
   same transaction and locking/conditional-write plan as that write.
8. The compiler analyses the complete effect graph, including database writes,
   projection publication, cache operations, events, jobs, and service calls.
   An irreversible or incompatible effect cannot hide inside an atomic
   boundary.

The compiler derives and validates the transaction domain, connection/context
propagation, commit/rollback, savepoint, and affected-query plan. Authored source
does not manually thread connection or transaction objects. The compiler must
also expose the chosen isolation, locking/conditional-write, deadlock-ordering,
and retry plan in audit output and reject the declaration when it cannot prove
the requested atomicity. Exact default isolation and retry rules remain TX-001
fixture work; an automatically opened transaction must never be presented as a
substitute for a proved concurrency contract. The owner selected default-on safe
transient retries with bounded exponential backoff and independent jitter on
2026-10-01. The compiler/runtime determines replay safety and shares retry budgets;
uncertain commits and non-repeatable effects are never automatically replayed.
Exact adapter classification, defaults and evidence remain RM-401/RM-402 work;
see the [recorded direction](decision-register.md#delivery-defaults-and-planning-directions--2026-10-01).

## 9. Cross-store consistency

The source-level experience is one entity change, even when that entity has a
Postgres authority, a Redis cache, and a Neo4j read model. The guarantee is
selected from four distinct consistency mechanisms:

| Mechanism | Valid participants | Success means | Important cost |
|---|---|---|---|
| Local atomic | One compatible transaction domain | Every write committed or none did | Limited to that domain |
| Prepared atomic | Adapters sharing a compiler-verified prepare/commit and recovery protocol | The distributed decision is durable and will resolve atomically | Lower availability, in-doubt recovery, and adapter support required |
| Durable projection | One authority plus derived stores | Authority and its change record committed; every projection update is replayable | Derived stores may temporarily lag |
| Durable workflow | Multiple independent authorities or external services | Persisted steps will retry, compensate, or reach an explicit terminal intervention state | Compensation is domain work, not rollback |

`transaction` and `atomic` are reserved for the first two rows. Durable
projection, read-your-writes, eventual convergence, and compensation are
different guarantees and remain distinguishable in source, diagnostics, and
audit output.

### 9.1 Authority and derived representations

Every mutable domain fact has exactly one declared authority. A cache, search
index, graph model, analytics table, or denormalised record derived from that
fact is not independently writable application state. If a graph relationship
is authoritative, a relational copy of it is a projection; if Postgres is
authoritative, the graph edge is a projection. The declaration cannot imply
both.

The initial implementation may restrict one entity's mutable fields to one
authoritative persistence domain. A later entity may contain facts with
different authorities, but an action changing more than one such domain is a
durable workflow unless prepared atomicity is supported and explicitly chosen.

### 9.2 Durable projection protocol

For a declared durable cache or projection, the compiler:

1. updates authoritative state and appends a change record in the same local
   transaction;
2. acknowledges success only after both are committed;
3. delivers the change at least once with a stable entity identity, projection
   identity, idempotency key, and monotonic authority revision;
4. enforces idempotent application and per-entity ordering at the derived
   representation;
5. retries recoverable failures and exposes terminal delivery failure to
   operations rather than losing or silently skipping the change; and
6. provides replay, rebuild, watermark, lag, and reconciliation mechanisms so
   a derived store can be proved against its authority.

The durable change record may be implemented as an outbox, change journal, or
an adapter-native equivalent, but that choice is compiler/runtime machinery.
Authored entity actions do not dual-write stores or publish their own sync
events. Cache declarations select a safe generated strategy such as versioned
replacement or invalidation; they are never treated as another authority by
accident.

An action returning success therefore means that the authoritative change and
the obligation to update every durable projection cannot be lost. It does not
mean every derived store has already caught up. Query freshness contracts close
that gap where the caller needs a stronger observation.

### 9.3 Multiple authorities

A payment provider, independently writable graph, separate database owning
different facts, or another service of record is an authority rather than a
projection. Cross-authority work uses a compiler-managed durable workflow with
persisted progress, idempotent steps, retries, timeouts, explicit compensation,
reconciliation, and an `OutcomeUnknown`-style operational state where reality
cannot yet be determined. Compensation is authored domain behaviour and is
never described as rollback.

True cross-domain atomicity remains available only as a later adapter
capability: every participant must prove compatible prepare, commit, durable
coordinator recovery, and failure semantics. The compiler rejects `atomic`
rather than silently degrading it to a saga or best-effort sequence.

## 10. Project roles

The accepted initial source roles are:

```text
app.jadpo
entities/       # one authoritative identity-bearing concept per file
values/         # identity-free domain values and projections
queries/        # named cross-entity read models
workflows/      # multi-entity and application-level actions
routes/         # transport bindings that call operations
jobs/           # scheduled or durable entry points once specified
```

For the current executable layout, put shared `failure` declarations in root
`app.jadpo`; an entity dossier file may also contain its supporting failures.
`values/` accepts value declarations only, so a failure beside a value in that
directory is rejected. Root `app.jadpo` can also hold authored tests and fixtures;
no separate failure or test directory role is required for a small application.


Unused roles need not exist. Directories organise recognised semantic roles and
are validated by the project loader, while explicit declarations, modules, and
stable compiler IDs remain the source of semantic identity.

Routes bind transport and call operations; they do not query or mutate
persistence. Pure functions never access persistence. Jobs eventually call the
same named queries, entity actions, and workflows rather than acquiring a
second data-access model.

## 11. Audit, tooling, and LLM contract

The compiler maintains a complete query and mutation graph independent of
physical source location. For every entity it can report:

- its identity and capabilities;
- every named query that reads it and every selected field;
- every entity action that mutates it;
- every workflow, route, or job that reaches those operations;
- applicable policy, lifecycle, failure, and transaction rules;
- every fact's authority and every dependent representation;
- each projection's commit point, delivery, ordering, idempotency, revision,
  watermark, lag, replay, fallback, and reconciliation contract;
- every query freshness requirement and the plan that satisfies it;
- every multi-authority workflow step, compensation, retry, and intervention
  state;
- predicate, ordering, pagination, and bounded-load shapes;
- static index coverage, duplicate or near-duplicate query shapes, and likely
  repeated-load hazards; and
- affected call sites for a schema, query, capability, or operation change.

Static index evidence cannot determine production selectivity, table size,
write amplification, or workload. Live adapter evidence remains necessary for
operational index tuning.

The language service and agent view aggregate external queries, workflows, and
routes back onto the authoritative entity dossier. One-file locality therefore
does not hide whole-program effects, and whole-program visibility does not
require physically placing every report or workflow inside an entity.

## 12. Explicitly rejected implications

This decision does not accept:

- entityhood defined by database persistence;
- a separate `PersistentCustomer` domain type;
- free-standing authored repository objects;
- lazy-loading entity fields or relationships;
- automatic saving after dot calls;
- arbitrary inline database access throughout the application;
- raw writes from workflows or routes;
- arbitrary unique fields as instance receivers;
- entity inheritance, extension methods, overloading, or dynamic dispatch;
- a transaction per entity method;
- silent transaction widening merely because another mutation becomes reachable;
- hand-written dual writes from one entity action;
- treating a cache or projection as a second writable authority;
- claiming projections are current merely because the authority committed;
- silently weakening `atomic` to best effort, eventual consistency, or a saga;
- unverified distributed atomic transactions across adapters; or
- the claim that compiler index advice replaces live workload evidence.

## 13. Implementation boundary

The bounded compiler foundation now defines and fixtures the entity dossier,
reference/value receivers, entity and top-level query declarations, qualified
and dot calls, mutation ownership, project-role validation, explicit atomic
intent, same-domain enforcement, transaction joining, handled-failure
savepoints, freshness requirements, authority/representation declarations, and
entity/transaction audit output. SQLite runtime evidence proves that authority
writes and per-entity revisioned change records share the local transaction and
that a handled nested failure rolls both back to a compiler-owned savepoint.

The remaining boundary is deliberately not claimed: physical derived-store
delivery, idempotent retry, replay/rebuild, watermarks, reconciliation,
revision-token propagation, and a persisted multi-authority workflow runtime
need adapter and operational-control decisions. Target generation fails closed
for non-primary authority stores and `durable_workflow` execution. The legacy
`type` plus top-level `persist` path remains supported compatibility evidence.

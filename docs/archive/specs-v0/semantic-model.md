# Semantic model

**Status:** conceptual draft  
**Related:** [type system](type-system.md), [failure model](failure-model.md),
[syntax](syntax.md), [assurance model](assurance-model.md),
[naming and qualification](naming-and-qualification.md),
[decision register](decision-register.md)

The semantic model matters more than the parser grammar. It defines the concepts
the compiler can reason about and the guarantees each concept carries. Surface
syntax should remain replaceable until complete example programs demonstrate
that these are the right abstraction boundaries.

**Accepted boundary:** DATA-007 defines an `entity` as an identity-bearing
domain subject independent of persistence. Persistence is an optional explicit
entity capability. Entities own their mutations; named queries own reads; and
top-level actions compose multi-entity workflows and consistency intent. Every
mutable fact has one authority; declared derived representations converge under
compiler-managed delivery and freshness contracts. The complete accepted
contract is the [entity, query, and transaction
model](entity-query-model.md). The current `type` plus separate `persist`
compiler path remains prototype evidence until the accepted model has fixtures
and executable support.

**Accepted time boundary:** `Instant` is the ordinary global timestamp;
`CalendarDate` is a date-only fact; resolved `Time` retains an `Instant` plus a
compiler-generated `Zone` enum value. The compiler-owned `temporal` library,
stable operation clock, monotonic deadlines, lifecycle timestamp ownership,
formatting boundary, database decoding, and deterministic test capabilities are
specified by the digest-pinned [TIME-001/TEST-001 contract](time-testing-plan.md).

**Accepted naming boundary:** every name has one semantic owner and canonical
source form. Authored free callables are unqualified in module/import scope;
entity operations are entity-qualified or receiver-qualified; compiler
standard-library families use mandatory lowercase namespaces. Casing, imports,
constructors, variants, capability access, diagnostics, and alias rejection are
fixed by the digest-pinned [naming and qualification
contract](naming-and-qualification.md).

## 1. Program

An application is a closed, analysable graph of declarations and relationships.
The compiler should be able to answer:

- which routes exist and why;
- what each route accepts and returns;
- who may call it and which records they may access;
- which entities and fields it reads or mutates;
- which external services and secrets it can use;
- which events/jobs it creates;
- which domain and operational failures can occur;
- which human-approved policies apply;
- which tests and generated checks support those claims.

Unknown behaviour should not hide inside framework callbacks or arbitrary code.

## 2. Core concepts

### 2.1 `app`

The application boundary gives declarations a shared identity and build target.
It owns global configuration, generated artifacts, deployment metadata, and the
complete route/effect graph.

The exact module/file relationship is open.

### 2.2 `type`

A type is an executable contract. It may include domain constraints in addition
to structure. One declaration may generate or inform:

- compile-time checks;
- runtime validators;
- Postgres column types and `CHECK` constraints;
- input and output schemas;
- OpenAPI and documentation;
- generated tests.

Typing inside trusted code means the value was actually validated, not merely
annotated.

Application semantic types are nominal rather than structurally compatible. A
shared primitive representation is an implementation fact, not permission to
substitute one domain value for another. Raw text therefore cannot stand in for
an email address, and one text-represented domain type cannot stand in for
another without an explicit, validated conversion.

`Type(value)` is a validated constructor for semantic and field types. A
constant argument is checked during compilation. A dynamic argument is checked
at runtime and introduces a typed validation failure that must be declared,
mapped, or handled. The form never means unchecked casting.

A constrained semantic type describes validated values, not a closed list of
named alternatives. For example, `InviteCode` is an open set of text values
that satisfy its constraints. If the domain has a finite state such as
`active`, `blocked`, `used`, or `expired`, that state belongs to an enum or
another explicit domain model. The language must not blur enum membership with
a distinguished or magic string value. Authored source references enum variants
only with qualified dot syntax such as `InviteStatus.blocked`; call-style forms
such as `InviteStatus("blocked")` or `InviteStatus(raw_text)` are invalid.
External representations enter through boundary decoding or an explicitly
typed parsing operation, not ordinary enum construction.

**Provisional:** an enum may later be a tagged sum whose variants carry
different typed payloads. A `paid` payment outcome can then require receipt and
time data while a `declined` outcome requires a decline reason, without placing
all of those fields into one nullable record. The active variant determines
which fields exist, and exhaustive matching provides the proof needed to use
them. This is intended to make invalid domain states unrepresentable, not to
add enum methods or presentation behaviour. Boundary tagging, payload
validation, persistence, database constraints, compatibility, and migration
semantics must be specified before such enums become implementable.

### 2.3 Boundary input role

An input is an ordinary type used for untrusted data crossing into application
code. Its object shape distinguishes:

- required versus omitted fields;
- values versus explicit `none`;
- field and domain constraints;
- unknown/extra-field behaviour.

Successful validation produces a trusted typed value. Business code should not
repeat the same validation.

### 2.4 Boundary input and output roles

Input and output are uses of ordinary declared types rather than separate kinds
of declaration. Input use derives recursive closed decoding and validation;
output use derives recursive closed validation and serialization against that
exact type.

Returning a richer internal value where a narrower public output is declared
must not accidentally expose extra fields. Either the compiler proves a safe
projection or the serializer rejects the mismatch.

### 2.5 Object types and entities

`type Name = Object { ... }` declares complete structured value data without
enduring identity. Examples include addresses, money breakdowns, date ranges,
provider contracts, request shapes, response shapes, projections, and
snapshots. Equal values are interchangeable; a concrete value is complete and
declared fields are never mysteriously missing.

`entity Name { ... }` declares a nominal identity-bearing domain subject. An
entity may be persistent, externally backed, cached, graph-backed,
request-scoped, or not stored. Entityhood intrinsically supplies stable
identity, nominal equality/reference semantics, an authoritative source home,
and a semantic target for explicit capabilities, policy, lifecycle,
operations, audit, and tooling. It does not automatically supply persistence,
CRUD, routes, authentication, caching, graph participation, or mutation.

A complete entity value is an immutable validated snapshot, not a live ORM
proxy. Field access never performs I/O. Every entity also exposes a
compiler-owned nominal reference derived from its declared identity. A complete
value may project to its reference; a reference never loads a complete value
implicitly.

### 2.6 Persistence

Persistence is an optional explicit capability inside an entity's
authoritative contract. Absence of that capability is the canonical statement
that the entity has no generated database operations. Persistence does not
create the entity's identity; it binds that already-declared identity and state
to a store.

The current executable top-level `persist Type { ... }` form remains prototype
and migration evidence rather than final source authority. The accepted entity
capability initially maps to Postgres and supplies:

- identity, defaults, and storage constraints for fields declared by the type;
- relationships and ownership-relevant references;
- indexes and uniqueness;
- migration implications;
- lifecycle consequences when fields or records change or disappear.

Fields are non-nullable by default. A nullable database column has language type
`T?`, with database `NULL` mapped to `none`.

A fetched entity is complete. Partial field selection produces a projection,
not a partially populated entity pretending to be complete.

Every field on a named object declaration introduces a semantic field type
referenced as `Type.field`, for example `Customer.id` or `Customer.email`. A
field value retains this identity rather than collapsing to its representation
primitive. This lets inputs, values, functions, actions, events, and other
declarations reuse a field contract without duplicating its validation rules.

A field type is a nominal refinement of its declared type. It can widen to that
declared type, but a base value cannot implicitly narrow to the field type and
sibling fields cannot be substituted for one another. Thus both
`Customer.email` and `Supplier.email` can be supplied where shared `Email` is
expected, while only the former satisfies a parameter typed `Customer.email`.

Field-type references carry value semantics and intrinsic validation, including
nullability. Presence or omission remains a property of the containing input or
object shape. References do not carry storage or stateful facts such as
defaults, indexes, uniqueness, referential existence, or authorisation. In
particular, validating a `Customer.id` does not claim that the customer exists,
is current, or is authorised for the actor. Loading it under application policy
produces a complete trusted `Customer`.

Code that wants interoperability across several fields names their shared
semantic type, such as `Email`. Code that names `Customer.email` deliberately
asks for the narrower field identity. This makes compatibility an authored
decision rather than an accident of structural typing.

When a field's declared parent is itself a named record, nested selection uses
that declared record's field identities. For example, if
`Customer.billing_address` refines `Address`, then
`customer.billing_address.postal_code` has type `Address.postal_code`, not a
synthetic `Customer.billing_address.postal_code`. Reusing `Address` deliberately
shares its nested contracts; distinct nested semantics require distinct named
record types. Nullable record fields must be handled before selection.

### 2.7 Projection

A projection is a complete type derived from an explicit subset or transformation
of data. It prevents the ORM-style state in which an object claims to be a
`Customer` while some fields were never selected.

The compiler may infer anonymous projection types or allow them to be named.

### 2.8 `query`

A query is a named compiler-understood read-only runtime operation. It may
suspend and produce normalized operational problems, so a fallible invocation
requires `attempt` or exhaustive outcome matching. It may call pure functions
and other queries, but it cannot create, update, delete, call an action or
service, emit an event, or access a secret. The semantic model knows:

- entity/projection returned;
- cardinality: many, optional one, or required one;
- filters and ownership scope;
- ordering, limits, and pagination;
- whether policy permits the selected rows and fields;
- the selected authority or derived representation and required freshness;
- resource/cost bounds where knowable.

Raw SQL is not an ordinary query. Complex capabilities should first be added as
safe declarative query forms. Any raw escape hatch is explicitly audited.

Entity-centred queries live in the entity's authoritative file. Cross-entity
reports, searches, dashboards, and other projections are named top-level
queries under the recognised query role and return complete value types. Raw
query expressions do not appear in routes, functions, ordinary top-level
actions, workflows, jobs, policy, or configuration; those declarations call
named queries. When invoked inside an action transaction, a query uses the same
transaction context.

A query over derived state declares one of four semantic freshness levels:
authoritative, read-your-writes, bounded staleness, or eventual. Exact syntax
remains fixture work. The compiler may use a stronger representation but may
not silently weaken the declaration. Within one operation graph,
read-your-writes propagates a compiler-owned authority revision; a lagging
projection must be awaited, replaced by an equivalent authoritative plan, or
reported as a typed availability failure. A later request needs a typed
revision token if it requires continuity with an earlier commit.

Freshness never weakens authorisation. Policy, lifecycle, ownership, and
invariant decisions default to authoritative state. A derived representation
may supply them only when its propagation and revocation contract satisfies the
policy proof; otherwise selected identities are revalidated at authority before
data is released or changed.

### 2.9 `action`

An action is a runtime-managed application operation that may perform persistent
or declared external effects. It may also be effect-free when it serves as a
route or stable operation boundary. It owns or makes visible:

- reads and mutations;
- authorisation preconditions;
- transaction boundaries;
- emitted events;
- service operations;
- typed domain failures;
- operational failure policy.

Every database mutation belongs to an action owned by the entity being
mutated. Routes and application workflows cannot issue raw mutations; they call
named entity actions. The compiler must be able to see the complete action
effect graph and can therefore prevent bypass of entity invariants, lifecycle,
and policy.

An entity action is independently failure-atomic when invoked directly. The
compiler manages its connection, commit, rollback, and validated result; source
does not receive a transaction object. Composition does not silently widen that
boundary. When an enclosing action reaches multiple mutation scopes, the author
must explicitly declare atomic intent or choose a durable-workflow disposition.
Omission is a compile error rather than an inferred semantic choice.

Multi-entity operations are ordinary top-level application actions. The
action explicitly declares when those operations must form one atomic boundary.
The compiler then derives and validates the transaction domain and passes one
transaction-scoped persistence capability through nested entity actions and
queries; nested actions never commit independently and their success is
provisional until the outer boundary commits. A propagated failure rolls back
the declared boundary. A nested failure that is handled locally rolls the
callee back to a compiler-owned savepoint before recovery continues. Policy,
lifecycle, and invariant reads guarding a mutation use that same transaction
and concurrency plan. Atomic participants must share one compatible
transaction domain unless every adapter proves one compiler-supported
prepare/commit and durable-recovery protocol. Isolation, locking or
conditional-write strategy, deadlock ordering, and safe retry behaviour are
part of the checked/audited contract rather than implied by the word
`transaction`.

A mutating entity action normally receives an entity reference. A complete
entity value remains an immutable snapshot, not a concurrency token. If a
mutating value receiver is accepted, the action must reload or lock current
authoritative state in its transaction or use a compiler-checked revision or
conditional-write precondition; an unguarded stale-snapshot write is invalid.

Actions may call functions or actions and may suspend internally. Authored
source has no `async`, `await`, promise, or detached-call type; an ordinary
action invocation always completes or produces a declared failure before its
caller continues. The compiler derives target suspension through the action
call graph. Parallel or background work requires a later explicit structured-
concurrency, event, or durable-job boundary.

### 2.10 Authority, projections, and cross-store consistency

Each mutable domain fact has exactly one declared authority. Other copies in a
cache, graph, search index, analytics store, or denormalised table are derived
representations and cannot be directly mutated by application source. The
initial implementation may require all mutable fields of one entity to share
one authority. A later split remains legal only when every fact is unambiguous
and any cross-authority change uses the consistency contract below.

Jadpo distinguishes four mechanisms:

- **local atomic** — one compatible transaction domain commits all writes or
  none;
- **prepared atomic** — every participant implements one compiler-verified
  prepare/commit and recovery protocol, accepting lower availability and
  possible in-doubt recovery;
- **durable projection** — authoritative state and a change record commit in
  one local transaction, after which derived stores converge through ordered,
  at-least-once, idempotent, replayable delivery; and
- **durable workflow** — independently authoritative systems are coordinated
  through persisted progress, idempotent steps, retry, timeout, authored
  compensation, reconciliation, and explicit outcome uncertainty.

`transaction` and `atomic` refer only to the first two. The compiler rejects an
atomic declaration it cannot prove and never substitutes eventual consistency
or compensation. Durable projection success means the authoritative change and
its delivery obligation are committed, not that every projection has already
caught up. Watermarks, lag, rebuild, reconciliation, and query freshness make
that temporary divergence explicit and manageable.

Entity actions express one authoritative domain change. For every declared
durable representation, the runtime writes an outbox/change-journal record (or
adapter-native equivalent) at the authority commit point and generates stable
entity, projection, idempotency, and monotonic revision identities. Application
code neither dual-writes derived stores nor publishes ad hoc synchronisation
events.

An external service or store that owns facts is another authority, not a
projection. Work spanning such authorities is a durable workflow unless every
participant supports prepared atomicity. Compensation is domain behaviour, not
rollback, and unresolved external reality is represented by an
`OutcomeUnknown`-style operational state rather than guessed success or
failure.

### 2.11 Function

A function performs pure, non-suspending computation from its arguments. It may
call functions and may produce declared failures, but it cannot read or mutate
persistence, call services or actions, emit events, access secrets, or read
ambient time or randomness. Generated target scheduling never changes that
source-level contract.

The current design direction prohibits raw representation primitives in normal
application-defined callable signatures. Parameters and results use named
semantic types or field types. Primitives remain the substrate for type
declarations, local computation, operators, and low-level standard-library
facilities; whether authored low-level utilities need an explicit escape hatch
will be tested in the golden applications.

### 2.12 `route`

A route binds an HTTP transport boundary to typed behaviour. It declares or
inherits:

- method and path;
- authentication and authorisation;
- path/query/header/body input;
- action or local behaviour;
- output type and serialisation;
- mapping from domain failures to HTTP responses;
- rate/resource policy;
- generated and authored tests.

Authentication is required by default. Disabling it is an explicit weakening,
written `auth: none`, not the result of an omitted authentication line.

The implemented AUTH-P1a model gives one top-level `application` declaration a
project-wide authentication default naming one provider-independent
`principal`. That principal is closed over exactly one `user` variant and one
`service` variant. The application selects either immediate revocation or
bounded revocation with a positive maximum delay. These facts are stable
semantic graph nodes; they do not yet claim credential validation or runtime
principal construction.

Authentication mechanism is not route business logic. Session cookies,
OIDC/JWT, API keys, service identities, and future strategies terminate at a
compiler-generated boundary that validates and normalises them into one typed
principal context. Routes inherit the requirement for an authenticated principal and
may describe stronger requirements or an explicit public exception; they do
not select a provider. Actions and policy see only stable identity, allowlisted
user information, and authentication strength from this boundary. Qualified
scoped roles are resolved separately through authoritative POLICY-001 bindings,
never credential claims. A route may eventually require a stronger application-defined
authentication capability, but it does not select a provider. When several
strategies are enabled, selection and conflicts are deterministic and
privileges are never combined implicitly.

Path placeholders use `{name}` in the route template and are typed together in
a brace-delimited `path: { name: Type }` group. The template and group must have
exactly the same names with no duplicates. Decoded and validated values are
available as `path.name`, keeping them distinct from query, header, and body
inputs. Authentication headers remain reserved for the generated
authentication boundary.

A route contains exactly one behaviour form. Small one-off behaviour is an
inline brace-delimited `action`, which has the same effect, transaction, and
exhaustive `fails` rules as a named action. Reusable behaviour, a stable domain
command, or behaviour deserving an independently reviewed boundary is a named
action invoked with `run:`. The route keeps the explicit mapping from transport
values to that action's parameters.

### 2.13 `policy`

Policy is a human-owned statement of permitted behaviour. The accepted
[POLICY-001 contract](policy-plan.md) uses qualified scoped roles, authoritative
direct or membership role bindings, and one role-first matrix on each entity.
The compiler derives `create`, `read`, `update`, and `delete` effects from the
checked operation graph and automatically scopes queries and mutations before
data is released or changed. Routine actions do not repeat policy calls or
tenant predicates. The only compiler-owned non-role subjects are the qualified
`Access.public` and `Access.authenticated`; every domain-specific authority uses
an authoritative role binding.

Ordinary fields inherit entity policy. Exceptional field policy may narrow but
never widen it. Input validation, supplied-field tracking, field write
ownership, lifecycle/business rules, database integrity, result decoding,
projection authorisation, and exact output validation remain separate
fail-closed gates. Policy never silently strips input or output fields.

Policy is semantically colocated with the protected entity, field, or
non-entity operation. Implementation is checked against policy; audit is
derived from the complete semantic graph; and a weakening is release-blocked
without a protected approval bound to the exact before/after policy and graph
digests. The agent may propose but may not silently authorise that weakening.

### 2.14 Intent, rule, or decision

The conversation identified a need to preserve why behaviour exists and which
choices required human approval. The semantic concept is accepted; its shape is
open.

Possible uses include:

- natural-language intent linked to a compiler-understood rule;
- durable design decisions and rationale;
- markers that a compiler diagnostic requires a human rather than an agent;
- traceability from policy to implementation and tests.

### 2.15 `event`

An event is an explicit fact emitted by an action. Its schema is typed. The
compiler knows which actions emit it and which handlers consume it.

Delivery, transaction/outbox behaviour, retention, idempotency, ordering, and
replay semantics are open but must be declared or safely defaulted.

### 2.16 `job`

A job is scheduled or background work. It declares schedule/trigger,
concurrency, resource bounds, effects, retries, idempotency, and failures.

Async work must not inherit accidental behaviour from a queue library. The
compiler should require unresolved delivery decisions explicitly.

### 2.17 `service`

A service is a reviewed external contract. It defines permitted operations,
schemas, authentication/secrets, timeouts, retries, idempotency, and provider
error mapping.

OpenAPI may be imported directly. Documentation without a machine-readable spec
may be converted by an LLM once, then reviewed and committed as a contract. From
that point on, use is typed and auditable rather than arbitrary network code.

Provider-specific failures are normalised at this boundary into domain failures
or operational faults.

### 2.18 `test`

A test is either:

- generated from semantics the compiler already knows, such as authentication,
  ownership, malformed IDs, constraints, and response shape; or
- authored to prove business intent that cannot be inferred.

Tests may live beside behaviour to preserve locality, while reusable fixtures
and broader scenarios still need a module-level design.

## 3. Trust model

### 3.1 Untrusted boundaries

Values are untrusted when entering from:

- HTTP input;
- HTTP output implementations before serialization;
- database reads;
- database writes;
- environment and configuration;
- queues and events;
- third-party API responses;
- deserialised caches.

Validation is built into the boundary. A boundary mismatch is a hard contract
failure, not a partially valid application value.

### 3.2 Trusted application values

Once a value has a declared application type, code may rely on that type and its
constraints. This is the invariant:

> Inside trusted application code, typed values are actually valid.

The model should prevent an LLM from wondering whether it needs another Zod
parse or defensive field check.

### 3.3 Absence and shape

The semantic model separates two facts:

- `T?` means a present field may contain a `T` or `none`;
- `optional` means an input/object-shape field may be omitted.

Omission is not stored as `undefined`. This distinction is essential for patch
operations, where omitted means “do not change” and explicit `none` means
“clear”.

## 4. Failure model

### 4.1 Domain failures

Expected business outcomes are named, declared, and exhaustively propagated or
handled: `NotOwner`, `BookingUnavailable`, `PaymentDeclined`, and similar.

They exist at the semantic level without forcing the source to carry a
`Result<T, E>` wrapper through every line.

A fallible expression is acknowledged either with `attempt`, which propagates
its exact failure set, or with an exhaustive outcome `match`. The match has one
`success(value)` arm and one named arm per failure, with no wildcard. Each
failure arm must recover with a compatible successful value, `reject` a new
declared failure, or `propagate` the matched failure. Failure-context binding
remains a separate follow-up decision and is not implied by this executable
core.

Every application failure derives from one standard kind such as `NotFound`,
`Conflict`, or `Rejected`. The standard kind owns default boundary behaviour;
the application failure owns a stable public code and domain meaning. Domain
code does not choose raw HTTP statuses or construct error responses.

Public failure data is an explicit schema and empty by default. Diagnostic
context is separately declared, redacted, and never implicitly serialised to a
client. Expected failures use semantic propagation traces rather than native
stacks by default.

### 4.2 Operational faults

Infrastructure problems—database unavailability, timeouts, provider
misconfiguration, network failures—are classified and handled through runtime
or service policy. Business code does not implement ad hoc retry loops.

Some provider errors become domain failures; others may only become operational
faults. For example, a declined card is business-relevant, while invalid Stripe
credentials indicate a broken integration and should be logged/alerted rather
than shown as an ordinary checkout choice.

Operational faults retain an internal cause chain and low-level stack. Their
external response is generic unless a reviewed boundary contract maps the
condition into a declared domain failure.

### 4.3 Bugs and impossible states

Compiler/runtime defects and violated invariants are internal errors. They map
to enriched diagnostics and 500-class responses, not declared business flow.

Arbitrary generated-target exceptions are caught only at runtime containment
boundaries and reported as invariant breaches. They never become an untyped
application control-flow mechanism.

## 5. Data lifecycle semantics

Deleting an entity or field is not merely a generated migration. The compiler
must discover dependants and require an explicit lifecycle decision:

- cascade;
- retain;
- anonymise;
- reject;
- another deliberately modelled domain action.

The same principle applies when schema changes meet existing data. Migrations
are generated, but unresolved data meaning cannot be guessed.

## 6. Effect graph

The compiler maintains a graph connecting routes, actions, queries, mutations,
events, jobs, services, secrets, entities, fields, and policies. The graph
supports:

- authorisation proofs;
- secret-flow and output checks;
- declared-egress enforcement;
- explicit atomic-intent and transaction-plan validation;
- authority, projection, freshness, delivery, and reconciliation validation;
- durable-workflow step, idempotency, compensation, and outcome analysis;
- lifecycle impact analysis;
- route inventory and documentation;
- audit output;
- generated tests;
- human-review summaries.

The exact proof system is open, but this graph—not generated TypeScript—is the
core value of the compiler.

## 7. Locality and abstraction

The language should keep small behaviour local rather than force it through
controller, service, repository, DTO, mapper, factory, and middleware layers.
A route may contain one inline action and related tests. It is extracted to a
named action only for reuse, a stable domain command, or an independently useful
transaction, policy, or testing boundary. Pure reusable computation is
extracted to a function.

Locality does not introduce indentation-sensitive structure. Actions, handlers,
and every other executable block are delimited by braces; formatting is
canonical but indentation has no semantic meaning.

Abstraction remains possible but should be more intentional than in TypeScript.
LLMs frequently invent layers that increase navigation and context without
adding domain meaning.

## 8. Unresolved semantic boundaries

Complete example programs must determine:

- module and visibility semantics;
- canonical atomic-intent spelling and the initial isolation,
  locking/conditional-write, deadlock-ordering, savepoint, and safe-retry matrix;
- canonical syntax, adapter capability proofs, runtime state, and operational
  controls for the accepted durable-projection and multi-authority-workflow
  semantics;
- advanced query cardinality and absence behaviour beyond the accepted
  optional/required/many core;
- pagination and streaming;
- representation strategies for immutable values at large-data and foreign-
  buffer boundaries; copying, moving, sharing, and copy-on-write must remain
  unobservable, and caller-visible mutation remains absent until separately
  justified;
- relationship and ownership inference;
- policy expressiveness and proof rules;
- event/job delivery guarantees;
- service contract versioning;
- configuration and secret types;
- extension/escape-hatch boundaries;
- concurrency, resource, and execution limits;
- schema/data migration decisions;
- authored intent and decision semantics.

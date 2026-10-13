# Runtime and generated validation rules v0.1

**Status:** candidate normative specification for P10R review  
**Purpose:** name assurance boundaries that are not static policy proofs

These rules complement the [policy/proof kernel](policy-proof-v0.1.md). Passing
one of them must be described as runtime validation, generated enforcement, or
operational evidence—not as a compiler proof.

### BOUNDARY-DECODE

HTTP, authentication, configuration, database, queue, cache, and provider data
remain untrusted until the generated boundary decoder constructs the declared
nominal type. Unknown fields, malformed representations, invalid enum variants,
and failed constraints cannot enter trusted application values.

### OUTPUT-VALIDATE

Every public output is closed-serialized and validated against its declared
schema before bytes are sent. An undeclared field, invalid nominal value, secret
classification, or incomplete value fails closed behind the safe boundary.

### CONFIG-VALIDATE

Local development, generated deployment integration, and runtime startup use
the same environment-bound validator for required values, types, constraints,
and defaults without disclosing values. Local `jadpo config check` reports safe
presence/validity status; production validation is automatic rather than a
manual preflight step. Invalid configuration returns stable binding diagnostics
and a non-zero status.

### STARTUP-BEFORE-LISTEN

Runtime startup repeats actual-value validation and does not bind a public
listener or announce readiness until all required startup-bound configuration
and local initialization have succeeded.

### READINESS-LIVENESS-SEPARATION

Liveness reports process/runtime health without checking external dependencies.
Deployment-plane readiness performs bounded required/advisory dependency checks,
fails safely during outage, and can recover without forcing a restart loop.

### PERSISTENCE-INTEGRITY

Generated parameterised adapters and named database constraints enforce identity,
uniqueness, references, nullability, and declared lifecycle actions. Native
errors are normalised before leaving the adapter; driver text is not public
contract data.

### QUERY-BOUND

Every collection-producing query or traversal has deterministic ordering and a
finite semantic bound. Generated plan metadata records query count, bounds, and
join/batch strategy so N+1 and Cartesian behavior can be tested.

### TRANSACTION-ATOMIC

An entity action invoked directly is failure-atomic. An enclosing action that
reaches multiple mutation scopes declares atomic or durable intent; omission is
invalid. Inside a proved atomic boundary, nested mutations and guarding reads
reuse one compatible transaction context, nested success remains provisional,
and a failed domain/operational/cardinality check commits no partial state.

### PROJECTION-DURABLE

An authoritative mutation and its durable change record commit together.
Derived-store delivery uses stable revision and idempotency identities, preserves
per-entity ordering, retries safely, exposes lag/watermarks, and supports
replay, rebuild, and reconciliation. This proves durable convergence machinery,
not that every projection is current at authority commit time.

### FRESHNESS-POLICY

A cache, graph, search index, or other derived representation never weakens
authorisation, ownership, lifecycle, or invariant decisions. The compiler
proves its freshness and revocation contract satisfies policy or revalidates
candidate identities against authority before releasing or changing data.

### DURABLE-WORKFLOW

An operation spanning independent authorities persists step progress and uses
stable idempotency, bounded retry/timeout, authored compensation,
reconciliation, and explicit outcome/intervention states. It is never labelled
atomic unless every participant proves one supported prepare/commit and durable
recovery protocol.

### FAILURE-NORMALISE

Database, authentication, configuration, and provider failures cross exactly one
generated normalization boundary. Public responses use a declared safe envelope;
raw provider/database bodies, credentials, queries, causes, and stacks stay
internal.

### IDEMPOTENT-DELIVERY

An externally retried operation uses a stable declared idempotency key, bounded
retry/time policy, and replay evidence. This reduces duplicate effects but does
not prove that an external provider honours its contract; that remains residual
risk.

### ARTIFACT-REPRODUCIBLE

Semantic and generated artifacts are byte-stable for identical canonical input,
contain no machine-specific paths or nondeterministic values, and are rebuilt
and digest-checked from a clean release checkout.

### APPROVAL-ATTESTATION

A release gate reconstructs the canonical approval subject and validates an
external protected attestation's reviewer authority, separation, digest scope,
version, expiry, revocation, and complete decision coverage. This validates the
approval mechanism; it does not prove reviewer comprehension or policy wisdom.

## Evidence rule

Each use of a validation rule must name the executable/integration/operational
case that exercises it and the relevant threat-model entry. If the case does not
yet exist, the evidence map must label it `planned`; candidate documentation may
state the intended property but not claim it is implemented.

# Failure model

**Status:** semantic specification v0.1  
**Purpose:** make failure behaviour typed, safe by default, transport-aware, and
consistent across application code and runtime boundaries

The governing principle is:

> Application code states what failed. The compiler and runtime decide how that
> fact is safely represented at each boundary.

Ordinary application code does not throw arbitrary exceptions, construct HTTP
error responses, choose status integers, or decide ad hoc which diagnostic
details reach a client. It uses declared semantic failure types derived from a
small standard catalogue.

## 1. Goals

The failure model should:

- make every expected business failure visible in callable signatures;
- give every public failure a stable machine-readable code;
- map failure kinds consistently to HTTP and non-HTTP boundaries;
- disclose nothing beyond an explicitly public schema;
- preserve rich internal context without leaking it to clients or ordinary
  logs;
- distinguish expected outcomes from infrastructure faults and programming
  defects;
- normalise database and provider-specific errors at their boundaries;
- generate route error schemas, documentation, tests, metrics, and audit data;
- replace noisy stacks for expected outcomes with semantic propagation traces;
- make missing handling and accidental disclosure compile errors.

## 2. Three failure families

### 2.1 Domain failures

Domain failures are expected, named outcomes that application logic may reject,
handle, or propagate:

```text
CustomerNotFound
EmailAlreadyUsed
OrderAlreadyShipped
PaymentDeclined
NotOwner
```

They are declared in source, appear in `fails` sets, and are raised with
`reject`. They do not require `Result<T, E>` wrappers through ordinary code.

Domain failures usually do not capture a physical stack trace. The compiler
already knows their declaration, rejection site, and semantic propagation path.

### 2.2 Operational faults

Operational faults arise from infrastructure or integration behaviour:

- database unavailability;
- provider timeouts;
- invalid deployment configuration;
- queue or cache failures;
- exhausted runtime resources;
- corrupt persisted or cached data.

They are classified and normalised by compiler-known adapters and runtime
policy. Application code cannot throw arbitrary operational faults. A service
contract may deliberately map a provider condition into a declared domain
failure when callers need to reason about it.

Operational faults retain an internal cause chain and low-level stack. Their
client representation is generic unless a reviewed mapping declares otherwise.

### 2.3 Defects and impossible states

Violated compiler invariants, impossible states, runtime bugs, and generated
target defects are internal failures. They are not declared business flow and
cannot be caught merely to continue as if nothing happened.

They produce a generic internal response at a public boundary and a full
internal diagnostic, cause chain, source mapping, and stack trace.

## 3. Standard failure kinds

Every application-defined domain failure derives from exactly one standard
kind. The kind supplies boundary defaults; the application failure supplies
domain meaning.

The initial HTTP catalogue is deliberately opinionated:

| Standard kind | HTTP | Default client meaning | Default runtime treatment |
|---|---:|---|---|
| `InvalidRequest` | 400 | Request could not be decoded | expected, no retry |
| `InvalidValue` | 422 | Well-formed value violates a contract | expected, no retry |
| `Unauthenticated` | 401 | Valid authentication is required | expected/security event |
| `NotPermitted` | 403 | Actor may not perform the operation | expected/security event |
| `NotVisible` | 404 | Resource is unavailable to this actor | expected/concealed denial |
| `NotFound` | 404 | Requested domain resource does not exist | expected, no retry |
| `Conflict` | 409 | Current state conflicts with the request | expected, no retry |
| `PreconditionFailed` | 412 | Declared request precondition failed | expected, no retry |
| `Rejected` | 422 | Domain rule rejected the operation | expected, no retry |
| `RateLimited` | 429 | Caller must wait before retrying | expected, retry metadata |
| `Unavailable` | 503 | Required capability is temporarily unavailable | operational policy |
| `TimedOut` | 504 | Required dependency exceeded its deadline | operational policy |
| `OutcomeUnknown` | no default | The side effect may have committed, so blind retry is unsafe | requires explicit boundary decision |
| `Misconfigured` | 500 | An internal deployment/configuration error occurred | alert, no blind retry |
| `InternalFault` | 500 | An internal error occurred | alert and diagnose |

The owner selected an explicit Bun HTTP mapping for `OutcomeUnknown` on
2026-10-01: 500 with safe code `outcome_unknown`, request ID and a short static
message that the operation may have completed, without an automatic retry or
retry instruction. The generated Bun HTTP boundary now emits this envelope for
a compiler-owned `PersistenceFault` with `unknown` kind. The generated-route
test now commits an actual write and injects lost acknowledgement at the
transaction boundary, checks the durable row and one transaction attempt, and
verifies the route inventory, OpenAPI and audit envelope. This is controlled
boundary injection, not network-phase classification. The
RM-401 phase probe separately
establishes a committed PostgreSQL write whose network acknowledgement was lost.
Provider/job uncertainty and other transports remain unexecuted. This Bun-specific
contract is not a universal mapping. `OutcomeUnknown` requires an effect that may
have committed or been accepted; a failed read with no possible side effect is
`Unavailable` or `TimedOut` according to its cause. The shared semantic manifest
now reports a null HTTP default for `OutcomeUnknown`; the Bun target explicitly
selects 500. The [boundary case matrix](../tests/assurance/operational-boundary-v0.1.json)
has passed independent contract review; runtime integration remains open. See the
[recorded decision](decision-register.md#delivery-defaults-and-planning-directions--2026-10-01).

The Bun target now maps `SQLITE_BUSY`, `SQLITE_LOCKED` and
`ERR_POSTGRES_CONNECTION_CLOSED` to 503 `read_unavailable` when the route invokes
a compiler-checked query-only call graph, with simple name/literal arguments and
no opaque effects. This classification begins after authentication. A
transaction fault, unknown outcome, cardinality failure or unlisted driver cause
cannot enter this path; message text is never a classification input. The safe
response contains only a static message and request ID, with no automatic replay
or retry instruction. Route inventory, failure audit and OpenAPI expose the same
mapping. Native SQLite exclusive-lock and PostgreSQL dropped-read-response tests
exercise the generated boundary; injected negative cases cover misleading text,
SQL defects, cancellation code `57014`, cardinality and unknown outcomes.

This is a bounded supported-read mapping. Action/inline routes and opaque effect
graphs retain conservative containment. A PostgreSQL query-cancelled code alone
does not establish a timeout; deadline-aware `TimedOut` mapping remains pending,
along with provider/job integration and independent runtime review.

The exact catalogue can evolve through golden applications, but applications do
not invent status numbers. A missing semantic kind is a language-design issue,
not permission to write `status: 418` beside a rejection.

Some kinds are normally produced by generated boundaries or runtime adapters.
For example, malformed JSON becomes `InvalidRequest`, failed authentication
becomes `Unauthenticated`, and an invariant violation becomes `InternalFault`.
Applications may define domain-specific failures from the expected kinds.

## 4. Declaring a domain failure

An application failure names its standard kind, stable public code,
optional safe message, public payload schema, and internal diagnostic schema:

```text
failure CustomerNotFound {
    kind: NotFound
    code: "customer_not_found"
    message: "Customer not found."

    internal {
        customer_id: Customer.id
    }
}
```

The public code is deliberately independent of the source type name. It is a
versioned client contract and does not change merely because the declaration is
renamed.

If `message` is omitted, the standard kind supplies a generic safe message.
Messages are static safe text by default. Runtime interpolation of internal
values is not allowed to drift into a public message.

### 4.1 Public details

A failure exposes no application data unless its declaration contains a
`public` schema:

```text
failure QuantityUnavailable {
    kind: Conflict
    code: "quantity_unavailable"
    message: "The requested quantity is unavailable."

    public {
        available: Quantity
    }

    internal {
        product_id: Product.id
        requested: Quantity
    }
}
```

Public fields are part of the API contract. The compiler checks them against
output policy and data classification. Secrets are never public. Private or
sensitive fields require explicit policy permission rather than merely being
listed here.

### 4.2 Internal diagnostic context

The `internal` schema describes structured diagnostic context, not a promise to
log every value in plaintext. The runtime applies redaction, hashing, sampling,
and retention policy according to the value's semantic type and deployment
policy.

Internal data is never copied into the client response, public message, or
headers unless separately declared safe in `public`.

### 4.3 Stable codes

Public failure codes must be unique within an application's public contract
unless an explicitly reviewed compatible alias is required for concealment.
Compatible aliases must have the same public status, message policy, headers,
and detail schema. Codes use a canonical stable format such as lowercase snake
case. Renaming a code is an API compatibility change even if the source
declaration retains the same shape.

Compiler metadata tracks both the source declaration identity and public code.

## 5. Raising and propagating failures

### 5.1 Rejection

Expected domain failures are raised only with `reject`:

```text
action load_customer(id: Customer.id) -> Customer
    fails CustomerNotFound
{
    var customer = attempt query optional Customer {
        where: id == id
    }

    if customer == none {
        reject CustomerNotFound {
            customer_id: id
        }
    }

    return customer
}
```

A rejection supplies one flat object and must populate every required public
and internal field. Extra fields are invalid. The failure declaration owns the
public/internal split, and a field name cannot occur in both schemas. The
compiler attaches the failure declaration, rejection site, enclosing operation,
and source node automatically.

### 5.2 Declared failure sets

Every callable's authored `fails` set must exactly equal its reachable,
unhandled failure set. A callable that may directly or transitively reject a
domain failure must:

- list it in `fails`;
- handle it locally; or
- map it to another declared failure.

An undeclared failure path and a stale `fails` member are compile errors. Every
fallible expression is explicitly prefixed with `attempt`; the compiler derives
the full failure graph rather than trusting handwritten route documentation.

### 5.3 Propagation

A failure can propagate through a caller that declares the same type. No
ambient exception or `Result` wrapper is exposed in ordinary source.

Propagation adds semantic trace frames: the rejection site and each callable
through which the failure travelled. It does not require capturing a native
stack.

### 5.4 Mapping and handling

`attempt` is the executable prefix for acknowledging a fallible expression:

```text
var payment = attempt charge_card(order)
```

The exact handler-arm grammar after `attempt` remains open. Its required
semantics are not open: handling is exhaustive, mapped failures are declared,
causes can be retained internally, and an unhandled declared failure cannot
disappear.

## 6. HTTP boundary behaviour

### 6.1 Automatic mapping

Routes do not manually map individual failures to numeric statuses. A failure's
standard kind provides the status and envelope. A route's possible error
responses are derived from the operations it invokes.

```text
route GET /customers/{id} {
    path: {
        id: Customer.id
    }
    output: CustomerOutput
    run: load_customer(path.id)
}
```

If `load_customer` fails with `CustomerNotFound` of kind `NotFound`, the HTTP adapter
returns 404 automatically and the generated OpenAPI document includes that
response.

### 6.2 Canonical client envelope

The provisional JSON envelope is:

```json
{
  "error": {
    "code": "customer_not_found",
    "message": "Customer not found.",
    "request_id": "req_28a91"
  }
}
```

When public detail exists, it appears in a dedicated `details` object validated
against the failure's public schema. Unknown fields are never serialised.

The envelope never contains:

- a stack trace;
- generated target paths;
- SQL, database names, or constraint names;
- provider response bodies;
- internal diagnostic fields;
- secret or unapproved sensitive values;
- arbitrary exception messages.

### 6.3 Headers

Standard kinds may control safe protocol headers. Examples include
`WWW-Authenticate` for `Unauthenticated` and `Retry-After` for `RateLimited` or
some `Unavailable` responses. Header values must come from typed runtime policy
or explicitly public failure data, not an arbitrary internal exception.

### 6.4 Status overrides

Ordinary route code cannot override a failure's HTTP status. If a transport
needs different behaviour, it should normally select a different semantic kind.
A rare adapter-level override, if eventually needed, must be explicit,
policy-reviewed, and visible in generated audit output.

## 7. Non-HTTP boundaries

Failure kinds are semantic, not aliases for HTTP status codes. Other adapters
map the same failure differently.

### 7.1 Jobs and event handlers

For background work, the runtime maps failure families to completion, retry,
dead-letter, and alert behaviour:

- expected domain failures are non-retryable by default;
- `RateLimited`, `Unavailable`, and `TimedOut` follow declared retry policy only
  after the runtime proves no effect or no commit, a repeatable effect graph,
  stable logical delivery identity, and remaining attempt/deadline budget;
- defects alert and dead-letter; bounded retry additionally requires the same
  certainty/repeatability gate and a separately classified transient cause;
- handlers may explicitly map a domain failure when the event contract requires
  different treatment.

Retryability is never inferred from an English error message. These proof
conditions take precedence over job retry configuration. A defect or timeout
after possible dispatch or commit retains an inspectable uncertain delivery
state; lease expiry and worker restart do not authorise resend. Reconciliation
must establish the outcome before any further effect is attempted.

### 7.2 Service boundaries

External contracts translate provider responses into declared domain failures
or operational faults. Provider status codes and response bodies do not escape
as application failures.

### 7.3 Command-line and internal RPC boundaries

A CLI may map the standard kind to a stable exit category. RPC or GraphQL
adapters may map it to their own canonical envelopes. The semantic failure code
and disclosure schema remain stable even when the transport changes.

## 8. Disclosure and observability channels

Each occurrence produces up to four deliberately different views.

### 8.1 Client view

The client receives only:

- stable public code;
- static safe message or standard-kind message;
- request/correlation ID;
- fields explicitly allowed by the public schema;
- safe standard headers.

The default is no application-specific details.

### 8.2 Structured internal event

Internal telemetry may include:

- failure or fault type and standard kind;
- semantic source node and rejection site;
- route, action, job, service operation, and propagation path;
- request, trace, tenant, and actor correlation under redaction policy;
- declared internal diagnostic fields;
- duration, retry count, and dependency identity;
- normalised cause category.

Structured context is preferable to parsing message strings.

### 8.3 Semantic trace

Every declared failure records a cheap semantic trace:

```text
CustomerNotFound rejected at:
  action load_customer
  customers.<source>:42

propagated through:
  route GET /customers/:id
  customers.<source>:81
```

This is normally more useful for expected failures than a generated JavaScript
stack.

### 8.4 Low-level stack and cause chain

Operational faults and defects retain their low-level cause chain and stack for
authorised diagnostics. Expected domain failures do not capture one by default,
reducing noise and cost.

No low-level stack is ever returned to a public client.

### 8.5 Metrics and alerting

Metrics use stable failure code, kind, operation, and bounded labels. Internal
IDs and unbounded messages are not metric labels.

Expected failures normally contribute to product or security metrics without
paging operators. Operational faults and defects follow severity and alert
policy. Repeated authorisation failures may trigger a security signal without
revealing additional information to the caller.

## 9. Security-sensitive failures

### 9.1 Authentication and permission

`Unauthenticated` means valid identity is absent. `NotPermitted` means the actor
is known but may not perform the operation. The public response remains generic;
the internal policy decision is structured telemetry.

### 9.2 Concealed existence

Returning 403 can reveal that a resource exists. `NotVisible` deliberately maps
an inaccessible or tenant-invisible resource to the same public shape as a
missing resource:

```text
failure CustomerNotVisible {
    kind: NotVisible
    code: "customer_not_found"
    message: "Customer not found."

    internal {
        customer_id: Customer.id
        policy_reason: PolicyReason
    }
}
```

Internally, the runtime retains the actual denial reason. Externally, the caller
cannot distinguish absence from concealment.

The compiler and policy model should select or require this kind for scoped
record lookup rather than relying on every developer to remember anti-enumeration
behaviour.

### 9.3 Validation detail

Generated `InvalidValue` responses may identify safe input paths and stable
constraint codes. They must not echo secret values, reveal database state, or
include arbitrary validator messages.

Whether a particular constraint failed can itself be sensitive. For example,
registration should not necessarily reveal that an email already belongs to an
account. The applicable failure declaration and policy determine disclosure.

## 10. Validation failures

Boundary validation automatically produces standard `InvalidRequest` or
`InvalidValue` failures. Application code does not hand-construct them for
ordinary input decoding.

Dynamic validated construction such as `Email(raw_text)` is fallible. The
containing callable must map the compiler-visible validation failure to a
declared failure, propagate an allowed standard validation failure, or handle
it locally. It cannot convert the failure to `none` unless the program
explicitly models that semantic choice.

Constant construction such as `Email("invalid")` fails compilation and never
becomes a runtime failure.

## 11. Provider and database normalisation

### 11.1 Providers

A reviewed service contract classifies provider outcomes:

```text
service PaymentProvider {
    operation charge(...) -> Charge
        fails CardDeclined, ProviderRateLimited

    timeout 5s
    retry 2
}
```

A declined card may become declared `PaymentDeclined` of kind `Rejected`. A provider
timeout normally becomes an operational `TimedOut` fault or a deliberate
`CheckoutUnavailable` domain failure of kind `Unavailable`. An interrupted
write whose commit state cannot be proved becomes `OutcomeUnknown`, never a
blindly retryable timeout. Invalid credentials are a
misconfiguration fault, never a client-visible “card declined.”

Provider error text, proprietary codes, and response bodies remain internal
unless the reviewed contract explicitly transforms safe fields.

### 11.2 Database errors

Database constraint and driver errors are translated by compiler-known schema
metadata:

- a known uniqueness conflict may become a declared `Conflict` failure;
- a known optimistic concurrency miss may become `PreconditionFailed`;
- database unavailability remains an operational fault;
- corrupt data violating an entity contract becomes an internal fault;
- raw SQLSTATE values and constraint names never reach the client.

The compiler should require an explicit domain mapping when more than one
meaning is plausible.

Mutations may bind distinct compiler-known uniqueness identities and may add
one fallback `conflict:` binding. For example,
`conflict Account.tenant_owner: AccountTenantOwnerTaken` selects one authored
compound constraint, while `conflict: AccountMutationConflict` handles any
remaining normalised constraint. Duplicate and unknown mappings are compile
errors. Generated adapters translate PostgreSQL constraint names and SQLite
unique signatures to the same compiler-owned identity; raw codes, names, and
driver messages remain internal. Foreign-key-specific mapping remains open
until both adapters can identify it without ambiguous SQLite text.

## 12. Compiler invariants

The compiler rejects source when:

- `reject` names an undeclared failure or a string;
- a domain failure does not derive from a standard kind;
- a callable can propagate a failure absent from its `fails` set;
- a failure mapping is non-exhaustive;
- required public or internal context is missing;
- undeclared context is supplied;
- two public failures use the same code incompatibly;
- a public payload contains a secret or violates output policy;
- domain code chooses a raw HTTP status or constructs an error response;
- application code throws an arbitrary exception;
- provider or database errors cross their boundary without normalisation;
- a route's generated failure responses cannot be represented by its public
  contract;
- a retry policy treats an explicitly non-retryable failure as transient
  without a reviewed override.

The runtime provides a final containment boundary: any unclassified thrown
target exception is a compiler/runtime defect, becomes generic
`InternalFault` externally, and is reported internally as an invariant breach.

## 13. Generated artifacts

From the failure graph, the toolchain generates:

- HTTP error schemas and OpenAPI responses;
- route/action/job failure inventories;
- client code enums and discriminated payload types;
- safe serializers and redaction plans;
- metrics and tracing dimensions;
- retry/dead-letter behaviour for background work;
- tests for status, envelope, disclosure, and non-disclosure;
- audit entries showing public and internal fields;
- compatibility warnings when codes or payload schemas change.

The generated audit should make disclosure obvious:

```text
GET /customers/:id
  failures:
    customer_not_found
      kind: NotFound
      http: 404
      public fields: none
      internal fields: customer_id (redacted by policy)
      stack: not captured
```

## 14. Diagnostics

A missing failure declaration should produce a semantic repair task:

```text
UNDECLARED FAILURE

action cancel_order may reject:
  OrderAlreadyShipped

but declares:
  NotOwner

Choose one:
- add OrderAlreadyShipped to `fails`;
- handle it with `attempt`;
- map it to another declared failure.
```

A disclosure violation should identify the exact forbidden flow without
printing the secret value:

```text
PUBLIC FAILURE DATA VIOLATION

PaymentFailed.public.provider_message may contain unreviewed provider data.

Provider response fields are internal by default.
Declare a reviewed safe transformation or remove the public field.
```

Machine-readable diagnostics include stable IDs for the failure declaration,
kind, rejection site, propagation path, boundary, and policy rule.

## 15. Rejected alternatives

- Arbitrary `throw Error(message)` in application code.
- Returning numeric HTTP statuses from domain logic.
- Treating every failure as a generic `Error` with a message string.
- Exposing stack traces or provider/database messages to clients.
- Capturing full stacks for every expected validation or not-found outcome.
- Making domain failures inherit directly from transport-specific HTTP classes.
- Allowing route-by-route status mapping to drift for the same failure type.
- Swallowing a failure into `none` without an explicit semantic choice.
- Using log-message parsing to decide retry, alert, or response behaviour.

## 16. Open questions

The golden applications must resolve:

- exact handler-arm grammar after the accepted `attempt` prefix;
- exact public envelope and validation-detail schema;
- localisation ownership for human-facing messages;
- later standard-kind catalogue additions and whether 422 is used consistently;
- policy and syntax for reviewed status overrides, if any;
- data-classification syntax for public and internal fields;
- redaction, sampling, and retention policy declarations;
- exact retry/dead-letter defaults outside HTTP;
- cause-wrapping syntax when mapping one failure to another;
- compatibility/versioning rules for public failure payloads;
- mappings for streaming responses after headers have been sent;
- adapters for GraphQL, RPC, and CLI environments;
- whether application authors may explicitly create any operational fault;
- whether application authors can invoke an explicit impossible-state construct.

These questions do not reopen arbitrary exceptions, client stack disclosure, or
manual status selection in ordinary domain code.

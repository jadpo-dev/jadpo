---
title: Failures
area: 4
status: in-review
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - failure.context_field_overlap
  - failure.duplicate_code
  - failure.duplicate_context_field
  - failure.duplicate_declaration
  - failure.missing_context_field
  - failure.mutation_conflict_not_conflict
  - failure.patch_empty_not_invalid_value
  - failure.required_mutation_not_not_found
  - failure.required_query_not_not_found
  - failure.stale_declaration
  - failure.undeclared_propagation
  - failure.unknown_context_field
  - syntax.expected_failure_item
  - syntax.failure_code_required
  - syntax.failure_item_colon_required
  - syntax.failure_kind_required
---

# Failures

How a Jadpo program says that something went wrong: declaring failures, the
kinds that decide how each one reaches a client, listing, raising, passing on
and mapping failures, the faults the runtime raises by itself, and what is
logged.

> In review. Code outside the collapsed boxes applies the decisions from areas 1 to 3 and this page's recommendations; each recommendation names its open question. Today's compiled syntax is collapsed beneath each section that changes.

The rule behind all of it, from the failure model: **application code says
what failed; the compiler and runtime decide how that fact is shown at each
boundary.** Code never throws, never picks an HTTP status and never builds an
error response.

| | Declared by | Listed in `fails` | The client sees | The log gets |
|---|---|---|---|---|
| **Domain failure**: an expected outcome such as `TodoNotFound` | you, with `failure` | yes | its code, message and client fields | its code, where it was raised, the path it travelled and its internal fields |
| **Fault**: the database is down, a provider timed out, a write may or may not have committed | the runtime | no (question 5) | a fixed code for that fault | the cause chain |
| **Defect**: overflow, the recursion limit, a compiler bug | the runtime | no | `internal_fault` | the cause and stack |

## Declaring a failure

```jadpo fragment
/// The todo does not exist.
failure TodoNotFound {
    kind: not_found
    message: "Todo not found."
    internal: {
        todo_id: Todo.id
    }
}

failure QuantityUnavailable {
    kind: conflict
    message: "The requested quantity is unavailable."
    client: {
        available: Quantity
    }
    internal: {
        product_id: Product.id
        requested: Quantity
    }
}
```

- `kind:` names one of the kinds below. The kind sets the HTTP status and how
  background work treats the failure; a failure never states a status itself.
- The public code is the name in lower_snake_case: `TodoNotFound` is
  `todo_not_found` (question 1). Write `code: "todo_missing"` only to keep a
  code clients already use after renaming the failure.
- `message:` is fixed, safe text. It cannot include values, and when it is
  left out the kind's generic message is used.
- `client: { }` holds fields sent to the caller in the response's `details`
  (question 2); `internal: { }` holds fields that never leave the server. A
  field name cannot be in both.
- Context fields are ordinary fields: `T?` is allowed, `optional` and
  `default` are not (area 2 limits them to `Input`).
- Where a failure lives was decided in area 1: in the file that raises it, in
  the domain's `types/` once a second file raises it, and `public` when another
  domain sees it.

`failure` stays a keyword rather than `type X = Failure { }`, like `entity`: a
failure carries a kind, a code and disclosure rules, not only a shape.

::: today

Kinds are UpperCamelCase, `code:` is required and repeats the name, and the
two context blocks are `public { }` and `internal { }` without a colon:

```jadpo project=declare
type Quantity = Int {
    min: 1
    max: 1000
}

type ProductId = Uuid {}

failure QuantityUnavailable {
    kind: Conflict
    code: "quantity_unavailable"
    message: "The requested quantity is unavailable."

    public {
        available: Quantity
    }

    internal {
        product_id: ProductId
        requested: Quantity
    }
}
```

:::

## Kinds

Carried from area 2: kinds are lower_snake_case option names. The "raised
by" column is question 3's recommendation; today every kind can be declared
and rejected.

| Kind | HTTP | Meaning | Raised by |
|---|---:|---|---|
| `invalid_request` | 400 | The request could not be read (bad JSON, wrong shape) | runtime |
| `invalid_value` | 422 | A well-formed value breaks a rule | you; the runtime for request input |
| `unauthenticated` | 401 | No valid identity | runtime (area 8) |
| `not_permitted` | 403 | The caller is known but may not do this | you; policy (area 9) |
| `not_visible` | 404 | Hidden from this caller | policy only (question 10) |
| `not_found` | 404 | The record does not exist | you |
| `conflict` | 409 | The current state conflicts with the request | you |
| `precondition_failed` | 412 | An HTTP precondition such as `If-Match` failed | runtime (area 7) |
| `rejected` | 422 | A business rule refused the operation | you |
| `rate_limited` | 429 | Wait before retrying | you; runtime |
| `unavailable` | 503 | A dependency is temporarily unavailable | you; runtime |
| `timed_out` | 504 | A dependency or deadline ran out of time | runtime |
| `outcome_unknown` | 500 | The change may or may not have happened, so retrying blindly is unsafe | runtime |
| `misconfigured` | 500 | Deployment configuration is wrong | runtime |
| `internal_fault` | 500 | A defect | runtime |

```jadpo fragment
failure CheckoutUnavailable {
    kind: unavailable            // fine: a business outcome callers handle
    message: "Checkout is temporarily unavailable."
}

failure Broken {
    kind: internal_fault         // error: only the runtime raises internal_fault (question 3)
}
```

`invalid_value` and `rejected` both use 422, and 400 is kept for requests
that cannot be read at all. An application cannot add a kind or choose a
status; a missing kind is a language change.

## What the client sees

Every failure reaches an HTTP client in one envelope. The failure's client
fields appear under `details`; internal fields, stacks, SQL, provider text and
source locations never appear.

```json
{
  "error": {
    "code": "quantity_unavailable",
    "message": "The requested quantity is unavailable.",
    "request_id": "req_0b1e…",
    "details": {
      "available": 5
    }
  }
}
```

The generated OpenAPI lists every failure a route can reach, under its
status. `build/compatibility/public-failure-codes.json` records each public
code with its status, message and client fields; changing any of them is a
breaking change.

Request input that breaks a rule is reported with the field and the rule it
broke, never the value (question 4):

```json
{
  "error": {
    "code": "invalid_input",
    "message": "Request body value is invalid.",
    "request_id": "req_0b1e…",
    "details": {
      "problems": [
        { "field": "title", "rule": "max_length" },
        { "field": "due_at", "rule": "required" }
      ]
    }
  }
}
```

::: today

The envelope is as above, verified by running a generated route under Bun
1.2.20. Invalid input returns `invalid_input` with no details, and the
OpenAPI document omits the 400 and 422 responses that request validation
produces.

:::

## Saying what can fail: `fails`

```jadpo fragment
function check_title(title: TodoTitle) fails TitleBlank -> TodoTitle {
    if (title == "") {
        reject TitleBlank
    }
    return title
}

action rename(todo_id: Todo.id, title: TodoTitle) fails TodoNotFound, TitleBlank -> TodoView {
    var checked: TodoTitle = attempt check_title(title)
    var todo: Todo = attempt Todo.get(todo_id)
    ...
}
```

`fails` lists exactly the failures that can leave the callable: everything a
`reject` or an `attempt` in its body can produce, less what a `match` handles.
A missing one is `failure.undeclared_propagation`, one that can never happen
is `failure.stale_declaration`, and a repeat is
`failure.duplicate_declaration`. Functions and actions follow the same rule.
`fails` comes before `->` (decided in area 3), and faults are never listed
(question 5).

::: today

The formatter breaks every signature that has `fails` over three lines:

```jadpo project=fails
type Title = Text {
    max_length: 200
}

type Count = Int {
    min: 0
}

failure TitleBlank {
    kind: InvalidValue
    code: "title_blank"
    message: "Enter a title."
}

failure TitleTaken {
    kind: Conflict
    code: "title_taken"
    message: "That title is already in use."
}

function check_title(title: Title)
    fails TitleBlank
    -> Title
{
    if title == Title("") {
        reject TitleBlank
    }
    return title
}

action rename(title: Title, existing: Count)
    fails TitleBlank, TitleTaken
    -> Title
{
    var checked = attempt check_title(title)
    if existing > 0 {
        reject TitleTaken
    }
    return checked
}
```

:::

## Raising a failure: `reject`

```jadpo fragment
action reserve(product: Product, requested: Quantity) fails QuantityUnavailable -> Reservation {
    if (requested > product.available) {
        reject QuantityUnavailable {
            available: product.available
            product_id: product.id
            requested: requested
        }
    }
    ...
}
```

`reject` names a declared failure and supplies every client and internal
field in one block; a failure with no fields is rejected by name alone. A
missing, unknown or repeated field is an error. There is no `reject "text"`
and no `throw` (area 3). `reject` can also be a `match` arm (area 3). Secret
configuration cannot be put in a failure's fields (`configuration.secret_flow`,
area 10), and nor can the signed-in principal (area 8).

::: today

The condition is in brackets because today `requested > available {` is read
as building an object called `available` (an area 3 bug):

```jadpo project=reject
type Quantity = Int {
    min: 1
    max: 1000
}

type ProductId = Uuid {}

failure QuantityUnavailable {
    kind: Conflict
    code: "quantity_unavailable"
    message: "The requested quantity is unavailable."

    public {
        available: Quantity
    }

    internal {
        product_id: ProductId
        requested: Quantity
    }
}

action reserve(product_id: ProductId, requested: Quantity, available: Quantity)
    fails QuantityUnavailable
    -> Quantity
{
    if (requested > available) {
        reject QuantityUnavailable {
            available: available
            product_id: product_id
            requested: requested
        }
    }
    return requested
}
```

:::

## Passing a failure on: `attempt`

Decided in area 3: `attempt` starts a statement, only on a call that can
fail, and passes every failure of that call on to the caller's `fails` list.

```jadpo fragment
action reorder(order_id: Order.id) fails OrderNotFound, QuantityUnavailable -> Reservation {
    var order: Order = attempt Order.get(order_id)          // can fail with OrderNotFound
    return attempt inventory.reserve_for_order(order)       // can fail with QuantityUnavailable
}
```

Decided in area 2: a runtime construction can fail and is written
`attempt Type(value) invalid: SomeFailure`. When the declared ranges prove a
construction cannot break the target's rules, it is not fallible, so it takes
no `attempt` (question 6):

```jadpo fragment
function line_total(price: Pence, quantity: Quantity) -> Pence {
    var total: Pence = Pence(price * quantity)     // both at least 0, Pence has no maximum
    return total
}

function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) invalid: QuantityTooLarge   // can exceed 1000
    return result
}
```

::: today

Construction never needs `attempt`. This compiles, and when `quantity` is
above 500 the route that reaches it answers 500 `internal_fault` (verified
under Bun 1.2.20):

```jadpo project=construction
type Quantity = Int {
    min: 1
    max: 1000
}

function doubled(quantity: Quantity) -> Quantity {
    return Quantity(quantity * 2)
}
```

:::

## Turning one failure into another

A failure can come from a condition the compiler knows about (a read that
finds nothing, a unique rule a write breaks, a construction) or from another
call. Every mapping is written `condition: Failure` in the operation's block
(question 7), and the failure's kind must fit the condition:

```jadpo fragment
// a read that finds nothing (the operation's own syntax is area 6)
var todo: Todo = attempt query required Todo {
    where: Todo.id == todo_id
    missing: TodoNotFound {
        todo_id: todo_id
    }
}

// a runtime construction (area 2)
var quantity: Quantity = attempt Quantity(count) invalid: QuantityOutOfRange

// a write that breaks a unique rule (area 6)
var account: Account = attempt create Account {
    ...
    conflict: {
        Account.handle: HandleTaken
        other: AccountConflict
    }
}

// a failure from a call (area 3)
var receipt: Receipt = match payments.charge(order) {
    success(receipt) -> receipt
    CardDeclined -> reject PaymentDeclined
}
```

| Condition | Required kind |
|---|---|
| `missing:` | `not_found` |
| `conflict:` | `conflict` |
| `empty:` (a patch that changes nothing) | `invalid_value` |
| `invalid:` (a construction) | `invalid_value` |

The failure that was mapped is kept as the internal cause of the new one, so
the log shows both (question 8):

```text
PaymentDeclined rejected at domains/checkout/actions/pay.jadpo:14
  caused by CardDeclined from payments.charge (internal: decline_code)
  propagated through route POST /checkout
```

::: today

Data operations already map with `missing:`, `conflict:` and `empty:`, and
the kind is checked; `invalid:` does not exist yet. Services use
`maps provider.x -> Failure` and outcome matches use `failure F => reject G`.
The mapped failure is not kept anywhere.

```jadpo project=mapping
entity Customer {
    id: Uuid identity
}

failure CustomerNotFound {
    kind: NotFound
    code: "customer_not_found"
    message: "Customer not found."

    internal {
        customer_id: Customer.id
    }
}

action find_customer(id: Customer.id)
    fails CustomerNotFound
    -> Customer
{
    return attempt query required Customer {
        where: id == id
        missing: CustomerNotFound {
            customer_id: id
        }
    }
}
```

:::

## Handling a failure in `match`

Decided in area 3: one `match` handles a call's outcome. Every failure the
call can produce needs an arm, with no `_`; an arm recovers with a value,
maps with `reject` or passes the failure on with `propagate`.

```jadpo fragment
function lookup_or_default(key: LookupCode) fails LookupRejected -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "not found"
        LookupRefused -> reject LookupRejected
    }
}
```

Today's form and its errors (`failure.outcome_*`) are in area 3.

## Faults

Faults are raised by the runtime, never declared, listed in `fails`, rejected
or matched (questions 3 and 5). They go straight to the boundary, which
answers with a fixed code and logs the cause. Code that must react to a
dependency problem gets a domain failure from where the problem arises: an
integration maps provider conditions to declared failures (area 11).

```jadpo fragment
// an integration's mapping (syntax settled in area 11)
failures: {
    provider.invalid_recipient: ReminderRejected
    provider.rate_limited: ReminderDelayed
}

// the action lists domain failures only, never `unavailable` or `outcome_unknown`
action send_reminder(todo: Todo) fails ReminderRejected, ReminderDelayed -> Receipt {
    return attempt mail.send_reminder(todo)
}
```

| Condition | The client sees |
|---|---|
| The request body is not valid JSON | 400 `invalid_request` |
| Request input breaks a rule | 422 `invalid_input` |
| The route's deadline passed | 504 `deadline_exceeded` |
| Storage failed before anything was committed | 503 `transaction_unavailable` or `read_unavailable` |
| A write may have committed | 500 `outcome_unknown` |
| `Int` overflow, the recursion limit, any defect | 500 `internal_fault` |

Runtime faults that come from code are defects (question 9): `Int` overflow
and the recursion limit (area 3) end the operation with `internal_fault`,
roll back its writes, and log which one happened. Division by zero cannot
happen, because area 3 made the divisor's type exclude zero; a failed runtime
construction cannot reach the boundary, because it needs `attempt`.

```json
{
  "event": "operation.failed",
  "classification": "int_overflow",
  "operation": "action:price_line",
  "site": "domains/ordering/actions/price_line.jadpo:4",
  "request_id": "req_0b1e…"
}
```

::: today

Any kind can be declared and rejected, and a standard kind can be listed in
`fails` and rejected by name. This compiles, and the route that reaches
`save` answers 500 `internal_fault` and logs an unhandled fault rather than
503 (verified under Bun 1.2.20). Integrations list faults in `fails` and map
them with `maps provider.authentication -> fault Misconfigured`.

```jadpo project=faults
type Note = Text {}

failure Broken {
    kind: InternalFault
    code: "broken"
}

action save(note: Note)
    fails Unavailable
    -> Note
{
    reject Unavailable
}
```

Verified under Bun 1.2.20: `Int` overflow is silent
(`9007199254740991 * 9007199254740991` returned `8.112963841460666e+31` with
200); dividing by a computed zero answers 500 `internal_fault`; unbounded
recursion never answered within 25 seconds.

:::

## Concealing records

When policy hides a record from a caller, saying "forbidden" would reveal that
it exists. A hidden record fails with the same `not_found` failure the code
already maps, so the client cannot tell hidden from absent; the log records
that policy concealed it (question 10).

```jadpo fragment
// policy (area 9) limits every read of Todo to its owner
var todo: Todo = attempt query required Todo {
    where: Todo.id == todo_id
    missing: TodoNotFound {
        todo_id: todo_id
    }
}
// someone else's todo fails with TodoNotFound, exactly like a missing one;
// the log adds `concealed_by: policy`
```

::: today

`NotVisible` is a kind you can declare, intended to share its code with the
matching not-found failure, but two failures cannot share a code (see
`failure.duplicate_code` below), so the pair cannot be written.

:::

## Logging

Every failure and fault writes one structured event. A domain failure's event
holds its code, kind, where it was raised, the path it travelled and its
internal fields; text-based internal values are logged as a keyed hash so
equal values still match up (question 11). No stack is captured for a domain
failure.

```json
{
  "event": "failure.rejected",
  "code": "quantity_unavailable",
  "kind": "conflict",
  "site": "domains/inventory/actions/reserve.jadpo:6",
  "trace": ["action reserve", "action place_order", "route POST /orders"],
  "request_id": "req_0b1e…",
  "internal": {
    "product_id": "0192f3a4-…",
    "requested": 12
  }
}
```

::: today

Verified under Bun 1.2.20: a rejected domain failure writes nothing to the
log, and its internal fields are discarded. A fault writes one
`operation.failed` event with a classification such as
`RUNTIME_UNHANDLED_FAULT`, no cause, and a stack only when
`JADPO_DEBUG_TARGET_STACKS=1` is set.

:::

## Messages and languages

A failure's `message` is fixed English for developers and logs. Clients
translate by `code`, which is stable (question 12):

```json
{ "error": { "code": "todo_not_found", "message": "Todo not found.", "request_id": "req_0b1e…" } }
```

```json
{ "todo_not_found": "Tâche introuvable." }
```

The second block is a client's own French message table. Today there is no
translation at all, and the OpenAPI document fixes each message as a
constant.

## Errors

> The examples in this section use today's syntax, because they show the compiler's real output; they are rewritten as the rework lands.

Every error this area can produce, with an example that triggers it and the
compiler's actual output.

### `syntax.failure_kind_required`

```jadpo project=e-kind-required expect=syntax.failure_kind_required
failure Missing {
    code: "missing"
}
```

### `syntax.failure_code_required`

```jadpo project=e-code-required expect=syntax.failure_code_required
failure Missing {
    kind: NotFound
}
```

### `syntax.failure_item_colon_required`

```jadpo project=e-item-colon expect=syntax.failure_item_colon_required
failure Missing {
    kind NotFound
    code: "missing"
}
```

### `syntax.expected_failure_item`

A failure cannot choose its own HTTP status:

```jadpo project=e-failure-item expect=syntax.expected_failure_item
failure Missing {
    kind: NotFound
    code: "missing"
    status: 404
}
```

### `failure.duplicate_code`

A concealed and a genuine not-found cannot share a code today:

```jadpo project=e-duplicate-code expect=failure.duplicate_code
failure CustomerNotFound {
    kind: NotFound
    code: "customer_not_found"
    message: "Customer not found."
}

failure CustomerNotVisible {
    kind: NotVisible
    code: "customer_not_found"
    message: "Customer not found."
}
```

### `failure.context_field_overlap`

```jadpo project=e-context-overlap expect=failure.context_field_overlap
type Reference = Text {
    max_length: 64
}

failure PaymentRefused {
    kind: Rejected
    code: "payment_refused"

    public {
        reference: Reference
    }

    internal {
        reference: Reference
    }
}
```

### `failure.missing_context_field`

```jadpo project=e-context-missing expect=failure.missing_context_field
type Lookup = Text {}

failure Missing {
    kind: NotFound
    code: "missing"

    internal {
        lookup: Lookup
    }
}

action find(lookup: Lookup)
    fails Missing
    -> Lookup
{
    reject Missing
}
```

### `failure.unknown_context_field`

```jadpo project=e-context-unknown expect=failure.unknown_context_field
type Lookup = Text {}

failure Missing {
    kind: NotFound
    code: "missing"

    internal {
        lookup: Lookup
    }
}

action find(lookup: Lookup)
    fails Missing
    -> Lookup
{
    reject Missing {
        lookup: lookup
        query: lookup
    }
}
```

### `failure.duplicate_context_field`

```jadpo project=e-context-duplicate expect=failure.duplicate_context_field
type Lookup = Text {}

failure Missing {
    kind: NotFound
    code: "missing"

    internal {
        lookup: Lookup
    }
}

action find(lookup: Lookup)
    fails Missing
    -> Lookup
{
    reject Missing {
        lookup: lookup
        lookup: lookup
    }
}
```

### `failure.undeclared_propagation`

```jadpo project=e-undeclared expect=failure.undeclared_propagation
type Note = Text {}

failure RegistrationClosed {
    kind: Conflict
    code: "registration_closed"
}

action register(note: Note) -> Note {
    reject RegistrationClosed
}
```

### `failure.stale_declaration`

```jadpo project=e-stale expect=failure.stale_declaration
type Note = Text {}

failure Refused {
    kind: Rejected
    code: "refused"
}

action complete(note: Note)
    fails Refused
    -> Note
{
    return note
}
```

### `failure.duplicate_declaration`

```jadpo project=e-duplicate-fails expect=failure.duplicate_declaration
type Note = Text {}

failure Refused {
    kind: Rejected
    code: "refused"
}

action execute(note: Note)
    fails Refused, Refused
    -> Note
{
    reject Refused
}
```

The four errors below check that a mapping names a failure of the right
kind.

### `failure.required_query_not_not_found`

```jadpo project=e-query-kind expect=failure.required_query_not_not_found
entity Customer {
    id: Uuid identity
}

failure LookupRejected {
    kind: Rejected
    code: "lookup_rejected"
}

action find_customer(id: Customer.id)
    fails LookupRejected
    -> Customer
{
    return attempt query required Customer {
        where: id == id
        missing: LookupRejected
    }
}
```

### `failure.required_mutation_not_not_found`

```jadpo project=e-mutation-missing-kind expect=failure.required_mutation_not_not_found
entity Customer {
    id: Uuid identity
}

failure WrongMissing {
    kind: Rejected
    code: "wrong_missing"
}

failure CustomerConflict {
    kind: Conflict
    code: "customer_conflict"
}

action delete_customer(id: Customer.id)
    fails WrongMissing, CustomerConflict
    -> Customer
{
    return attempt delete required Customer {
        where: id == id
        missing: WrongMissing
        conflict: CustomerConflict
    }
}
```

### `failure.mutation_conflict_not_conflict`

```jadpo project=e-mutation-conflict-kind expect=failure.mutation_conflict_not_conflict
entity Customer {
    id: Uuid identity
}

failure CustomerNotFound {
    kind: NotFound
    code: "customer_not_found"
}

failure WrongConflict {
    kind: Rejected
    code: "wrong_conflict"
}

action delete_customer(id: Customer.id)
    fails CustomerNotFound, WrongConflict
    -> Customer
{
    return attempt delete required Customer {
        where: id == id
        missing: CustomerNotFound
        conflict: WrongConflict
    }
}
```

### `failure.patch_empty_not_invalid_value`

```jadpo project=e-patch-empty-kind expect=failure.patch_empty_not_invalid_value
entity Customer {
    id: Uuid identity
    email: Text
}

input PatchCustomer {
    email: Customer.email optional
}

failure WrongEmpty {
    kind: Conflict
    code: "wrong_empty"
}

failure CustomerNotFound {
    kind: NotFound
    code: "customer_not_found"
}

failure CustomerConflict {
    kind: Conflict
    code: "customer_conflict"
}

action patch_customer(id: Customer.id, input: PatchCustomer)
    fails WrongEmpty, CustomerNotFound, CustomerConflict
    -> Customer
{
    return attempt update required Customer {
        where: id == id
        patch: input
        empty: WrongEmpty
        missing: CustomerNotFound
        conflict: CustomerConflict
    }
}
```

## Review

### Status

Each row says how it was checked: compiled with `jadpo check`, run under Bun
1.2.20 through the generated `handleRequest`, or read from generated code.

| Feature | Status |
|---|---|
| Declaring failures | Works: `kind` and `code` are required, members need a colon, unsupported items (`status: 404`) are rejected, and a kind must be a standard kind (`kind: Teapot` is `semantic.wrong_name_kind`; `kind: not_found` is `semantic.unknown_name` with "Did you mean `NotFound`?"). **Gaps (compiled):** `code: ""`, `code: "Not A Snake-Code"` and `message: ""` are accepted; a failure may take a compiler-owned code (`internal_fault`, `invalid_input`); `optional` is accepted in context fields although grammar §9 forbids it. |
| Kinds | All 15 are declarable, including `InternalFault`, `OutcomeUnknown` and `Misconfigured` (compiled). **Bug:** `fails Unavailable` with `reject Unavailable` compiles, and the route answers 500 `internal_fault` and logs an unhandled fault instead of 503 (run). |
| HTTP mapping and envelope | Works: a rejected `InvalidValue` failure answered 422 with `code`, `message`, `request_id` and its public `details`, and no internal fields (run). A failure without `message` gets "Request failed." whatever its kind (read from generated code). |
| OpenAPI and compatibility | Declared failures are listed per status, with `oneOf` when two share one (read from generated output). **Gap:** the 400 `invalid_request` and 422 `invalid_input` responses that every input-taking route can produce are missing. `public-failure-codes.json` records code, status, message and public fields as breaking-change keys. |
| `fails` | Works: the exact set is enforced (`undeclared_propagation`, `stale_declaration`, `duplicate_declaration`), for functions and actions. `-> R fails F` is still accepted (area 3). |
| `reject` and context | Works: missing, unknown, duplicate and overlapping fields are errors; `reject "text"` is a syntax error; secret configuration in either block is `configuration.secret_flow` (compiled). |
| Mapping | `missing:`, `conflict:` and `empty:` check the failure's kind (compiled). No cause is kept for a mapped failure (read from generated code). |
| Runtime construction | No `attempt`; a failed check answered 500 `internal_fault` (run). |
| Overflow | **Bug (run):** silent. `Int` is a JavaScript number, so `9007199254740991 * 9007199254740991` returned `8.112963841460666e+31` and `4294967297²` returned `18446744082299486000`, both with 200. |
| Division by zero | A `Decimal` built from a division by a computed zero answered 500 `internal_fault` (run). |
| Recursion | **Bug (run):** unbounded recursion, tail or not, never answered within 25 seconds under Bun 1.2.20. The same JavaScript under Node 24 throws `RangeError` after about 10,000 frames, so the hang is specific to Bun on this machine. |
| Logging | **Gap (run):** a rejected domain failure writes nothing, and its internal fields are dropped. A fault writes one `operation.failed` event with no cause; the stack is printed only with `JADPO_DEBUG_TARGET_STACKS=1`. |
| Concealment | `NotVisible` exists, but it cannot share a code with the matching not-found failure (`failure.duplicate_code`), so FAIL-002's alias cannot be written. |
| Localisation | None. Messages are static strings and the OpenAPI document fixes each as a constant. |

### Consistency with the rest of the language

- **Kinds look like types.** `kind: NotFound` is UpperCamelCase like a type
   name but is an option from a fixed set; area 2 decided `kind: not_found`.
- **`code` repeats the name** in every failure in the repository
   (`TodoNotFound`, `"todo_not_found"`), which area 2's "explicit, never
   redundant" rule removes elsewhere.
- **`public` means two things.** In a failure it means "sent to the client";
   on a declaration (`public action reserve_for_cart`) it means "visible to
   other domains".
- **Five mapping spellings.** `missing: F`, `conflict Account.handle: F`,
   `inactive: F` (authentication), `maps provider.x -> F` (services) and
   `failure F => reject G`; area 2 adds `invalid: F`.
- **Faults are listed for integrations only.** A service operation lists
   `Unavailable`, `OutcomeUnknown` and `Misconfigured` in `fails` and callers
   match them, while a database fault never appears in any `fails` list.
   `fault` is a keyword only in `maps ... -> fault X`, and the same names are
   bare in `fails`.
- **`optional` in failure context** contradicts area 2, which allows it only
   on `Input` fields, and grammar §9.
- **Compiler-owned codes are outside the catalogue.** `invalid_input`,
   `route_not_found`, `deadline_exceeded`, `transaction_unavailable`,
   `read_unavailable` and `internal_fault` are emitted by the runtime but are
   not reserved, so an application failure can reuse them.
- **The failure model spec is ahead of the compiler.** It promises structured
   internal events, semantic traces, redaction and a generic message per kind;
   the generated runtime has none of them yet.
- **Area 3's division decision removes one runtime fault.** With the
   divisor's type excluding zero, the remaining runtime faults from code are
   overflow and the recursion limit.

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Data operations' mapping clauses (`missing:`, `conflict: { ... }`, `empty:`) in the `condition: Failure` form, and foreign-key conflicts | 6 | Input to area 6 |
| A route's error responses come from what its action can fail with; no route-level status mapping; validation details and envelope in OpenAPI | 7 | Input to area 7 |
| `precondition_failed` for HTTP preconditions such as `If-Match` | 7 | Input to area 7 |
| Authentication failures: `unauthenticated`, the `inactive:` mapping, principal values kept out of failure context (`type.auth_*` rules) | 8 | Input to area 8 |
| Concealment by policy instead of a declared `not_visible` failure (question 10) | 9 | Depends on question 10 |
| Data classification of internal fields for log redaction, beyond hashing text | 9 | Input to area 9 |
| Server-side translation keyed by failure code, if ever needed | 10 | Depends on question 12 |
| Integration failure mapping as a `failures: { provider.x: F }` block; `maps` and `fault` removed; provider timeouts and `outcome_unknown` | 11 | Input to area 11 |
| Retry, dead-letter and alerting for schedules and subscribers by kind; `outcome_unknown` never retried blindly | 12 | Input to area 12 |
| Asserting that a call fails with a given failure in tests | 13 | Input to area 13 |
| Runtime diagnostics `runtime.unhandled_fault`, `runtime.outcome_unknown` and `runtime.startup_failed`, which `jadpo check` cannot trigger | 14 | Input to area 14 |
| Streaming responses that fail after headers are sent (ROUTE-002) | 7 | Deferred |

### Open questions

1. **Should the public code come from the failure's name?** Every failure in
   the repository writes `code: "todo_not_found"` beside `TodoNotFound`.
   *Recommendation:* derive it (lower_snake_case of the name), and write
   `code:` only to keep a published code after a rename. The compatibility
   file already treats a changed code as breaking, so a rename that changes
   the code is caught at review rather than shipped. Stripe and GitHub
   publish stable snake_case codes; deriving keeps that without the
   repetition.
2. **Rename the `public` block to `client`?** `public` already means
   cross-domain visibility. *Recommendation:* yes, `client: { }` and
   `internal: { }`, which say who sees each field.
3. **Which kinds can an application declare?** Today all 15, and
   `reject Unavailable` compiles but answers 500. *Recommendation:* you may
   declare `invalid_value`, `not_found`, `not_permitted`, `conflict`,
   `rejected`, `rate_limited` and `unavailable`; the other eight are raised
   only by the runtime, and the standard kinds are never failure names in
   `fails` or `reject`. Keep 422 for both `invalid_value` and `rejected` and
   400 for unreadable requests, as GitHub's API does; changing it later is a
   breaking change for every client. This closes FAIL-003.
4. **What does a client see when request input breaks a rule?** Today
   `invalid_input` with no detail, so a form cannot show which field is
   wrong. *Recommendation:* list each problem as `{ field, rule }` under
   `details`, never the value; Stripe's `param` and RFC 9457's problem
   details give the same kind of pointer. Area 7 confirms the field path
   format.
5. **Are faults ever listed in `fails`?** Integrations list `Unavailable`,
   `OutcomeUnknown` and `Misconfigured` today, while database faults never
   appear. *Recommendation:* no: `fails` lists domain failures only, faults go
   straight to the boundary, and code that must react to a provider problem
   gets a declared failure from the integration's mapping (area 11). One
   rule, and actions do not grow a list of infrastructure names. This is
   Rust's split between `Result` and `panic`, and Elixir's "let it crash"
   for the unexpected.
6. **Does a construction the declared ranges prove safe still need
   `attempt`?** *Recommendation:* no. Area 3 made `attempt` an error on
   anything that cannot fail, so the compiler decides from the ranges, and
   its error says which case applies. `Pence(price * quantity)` with both at
   least 0 and no maximum on `Pence` stays plain; `Quantity(quantity * 2)`
   with a maximum of 1000 needs `attempt ... invalid:`. Changing a range
   flips call sites, and the compiler reports each one.
7. **One mapping form everywhere?** Five spellings exist today.
   *Recommendation:* `condition: Failure` inside the operation's block, for
   data operations, constructions, authentication and integrations, with the
   kind checked against the condition (the table above). `match` arms stay as
   area 3 decided. Each area keeps its condition names.
8. **Is the original failure kept when one is mapped to another?** FAIL-001
   left cause wrapping open. *Recommendation:* yes, always and without
   syntax: the original failure and its internal fields become the internal
   cause of the new one, shown in the log and never to the client. Rust's
   `source()` chain and Java's `cause` do this by convention; here the
   compiler knows every mapping, so it can always do it.
9. **How are overflow and the recursion limit reported?** Overflow is silent
   today and unbounded recursion hangs under Bun. *Recommendation:* both are
   defects: the operation ends with 500 `internal_fault`, its writes roll
   back, and the log names the cause (`int_overflow`, `recursion_limit`).
   They are not declarable or catchable, because a caller cannot do anything
   sensible with "the arithmetic overflowed". Rust panics on overflow in
   debug builds and Swift always traps; Go and Kotlin wrap silently, which
   area 2 ruled out.
10. **How is a record hidden by policy reported?** FAIL-002 wanted a
    `NotVisible` failure sharing its code with the genuine not-found, which
    today's duplicate-code check forbids. *Recommendation:* policy reports a
    hidden record with the same `not_found` failure the code already maps, and
    the log adds that policy concealed it; `not_visible` is never declared and
    no alias syntax is needed. GitHub answers 404 for private repositories the
    same way. Area 9 confirms how policy applies it.
11. **Which internal fields reach the log?** Today none. *Recommendation:*
    identifiers, numbers, enums, booleans and times as values; text-based
    values (which can hold names or email addresses) as a keyed hash, so equal
    values still match up across events; secrets cannot be failure fields at
    all, as today. No new syntax; area 9's data classification can refine it
    later.
12. **Who translates failure messages?** FAIL-004. *Recommendation:* the
    client, by `code`. `message` stays fixed English for developers and logs,
    which keeps it a stable part of the API contract. Server-side translation
    can be added later in area 10, keyed by the same code, without changing a
    failure declaration.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Kind names | Carried from area 2 (Richard, 2026-10-12): kinds are lower_snake_case option names, `kind: not_found` | 2026-10-12 |
| Runtime construction | Carried from area 2: a runtime `Type(value)` check needs `attempt ... invalid: SomeFailure` naming an `invalid_value` failure; literals are checked while compiling | 2026-10-12 |
| Failure arms | Carried from area 3: one `match` handles call outcomes; failure arms have no `failure` prefix, every failure has an arm, and an arm recovers, `reject`s or `propagate`s | 2026-10-12 |
| `attempt` placement | Carried from area 3: only at the start of a statement, and only on a call that can fail | 2026-10-12 |
| Signature | Carried from area 3: the whole signature on the header line, `fails` before `->` | 2026-10-12 |
| Colon rule | Carried from area 3: failure members are `name: value`, so context blocks are written `internal: { ... }` | 2026-10-12 |
| Public code | | |
| `public` block name | | |
| Declarable kinds | | |
| Invalid input detail | | |
| Faults in `fails` | | |
| Construction proven safe | | |
| Mapping form | | |
| Cause on mapping | | |
| Overflow and recursion limit | | |
| Concealment | | |
| Internal fields in logs | | |
| Message translation | | |

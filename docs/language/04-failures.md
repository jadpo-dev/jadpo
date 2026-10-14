---
title: Failures
area: 4
status: decided
checked-with: this checkout's jadpo on every gate run (archived examples: v0 3ce094d0)
---

# Failures

How a Jadpo program says that something went wrong: declaring failures, the
kinds that decide how each one reaches a client, listing, raising, passing on
and mapping failures, the faults the runtime raises by itself, and what is
logged.

> Decided on 2026-10-12. Code on this page shows the language after the rework, applying the decisions from areas 1 to 4. Today's compiled syntax is collapsed beneath each section that changes.

The rule behind all of it, from the failure model: **application code says
what failed; the compiler and runtime decide how that fact is shown at each
boundary.** Code never throws, never picks an HTTP status and never builds an
error response.

| | Declared by | Listed in `fails` | The client sees | The log gets |
|---|---|---|---|---|
| **Domain failure**: an expected outcome such as `TodoNotFound` | you, as `type X = Failure` | yes | its code, message and client fields | its code, where it was raised, the path it travelled and its internal fields |
| **Fault**: the database is down, a provider timed out, a write may or may not have committed | the runtime | no (question 5) | a fixed code for that fault | the cause chain |
| **Defect**: overflow, the recursion limit, a compiler bug | the runtime | no | `internal_fault` | the cause and stack |

## Declaring a failure

```jadpo check project=declaring file=types.jadpo hidden
type Quantity = Int {
    min: 1
    max: 1000
}

entity Todo {
    id: Uuid
}

entity Product {
    id: Uuid
}
```

```jadpo check project=declaring
/// The todo does not exist.
type TodoNotFound = Failure {
    kind: FailureKind.not_found
    message: "Todo not found."
    internal: {
        todo_id: Todo.id
    }
}

type QuantityUnavailable = Failure {
    kind: FailureKind.conflict
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
  field name cannot be in both. Fields exist only inside these two blocks, so
  leaving `client` out means the caller gets only the code, message and
  request id.
- Context fields are ordinary fields: `T?` is allowed, `optional` and
  `default` are not (area 2 limits them to `Input`).
- Each failure has its own file in its domain's `failures/` folder
  (`domains/planning/failures/todo_not_found.jadpo`), or in
  `shared/failures/` once two or more domains raise it (question 15). It is
  marked `public` when another domain handles it.

Decided (question 13): a failure is written like every other type,
`type TodoNotFound = Failure { ... }`, so code reads the same everywhere.
`Failure` as a base brings `kind`, `message`, `client` and `internal`, as
`Input` brings optional fields.

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

Carried from area 2: kinds are lower_snake_case option names (superseded
2026-10-12: written `FailureKind.not_found`; see area 2 question 17). The
"raised by" column is decided (question 3); today every kind can still be
declared and rejected.

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

```jadpo check project=kinds expect=failure.kind_runtime_only
type CheckoutUnavailable = Failure {
    kind: FailureKind.unavailable  // fine: a business outcome callers handle
    message: "Checkout is temporarily unavailable."
}

type Broken = Failure {
    kind: FailureKind.internal_fault  // error: only the runtime raises internal_fault (question 3)
}
```

```text output
error: `Broken` can't have the kind `internal_fault`
 --> main.jadpo:7:11
  |
7 |     kind: FailureKind.internal_fault  // error: only the runtime raises internal_fault (question 3)
  |           ^^^^^^^^^^^^^^^^^^^^^^^^^^ raised only by the runtime
  reason: The runtime raises `internal_fault` itself; an application declares `invalid_value`, `not_found`, `not_permitted`, `conflict`, `rejected`, `rate_limited` and `unavailable`, so a client's status always means the same thing.
  next: Use the kind an application declares that fits this outcome (agent)
  rule: failure.kind_runtime_only
  help: docs/language/04-failures.html#failure-kind-runtime-only

check failed: 1 error, 1 file
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

```jadpo check project=reorder file=app.jadpo hidden
application {}
```

```jadpo check project=reorder file=domains/ordering/entities/order.jadpo hidden
public entity Order {
    id: Uuid
}
```

```jadpo check project=reorder file=domains/ordering/failures/order_not_found.jadpo hidden
type OrderNotFound = Failure {
    kind: FailureKind.not_found
}
```

```jadpo check project=reorder file=domains/inventory/types/reservation.jadpo hidden
public type Reservation = Object {
    order_id: Order.id
}
```

```jadpo check project=reorder file=domains/inventory/failures/quantity_unavailable.jadpo hidden
public type QuantityUnavailable = Failure {
    kind: FailureKind.conflict
}
```

```jadpo check project=reorder file=domains/inventory/actions/reserve_for_order.jadpo hidden
public action reserve_for_order(order: Order) fails QuantityUnavailable -> Reservation {
    reject QuantityUnavailable
}
```

```jadpo check project=reorder file=domains/ordering/actions/reorder.jadpo migrate=initial
action reorder(order_id: Order.id) fails OrderNotFound, QuantityUnavailable -> Reservation {
    var order: Order = attempt Order.get(order_id)  // can fail with OrderNotFound
    return attempt inventory.reserve_for_order(order)  // can fail with QuantityUnavailable
}
```

Decided in area 2: a runtime construction can fail, and its `attempt` maps
each rule that can break in a block after the call,
`attempt Type(value) { max: SomeFailure }`, with `invalid:` covering every
rule without an arm of its own (area 2, "Construction failures by rule").
When the declared ranges, or the conditions it sits under (area 2,
"Conditions as proof"), prove a construction cannot break any of the
target's rules, it is not fallible, so it takes no `attempt` (question 6):

```jadpo check project=proven file=types.jadpo hidden
type Pence = Int {
    min: 0
}

type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=proven
function line_total(price: Pence, quantity: Quantity) -> Pence {
    var total: Pence = Pence(price * quantity)  // both at least 0, Pence has no maximum
    return total
}

function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {  // at least 2, but can exceed 1000
        max: QuantityTooLarge
    }
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
// a read that finds nothing: queries/todo_by_id.jadpo (area 6)
query todo_by_id(todo_id: Todo.id) fails TodoNotFound -> TodoView {
    from: Todo
    where: Todo.id == todo_id
    missing: TodoNotFound {
        todo_id: todo_id
    }
}

// a runtime construction, keyed by the rule that can break (area 2)
var quantity: Quantity = attempt Quantity(count) {
    max: QuantityTooLarge
    invalid: QuantityOutOfRange
}

// a write that breaks a unique rule (area 6)
var account: Account = attempt Account.create {
    set: {
        ...
    }
    conflict: {
        Account.handle: HandleTaken
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
| A construction's rule keys, such as `max:`, and `invalid:` covering the rest | `invalid_value` |

`invalid:` is the catch-all only for a construction's rules. A read's,
a write's or an integration call's conditions each need their own arm, and
`invalid:` in their block is an error (`failure.invalid_outside_construction`).

The failure that was mapped is kept as the internal cause of the new one, so
the log shows both (question 8):

```text
PaymentDeclined rejected at domains/checkout/actions/pay.jadpo:14
  caused by CardDeclined from payments.charge (internal: decline_code)
  propagated through route POST /checkout
```

::: today

Data operations already map with `missing:`, `conflict:` and `empty:`, and
the kind is checked; constructions map nothing. Services use
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
maps with `reject` or passes the failure on with `propagate`. The arms
handle only the call itself: a failure that leaves while its arguments are
worked out leaves the callable around the `match`, as it would anywhere
else (built in chunk 2.7).

```jadpo check project=outcome file=lookup.jadpo hidden
type LookupCode = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

type LookupRefused = Failure {
    kind: FailureKind.rejected
}

type LookupRejected = Failure {
    kind: FailureKind.rejected
}

function lookup(key: LookupCode) fails LookupMissing, LookupRefused -> LookupResult {
    if (key == "missing") {
        reject LookupMissing
    }
    if (key == "refused") {
        reject LookupRefused
    }
    return "found"
}
```

```jadpo check project=outcome
function lookup_or_default(key: LookupCode) fails LookupRejected -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "not found"
        LookupRefused -> reject LookupRejected
    }
}
```

Its errors (`failure.outcome_*`) are in area 3.

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
var todo: Todo = attempt Todo.get(todo_id)
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

Every error this area can produce, with an example that triggers it and the
new compiler's actual output. Eight entries keep the archived compiler's
example: four because the decided language has no such mistake any more, or
the new compiler reports it under another rule, named in each note; and the
four that check a data operation's mapping, which are built with data
operations (area 6).

### `syntax.fails_after_result`

```jadpo check project=e-fails-after expect=syntax.fails_after_result
type Title = Text {
    max_length: 200
}

type TitleBlank = Failure {
    kind: FailureKind.invalid_value
    message: "Enter a title."
}

function check_title(title: Title) -> Title fails TitleBlank {
    if (title == "") {
        reject TitleBlank
    }
    return title
}
```

```text output
error: `fails` goes before `->`
 --> main.jadpo:10:45
   |
10 | function check_title(title: Title) -> Title fails TitleBlank {
   |                                             ^^^^^^^^^^^^^^^^ after the result type
  reason: A signature says what can fail before what it returns: `fails A, B -> Result`.
  next: Move `fails TitleBlank` before `->` (agent)
  rule: syntax.fails_after_result
  help: docs/language/04-failures.html#syntax-fails-after-result

check failed: 1 error, 1 file
```

### `failure.kind_required`

```jadpo check project=e-kind expect=failure.kind_required
type Missing = Failure {
    message: "Not found."
}
```

```text output
error: `Missing` has no `kind`
 --> main.jadpo:1:6
  |
1 | type Missing = Failure {
  |      ^^^^^^^ a failure without a kind
  reason: A failure's kind decides how it reaches a client, its HTTP status included, and how background work treats it, so every failure names one.
  next: Add `kind:` with the `FailureKind` that fits, such as `kind: FailureKind.not_found` (agent)
  rule: failure.kind_required
  help: docs/language/04-failures.html#failure-kind-required

check failed: 1 error, 1 file
```

### `syntax.failure_kind_required`

> The archived compiler's error: a failure's missing `kind:` is now `failure.kind_required`, above.

```jadpo project=e-kind-required expect=syntax.failure_kind_required
failure Missing {
    code: "missing"
}
```

### `syntax.failure_code_required`

> The archived compiler's error: `code:` is optional now; it defaults to the failure's name (question 1).

```jadpo project=e-code-required expect=syntax.failure_code_required
failure Missing {
    kind: NotFound
}
```

### `syntax.failure_item_colon_required`

> The archived compiler's error: the new parser reports a missing colon as `syntax.colon_required` (area 3).

```jadpo project=e-item-colon expect=syntax.failure_item_colon_required
failure Missing {
    kind NotFound
    code: "missing"
}
```

### `syntax.expected_failure_item`

> The archived compiler's error: a setting a `Failure` doesn't take, such as `status: 404`, is now `type.unknown_setting` (area 2).

A failure cannot choose its own HTTP status:

```jadpo project=e-failure-item expect=syntax.expected_failure_item
failure Missing {
    kind: NotFound
    code: "missing"
    status: 404
}
```

### `failure.kind_runtime_only`

```jadpo check project=e-kind-runtime expect=failure.kind_runtime_only
type Broken = Failure {
    kind: FailureKind.internal_fault
}
```

```text output
error: `Broken` can't have the kind `internal_fault`
 --> main.jadpo:2:11
  |
2 |     kind: FailureKind.internal_fault
  |           ^^^^^^^^^^^^^^^^^^^^^^^^^^ raised only by the runtime
  reason: The runtime raises `internal_fault` itself; an application declares `invalid_value`, `not_found`, `not_permitted`, `conflict`, `rejected`, `rate_limited` and `unavailable`, so a client's status always means the same thing.
  next: Use the kind an application declares that fits this outcome (agent)
  rule: failure.kind_runtime_only
  help: docs/language/04-failures.html#failure-kind-runtime-only

check failed: 1 error, 1 file
```

### `failure.code_format`

```jadpo check project=e-code-format expect=failure.code_format
type Missing = Failure {
    kind: FailureKind.not_found
    code: "Not Found!"
}
```

```text output
error: The code `Not Found!` isn't lower_snake_case
 --> main.jadpo:3:11
  |
3 |     code: "Not Found!"
  |           ^^^^^^^^^^^^ not lower_snake_case
  reason: A failure's code is what clients branch on and translate by, so it is lower_snake_case, like a failure's name turned into one.
  next: Write `"not_found"` (agent)
  rule: failure.code_format
  help: docs/language/04-failures.html#failure-code-format

check failed: 1 error, 1 file
```

### `failure.duplicate_code`

Two failures cannot share a code, whether it is written or comes from the
name:

```jadpo check project=e-duplicate-code expect=failure.duplicate_code
type CustomerNotFound = Failure {
    kind: FailureKind.not_found
    message: "Customer not found."
}

type CustomerMissing = Failure {
    kind: FailureKind.not_found
    code: "customer_not_found"
    message: "Customer not found."
}
```

```text output
error: The code `customer_not_found` is already used by `CustomerNotFound`
 --> main.jadpo:8:11
  |
8 |     code: "customer_not_found"
  |           ^^^^^^^^^^^^^^^^^^^^ the same code
  reason: `CustomerNotFound` has the code `customer_not_found` too. A client relies on a code always meaning the same failure, with the same status, message and fields, so every code belongs to one failure.
  next: Give one of them a code of its own, or use one failure for both (human)
  rule: failure.duplicate_code
  help: docs/language/04-failures.html#failure-duplicate-code

check failed: 1 error, 1 file
```

### `failure.context_field_overlap`

```jadpo check project=e-context-overlap expect=failure.context_field_overlap
type Reference = Text {
    max_length: 64
}

type PaymentRefused = Failure {
    kind: FailureKind.rejected
    client: {
        reference: Reference
    }
    internal: {
        reference: Reference
    }
}
```

```text output
error: `reference` is in both `client` and `internal`
 --> main.jadpo:11:9
   |
 8 |         reference: Reference
   |         --------- first declared here
11 |         reference: Reference
   |         ^^^^^^^^^ declared again
  reason: `client` fields are sent to the caller and `internal` fields never leave the server, so a field is one or the other.
  next: Keep `reference` in one of the two blocks (agent)
  rule: failure.context_field_overlap
  help: docs/language/04-failures.html#failure-context-field-overlap

check failed: 1 error, 1 file
```

### `failure.missing_context_field`

```jadpo check project=e-context-missing expect=failure.missing_context_field
type Lookup = Text {}

type Missing = Failure {
    kind: FailureKind.not_found
    internal: {
        lookup: Lookup
    }
}

action find(lookup: Lookup) fails Missing -> Lookup {
    reject Missing
}
```

```text output
error: `Missing` is missing `lookup`
 --> main.jadpo:11:12
   |
11 |     reject Missing
   |            ^^^^^^^ fields missing
  reason: A failure is raised with every field its `client` and `internal` blocks declare, so the client and the log always get them.
  next: Give its fields in a block: `Missing { lookup: ... }` (agent)
  rule: failure.missing_context_field
  help: docs/language/04-failures.html#failure-missing-context-field

check failed: 1 error, 1 file
```

### `failure.unknown_context_field`

```jadpo check project=e-context-unknown expect=failure.unknown_context_field
type Lookup = Text {}

type Missing = Failure {
    kind: FailureKind.not_found
    internal: {
        lookup: Lookup
    }
}

action find(lookup: Lookup) fails Missing -> Lookup {
    reject Missing {
        lookup: lookup
        search: lookup
    }
}
```

```text output
error: `Missing` has no field `search`
 --> main.jadpo:13:9
   |
13 |         search: lookup
   |         ^^^^^^ not declared
  reason: `Missing`'s fields are `lookup`. A failure carries only the fields its `client` and `internal` blocks declare.
  next: Remove `search`, or declare it in `Missing`'s `client` or `internal` block (agent)
  rule: failure.unknown_context_field
  help: docs/language/04-failures.html#failure-unknown-context-field

check failed: 1 error, 1 file
```

### `failure.duplicate_context_field`

```jadpo check project=e-context-duplicate expect=failure.duplicate_context_field
type Lookup = Text {}

type Missing = Failure {
    kind: FailureKind.not_found
    internal: {
        lookup: Lookup
    }
}

action find(lookup: Lookup) fails Missing -> Lookup {
    reject Missing {
        lookup: lookup
        lookup: lookup
    }
}
```

```text output
error: `lookup` is given twice
 --> main.jadpo:13:9
   |
12 |         lookup: lookup
   |         ------ first given here
13 |         lookup: lookup
   |         ^^^^^^ given again
  reason: Each field of `Missing` is given once, so the log and the client never have to guess which value counts.
  next: Remove one `lookup` (agent)
  rule: failure.duplicate_context_field
  help: docs/language/04-failures.html#failure-duplicate-context-field

check failed: 1 error, 1 file
```

### `failure.undeclared_propagation`

```jadpo check project=e-undeclared expect=failure.undeclared_propagation
type Note = Text {}

type RegistrationClosed = Failure {
    kind: FailureKind.conflict
}

action register(note: Note) -> Note {
    reject RegistrationClosed
}
```

```text output
error: `RegistrationClosed` can leave `register`, but its `fails` list doesn't say so
 --> main.jadpo:8:5
  |
8 |     reject RegistrationClosed
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^ rejected here
  reason: `fails` lists exactly the failures that can leave an action, so every caller knows what it must handle.
  next: Add `RegistrationClosed` to `register`'s `fails` list (agent)
  rule: failure.undeclared_propagation
  help: docs/language/04-failures.html#failure-undeclared-propagation

check failed: 1 error, 1 file
```

### `failure.stale_declaration`

```jadpo check project=e-stale expect=failure.stale_declaration
type Note = Text {}

type Refused = Failure {
    kind: FailureKind.rejected
}

action complete(note: Note) fails Refused -> Note {
    return note
}
```

```text output
error: `Refused` can never leave `complete`
 --> main.jadpo:7:35
  |
7 | action complete(note: Note) fails Refused -> Note {
  |                                   ^^^^^^^ never raised or passed on
  reason: Nothing in `complete` rejects `Refused` or passes it on from a call, and `fails` lists exactly the failures that can leave, so callers aren't made to handle one that never happens.
  next: Remove `Refused` from `fails` (agent)
  rule: failure.stale_declaration
  help: docs/language/04-failures.html#failure-stale-declaration

check failed: 1 error, 1 file
```

### `failure.duplicate_declaration`

```jadpo check project=e-duplicate-fails expect=failure.duplicate_declaration
type Note = Text {}

type Refused = Failure {
    kind: FailureKind.rejected
}

action execute(note: Note) fails Refused, Refused -> Note {
    reject Refused
}
```

```text output
error: `Refused` is listed twice after `fails`
 --> main.jadpo:7:43
  |
7 | action execute(note: Note) fails Refused, Refused -> Note {
  |                                           ^^^^^^^ listed again
  |                                  ------- first listed here
  reason: `fails` lists each failure that can leave once.
  next: Remove the second `Refused` (agent)
  rule: failure.duplicate_declaration
  help: docs/language/04-failures.html#failure-duplicate-declaration

check failed: 1 error, 1 file
```

### `failure.rule_unmapped`

Each rule a construction can break has an arm, or `invalid:` covers it:

```jadpo check project=e-rule-unmapped expect=failure.rule_unmapped
type Count = Int {}

type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function from_count(n: Count) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(n) {
        max: QuantityTooLarge
    }
    return result
}
```

```text output
error: `Quantity(...)` can break `min`, and nothing maps it
 --> main.jadpo:13:28
   |
13 |     var result: Quantity = attempt Quantity(n) {
   |                            ^^^^^^^^^^^^^^^^^^^^^ can fail at runtime
  reason: The value is only known when the program runs, and neither the declared ranges nor a condition before it prove it follows `min: 1`; it needs an arm naming the `invalid_value` failure the caller gets, or `invalid:` covers every rule without its own.
  next: Add `min: SomeFailure` to the block, naming an `invalid_value` failure, or `invalid: SomeFailure` to cover every rule without an arm (agent)
  rule: failure.rule_unmapped
  help: docs/language/04-failures.html#failure-rule-unmapped

check failed: 1 error, 1 file
```

### `failure.rule_cannot_break`

An arm for a rule the declared ranges prove holds could never run:

```jadpo check project=e-rule-cannot-break expect=failure.rule_cannot_break
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooSmall = Failure {
    kind: FailureKind.invalid_value
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooSmall, QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        min: QuantityTooSmall
        max: QuantityTooLarge
    }
    return result
}
```

```text output
error: `Quantity(...)` can't break `min`, so it takes no arm
 --> main.jadpo:16:9
   |
16 |         min: QuantityTooSmall
   |         ^^^ never runs
  reason: The declared ranges prove the value always follows `min: 1`, so this arm could never run.
  next: Remove `min: ...` (agent)
  rule: failure.rule_cannot_break
  help: docs/language/04-failures.html#failure-rule-cannot-break

check failed: 1 error, 1 file
```

### `failure.invalid_covers_nothing`

```jadpo check project=e-invalid-covers-nothing expect=failure.invalid_covers_nothing
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

type QuantityRefused = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooLarge, QuantityRefused -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityTooLarge
        invalid: QuantityRefused
    }
    return result
}
```

```text output
error: `invalid:` covers nothing here
 --> main.jadpo:17:9
   |
17 |         invalid: QuantityRefused
   |         ^^^^^^^ no rule left to cover
  reason: Every rule `Quantity(...)` can break has its own arm (`max`), so `invalid:` could never run.
  next: Remove `invalid: ...` (agent)
  rule: failure.invalid_covers_nothing
  help: docs/language/04-failures.html#failure-invalid-covers-nothing

check failed: 1 error, 1 file
```

### `failure.invalid_outside_construction`

`invalid:` covers only a construction's rules, never another call's
failures or a write's conditions:

```jadpo check project=e-invalid-outside-construction expect=failure.invalid_outside_construction
type Note = Text {}

type NoteRefused = Failure {
    kind: FailureKind.rejected
}

type NoteInvalid = Failure {
    kind: FailureKind.invalid_value
}

action check_note(note: Note) fails NoteRefused -> Note {
    if (note == "") {
        reject NoteRefused
    }
    return note
}

action save_note(note: Note) fails NoteRefused, NoteInvalid -> Note {
    return attempt check_note(note) {
        invalid: NoteInvalid
    }
}
```

```text output
error: `check_note(...)` isn't a construction, so `invalid:` covers nothing
 --> main.jadpo:20:9
   |
20 |         invalid: NoteInvalid
   |         ^^^^^^^ not a construction
  reason: `check_note(...)` fails with the failures its `fails` list names; `invalid:` is the catch-all only for the rules a construction's value can break, never for another call's failures.
  next: Remove `invalid: ...`; `attempt` passes `check_note(...)`'s failures on, and `match` turns one into another (agent)
  rule: failure.invalid_outside_construction
  help: docs/language/04-failures.html#failure-invalid-outside-construction

check failed: 1 error, 1 file
```

### `failure.duplicate_mapping`

```jadpo check project=e-duplicate-mapping expect=failure.duplicate_mapping
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityTooLarge
        max: QuantityTooLarge
    }
    return result
}
```

```text output
error: `max:` is mapped twice
 --> main.jadpo:13:9
   |
12 |         max: QuantityTooLarge
   |         --- first mapped here
13 |         max: QuantityTooLarge
   |         ^^^ mapped again
  reason: Each condition is mapped once, so there is never a question of which failure the caller gets.
  next: Remove one `max:` (agent)
  rule: failure.duplicate_mapping
  help: docs/language/04-failures.html#failure-duplicate-mapping

check failed: 1 error, 1 file
```

### `failure.mapping_kind`

```jadpo check project=e-mapping-kind expect=failure.mapping_kind
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityRefused = Failure {
    kind: FailureKind.rejected
}

function doubled(quantity: Quantity) fails QuantityRefused -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityRefused
    }
    return result
}
```

```text output
error: `max:` needs an `invalid_value` failure, and `QuantityRefused` is `rejected`
 --> main.jadpo:12:14
   |
12 |         max: QuantityRefused
   |              ^^^^^^^^^^^^^^^ kind `rejected`
  reason: Each mapping's condition fixes the kind of failure it becomes: a value that breaks a type's rules is `invalid_value`, so the client gets the status that means.
  next: Name an `invalid_value` failure here, or make `QuantityRefused`'s kind `FailureKind.invalid_value` (agent)
  rule: failure.mapping_kind
  help: docs/language/04-failures.html#failure-mapping-kind

check failed: 1 error, 1 file
```

### `failure.unknown_mapping`

A construction's arms are the rules of its type:

```jadpo check project=e-unknown-mapping expect=failure.unknown_mapping
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        maximum: QuantityTooLarge
    }
    return result
}
```

```text output
error: `Quantity(...)` has no `maximum:` condition to map
 --> main.jadpo:12:9
   |
12 |         maximum: QuantityTooLarge
   |         ^^^^^^^ not a condition here
  reason: A construction's arms are the rules of its type and the types it builds on; `Quantity(...)`'s are `min` and `max`, and `invalid:` covers any without an arm of its own.
  next: Remove `maximum: ...`, or map one of `min` and `max` instead (agent)
  rule: failure.unknown_mapping
  help: docs/language/04-failures.html#failure-unknown-mapping

check failed: 1 error, 1 file
```

### `syntax.mapping_block_required`

An `attempt`'s mappings go in a block after the call, never after it on the
same line:

```jadpo check project=e-mapping-block-required expect=syntax.mapping_block_required
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) invalid: QuantityTooLarge
    return result
}
```

```text output
error: An `attempt`'s mappings go in a block after the call
 --> main.jadpo:11:59
   |
11 |     var result: Quantity = attempt Quantity(quantity * 2) invalid: QuantityTooLarge
   |                                                           ^^^^^^^^^^^^^^^^^^^^^^^^^ after the call, not in a block
  reason: An `attempt` maps its failures in a block, one per line, the same form writes and integration calls use; a construction's keys are the rules that can break, with `invalid:` covering the rest.
  next: Move `invalid: QuantityTooLarge` into a block after the call: `{ invalid: QuantityTooLarge }` (agent)
  rule: syntax.mapping_block_required
  help: docs/language/04-failures.html#syntax-mapping-block-required

check failed: 1 error, 1 file
```

The four errors below check that a data operation's mapping names a failure
of the right kind. The new compiler checks a construction's arms the same
way, as `failure.mapping_kind` above.

### `failure.required_query_not_not_found`

> The archived compiler's example: since chunk 3.2 a query's `missing:` naming a failure of another kind is `failure.mapping_kind`, above (area 6).

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

> The archived compiler's example: data operations' mappings are checked once data operations are built (area 6).

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

> The archived compiler's example: data operations' mappings are checked once data operations are built (area 6).

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

> The archived compiler's example: data operations' mappings are checked once data operations are built (area 6).

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
   name but is an option from a fixed set; area 2 decided `kind: FailureKind.not_found`.
- **`code` repeats the name** in every failure in the repository
   (`TodoNotFound`, `"todo_not_found"`), which area 2's "explicit, never
   redundant" rule removes elsewhere.
- **`public` means two things.** In a failure it means "sent to the client";
   on a declaration (`public action reserve_for_cart`) it means "visible to
   other domains".
- **Five mapping spellings.** `missing: F`, `conflict Account.handle: F`,
   `inactive: F` (authentication), `maps provider.x -> F` (services) and
   `failure F => reject G`; area 2 adds a construction's rule arms.
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
| Whether `event` should also become `type X = Event { ... }`, by the same reasoning as failures | 12 | Raised |
| A committed record of the public API (routes, fields, failure codes) so breaking changes are caught at review, accepted explicitly: the compiler flags the change and the build passes only once a person records an acceptance. Design one "accept a reviewed change" mechanism shared with database drops (area 5), policy (area 9) and domain changes (area 1) (Richard's idea) | 7 | Raised |

### Open questions

1. **Should the public code come from the failure's name?** *Decided
   2026-10-12 (Richard):* yes by default: `QuantityUnavailable` has the code
   `quantity_unavailable`. `code: "..."` overrides it when a different code is
   needed, such as keeping a published code after a rename. The compiler
   checks every code is lower_snake_case and unique across all failures (one
   code, one failure: a client relies on a code always meaning the same
   status, message and fields). Detecting that a published code changed needs
   a committed record of the public API, carried to area 7.

2. **Rename the `public` block to `client`?** `public` already means
   cross-domain visibility. *Decided 2026-10-12:* yes, `client: { }` and
   `internal: { }`, which say who sees each field.
3. **Which kinds can an application declare?** Today all 15, and
   `reject Unavailable` compiles but answers 500. *Decided 2026-10-12:* you may
   declare `invalid_value`, `not_found`, `not_permitted`, `conflict`,
   `rejected`, `rate_limited` and `unavailable`; the other eight are raised
   only by the runtime, and the standard kinds are never failure names in
   `fails` or `reject`. Keep 422 for both `invalid_value` and `rejected` and
   400 for unreadable requests, as GitHub's API does; changing it later is a
   breaking change for every client. This closes FAIL-003.
   A timeout an application cares about is either a business rule with its
   own failure (`ReservationExpired` with `kind: FailureKind.conflict`) or a provider
   problem mapped by the integration to a declared `unavailable` failure; how
   long to wait is a setting on the integration or route (areas 7 and 11), and
   the runtime raises the timeout kind itself.
   For example, an integration with a five-second limit maps its timeout to
   the application's own failure, which callers handle like any other:
   `timeout: 5s` and `timed_out: ChargeUnavailable` on the integration
   (area 11), then `ChargeUnavailable -> ...` in a `match`.

4. **What does a client see when request input breaks a rule?** Today
   `invalid_input` with no detail, so a form cannot show which field is
   wrong. *Decided 2026-10-12:* list each problem as `{ field, rule }` under
   `details`, never the value; Stripe's `param` and RFC 9457's problem
   details give the same kind of pointer. Area 7 confirms the field path
   format.
5. **Are faults ever listed in `fails`?** Integrations list `Unavailable`,
   `OutcomeUnknown` and `Misconfigured` today, while database faults never
   appear. *Decided 2026-10-12:* no: `fails` lists domain failures only, faults go
   straight to the boundary, and code that must react to a provider problem
   gets a declared failure from the integration's mapping (area 11). One
   rule, and actions do not grow a list of infrastructure names. This is
   Rust's split between `Result` and `panic`, and Elixir's "let it crash"
   for the unexpected.
6. **Does a construction the declared ranges prove safe still need
   `attempt`?** *Decided 2026-10-12:* no. Area 3 made `attempt` an error on
   anything that cannot fail, so the compiler decides from the ranges, and
   its error says which case applies. `Pence(price * quantity)` with both at
   least 0 and no maximum on `Pence` stays plain; `Quantity(quantity * 2)`
   with a maximum of 1000 needs `attempt` and an arm for `max`. Changing a range
   flips call sites, and the compiler reports each one. *Built in chunk 2.6,*
   with an `if` condition counting as proof (area 2, "Conditions as
   proof"): every call site that flips is an error where it is written,
   `failure.attempt_required` or `failure.attempt_infallible`, and when a
   comparison is what proves it, the error says so and points at it.
7. **One mapping form everywhere?** Five spellings exist today.
   *Decided 2026-10-12:* `condition: Failure` inside the operation's block, for
   data operations, constructions, authentication and integrations, with the
   kind checked against the condition (the table above). `match` arms stay as
   area 3 decided. Each area keeps its condition names.
8. **Is the original failure kept when one is mapped to another?** FAIL-001
   left cause wrapping open. *Decided 2026-10-12:* yes, always and without
   syntax: the original failure and its internal fields become the internal
   cause of the new one, shown in the log and never to the client. Rust's
   `source()` chain and Java's `cause` do this by convention; here the
   compiler knows every mapping, so it can always do it.
9. **How are overflow and the recursion limit reported?** Overflow is silent
   today and unbounded recursion hangs under Bun. *Decided 2026-10-12:* both are
   defects: the operation ends with 500 `internal_fault`, its writes roll
   back, and the log names the cause (`int_overflow`, `recursion_limit`).
   They are not declarable or catchable, because a caller cannot do anything
   sensible with "the arithmetic overflowed". Rust panics on overflow in
   debug builds and Swift always traps; Go and Kotlin wrap silently, which
   area 2 ruled out.
10. **How is a record hidden by policy reported?** FAIL-002 wanted a
    `NotVisible` failure sharing its code with the genuine not-found, which
    today's duplicate-code check forbids. *Decided 2026-10-12:* policy reports a
    hidden record with the same `not_found` failure the code already maps, and
    the log adds that policy concealed it; `not_visible` is never declared and
    no alias syntax is needed. GitHub answers 404 for private repositories the
    same way. Area 9 confirms how policy applies it.
11. **Which internal fields reach the log?** Today none. *Decided 2026-10-12:*
    identifiers, numbers, enums, booleans and times as values; text-based
    values (which can hold names or email addresses) as a keyed hash, so equal
    values still match up across events; secrets cannot be failure fields at
    all, as today. No new syntax; area 9's data classification can refine it
    later.
12. **Who translates failure messages?** FAIL-004. *Decided 2026-10-12:* the
    client, by `code`. `message` stays fixed English for developers and logs,
    which keeps it a stable part of the API contract. Server-side translation
    can be added later in area 10, keyed by the same code, without changing a
    failure declaration.

13. **Should a failure be a `type`?** Today `failure TodoNotFound { ... }`
    is its own keyword. *Decided 2026-10-12:* `type TodoNotFound = Failure { ... }`,
    the same form as every other type, so people and agents write code one
    way (Richard). It also matches how mainstream languages model errors, as a
    type built on an error base (`class X extends Error` in TypeScript,
    `struct X: Error` in Swift). `Failure` as a base brings `kind`, `message`, `client` and
    `internal`, as `Input` brings optional fields.
14. **What does a failure without a `client` block show?**
    *Decided 2026-10-12:* fields exist only inside `client: { }` and
    `internal: { }`, so a block left out means no fields of that kind and the
    caller gets only the code, message and request id; nothing reaches the
    client unless it is listed under `client`.

15. **Where do failures live in the folders: centralised, per domain, or
    both?** *Decided 2026-10-12 (Richard):* one failure per file, always in
    the raising domain's own `failures/` folder, or in `shared/failures/`
    once two or more domains raise it. Failures are numerous, are part of the
    public contract and are looked for as a group, so they get a folder of
    their own rather than sitting in `types/`. A failure another domain only
    handles stays where it is raised and is marked `public`. Generic
    behaviour comes from `kind`, and the runtime owns the built-in failures.
    The compiler also generates a catalogue of every failure for
    documentation and API consumers.

### Decisions

Decided by Richard on 2026-10-12. Checked by the new compiler since 2026-10-13 (roadmap chunk 1.6): declarations, kinds, codes, `fails`, `reject`, `attempt`, construction mappings and outcome `match`. A construction's failures are mapped by rule since chunk 2.5, and an `if` condition counts as proof since chunk 2.6. Data operations' mappings come with area 6, and the client envelope and OpenAPI with area 7. What runs came with the interpreter on 2026-10-14 (chunk 2.1): failures with their fields, where they were raised and the path they travelled, mapped failures keeping their cause, and the `int_overflow` and `recursion_limit` faults with their `.jadpo` frames; the log events come with the runtime's hosts.

| Topic | Decision | Date |
|---|---|---|
| Kind names | Carried from area 2 (Richard, 2026-10-12): kinds are lower_snake_case option names, `kind: not_found` (Superseded 2026-10-12: options are written `Type.value`; see area 2 question 17.) | 2026-10-12 |
| Runtime construction | Carried from area 2: a runtime `Type(value)` check needs `attempt` with a block mapping each rule that can break to an `invalid_value` failure, `invalid:` covering the rest (revised 2026-10-14, built in chunk 2.5); literals are checked while compiling | 2026-10-12 |
| Failure arms | Carried from area 3: one `match` handles call outcomes; failure arms have no `failure` prefix, every failure has an arm, and an arm recovers, `reject`s or `propagate`s | 2026-10-12 |
| `attempt` placement | Carried from area 3: only at the start of a statement, and only on a call that can fail | 2026-10-12 |
| Signature | Carried from area 3: the whole signature on the header line, `fails` before `->` | 2026-10-12 |
| Colon rule | Carried from area 3: failure members are `name: value`, so context blocks are written `internal: { ... }` | 2026-10-12 |
| Public code | Defaults to the failure name in lower_snake_case; `code:` overrides it; every code unique and lower_snake_case, one code per failure; detecting a changed published code is carried to area 7 | 2026-10-12 |
| `client` block | `client: { }` (sent to the caller) and `internal: { }` (never leaves the server) | 2026-10-12 |
| Declarable kinds | `invalid_value`, `not_found`, `not_permitted`, `conflict`, `rejected`, `rate_limited`, `unavailable`; the other eight are runtime-only; 422 for `invalid_value` and `rejected`, 400 for unreadable requests; time limits are settings on integrations and routes, and a timeout an application cares about is mapped to its own failure | 2026-10-12 |
| Invalid input detail | Each problem as `{ field, rule }` under `details`, never the value | 2026-10-12 |
| Faults in `fails` | Never; `fails` lists domain failures; integrations map provider problems to declared failures | 2026-10-12 |
| Construction proven safe | No `attempt` when declared ranges prove a construction cannot fail; the compiler decides and reports call sites that flip. An `if` condition counts as proof too (area 2, "Construction failures by rule", 2026-10-14); each call site that flips is its own error, naming the comparison that proves it when one does (built in chunk 2.6) | 2026-10-12 |
| Mapping form | One form everywhere: `condition: Failure` inside the operation's block, kind checked. Constructions join it on 2026-10-14, keyed by rule (area 2, "Construction failures by rule"), built in chunk 2.5; `invalid:` is the catch-all only for a construction's rules, and every other condition has its own arm | 2026-10-12 |
| Cause on mapping | The original failure is always kept as an internal-only cause | 2026-10-12 |
| Overflow and recursion limit | Defects: 500 `internal_fault`, writes rolled back, cause named in the log; not declarable or catchable | 2026-10-12 |
| Concealment | A record hidden by policy reports the same `not_found` failure; the log records the concealment | 2026-10-12 |
| Internal fields in logs | Identifiers, numbers, enums, booleans and times as values; text as a keyed hash; secrets never | 2026-10-12 |
| Message translation | The client translates by `code`; `message` stays fixed English | 2026-10-12 |
| Failure declaration form | `type Name = Failure { ... }`, the same form as every other type (Richard: consistency when writing code) | 2026-10-12 |
| Failure fields | Fields exist only in `client: { }` and `internal: { }`; without `client` the caller gets only the code, message and request id | 2026-10-12 |
| Request id | Every error response carries `request_id` (also the `x-request-id` header), and every failure, expected ones included, is logged with it, so a client can quote it and a developer can trace it | 2026-10-12 |
| Kind is an enum | `kind:` takes a `FailureKind` value written `FailureKind.not_found`, never a string (area 2 setting-options rule, revised 2026-10-12) | 2026-10-12 |
| Failure placement | One failure per file in the domain's `failures/` folder, or `shared/failures/` once two or more domains raise it; `public` when another domain handles it | 2026-10-12 |

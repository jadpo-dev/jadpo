---
title: Failures
area: 4
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Failures

How a Jadpo program says that something went wrong: declaring failures, the
kinds that decide how each one reaches a client, listing, raising, passing on
and mapping failures, the faults the runtime raises by itself, and what is
logged.

The rule behind all of it: **application code says
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

A failure is written like every other type,
`type TodoNotFound = Failure { ... }`, so code reads the same everywhere
(question 13). `Failure` as a base brings `kind`, `message`, `client` and `internal`, as
`Input` brings optional fields.

## Kinds

A kind is a `FailureKind` option, written `FailureKind.not_found` (area 2,
question 17). Declaring a kind only the runtime raises is
`failure.kind_runtime_only` (question 3).

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
`fails` comes before `->` (area 3), and faults are never listed
(question 5).

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

## Passing a failure on: `attempt`

`attempt` starts a statement, only on a call that can fail, and passes
every failure of that call on to the caller's `fails` list (area 3).

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
use ordering/entities { Order }

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
use inventory/types { Reservation }
use inventory/failures { QuantityUnavailable }
use ordering/entities { Order }

public action reserve_for_order(order: Order) fails QuantityUnavailable -> Reservation {
    reject QuantityUnavailable
}
```

```jadpo check project=reorder file=domains/ordering/actions/reorder.jadpo migrate=initial
use inventory/types { Reservation }
use inventory/failures { QuantityUnavailable }
use ordering/entities { Order }
use ordering/failures { OrderNotFound }
use inventory

action reorder(order_id: Order.id) fails OrderNotFound, QuantityUnavailable -> Reservation {
    var order: Order = attempt Order.get(order_id)  // can fail with OrderNotFound
    return attempt inventory.reserve_for_order(order)  // can fail with QuantityUnavailable
}
```

A runtime construction can fail, and its `attempt` maps
each rule that can break in a block after the call,
`attempt Type(value) { max: SomeFailure }`, with `invalid:` covering every
rule without an arm of its own (area 2, "Construction failures by rule").
When the declared ranges, or the conditions it sits under (area 2,
"Conditions as proof"), prove a construction cannot break any of the
target's rules, it is not fallible, so it takes no `attempt` (question 6):

```jadpo check project=proven file=types.jadpo hidden
type Stock = Int {
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
function stock_needed(per_pack: Stock, packs: Quantity) -> Stock {
    var total: Stock = Stock(per_pack * packs)  // both at least 0, Stock has no maximum
    return total
}

function doubled(packs: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(packs * 2) {  // at least 2, but can exceed 1000
        max: QuantityTooLarge
    }
    return result
}
```

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

A failure mapped from `json.read`'s `mismatch` that declares
`problem: DecodeProblem` receives the first problem in that field, so the
arm gives every other field and never `problem`; nothing else fills one
(area 19).

The failure that was mapped is kept as the internal cause of the new one, so
the log shows both (question 8):

```text
PaymentDeclined rejected at domains/checkout/actions/pay.jadpo:14
  caused by CardDeclined from payments.charge (internal: decline_code)
  propagated through route POST /checkout
```

## Handling a failure in `match`

One `match` handles a call's outcome (area 3). Every failure the
call can produce needs an arm, with no `_`; an arm recovers with a value,
maps with `reject` or passes the failure on with `propagate`. The arms
handle only the call itself: a failure that leaves while its arguments are
worked out leaves the callable around the `match`, as it would anywhere
else.

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

## Messages and languages

A failure's `message` is fixed English for developers and logs. Clients
show text by `code`, which is stable (question 12):

```json
{ "error": { "code": "todo_not_found", "message": "Todo not found.", "request_id": "req_0b1e…" } }
```

The text for people is written in the app, keyed by the failure's code in
its domain's language files (area 10, Translations), with the failure's
`client: { }` fields as placeholders; `kind:` and `message:` are not
translated. The wording is never written on the failure type, so all text
for people follows one rule. Every failure that can reach a response (the compiler knows
each route's failures) has wording in every supported locale; one that
never leaves the server needs none. Client fields that are dates or
numbers are sent as values (an `Instant` stays an instant on the wire) and
the client formats them for its own language before filling the
placeholder. The compiler writes, beside each API's OpenAPI document, one
file per locale named `<api>.failures.<locale>.json`, mapping each code to
its wording in the same `{field}` syntax and saying which placeholders need
formatting and how (date, time or number), for client apps to show:

```text
// domains/planning/translations/fr_fr.jadpo
translations {
    todo_not_found: "Tâche introuvable."
}
```

A library package's failure is worded by the package, and the app can
reword it in its root language files; to show it under a different code,
not just different words, map it to the app's own failure at the call with
`attempt` (area 10).

## What is built

| Feature | Status |
|---|---|
| Declaring failures, kinds and codes | Built, with the checks in Errors above. |
| `fails`, `reject`, `attempt` and handling in `match` | Built. |
| Mapping | Built: a construction's rule arms and `invalid:`, proof that a construction cannot break, a query's `missing:` and writes' `conflict:` and `empty:`, each with its kind check. |
| Faults from code | Built: `Int` and `Decimal` overflow and the recursion limit are faults at their site, and a failure keeps the path it travelled and the failure it was mapped from. |
| What the client sees | Built: the envelope, the validation details and the OpenAPI responses. |
| Concealing records | Built: a record policy hides reports the code's own not-found failure, and its log says `concealed_by: policy` (area 9). |
| Logging | Not built yet. |

## Errors

Every error this area can produce, with an example that triggers it and the
compiler's actual output.

- [`syntax.fails_after_result`](../errors/syntax.fails_after_result.md)
- [`failure.kind_required`](../errors/failure.kind_required.md)
- [`failure.kind_runtime_only`](../errors/failure.kind_runtime_only.md)
- [`failure.code_format`](../errors/failure.code_format.md)
- [`failure.duplicate_code`](../errors/failure.duplicate_code.md)
- [`failure.context_field_overlap`](../errors/failure.context_field_overlap.md)
- [`failure.missing_context_field`](../errors/failure.missing_context_field.md)
- [`failure.unknown_context_field`](../errors/failure.unknown_context_field.md)
- [`failure.duplicate_context_field`](../errors/failure.duplicate_context_field.md)
- [`failure.undeclared_propagation`](../errors/failure.undeclared_propagation.md)
- [`failure.stale_declaration`](../errors/failure.stale_declaration.md)
- [`failure.duplicate_declaration`](../errors/failure.duplicate_declaration.md)
- [`failure.rule_unmapped`](../errors/failure.rule_unmapped.md)
- [`failure.rule_cannot_break`](../errors/failure.rule_cannot_break.md)
- [`failure.invalid_covers_nothing`](../errors/failure.invalid_covers_nothing.md)
- [`failure.invalid_outside_construction`](../errors/failure.invalid_outside_construction.md)
- [`failure.duplicate_mapping`](../errors/failure.duplicate_mapping.md)
- [`failure.mapping_kind`](../errors/failure.mapping_kind.md)
- [`failure.unknown_mapping`](../errors/failure.unknown_mapping.md)
- [`syntax.mapping_block_required`](../errors/syntax.mapping_block_required.md)

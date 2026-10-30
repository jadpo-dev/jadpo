---
title: Failures
area: 4
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Failures
<!-- topic: failures -->

Your code says what went wrong, by name, and Jadpo decides how each
failure reaches whoever called: the HTTP status, the response body and
what is logged. Code never throws, never picks an HTTP status and never
builds an error response.

Three kinds of thing can go wrong, and only the first is yours to
declare:

- A **domain failure** is an outcome your app expects, such as a todo
  that doesn't exist. You declare it and say where it can happen.
- A **fault** is a problem outside your code: the database is down, a
  provider timed out, or a write may or may not have been saved. The
  runtime raises it.
- A **defect** is a bug: a number overflowed, the recursion limit was
  reached, or the compiler got something wrong. The runtime raises it
  too.

| | Declared by | Listed in `fails` | The client sees | The log gets |
|---|---|---|---|---|
| **Domain failure**, such as `TodoNotFound` | you, as `type X = Failure` | yes | its code, message and client fields | its code, where it was raised, the path it travelled and its internal fields |
| **Fault**, such as the database being down | the runtime | no | a fixed code for that fault | the cause chain |
| **Defect**, such as an overflow | the runtime | no | `internal_fault` | the cause and stack |

## Declaring a failure
<!-- topic: declaring-a-failure -->

A failure is declared as a type, like any other, on the base `Failure`:
`type TodoNotFound = Failure { ... }`. As `Input` brings optional fields,
`Failure` brings four settings: `kind`, `message`, `client` and
`internal`.

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

### `kind:`, which sets the status

`kind:` names one of the [kinds](#kinds). The kind sets the HTTP status
and how background work treats the failure. A failure never states a
status itself.

### The code clients see

A failure's public code is its name in lower_snake_case: `TodoNotFound`
is `todo_not_found`. Clients rely on it, so it stays the same. Write
`code: "todo_missing"` only to keep a code clients already use after you
rename the failure.

### `message:`

`message:` is fixed text that is safe to show anyone. It can't include
values. Leave it out and the kind's generic message is used.

### `client:` and `internal:` fields

A failure's fields, its context, go in two blocks:

- `client: { }` holds the fields sent to the caller, in the response's
  `details`.
- `internal: { }` holds the fields that never leave the server.

A field name can't be in both. Fields exist only inside these two blocks,
so leaving `client` out means the caller gets only the code, the message
and the request id.

Context fields are ordinary fields. A `T?` type, which may hold no
value, is allowed. `optional` and `default` are not, because only `Input`
types may have them ([One form for every type](02-values-and-types.md#one-form-for-every-type)).

### Where the file goes

Each failure has its own file in its domain's `failures/` folder, such
as `domains/planning/failures/todo_not_found.jadpo`. A domain is one
business area of the app, a folder under `domains/`
([Project layout](01-project-layout-and-modules.md#project-layout)). Once
two or more domains raise the same failure, its file moves to
`shared/failures/`. Mark it `public` when another domain handles it.

## Kinds
<!-- topic: kinds -->

Every failure has a kind, which decides the HTTP status a client gets and
what the failure means. A kind is a `FailureKind` value, written
`FailureKind.not_found` ([Enums](02-values-and-types.md#enums)).

You can declare seven of the kinds. The runtime raises the other eight
itself, and declaring one of those is the error
`failure.kind_runtime_only`.

| Kind | HTTP | Meaning | Raised by |
|---|---:|---|---|
| `invalid_request` | 400 | The request could not be read (bad JSON, wrong shape) | runtime |
| `invalid_value` | 422 | A well-formed value breaks a rule | you; the runtime for request input |
| `unauthenticated` | 401 | No valid identity | runtime ([Identity](08-identity.md)) |
| `not_permitted` | 403 | The caller is known but may not do this | you; [policy](09-policy.md) |
| `not_visible` | 404 | Hidden from this caller | policy only ([Concealing records](#concealing-records)) |
| `not_found` | 404 | The record does not exist | you |
| `conflict` | 409 | The current state conflicts with the request | you |
| `precondition_failed` | 412 | An HTTP precondition such as `If-Match` failed | runtime ([HTTP](07-http.md)) |
| `rejected` | 422 | A business rule refused the operation | you |
| `rate_limited` | 429 | Wait before retrying | you; runtime |
| `unavailable` | 503 | A dependency is temporarily unavailable | you; runtime |
| `timed_out` | 504 | A dependency or deadline ran out of time | runtime |
| `outcome_unknown` | 500 | The change may or may not have happened, so retrying blindly is unsafe | runtime |
| `misconfigured` | 500 | Deployment configuration is wrong | runtime |
| `internal_fault` | 500 | A defect | runtime |

Here `CheckoutUnavailable` is fine, because a checkout that is briefly
unavailable is a business outcome callers handle. `Broken` is refused,
because only the runtime raises `internal_fault`:

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
  help: docs/errors/failure.kind_runtime_only.md

check failed: 1 error, 1 file
```

`invalid_value` and `rejected` both use 422, and 400 is kept for requests
that can't be read at all. An application can't add a kind or choose a
status. If no kind fits, that is a change to the language, not to your
app.

## What the client sees
<!-- topic: what-the-client-sees -->

Every failure reaches an HTTP client in the same envelope. The failure's
client fields appear under `details`:

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

Internal fields, stacks, SQL, provider text and source locations never
appear.

Request input that breaks a rule is reported with the field and the rule
it broke, never the value
([How failures reach the client](07-http.md#how-failures-reach-the-client)):

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

The generated OpenAPI lists every failure a route can reach, under its
status. `build/compatibility/public-failure-codes.json` records each
public code with its status, message and client fields. Changing any of
them is a breaking change.

## Saying what can fail: `fails`
<!-- topic: saying-what-can-fail-fails -->

A function or action lists the failures it can end with after `fails`, in
its signature:

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

`fails` comes before the result type's `->`
([Signatures and settings](03-logic.md#signatures-and-settings)). It
lists exactly the failures that can leave the callable: everything a
`reject` or an `attempt` in its body can produce, less what a `match`
handles. Functions and actions follow the same rule.

- A failure that can leave but isn't listed is
  `failure.undeclared_propagation`.
- One that is listed but can never happen is `failure.stale_declaration`.
- One listed twice is `failure.duplicate_declaration`.

[Faults](#faults) are never listed.

## Raising a failure: `reject`
<!-- topic: raising-a-failure-reject -->

`reject` ends the callable with a declared failure:

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

The block after the failure's name gives every client and internal field
at once. A failure with no fields is rejected by its name alone. A
missing, unknown or repeated field is an error.

There is no `reject "text"` and no `throw`: every failure has a declared
type. `reject` can also be the result of a `match` arm
([Match](03-logic.md#match)).

Two things can never go in a failure's fields: a secret setting
(`configuration.secret_flow`, see [Secrets](10-configuration-and-time.md#secrets)),
and the signed-in caller
([Who is calling](08-identity.md#who-is-calling-current_principal)).

## Passing a failure on: `attempt`
<!-- topic: passing-a-failure-on-attempt -->

`attempt` passes every failure of a call on to your caller, so each one
must be in your own `fails` list. It starts a statement, and it works
only on a call that can fail ([Attempt](03-logic.md#attempt)).

Here `reorder` reads an order, which can fail with `OrderNotFound`, and
reserves stock for it in another domain, which can fail with
`QuantityUnavailable`. It lists both:

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

### Building a value at run time

Building a named type from a value known only at run time, such as
`Quantity(packs * 2)`, can break the type's rules. Its `attempt` maps each
rule that can break to a failure, in a block after the call:
`attempt Type(value) { max: SomeFailure }`. `invalid:` covers every rule
without an arm of its own
([Construction failures by rule](02-values-and-types.md#construction-failures-by-rule)).

### When a value can't break its type

When the declared ranges prove a construction can't break any of the
target type's rules, it can't fail, so it takes no `attempt`. The `if`
conditions it sits under count as proof too
([Conditions as proof](02-values-and-types.md#conditions-as-proof)).

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

In `stock_needed`, both values are at least 0 and `Stock` has no maximum,
so the result is always a valid `Stock`. In `doubled`, `packs * 2` is at
least 2 but can pass 1000, `Quantity`'s maximum, so it needs `attempt`
and an arm for `max`.

## Turning one failure into another
<!-- topic: turning-one-failure-into-another -->

A failure can come from something the compiler knows can go wrong, such
as a read that finds nothing, a write that breaks a unique rule or a
construction, or from another call. You turn it into your own failure
with `condition: Failure` in the operation's block. The failure's kind
must fit the condition:

| Condition | Required kind |
|---|---|
| `missing:` | `not_found` |
| `conflict:` | `conflict` |
| `empty:` (a patch that changes nothing) | `invalid_value` |
| A construction's rule keys, such as `max:`, and `invalid:` covering the rest | `invalid_value` |

### A read that finds nothing: `missing:`

A query names the failure for a record that isn't there
([Named queries](06-reads-and-writes.md#named-queries)):

```jadpo fragment
query todo_by_id(todo_id: Todo.id) fails TodoNotFound -> TodoView {
    from: Todo
    where: Todo.id == todo_id
    missing: TodoNotFound {
        todo_id: todo_id
    }
}
```

### A value built at run time: rule keys and `invalid:`

A construction is keyed by the rule that can break
([Construction failures by rule](02-values-and-types.md#construction-failures-by-rule)):

```jadpo fragment
var quantity: Quantity = attempt Quantity(count) {
    max: QuantityTooLarge
    invalid: QuantityOutOfRange
}
```

`invalid:` is the catch-all only for a construction's rules. A read's, a
write's or an integration call's conditions each need their own arm, and
`invalid:` in their block is the error
`failure.invalid_outside_construction`.

### A write that breaks a unique rule: `conflict:`

A write names a failure for each unique field it could clash on
([When a write is refused](06-reads-and-writes.md#when-a-write-is-refused)):

```jadpo fragment
var account: Account = attempt Account.create {
    set: {
        ...
    }
    conflict: {
        Account.handle: HandleTaken
    }
}
```

### A failure from a call: `match`

A `match` on the call gives each of its failures an arm
([Handling a failure in `match`](#handling-a-failure-in-match)):

```jadpo fragment
var receipt: Receipt = match payments.charge(order) {
    success(receipt) -> receipt
    CardDeclined -> reject PaymentDeclined
}
```

### Decoding JSON

`json.read` fails with `mismatch` when the JSON doesn't fit the type
([`json`](19-standard-library.md#json)). When the failure you map
`mismatch` to declares the field `problem: DecodeProblem`, Jadpo fills it
with the first problem found. So the arm gives every other field of the
failure, and never `problem`. No other mapping fills a `DecodeProblem`
field.

### The original failure is kept

The failure that was mapped is kept as the internal cause of the new one,
so the log shows both:

```text
PaymentDeclined rejected at domains/checkout/actions/pay.jadpo:14
  caused by CardDeclined from payments.charge (internal: decline_code)
  propagated through route POST /checkout
```

## Handling a failure in `match`
<!-- topic: handling-a-failure-in-match -->

A `match` on a call handles its outcome: one arm for success, and one for
each failure the call can produce ([Match](03-logic.md#match)):

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

Every failure needs an arm, and there is no `_` for the rest. An arm can
recover with a value, as `LookupMissing` does here, map to another
failure with `reject`, or pass the failure on with `propagate`.

The arms handle only the call itself. A failure that leaves while the
call's arguments are worked out leaves the callable around the `match`, as
it would anywhere else.

The `match` errors, the `failure.outcome_*` rules, are listed on
[Logic](03-logic.md#errors).

## Faults
<!-- topic: faults -->

A fault is raised by the runtime when something outside your code goes
wrong. Faults are never declared, listed in `fails`, rejected or matched.
A fault goes straight to the edge of the app, where the client gets a
fixed code and the cause is logged:

| Condition | The client sees |
|---|---|
| The request body is not valid JSON | 400 `invalid_request` |
| Request input breaks a rule | 422 `invalid_input` |
| The route's deadline passed | 504 `deadline_exceeded` |
| Storage failed before anything was committed | 503 `transaction_unavailable` or `read_unavailable` |
| A write may have committed | 500 `outcome_unknown` |
| `Int` overflow, the recursion limit, any defect | 500 `internal_fault` |

### Reacting to a provider's problem

Code that must react to a dependency's problem gets a domain failure from
the place the problem arises. An integration maps the provider's
conditions to failures you declare ([Calling](11-integrations.md#calling)):

```jadpo fragment
failures: {
    provider.invalid_recipient: ReminderRejected
    provider.rate_limited: ReminderDelayed
}
```

The action then lists only those domain failures, never `unavailable` or
`outcome_unknown`:

```jadpo fragment
action send_reminder(todo: Todo) fails ReminderRejected, ReminderDelayed -> Receipt {
    return attempt mail.send_reminder(todo)
}
```

### Defects

A runtime fault that comes from your code is a defect. `Int` overflow and
the recursion limit ([Recursion](03-logic.md#recursion)) end the operation
with `internal_fault`, undo its writes, and log which one happened:

```json
{
  "event": "operation.failed",
  "classification": "int_overflow",
  "operation": "action:price_line",
  "site": "domains/ordering/actions/price_line.jadpo:4",
  "request_id": "req_0b1e…"
}
```

Division by zero can't happen, because a divisor's type must exclude zero
([Expressions and operators](03-logic.md#expressions-and-operators)). A
construction that fails at run time never reaches the client as a fault
either, because it needs an `attempt`.

## Concealing records
<!-- topic: concealing-records -->

When [policy](09-policy.md) hides a record from a caller, saying
"forbidden" would reveal that it exists. So a hidden record fails with the
same `not_found` failure your code already maps, and the client can't
tell hidden from absent.

Say policy lets only a todo's owner read it. Then this read of someone
else's todo fails with `TodoNotFound`, exactly like a todo that doesn't
exist:

```jadpo fragment
var todo: Todo = attempt Todo.get(todo_id)
```

The log adds `concealed_by: policy`, so you can still tell the two apart
([When policy says no](09-policy.md#when-policy-says-no)).

## Logging
<!-- topic: logging -->

Every failure and fault writes one structured event. A domain failure's
event holds its code, kind, where it was raised, the path it travelled and
its internal fields. No stack is captured for a domain failure.

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

Internal values made of text, which can hold names or email addresses,
are logged as a keyed hash, so equal values still match up across events.

Logging isn't built yet ([What is built](#what-is-built)).

## Messages and languages
<!-- topic: messages-and-languages -->

A failure's `message` is fixed English for developers and logs. Clients
show text by `code`, which is stable:

```json
{ "error": { "code": "todo_not_found", "message": "Todo not found.", "request_id": "req_0b1e…" } }
```

The text people see is written in the app, keyed by the failure's code,
in its domain's language files
([Translations as messages](10-configuration-and-time.md#translations-as-messages)).
The failure's `client: { }` fields are its placeholders. `kind:` and
`message:` aren't translated, and the wording is never written on the
failure type, so all text for people follows one rule.

In `domains/planning/translations/fr_fr.jadpo`:

```text
translations {
    todo_not_found: "Tâche introuvable."
}
```

Every failure that can reach a response has wording in every supported
locale. The compiler knows each route's failures, so it checks this. A
failure that never leaves the server needs none.

Client fields that are dates or numbers are sent as values: an `Instant`
stays an instant on the wire. The client formats them for its own
language before filling the placeholder.

Beside each API's OpenAPI document, the compiler writes one file per
locale, `<api>.failures.<locale>.json`, for client apps to show. It maps
each code to its wording, in the same `{field}` syntax, and says which
placeholders need formatting and how: as a date, a time or a number
([Failures for client apps](10-configuration-and-time.md#failures-for-client-apps)).

A library package's failure is worded by the package, and the app can
reword it in its root language files. To show it under a different code,
not just in different words, map it to the app's own failure at the call
with `attempt`.

## What is built
<!-- topic: failures-built -->

| Feature | Status |
|---|---|
| Declaring failures, kinds and codes | Built, with the checks under [Errors](#errors). |
| `fails`, `reject`, `attempt` and handling in `match` | Built. |
| Mapping | Built: a construction's rule arms and `invalid:`, proof that a construction cannot break, a query's `missing:` and writes' `conflict:` and `empty:`, each with its kind check. |
| Faults from code | Built: `Int` and `Decimal` overflow and the recursion limit are faults at their site, and a failure keeps the path it travelled and the failure it was mapped from. |
| What the client sees | Built: the envelope, the validation details and the OpenAPI responses. |
| Concealing records | Built: a record [policy](09-policy.md) hides reports the code's own not-found failure, and its log says `concealed_by: policy`. |
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

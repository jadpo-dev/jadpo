---
title: Logic
area: 3
status: decided
checked-with: this checkout's jadpo on every gate run (archived examples: v0 3ce094d0)
---

# Logic

How behaviour is written: functions and actions, variables, statements,
operators, `match`, `attempt`, loops, and what the language says about passing
functions, recursion and generic code.

> Decided on 2026-10-12. Code on this page applies the decisions from areas 1 to 3. Built in the new compiler in roadmap chunk 1.5, with the failure side of `attempt` and `match` in chunk 1.6 and the runtime recursion limit in chunk 2.1.

## Functions and actions

There are three kinds of callable. A **function** is pure: it computes a
result from its arguments and nothing else. An **action** is an application
operation that may read and write data, call integrations and call other
actions. A **query** is a read-only operation, described in area 6.

```jadpo check project=line-total file=numbers.jadpo hidden
type Pence = Int {
    min: 0
}

type Quantity = Int {
    min: 1
    max: 1000
}
```

```jadpo check project=line-total
/// Total for one order line before discounts.
function line_total(price: Pence, quantity: Quantity) -> Pence {
    var total: Pence = Pence(price * quantity)
    return total
}
```

```jadpo fragment
action complete_todo(todo_id: Todo.id) fails TodoNotFound -> TodoView {
    var todo: Todo = attempt Todo.get(todo_id)
    var done: Todo = Todo.update(todo) {
        set: {
            status: TodoStatus.done
        }
    }
    return TodoView(done)
}
```

| Caller | May call | May not |
|---|---|---|
| `function` | functions, library functions, the standard library | actions, queries, persistence, integrations, `clock.now` |
| `query` | functions and queries | actions, writes |
| `action` | functions, queries and actions | |
| Route `run:` | one action | a function or query directly (area 7) |

A function may still fail: it can declare `fails` and `reject` (area 4).
There is no `async`, `await` or promise: every call completes before the
caller continues.

### Signatures and settings

Parameters are positional and every one has a type; the result type follows
`->`, and `fails` comes before it. A callable that returns nothing returns
`Unit`. Named arguments exist only for standard-library operations
(`temporal.resolve(date: d, zone: z, ...)`); your own callables are called
positionally. Parameters have no defaults.

Decided (question 15): the whole signature is on the header line and an
action has no settings, because the safe choice is the default: every action
is all-or-nothing, writes check for concurrent change, and reads are current.
Who may call it is policy (area 9).

```jadpo fragment
action reschedule(todo_id: Todo.id, due_at: DueAt) fails TodoNotFound, DueDateInPast -> TodoView {
    var todo: Todo = attempt Todo.get(todo_id)
    ...
}
```

When a header is too long the formatter puts one parameter per line and keeps
`fails` and `->` after the closing parenthesis.

### Calling

Decided in area 1: the dot names the owner of what you call. Your own
domain's actions and queries, and a helper function used only in its own
file, are called bare. A helper shared by several files lives in a `library`
and is called through it like the standard library, and another domain's
public actions are called through that domain.

```jadpo fragment
library rounding {
    function half_even(amount: Price) -> Price { ... }
}

var total: Price = rounding.half_even(subtotal)                    // a library
var line: Pence = line_total(price, quantity)                      // a helper in this file
var stock: Reservation = attempt inventory.reserve_for_cart(cart)  // another domain
var todo: Todo = attempt Todo.get(todo_id)                         // an entity
var local: ZonedTime = temporal.in_zone(at, zone)                  // the standard library
```

A library holds only functions.

## Variables

> Decided: `var` stays, immutable by default, with `var mut` when it can change (question 2), and every variable states its type (area 2).

```jadpo fragment
var subtotal: Pence = line_total(price, quantity)
var mut total: Pence = subtotal
total = Pence(total + delivery)
```

- `var` names a value that never changes. `var mut` names one that can be
   pointed at a new value of the same type with `name = value`.
- Only a `var mut` local can be assigned. Parameters, fields and
   declarations cannot (area 2: values never change).
- A variable is visible from its declaration to the end of its block.
- A name already in scope, including a parameter, cannot be declared again
   (decided, question 3), and an unused variable is an error (question 4).

## Statements

A body is a list of statements, one per line, with no semicolons:

| Statement | Form |
|---|---|
| Variable | `var name: T = value`, `var mut name: T = value` |
| Assignment | `name = value` |
| Return | `return value` |
| Rejection | `reject Failure { ... }` (area 4) |
| Condition | `if (condition) { } else if (condition) { } else { }` |
| Match | `match value { ... }` |
| Loop | `for item in items { }` (decided in area 2) |
| Test only | `assert`, `advance clock by` (area 13) |

There is no `throw`. Decided (question 4): three more statements may stand
alone, so a result nobody needs never has to be bound to a name:

```jadpo check project=archive file=todos.jadpo hidden
type TodoStatus = Enum {
    open
    archived
}

entity Todo {
    id: Uuid
    status: TodoStatus
}

type TodoNotFound = Failure {
    kind: FailureKind.not_found
}

type Archived = Bool {}

function already_archived(todo_id: Todo.id) -> Archived {
    return false
}

action notify_owner(todo_id: Todo.id) -> Unit {}
```

```jadpo check project=archive
action archive_todo(todo_id: Todo.id) fails TodoNotFound -> Unit {
    attempt Todo.get(todo_id)  // fails or carries on; the value is not needed
    if (already_archived(todo_id)) {
        return  // leaves a Unit action early
    }
    notify_owner(todo_id)  // a Unit action called for its effect
}
```

### Conditions

```jadpo check project=due-dates file=types.jadpo hidden
type DueAt = Instant {}

type DueDateInPast = Failure {
    kind: FailureKind.invalid_value
}

type DueDateTooFar = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=due-dates
action check_due(due_at: DueAt, latest_due_at: DueAt) fails DueDateInPast, DueDateTooFar -> Unit {
    if (due_at <= clock.now) {
        reject DueDateInPast
    } else if (due_at > latest_due_at) {
        reject DueDateTooFar
    }
}
```

The condition must be a `Bool` (or a type built on `Bool`), and there is no
truthiness. `if` is a statement; a choice between two values is a `match`.
Decided (question 5): conditions are always in brackets, and `else if` chains are allowed.

## Expressions and operators

| Precedence | Operators | Operands | Result |
|---|---|---|---|
| 1 (tightest) | `not`, unary `-` | `Bool`; a number | same |
| 2 | `*`, `/`, `%` | `*` and `%` may mix named types; `/` needs the same named type: `Int` drops the remainder, `Decimal` is exact (question 7) | `Int`, or `Decimal` if either side is `Decimal` |
| 3 | `+`, `-` | the same named type after widening, or a literal; `+` also joins two `Text` values | as above |
| 4 | `<`, `<=`, `>`, `>=` | two numbers or two time values of the same named type; text has no order (question 6) | `Bool` |
| 5 | `==`, `!=` | two values of the same type after widening, or a nullable value and `none` | `Bool` |
| 6 | `and` | `Bool` | `Bool` |
| 7 (loosest) | `or` | `Bool` | `Bool` |

In `+`, `-` and `/` a literal takes the other side's type exactly as it does
in `==`, so `price + 3` with a `Decimal` price is an error as `price == 3`
is (write `3.00`), and `+` joins two `Text` values only when they are the
same named type after widening, or one is a literal, as `==` compares them
(chunk 1.9, reading the table above conservatively). `*` and `%` still mix.

Operators work on the representation, so their result is a plain `Int`,
`Decimal`, `Text` or `Bool`, and turning it back into a named type is a
constructor (area 2). Parentheses group. `and` and `or` stop as soon as the
answer is known. `a < b < c` is an error, because `a < b` is a `Bool`.
Logical operators are words; `!`, `&&` and `||` are not Jadpo, and each is
one error whose edit writes the word (`syntax.logical_operator`). Nor are
`===` and `!==`: `==` and `!=` already compare values of one type, so each
is one error whose edit writes `==` or `!=` (`syntax.equality_operator`,
chunk 3.5), as are SQL's `<>`, `=` and `IS NULL` (chunk 3.8, [Habits from
other languages](#habits-from-other-languages)).

Questions 6 and 7 decided operator typing and division:

```jadpo fragment
var wrong: Pence = Pence(price + quantity)       // error: adds a Quantity to Pence
var line: Pence = Pence(price * quantity)        // fine: multiplying different units
var earlier: Bool = name < other_name             // error: text has no order; use a standard function
var share: Price = decimal.divide(total, people, rounding: Rounding.half_even)
var boxes: BoxCount = integer.divide(items, box_size, rounding: Rounding.up)   // box_size can't be zero
```

## Match

> Decided in area 2 and carried here: match arms use `->`, the same arrow as result types; `=>` is removed.

`match` chooses between the cases of a value. It must cover every case, and it
can be used as a statement or as an expression.

```jadpo check project=matching file=shapes.jadpo hidden
type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
    declined
}

type ProductName = Text {
    min_length: 1
    max_length: 120
}

type Listing = Object {
    name: ProductName
    subtitle: ProductName?
}

type TodoStatus = Enum {
    open
    done
}

entity Todo {
    id: Uuid
    status: TodoStatus
}

action notify_owner(todo_id: Todo.id) -> Unit {}
```

```jadpo check project=matching
function receipt(outcome: PaymentOutcome) -> ReceiptId? {
    return match outcome {
        PaymentOutcome.pending -> none
        PaymentOutcome.paid(paid) -> paid.receipt_id
        PaymentOutcome.declined -> none
    }
}

function headline(listing: Listing) -> ProductName {
    return match listing.subtitle {
        some(subtitle) -> subtitle
        none -> listing.name
    }
}

action follow_up(todo: Todo) -> Unit {
    match todo.status {
        TodoStatus.open -> {
            notify_owner(todo.id)
        }
        TodoStatus.done -> {}
    }
}
```

A `match` on a call's outcome names its success value and each failure
(area 4):

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

| Pattern | Matches |
|---|---|
| `Enum.variant` | a plain variant, or a variant whose fields this arm does not read |
| `Enum.variant(name)` | a variant with fields, binding them as `name` (question 9); `name.field` has the field type `Enum.variant.field` |
| `some(name)`, `none` | a nullable value |
| `true`, `false`, `"text"`, `42` | a literal |
| `_` | anything else; only where the cases cannot be listed, such as text and numbers (question 10) |
| `success(name)`, `FailureName` | the result of a call that can fail: the value, or one of its failures |

An arm is `pattern -> expression`, `pattern -> reject F`, `pattern -> propagate`
or `pattern -> { statements }`. A failure arm can recover with a value, map to
another failure with `reject`, or pass the failure on with `propagate`; every
failure the call can produce needs an arm (area 4 covers the failure side).

## Attempt

A call that can fail must say so at the call site. `attempt` passes the
call's failures on to the caller, whose `fails` list must include them;
`match` on the call handles them instead.

```jadpo fragment
var todo: Todo = attempt Todo.get(todo_id)            // a variable
return attempt todo_by_id(todo_id)                    // a return
attempt Todo.get(todo_id)                             // on its own (question 4)
var total: Pence = Pence(attempt price(a) + attempt price(b))   // rejected: inside an expression
```

Decided (question 11): `attempt` only starts a statement, so every point
where a callable can stop early is at the start of a line, and `attempt` on
a call that cannot fail is an error. `reject` and `propagate` follow the same
rule: each starts a statement or is the result of a `match` arm, never part
of a value (`failure.reject_placement`, `failure.propagate_placement`; chunk
1.9).

## Loops

Decided in area 2: `for item in items` iterates a list, and the loop variable
takes its type from the list.

```jadpo check project=order-total file=lines.jadpo hidden
type Pence = Int {
    min: 0
}

type OrderLine = Object {
    subtotal: Pence
}
```

```jadpo check project=order-total
function order_total(lines: List<OrderLine>) -> Pence {
    var mut total: Pence = 0
    for line in lines {
        total = Pence(total + line.subtotal)
    }
    return total
}
```

Decided (question 12): `for` over a list is the only loop: no `while`,
`break` or `continue`, with `return` allowed inside the body.

## Passing functions

Area 2 decided on `collection.map` and `collection.filter`, which need a
function as an argument. Decided (question 13): inline functions are allowed only as
arguments to standard-library collection operations, using `->` for
"produces":

```jadpo fragment
var open: List<Todo> = collection.filter(todos, todo -> todo.status == TodoStatus.open)
var titles: List<TodoTitle> = collection.map(todos, todo -> todo.title)
```

The inline function is one expression, takes its parameter's type from the
list, may read the variables around it (safe because values never change),
and cannot call actions or use `attempt`. Functions are not values: they
cannot be stored in variables or passed to your own callables.

## Recursion

A function may call itself or form a cycle with other functions. Recursive
types are accepted (`replies: List<Comment>`), and recursion is the only way
to walk them:

```jadpo check project=replies file=comments.jadpo hidden
type CommentBody = Text {
    max_length: 2000
}

type Comment = Object {
    body: CommentBody
    replies: List<Comment>
}

type ReplyTotal = Int {
    min: 0
}
```

```jadpo check project=replies
function reply_count(comment: Comment) -> ReplyTotal {
    var mut count: ReplyTotal = 0
    for reply in comment.replies {
        count = ReplyTotal(count + 1 + reply_count(reply))
    }
    return count
}
```

Decided (question 14): recursion is allowed in functions only, with a runtime
depth limit that ends in a defined fault.

## Generic code

Only the standard library is generic (`List<T>`, `collection.count`). Your own
callables take named types, and a parameter that accepts several types is an
enum whose variants carry each one (from area 2):

```jadpo check project=parties file=parties.jadpo hidden
entity Customer {
    id: Uuid
}

entity Supplier {
    id: Uuid
}

type PartyTitle = Text {
    max_length: 80
}

function customer_label(id: Customer.id) -> PartyTitle {
    return "customer"
}

function supplier_label(id: Supplier.id) -> PartyTitle {
    return "supplier"
}
```

```jadpo check project=parties
type PartyId = Enum {
    customer: {
        id: Customer.id
    }
    supplier: {
        id: Supplier.id
    }
}

function party_label(party: PartyId) -> PartyTitle {
    return match party {
        PartyId.customer(customer) -> customer_label(customer.id)
        PartyId.supplier(supplier) -> supplier_label(supplier.id)
    }
}
```

`function first<T>(items: List<T>) -> T` and `Customer.id | Supplier.id` are
not Jadpo: question 16 decided against generics and unions in your own code.

## Errors

Every error this area can produce, with an example that triggers it and the
new compiler's actual output.

### `effect.function_calls_action`

```jadpo check project=e-fn-action expect=effect.function_calls_action
type Note = Text {}

action save(note: Note) -> Note {
    return note
}

function pretend_pure(note: Note) -> Note {
    return save(note)
}
```

```text output
error: A function can't call action `save`
 --> main.jadpo:8:12
  |
8 |     return save(note)
  |            ^^^^ an action
  reason: A function is pure: it computes its result from its arguments alone, so it calls only functions, library functions and the standard library; an action may read and write data.
  next: Make the caller an action, or call `save` from the action that uses this function (agent)
  rule: effect.function_calls_action
  help: docs/language/03-logic.html#effect-function-calls-action

check failed: 1 error, 1 file
```

### `effect.function_persistence`

```jadpo check project=e-fn-persist expect=effect.function_persistence
entity Customer {
    id: Uuid
}

function find_customer(id: Customer.id) -> Customer {
    return attempt Customer.get(id)
}
```

```text output
error: A function can't call `Customer.get`
 --> main.jadpo:6:20
  |
6 |     return attempt Customer.get(id)
  |                    ^^^^^^^^^^^^ reads or writes stored data
  reason: A function is pure: it computes its result from its arguments alone, so reading and writing entities belongs in an action.
  next: Make the caller an action, or pass the data in as an argument (agent)
  rule: effect.function_persistence
  help: docs/language/03-logic.html#effect-function-persistence

check failed: 1 error, 1 file
```

### `effect.recursive_action`

Only functions may call themselves; an action in a call cycle is reported at the call that closes it:

```jadpo check project=e-recursive-action expect=effect.recursive_action
type Count = Int {
    min: 0
}

action ping(count: Count) -> Unit {
    pong(count)
}

action pong(count: Count) -> Unit {
    ping(count)
}
```

```text output
error: Action `pong` is in a call cycle: `ping` → `pong` → `ping`
 --> main.jadpo:10:5
   |
10 |     ping(count)
   |     ^^^^ calls back round
  reason: Only functions may call themselves; an action in a call cycle repeats its effects with no data reason, and an unbounded one never finishes.
  next: Move the repeated logic into a function, or loop over a list with `for` (human)
  rule: effect.recursive_action
  help: docs/language/03-logic.html#effect-recursive-action

check failed: 1 error, 1 file
```

### `failure.attempt_required`

```jadpo check project=e-attempt expect=failure.attempt_required
type Note = Text {}

type Refused = Failure {
    kind: FailureKind.rejected
}

action child(note: Note) fails Refused -> Note {
    reject Refused
}

action parent(note: Note) fails Refused -> Note {
    return child(note)
}
```

```text output
error: `child` can fail, and nothing handles it
 --> main.jadpo:12:12
   |
12 |     return child(note)
   |            ^^^^^^^^^^^ can fail with `Refused`
  reason: A call that can fail says so where it is made: `attempt` passes its failures on to the caller's `fails` list, and `match` handles each one.
  next: Start the statement with `attempt`, or `match` on `child(...)` and handle `Refused` (agent)
  rule: failure.attempt_required
  help: docs/language/03-logic.html#failure-attempt-required

check failed: 1 error, 1 file
```

### `failure.attempt_infallible`

```jadpo check project=e-attempt-infallible expect=failure.attempt_infallible
type Count = Int {}

function plain(n: Count) -> Count {
    return n
}

function wrapper(n: Count) -> Count {
    return attempt plain(n)
}
```

```text output
error: `plain(...)` can't fail, so it takes no `attempt`
 --> main.jadpo:8:12
  |
8 |     return attempt plain(n)
  |            ^^^^^^^ nothing to pass on
  reason: `plain(...)` lists no failures after `fails`, so it always gives its result; `attempt` marks only calls that can fail.
  next: Remove `attempt` (agent)
  rule: failure.attempt_infallible
  help: docs/language/03-logic.html#failure-attempt-infallible

check failed: 1 error, 1 file
```

The outcome-match examples below share a hidden file declaring
`lookup(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult`.

### `failure.outcome_missing_arm`

```jadpo check project=e-outcome-missing file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

type LookupRefused = Failure {
    kind: FailureKind.rejected
}

function lookup(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {
    if (key == "missing") {
        reject LookupMissing
    }
    if (key == "refused") {
        reject LookupRefused
    }
    return "found"
}
```

```jadpo check project=e-outcome-missing expect=failure.outcome_missing_arm
function missing_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
    }
}
```

```text output
error: This `match` doesn't handle `LookupRefused`
 --> main.jadpo:2:12
  |
2 |     return match lookup(key) {
  |            ^^^^^^^^^^^^^^^^^ failures not handled
  reason: `lookup(...)` can fail with `LookupMissing` and `LookupRefused`, and a `match` on its outcome has an arm for every one: a value, `reject` or `propagate`.
  next: Add an arm for `LookupRefused` (agent)
  rule: failure.outcome_missing_arm
  help: docs/language/03-logic.html#failure-outcome-missing-arm

check failed: 1 error, 2 files
```

### `failure.outcome_duplicate_arm`

```jadpo check project=e-outcome-dup file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

type LookupRefused = Failure {
    kind: FailureKind.rejected
}

function lookup(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {
    if (key == "missing") {
        reject LookupMissing
    }
    if (key == "refused") {
        reject LookupRefused
    }
    return "found"
}
```

```jadpo check project=e-outcome-dup expect=failure.outcome_duplicate_arm
function duplicate_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupMissing -> "missing again"
        LookupRefused -> "refused"
    }
}
```

```text output
error: `LookupMissing` is handled twice
 --> main.jadpo:5:9
  |
4 |         LookupMissing -> "missing"
  |         ------------- first matched here
5 |         LookupMissing -> "missing again"
  |         ^^^^^^^^^^^^^ matched again
  reason: Each failure has one arm; the first takes every `LookupMissing`, so this one never runs.
  next: Remove this arm, or merge its body into the first (agent)
  rule: failure.outcome_duplicate_arm
  help: docs/language/03-logic.html#failure-outcome-duplicate-arm

check failed: 1 error, 2 files
```

### `failure.outcome_unknown_arm`

```jadpo check project=e-outcome-unknown file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

type LookupRefused = Failure {
    kind: FailureKind.rejected
}

function lookup(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {
    if (key == "missing") {
        reject LookupMissing
    }
    if (key == "refused") {
        reject LookupRefused
    }
    return "found"
}
```

```jadpo check project=e-outcome-unknown expect=failure.outcome_unknown_arm
type LookupTimedOut = Failure {
    kind: FailureKind.rejected
}

function unknown_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
        LookupTimedOut -> "timed out"
    }
}
```

```text output
error: `lookup(...)` can't fail with `LookupTimedOut`
 --> main.jadpo:10:9
   |
10 |         LookupTimedOut -> "timed out"
   |         ^^^^^^^^^^^^^^ never happens here
  reason: `lookup(...)` fails with `LookupMissing` and `LookupRefused`, so an arm for anything else never runs.
  next: Remove this arm (agent)
  rule: failure.outcome_unknown_arm
  help: docs/language/03-logic.html#failure-outcome-unknown-arm

check failed: 1 error, 2 files
```

### `failure.outcome_wildcard`

```jadpo check project=e-outcome-wildcard file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

type LookupRefused = Failure {
    kind: FailureKind.rejected
}

function lookup(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {
    if (key == "missing") {
        reject LookupMissing
    }
    if (key == "refused") {
        reject LookupRefused
    }
    return "found"
}
```

```jadpo check project=e-outcome-wildcard expect=failure.outcome_wildcard
function wildcard_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        _ -> "fallback"
    }
}
```

```text output
error: `_` can't stand for a call's failures
 --> main.jadpo:4:9
  |
4 |         _ -> "fallback"
  |         ^ wildcard
  reason: Every failure a call can give has an arm of its own, so a failure added to the call later is caught at every `match` that has to consider it.
  next: Replace `_` with arms for `LookupMissing` and `LookupRefused` (agent)
  rule: failure.outcome_wildcard
  help: docs/language/03-logic.html#failure-outcome-wildcard

check failed: 1 error, 2 files
```

### `failure.outcome_success_missing`

```jadpo check project=e-outcome-no-success file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

type LookupRefused = Failure {
    kind: FailureKind.rejected
}

function lookup(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {
    if (key == "missing") {
        reject LookupMissing
    }
    if (key == "refused") {
        reject LookupRefused
    }
    return "found"
}
```

```jadpo check project=e-outcome-no-success expect=failure.outcome_success_missing
function missing_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```text output
error: This `match` doesn't handle `lookup(...)` succeeding
 --> main.jadpo:2:12
  |
2 |     return match lookup(key) {
  |            ^^^^^^^^^^^^^^^^^ no `success(...)` arm
  reason: A `match` on a call's outcome handles its value too, in a `success(name)` arm, as well as each failure.
  next: Add `success(value) -> ...` as the first arm (agent)
  rule: failure.outcome_success_missing
  help: docs/language/03-logic.html#failure-outcome-success-missing

check failed: 1 error, 2 files
```

### `failure.outcome_success_duplicate`

```jadpo check project=e-outcome-two-success file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

type LookupRefused = Failure {
    kind: FailureKind.rejected
}

function lookup(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {
    if (key == "missing") {
        reject LookupMissing
    }
    if (key == "refused") {
        reject LookupRefused
    }
    return "found"
}
```

```jadpo check project=e-outcome-two-success expect=failure.outcome_success_duplicate
function duplicate_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        success(other) -> other
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```text output
error: The success arm is written twice
 --> main.jadpo:4:9
  |
3 |         success(value) -> value
  |         -------------- first matched here
4 |         success(other) -> other
  |         ^^^^^^^^^^^^^^ matched again
  reason: A call succeeds one way, so one `success(...)` arm takes its value; this one would never run.
  next: Remove this arm, or merge its body into the first (agent)
  rule: failure.outcome_success_duplicate
  help: docs/language/03-logic.html#failure-outcome-success-duplicate

check failed: 1 error, 2 files
```

### `failure.outcome_success_value_required`

```jadpo check project=e-outcome-success-value file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

type LookupRefused = Failure {
    kind: FailureKind.rejected
}

function lookup(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {
    if (key == "missing") {
        reject LookupMissing
    }
    if (key == "refused") {
        reject LookupRefused
    }
    return "found"
}
```

```jadpo check project=e-outcome-success-value expect=failure.outcome_success_value_required
function invalid_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> propagate
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```text output
error: The success arm has no failure to pass on
 --> main.jadpo:3:9
  |
3 |         success(value) -> propagate
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^ `propagate` on success
  reason: `propagate` passes a failure arm's failure on to the caller; the success arm has a value, which it gives, or a failure it `reject`s.
  next: Give the success value, or `reject` a failure (agent)
  rule: failure.outcome_success_value_required
  help: docs/language/03-logic.html#failure-outcome-success-value-required

check failed: 1 error, 2 files
```

### `failure.outcome_pattern_invalid`

A `match` on a call's outcome has no patterns for values:

```jadpo check project=e-outcome-pattern file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

type LookupRefused = Failure {
    kind: FailureKind.rejected
}

function lookup(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {
    if (key == "missing") {
        reject LookupMissing
    }
    if (key == "refused") {
        reject LookupRefused
    }
    return "found"
}
```

```jadpo check project=e-outcome-pattern expect=failure.outcome_pattern_invalid
function lookup_label(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
        none -> "nothing"
    }
}
```

```text output
error: `none` can't match a call's outcome
 --> main.jadpo:6:9
  |
6 |         none -> "nothing"
  |         ^^^^ a pattern for values
  reason: A `match` on `lookup(...)` has a `success(name)` arm for its value and an arm for each failure it can give; to look at the value's own cases, match on the success value.
  next: Remove this arm, or match on the success value inside `success(...)` (agent)
  rule: failure.outcome_pattern_invalid
  help: docs/language/03-logic.html#failure-outcome-pattern-invalid

check failed: 1 error, 2 files
```

### `failure.outcome_infallible`

```jadpo check project=e-outcome-infallible expect=failure.outcome_infallible
type LookupKey = Text {}

type LookupResult = Text {}

function plain(key: LookupKey) -> LookupResult {
    return "ok"
}

function needless(key: LookupKey) -> LookupResult {
    return match plain(key) {
        success(value) -> value
    }
}
```

```text output
error: `plain(...)` can't fail, so it has no outcome to match
 --> main.jadpo:10:12
   |
10 |     return match plain(key) {
   |            ^^^^^^^^^^^^^^^^ always succeeds
  reason: `plain(...)` lists no failures after `fails`, so it always gives its result and there is nothing for failure arms to handle.
  next: Use the call's result directly (agent)
  rule: failure.outcome_infallible
  help: docs/language/03-logic.html#failure-outcome-infallible

check failed: 1 error, 1 file
```

### `failure.outcome_subject_required`

```jadpo check project=e-outcome-subject expect=failure.outcome_subject_required
type LookupKey = Text {}

function not_a_call(key: LookupKey) -> LookupKey {
    return match key {
        success(value) -> value
    }
}
```

```text output
error: `key` isn't a call, so it has no outcome to match
 --> main.jadpo:4:12
  |
4 |     return match key {
  |            ^^^^^^^^^ not a call
  reason: `success(...)` and failure arms match the outcome of a call that can fail; a value is matched by its own cases, such as an enum's variants.
  next: Match `key`'s cases, or match on the call that can fail (agent)
  rule: failure.outcome_subject_required
  help: docs/language/03-logic.html#failure-outcome-subject-required

check failed: 1 error, 1 file
```

### `failure.propagate_placement`

`propagate` passes a failure arm's failure on, so it has no meaning anywhere else:

```jadpo check project=e-propagate expect=failure.propagate_placement
type Count = Int {}

type TicketStatus = Enum {
    open
    closed
}

function priority(status: TicketStatus) -> Count {
    return match status {
        TicketStatus.open -> 1
        TicketStatus.closed -> propagate
    }
}
```

```text output
error: `propagate` has no failure to pass on here
 --> main.jadpo:11:32
   |
11 |         TicketStatus.closed -> propagate
   |                                ^^^^^^^^^ not in a failure arm
  reason: `propagate` is an arm of a `match` on a call that can fail: it passes the failure that arm names on to the caller's `fails` list.
  next: Give a value, `return` or `reject` a failure here instead (agent)
  rule: failure.propagate_placement
  help: docs/language/03-logic.html#failure-propagate-placement

check failed: 1 error, 1 file
```

### `failure.reject_placement`

`reject` stops the callable, so it starts a statement or ends a `match` arm,
never part of a value (question 11):

```jadpo check project=e-reject-placement expect=failure.reject_placement
type Count = Int {}

type CountRefused = Failure {
    kind: FailureKind.rejected
}

function capped(count: Count) fails CountRefused -> Count {
    var checked: Count = reject CountRefused
    return checked
}
```

```text output
error: `reject` can only start a statement
 --> main.jadpo:8:26
  |
8 |     var checked: Count = reject CountRefused
  |                          ^^^^^^ inside an expression
  reason: Every place a callable can stop early is at the start of a line: `reject` is a statement of its own, or the result of a `match` arm, never part of a value.
  next: Move `reject` to a line of its own, under an `if` or as a `match` arm (agent)
  rule: failure.reject_placement
  help: docs/language/03-logic.html#failure-reject-placement

check failed: 1 error, 1 file
```

### `syntax.match_arrow`

```jadpo check project=e-match-arrow expect=syntax.match_arrow
type TicketStatus = Enum {
    open
    closed
}

type Rank = Int {
    min: 1
}

function rank(status: TicketStatus) -> Rank {
    return match status {
        TicketStatus.open => 1
        TicketStatus.closed -> 2
    }
}
```

```text output
error: Jadpo writes `->`, not `=>`
 --> main.jadpo:12:27
   |
12 |         TicketStatus.open => 1
   |                           ^^ write `->`
  reason: `=>` ends a match arm's pattern in Rust, Scala, C# and PHP; a Jadpo arm uses `->`, the same arrow as a result type.
  next: Use `->` instead of `=>` (agent)
  rule: syntax.match_arrow
  help: docs/language/03-logic.html#syntax-match-arrow

check failed: 1 error, 1 file
```

### `syntax.match_pattern`

A match arm names a case's data in brackets, not braces:

```jadpo check project=e-match-pattern expect=syntax.match_pattern
type ReceiptId = Uuid {}

type Label = Text {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

function label(outcome: PaymentOutcome) -> Label {
    return match outcome {
        PaymentOutcome.pending -> "pending"
        PaymentOutcome.paid { receipt_id } -> "paid"
    }
}
```

```text output
error: A match arm names a case's data in brackets
 --> main.jadpo:15:29
   |
15 |         PaymentOutcome.paid { receipt_id } -> "paid"
   |                             ^^^^^^^^^^^^^^ expected `(name)`
  reason: An arm binds a case's data to one name, `PaymentOutcome.paid(paid)`, and reads its fields from it, as in `paid.field`.
  next: Write `PaymentOutcome.paid(paid)` (agent)
  rule: syntax.match_pattern
  help: docs/language/03-logic.html#syntax-match-pattern

check failed: 1 error, 1 file
```

The `match` examples below share a hidden file declaring
`DeliveryState { pending, sent, failed }`,
`PaymentOutcome { pending, paid: { receipt_id } }` and `Label = Text`.

### `type.match_non_exhaustive`

```jadpo check project=e-match-exhaustive file=shapes.jadpo hidden
type DeliveryState = Enum {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo check project=e-match-exhaustive expect=type.match_non_exhaustive
function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.sent -> "Sent"
    }
}
```

```text output
error: This `match` doesn't cover `DeliveryState.failed`
 --> main.jadpo:2:12
  |
2 |     return match state {
  |            ^^^^^^^^^^^ 1 case not matched
  reason: A `match` on an enum lists every variant, so a variant added later is caught at every `match` that has to consider it; `_` isn't allowed on `DeliveryState`.
  next: Add an arm for `DeliveryState.failed` (agent)
  rule: type.match_non_exhaustive
  help: docs/language/03-logic.html#type-match-non-exhaustive

check failed: 1 error, 2 files
```

### `type.match_duplicate_pattern`

```jadpo check project=e-match-duplicate file=shapes.jadpo hidden
type DeliveryState = Enum {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo check project=e-match-duplicate expect=type.match_duplicate_pattern
function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.pending -> "Waiting"
        DeliveryState.sent -> "Sent"
        DeliveryState.failed -> "Failed"
    }
}
```

```text output
error: `DeliveryState.pending` is matched twice
 --> main.jadpo:4:9
  |
3 |         DeliveryState.pending -> "Pending"
  |         --------------------- first matched here
4 |         DeliveryState.pending -> "Waiting"
  |         ^^^^^^^^^^^^^^^^^^^^^ matched again
  reason: Each case is matched once; the first arm takes every value this one would, so this arm never runs.
  next: Remove this arm, or merge its body into the first (agent)
  rule: type.match_duplicate_pattern
  help: docs/language/03-logic.html#type-match-duplicate-pattern

check failed: 1 error, 2 files
```

### `type.match_unknown_variant`

```jadpo check project=e-match-variant file=shapes.jadpo hidden
type DeliveryState = Enum {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo check project=e-match-variant expect=type.match_unknown_variant
function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.snet -> "Sent"
        DeliveryState.failed -> "Failed"
    }
}
```

```text output
error: `DeliveryState` has no variant `snet`
 --> main.jadpo:4:23
  |
4 |         DeliveryState.snet -> "Sent"
  |                       ^^^^ not a variant
  reason: `DeliveryState`'s variants are `pending`, `sent` and `failed`. Did you mean `sent`?
  next: Use `DeliveryState.sent` (agent)
  rule: type.match_unknown_variant
  help: docs/language/03-logic.html#type-match-unknown-variant

check failed: 1 error, 2 files
```

### `type.match_unreachable_pattern`

```jadpo check project=e-match-unreachable file=shapes.jadpo hidden
type DeliveryState = Enum {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo check project=e-match-unreachable expect=type.match_unreachable_pattern
function shout(label: Label) -> Label {
    return match label {
        "" -> "unnamed"
        _ -> label
        "quiet" -> "QUIET"
    }
}
```

```text output
error: This arm never runs
 --> main.jadpo:5:9
  |
4 |         _ -> label
  |         - matches everything left
5 |         "quiet" -> "QUIET"
  |         ^^^^^^^ after `_`
  reason: `_` matches every value the arms above it don't, so no arm after it runs.
  next: Move this arm above `_` (agent)
  rule: type.match_unreachable_pattern
  help: docs/language/03-logic.html#type-match-unreachable-pattern

check failed: 1 error, 2 files
```

### `type.match_wildcard_required`

```jadpo check project=e-match-wildcard file=shapes.jadpo hidden
type DeliveryState = Enum {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo check project=e-match-wildcard expect=type.match_wildcard_required
function shout(label: Label) -> Label {
    return match label {
        "" -> "unnamed"
    }
}
```

```text output
error: This `match` needs a `_` arm
 --> main.jadpo:2:12
  |
2 |     return match label {
  |            ^^^^^^^^^^^ not every value is matched
  reason: `Label` is built on `Text`, so its values can't all be listed; `_` matches every value the arms above it don't.
  next: Add `_ -> ...` as the last arm (agent)
  rule: type.match_wildcard_required
  help: docs/language/03-logic.html#type-match-wildcard-required

check failed: 1 error, 2 files
```

### `type.match_wildcard_closed`

An enum, a `Bool` or a nullable value lists every case, so a case added later is caught:

```jadpo check project=e-match-wildcard-closed file=shapes.jadpo hidden
type DeliveryState = Enum {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo check project=e-match-wildcard-closed expect=type.match_wildcard_closed
function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        _ -> "Other"
    }
}
```

```text output
error: `_` can't stand for the rest of a `DeliveryState`
 --> main.jadpo:4:9
  |
4 |         _ -> "Other"
  |         ^ wildcard
  reason: On enums, `Bool` and nullable values every case is listed, so a case added later is caught at every `match` that has to consider it.
  next: Replace `_` with arms for `DeliveryState.sent` and `DeliveryState.failed` (agent)
  rule: type.match_wildcard_closed
  help: docs/language/03-logic.html#type-match-wildcard-closed

check failed: 1 error, 2 files
```

### `type.match_pattern_type`

```jadpo check project=e-match-type file=shapes.jadpo hidden
type DeliveryState = Enum {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo check project=e-match-type expect=type.match_pattern_type
function describe(state: DeliveryState) -> Label {
    return match state {
        true -> "Yes"
        DeliveryState.pending -> "Pending"
        DeliveryState.sent -> "Sent"
        DeliveryState.failed -> "Failed"
    }
}
```

```text output
error: A `Bool` can't match a `DeliveryState`
 --> main.jadpo:3:9
  |
3 |         true -> "Yes"
  |         ^^^^ wrong kind of pattern
  reason: A `DeliveryState` is matched by its variants, such as `DeliveryState.pending`.
  next: Write a pattern for a `DeliveryState` (agent)
  rule: type.match_pattern_type
  help: docs/language/03-logic.html#type-match-pattern-type

check failed: 1 error, 2 files
```

### `type.match_some_non_optional`

```jadpo check project=e-match-some file=shapes.jadpo hidden
type DeliveryState = Enum {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo check project=e-match-some expect=type.match_some_non_optional
function describe(label: Label) -> Label {
    return match label {
        some(text) -> text
        _ -> label
    }
}
```

```text output
error: `label` is never `none`, so `some(...)` can't match it
 --> main.jadpo:3:9
  |
3 |         some(text) -> text
  |         ^^^^^^^^^^ for nullable values
  reason: `some(name)` and `none` match a nullable value, written `T?`; `label` is a `Label`.
  next: Use `label` directly: it always has a value (agent)
  rule: type.match_some_non_optional
  help: docs/language/03-logic.html#type-match-some-non-optional

check failed: 1 error, 2 files
```

### `type.match_unknown_binding`

Brackets after a case name the data it carries, and a plain variant carries none:

```jadpo check project=e-match-binding file=shapes.jadpo hidden
type DeliveryState = Enum {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo check project=e-match-binding expect=type.match_unknown_binding
function describe(outcome: PaymentOutcome) -> Label {
    return match outcome {
        PaymentOutcome.pending(waiting) -> "Pending"
        PaymentOutcome.paid -> "Paid"
    }
}
```

```text output
error: `PaymentOutcome.pending` carries no data to name
 --> main.jadpo:3:31
  |
3 |         PaymentOutcome.pending(waiting) -> "Pending"
  |                               ^^^^^^^^^ no data
  reason: `PaymentOutcome.pending` is a plain variant; brackets after a case name the data it carries, and this one carries none.
  next: Write `PaymentOutcome.pending` without brackets (agent)
  rule: type.match_unknown_binding
  help: docs/language/03-logic.html#type-match-unknown-binding

check failed: 1 error, 2 files
```

### `type.match_arm_value`

```jadpo check project=e-match-arm-value file=shapes.jadpo hidden
type DeliveryState = Enum {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo check project=e-match-arm-value expect=type.match_arm_value
action notify(outcome: PaymentOutcome) -> Unit {}

action receipt(outcome: PaymentOutcome) -> ReceiptId? {
    return match outcome {
        PaymentOutcome.pending -> none
        PaymentOutcome.paid -> {
            notify(outcome)
        }
    }
}
```

```text output
error: This arm gives no value
 --> main.jadpo:6:9
  |
6 |         PaymentOutcome.paid -> {
  |         ^^^^^^^^^^^^^^^^^^^^^^^^ ends without a value
  reason: A `match` used as a value gives one from every arm: an expression, `reject` or `propagate`; this block can reach its end without leaving.
  next: Write the arm's value after `->`, or end the block with `return` or `reject` (agent)
  rule: type.match_arm_value
  help: docs/language/03-logic.html#type-match-arm-value

check failed: 1 error, 2 files
```

### `type.arithmetic_operand`

`+` and `-` need the same named type on both sides; multiplying a price by a quantity is meaningful, adding them is not:

```jadpo check project=e-arithmetic expect=type.arithmetic_operand
type Pence = Int {
    min: 0
}

type Quantity = Int {
    min: 1
}

function wrong_total(price: Pence, quantity: Quantity) -> Pence {
    return Pence(price + quantity)
}
```

```text output
error: `Pence` and `Quantity` can't be added
 --> main.jadpo:10:18
   |
10 |     return Pence(price + quantity)
   |                  ^^^^^^^^^^^^^^^^ different types
  reason: `+` and `-` need the same named type on both sides, or a literal, because `Pence` and `Quantity` are different units; `*` and `%` may mix types.
  next: Use two values of the same type (agent)
  rule: type.arithmetic_operand
  help: docs/language/03-logic.html#type-arithmetic-operand

check failed: 1 error, 1 file
```

### `type.ordering_operand`

A nullable value has to be matched before it can be compared:

```jadpo check project=e-ordering expect=type.ordering_operand
type Count = Int {}

type Flag = Bool {}

function over_maximum(count: Count?, maximum: Count) -> Flag {
    return Flag(count > maximum)
}
```

```text output
error: `count` may be `none`, so it can't be ordered
 --> main.jadpo:6:17
  |
6 |     return Flag(count > maximum)
  |                 ^^^^^ may be `none`
  reason: `count` is `Count?`; `<`, `<=`, `>` and `>=` order two values, and `none` has no place in an order.
  next: Match on `count` and compare inside `some(...)` (agent)
  rule: type.ordering_operand
  help: docs/language/03-logic.html#type-ordering-operand

check failed: 1 error, 1 file
```

### `type.incomparable`

```jadpo check project=e-incomparable expect=type.incomparable
type Pence = Int {
    min: 0
}

type Quantity = Int {
    min: 1
}

type Flag = Bool {}

function same(price: Pence, quantity: Quantity) -> Flag {
    return Flag(price == quantity)
}
```

```text output
error: `Pence` and `Quantity` can't be compared
 --> main.jadpo:12:17
   |
12 |     return Flag(price == quantity)
   |                 ^^^^^^^^^^^^^^^^^ different types
  reason: `==` compares two values of the same type, after widening; `Pence` and `Quantity` are different types, and neither widens to the other.
  next: Compare two values of the same type (agent)
  rule: type.incomparable
  help: docs/language/03-logic.html#type-incomparable

check failed: 1 error, 1 file
```

### `type.logical_operand`

```jadpo check project=e-logical expect=type.logical_operand
type Score = Int {}

type Flag = Bool {}

function both(score: Score) -> Flag {
    return Flag(score and score)
}
```

```text output
error: `and` needs `Bool` values, and `score` is a `Score`
 --> main.jadpo:6:17
  |
6 |     return Flag(score and score)
  |                 ^^^^^ this is `Score`
  reason: `and` and `or` work on `Bool` values and types built on `Bool`; Jadpo has no truthiness.
  next: Compare `score` to get a `Bool` (agent)
  rule: type.logical_operand
  help: docs/language/03-logic.html#type-logical-operand

check failed: 1 error, 1 file
```

### `type.unary_operand`

```jadpo check project=e-unary expect=type.unary_operand
type Phrase = Text {}

type Flag = Bool {}

function negate(phrase: Phrase) -> Flag {
    return Flag(not phrase)
}
```

```text output
error: `not` needs a `Bool`, and `phrase` is a `Phrase`
 --> main.jadpo:6:21
  |
6 |     return Flag(not phrase)
  |                     ^^^^^^ this is `Phrase`
  reason: `not` works on `Bool` values and types built on `Bool`; Jadpo has no truthiness.
  next: Compare `phrase` to get a `Bool` (agent)
  rule: type.unary_operand
  help: docs/language/03-logic.html#type-unary-operand

check failed: 1 error, 1 file
```

### `type.divisor_zero`

A divisor's type excludes zero, so division by zero is ruled out while compiling:

```jadpo check project=e-divisor expect=type.divisor_zero
type Pence = Int {
    min: 0
}

function split_evenly(total: Pence, portions: Pence) -> Pence {
    return Pence(total / portions)
}
```

```text output
error: `portions` may be zero
 --> main.jadpo:6:26
  |
6 |     return Pence(total / portions)
  |                          ^^^^^^^^ a `Pence` can be `0`
  reason: Division by zero is ruled out while compiling: a divisor's type excludes zero, as `Int { min: 1 }` does, and `Pence` allows `0`.
  next: Divide by a value whose type excludes zero, building it with its constructor where the zero case is handled (agent)
  rule: type.divisor_zero
  help: docs/language/03-logic.html#type-divisor-zero

check failed: 1 error, 1 file
```

### `type.assign_immutable`

```jadpo check project=e-assign-immutable expect=type.assign_immutable
type Choice = Text {}

function replace(initial: Choice, replacement: Choice) -> Choice {
    var fixed: Choice = initial
    fixed = replacement
    return fixed
}
```

```text output
error: `fixed` can't be changed
 --> main.jadpo:5:5
  |
4 |     var fixed: Choice = initial
  |         ----- declared without `mut`
5 |     fixed = replacement
  |     ^^^^^ assigned here
  reason: `var` names a value that never changes; only a `var mut` can be pointed at a new value.
  next: Declare it `var mut fixed` (agent)
  rule: type.assign_immutable
  help: docs/language/03-logic.html#type-assign-immutable

check failed: 1 error, 1 file
```

### `type.assign_unknown`

```jadpo check project=e-assign-unknown expect=type.assign_unknown
type Count = Int {}

function total(count: Count) -> Count {
    running = count
    return count
}
```

```text output
error: `running` isn't declared
 --> main.jadpo:4:5
  |
4 |     running = count
  |     ^^^^^^^ not declared
  reason: An assignment changes a `var mut` declared earlier in this block or one around it.
  next: Declare it first, as in `var mut running: Count = ...` (agent)
  rule: type.assign_unknown
  help: docs/language/03-logic.html#type-assign-unknown

check failed: 1 error, 1 file
```

### `type.unknown_value`

```jadpo check project=e-unknown-value expect=type.unknown_value
type Name = Text {}

function echo(name: Name) -> Name {
    return nmae
}
```

```text output
error: `nmae` isn't defined in this scope
 --> main.jadpo:4:12
  |
4 |     return nmae
  |            ^^^^ not defined
  reason: Jadpo could not find this name. Did you mean `name`?
  next: Use `name` instead of `nmae` (agent)
  rule: type.unknown_value
  help: docs/language/03-logic.html#type-unknown-value

check failed: 1 error, 1 file
```

### `semantic.unused_variable`

```jadpo check project=e-unused-variable expect=semantic.unused_variable
type Pence = Int {
    min: 0
}

function with_delivery(price: Pence, delivery: Pence) -> Pence {
    var subtotal: Pence = price
    return Pence(price + delivery)
}
```

```text output
error: `subtotal` is never used
 --> main.jadpo:6:9
  |
6 |     var subtotal: Pence = price
  |         ^^^^^^^^ never read
  reason: A value nobody reads is never bound to a name, so a reader never wonders what it was for.
  next: Use `subtotal`, or remove its `var` line (agent)
  rule: semantic.unused_variable
  help: docs/language/03-logic.html#semantic-unused-variable

check failed: 1 error, 1 file
```

### `semantic.unused_result`

A call stands on its own only when its result is `Unit`, and a function called on its own does nothing:

```jadpo check project=e-unused-result expect=semantic.unused_result
type Note = Text {}

function tidy(note: Note) -> Note {
    return note
}

action save(note: Note) -> Unit {
    tidy(note)
}
```

```text output
error: This `Note` is thrown away
 --> main.jadpo:8:5
  |
8 |     tidy(note)
  |     ^^^^^^^^^^ result not used
  reason: `tidy` only computes its result, so calling it without using the result does nothing.
  next: Use the value, or remove it (agent)
  rule: semantic.unused_result
  help: docs/language/03-logic.html#semantic-unused-result

check failed: 1 error, 1 file
```

### `semantic.unreachable_code`

```jadpo check project=e-unreachable expect=semantic.unreachable_code
type Count = Int {
    min: 0
}

type Size = Text {}

function size_of(count: Count) -> Size {
    return "small"
    if (count > 10) {
        return "large"
    }
}
```

```text output
error: This code never runs
 --> main.jadpo:9:5
  |
8 |     return "small"
  |     -------------- leaves on every path
9 |     if (count > 10) {
  |     ^^^^^^^^^^^^^^^^^ never reached
  reason: The statement before it ends in `return` or `reject` on every path, so nothing after it in this block can run.
  next: Remove it, or move it before the statement that leaves (agent)
  rule: semantic.unreachable_code
  help: docs/language/03-logic.html#semantic-unreachable-code

check failed: 1 error, 1 file
```

### `semantic.missing_return`

```jadpo check project=e-missing-return expect=semantic.missing_return
type Count = Int {
    min: 0
}

type Size = Text {}

function size_of(count: Count) -> Size {
    if (count > 10) {
        return "large"
    }
}
```

```text output
error: This function can end without returning a `Size`
 --> main.jadpo:11:1
   |
11 | }
   | ^ reached without `return`
  reason: Every path through a function with a result ends in `return` or `reject`; at least one path here reaches the end of the body.
  next: Return a `Size` at the end, or on every branch that reaches it (agent)
  rule: semantic.missing_return
  help: docs/language/03-logic.html#semantic-missing-return

check failed: 1 error, 1 file
```

### `type.argument_count`

```jadpo check project=e-argument-count expect=type.argument_count
type Name = Text {}

function choose(first: Name, second: Name) -> Name {
    return first
}

function incomplete(name: Name) -> Name {
    return choose(name)
}
```

```text output
error: `choose` takes 2 values, not 1
 --> main.jadpo:8:18
  |
8 |     return choose(name)
  |                  ^^^^^^ 1 given
  reason: `choose(first: Name, second: Name)` has 2 parameters; each is given, in order, and none has a default.
  next: Pass `second` as well (agent)
  rule: type.argument_count
  help: docs/language/03-logic.html#type-argument-count

check failed: 1 error, 1 file
```

### `type.named_argument_unsupported`

```jadpo check project=e-named-argument expect=type.named_argument_unsupported
type Number = Int {}

function add(left: Number, right: Number) -> Number {
    return Number(left + right)
}

function three() -> Number {
    return add(left: 1, right: 2)
}
```

```text output
error: `add` takes its arguments in order, without names
 --> main.jadpo:8:16
  |
8 |     return add(left: 1, right: 2)
  |                ^^^^ named
  reason: Named arguments exist only for standard-library operations; your own callables are called positionally, in the order of their parameters.
  next: Remove the argument names (agent)
  rule: type.named_argument_unsupported
  help: docs/language/03-logic.html#type-named-argument-unsupported

check failed: 1 error, 1 file
```

### `type.named_argument_unknown`

```jadpo check project=e-named-unknown expect=type.named_argument_unknown
type Pence = Int {
    min: 0
}

function line_count(lines: List<Pence>) -> Pence {
    return Pence(collection.count(itmes: lines))
}
```

```text output
error: `collection.count` has no argument `itmes`
 --> main.jadpo:6:35
  |
6 |     return Pence(collection.count(itmes: lines))
  |                                   ^^^^^ not a parameter
  reason: Its one argument is `items`.
  next: Use `items:` instead of `itmes:` (agent)
  rule: type.named_argument_unknown
  help: docs/language/03-logic.html#type-named-argument-unknown

check failed: 1 error, 1 file
```

### `type.inline_function_placement`

An inline function is written only as an argument to a standard-library collection operation:

```jadpo check project=e-inline-function expect=type.inline_function_placement
type Pence = Int {
    min: 0
}

function same_price(price: Pence) -> Pence {
    return price
}

function passed_on(price: Pence) -> Pence {
    return same_price(item -> price)
}
```

```text output
error: An inline function is only an argument to a collection operation
 --> main.jadpo:10:23
   |
10 |     return same_price(item -> price)
   |                       ^^^^^^^^^^^^^ inline function
  reason: Functions aren't values: `item -> expression` is written only as an argument to a standard-library collection operation, such as `collection.map` or `collection.filter`.
  next: Write the expression on its own, or pass it to a collection operation (agent)
  rule: type.inline_function_placement
  help: docs/language/03-logic.html#type-inline-function-placement

check failed: 1 error, 1 file
```

### `toolchain.not_built_yet`

The standard library declares operations that later areas build, such as
`temporal.format` (area 10). Until a chunk builds one, calling it is an error
rather than a program that can't run:

```jadpo check project=e-not-built-yet expect=toolchain.not_built_yet
type Pence = Int {
    min: 0
}

function price_text(price: Pence) -> Pence {
    var formatted_price: Pence = temporal.format(price)
    return formatted_price
}
```

```text output
error: `temporal.format` isn't built yet
 --> main.jadpo:6:34
  |
6 |     var formatted_price: Pence = temporal.format(price)
  |                                  ^^^^^^^^^^^^^^^ declared, not built
  reason: The language declares `temporal.format`, but this compiler has nothing to run it with yet, so a program that calls it couldn't run.
  next: Do without `temporal.format` for now (agent)
  rule: toolchain.not_built_yet
  help: docs/language/03-logic.html#toolchain-not-built-yet

check failed: 1 error, 1 file
```

### `syntax.named_argument_duplicate`

```jadpo check project=e-named-duplicate expect=syntax.named_argument_duplicate
type Day = CalendarDate {}

function london_morning(date: Day) -> Instant {
    return temporal.resolve(
        date: date,
        at: "09:00",
        zone: Zone.europe_london,
        zone: Zone.europe_london,
        dst: Dst.earlier
    )
}
```

```text output
error: `zone:` is given twice
 --> main.jadpo:8:9
  |
7 |         zone: Zone.europe_london,
  |         ---- first given here
8 |         zone: Zone.europe_london,
  |         ^^^^ given again here
  reason: Each named argument is given once.
  next: Remove one of the `zone:` arguments (agent)
  rule: syntax.named_argument_duplicate
  help: docs/language/03-logic.html#syntax-named-argument-duplicate

check failed: 1 error, 1 file
```

### `syntax.positional_after_named`

```jadpo check project=e-positional-after expect=syntax.positional_after_named
type Day = CalendarDate {}

function london_morning(date: Day) -> Instant {
    return temporal.resolve(
        date: date,
        "09:00",
        zone: Zone.europe_london,
        dst: Dst.earlier
    )
}
```

```text output
error: A positional argument follows a named one
 --> main.jadpo:6:9
  |
5 |         date: date,
  |         ---- named from here
6 |         "09:00",
  |         ^^^^^^^ not named
  reason: Once an argument is named, every argument after it is named too.
  next: Name this argument, or move it before `date:` (agent)
  rule: syntax.positional_after_named
  help: docs/language/03-logic.html#syntax-positional-after-named

check failed: 1 error, 1 file
```

### `syntax.expected_statement`

There is no `while` loop; `for` over a list is the only one:

```jadpo check project=e-statement expect=syntax.expected_statement
type Count = Int {
    min: 0
}

function first(count: Count) -> Count {
    while (count > 0) {
        return count
    }
    return count
}
```

```text output
error: Jadpo has no `while` loop
 --> main.jadpo:6:5
  |
6 |     while (count > 0) {
  |     ^^^^^ not a Jadpo statement
  reason: The only loop is `for item in items`, which always ends after its list; stop early with `return`.
  next: Loop over a list with `for`, or narrow it first with `collection.filter` (agent)
  rule: syntax.expected_statement
  help: docs/language/03-logic.html#syntax-expected-statement

check failed: 1 error, 1 file
```

### `syntax.attempt_placement`

`attempt` only starts a statement, so every early exit is at the start of a line:

```jadpo check project=e-attempt-placement expect=syntax.attempt_placement
type Count = Int {
    min: 0
    max: 1000
}

type TooBig = Failure {
    kind: FailureKind.invalid_value
}

function checked(count: Count) fails TooBig -> Count {
    if (count > 500) {
        reject TooBig
    }
    return count
}

function doubled(count: Count) fails TooBig -> Count {
    return Count(attempt checked(count) * 2)
}
```

```text output
error: `attempt` can only start a statement
 --> main.jadpo:18:18
   |
18 |     return Count(attempt checked(count) * 2)
   |                  ^^^^^^^ inside an expression
  reason: Every place a callable can stop early is at the start of a line: a `var`, an assignment, a `return`, or a statement of its own.
  next: Move the call into its own `var` statement with `attempt`, then use the variable (agent)
  rule: syntax.attempt_placement
  help: docs/language/03-logic.html#syntax-attempt-placement

check failed: 1 error, 1 file
```

### `syntax.colon_required`

Every member inside braces is `name: value`:

```jadpo check project=e-colon expect=syntax.colon_required
type ContactEmail = Text {
    format TextFormat.email
}
```

```text output
error: `format` needs a colon before its value
 --> main.jadpo:2:5
  |
2 |     format TextFormat.email
  |     ^^^^^^ expected `:` after `format`
  reason: Every member inside braces is `name: value`, with a colon even when the value is a block.
  next: Write `format: TextFormat.email` (agent)
  rule: syntax.colon_required
  help: docs/language/03-logic.html#syntax-colon-required

check failed: 1 error, 1 file
```

### `syntax.condition_brackets`

```jadpo check project=e-condition expect=syntax.condition_brackets
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    if count > 10 {
        return "large"
    }
    return "small"
}
```

```text output
error: This condition needs brackets
 --> main.jadpo:8:8
  |
8 |     if count > 10 {
  |        ^^^^^^^^^^ expected `(` before the condition
  reason: Conditions are always in brackets, `if (total > limit) {`, so the block can't be mistaken for part of the condition.
  next: Write `if (count > 10) {` (agent)
  rule: syntax.condition_brackets
  help: docs/language/03-logic.html#syntax-condition-brackets

check failed: 1 error, 1 file
```

### `syntax.expected_expression`

A variable is given its value on the line that declares it:

```jadpo check project=e-expression expect=syntax.expected_expression
type Count = Int {
    min: 0
}

function doubled(count: Count) -> Count {
    var result: Count =
    return result
}
```

```text output
error: `result` has no value
 --> main.jadpo:6:23
  |
6 |     var result: Count =
  |                       ^ expected a value after `=`
  reason: A variable is given its value on the line that declares it: `var name: Type = value`.
  next: Write `result`'s value after `=` (agent)
  rule: syntax.expected_expression
  help: docs/language/03-logic.html#syntax-expected-expression

check failed: 1 error, 1 file
```

### Habits from other languages

People and agents write constructs from other languages out of habit
(decision register, 2026-10-15). Each has an error of its own that names
the habit and the Jadpo form, with an edit, and parsing carries on as if
the Jadpo form had been written. A syntax error never brings a second on
its line: anything else the line holds is reported once the first is
fixed, so `pub fn total(): Pence {` shows `pub`, then `fn`, then `:`.
When the Jadpo form only renames or drops words (`let` for `var`, `&&` for
`and`, a `;` at the end of a line), the tree is the one the Jadpo form
gives and the rest of the project is checked too, leaving out only
problems on the habit's line. The catalogue is `jadpo_syntax::habits`,
and a corpus of broken files checks one error a line
(`crates/jadpo-syntax/tests/broken/`).

### `syntax.variable_keyword`

A variable is `var name: Type = value`, or `var mut` when it can change.

```jadpo check project=e-habit-let expect=syntax.variable_keyword
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    let kept: Count = count
    return kept
}
```

```text output
error: Jadpo writes `var`, not `let`
 --> main.jadpo:6:5
  |
6 |     let kept: Count = count
  |     ^^^ write `var`
  reason: `let` declares a variable in JavaScript, TypeScript, Swift and Rust; a Jadpo variable is `var name: Type = value`, or `var mut` when it can change.
  next: Use `var` instead of `let` (agent)
  rule: syntax.variable_keyword
  help: docs/language/03-logic.html#syntax-variable-keyword

check failed: 1 error, 1 file
```

### `syntax.function_keyword`

A function starts with `function`, not `fn`, `func`, `def` or `fun`.

```jadpo check project=e-habit-fn expect=syntax.function_keyword
type Count = Int {
    min: 0
}

fn same(count: Count) -> Count {
    return count
}
```

```text output
error: Jadpo writes `function`, not `fn`
 --> main.jadpo:5:1
  |
5 | fn same(count: Count) -> Count {
  | ^^ write `function`
  reason: `fn` declares a function in Rust; Jadpo starts one with `function`, or `action` when it reads or writes stored data.
  next: Use `function` instead of `fn` (agent)
  rule: syntax.function_keyword
  help: docs/language/03-logic.html#syntax-function-keyword

check failed: 1 error, 1 file
```

### `syntax.return_type_arrow`

The result type follows `->`.

```jadpo check project=e-habit-colon-result expect=syntax.return_type_arrow
type Count = Int {
    min: 0
}

function same(count: Count): Count {
    return count
}
```

```text output
error: Jadpo writes the result type after `->`, not `:`
 --> main.jadpo:5:28
  |
5 | function same(count: Count): Count {
  |                            ^ write `->`
  reason: `: Type` after the parameters is the result type in TypeScript and Kotlin; Jadpo writes `-> Type`, as in `function total(order: Order) -> Amount {`.
  next: Use `->` instead of `:` (agent)
  rule: syntax.return_type_arrow
  help: docs/language/03-logic.html#syntax-return-type-arrow

check failed: 1 error, 1 file
```

### `syntax.async_await`

A call gives its result when it returns, so nothing is marked `async` or `await`.

```jadpo check project=e-habit-await expect=syntax.async_await
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    return count
}

function same_twice(count: Count) -> Count {
    return await same(count)
}
```

```text output
error: Jadpo has no `await`
 --> main.jadpo:10:12
   |
10 |     return await same(count)
   |            ^^^^^ remove `await`
  reason: `async` and `await` mark code that waits for a result in JavaScript, TypeScript, Python, Rust, C# and Swift; a Jadpo call always gives its result when it returns, so there is nothing to mark.
  next: Remove `await` (agent)
  rule: syntax.async_await
  help: docs/language/03-logic.html#syntax-async-await

check failed: 1 error, 1 file
```

### `syntax.semicolon`

A statement or member ends at the end of its line.

```jadpo check project=e-habit-semicolon expect=syntax.semicolon
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    return count;
}
```

```text output
error: Jadpo has no `;`
 --> main.jadpo:6:17
  |
6 |     return count;
  |                 ^ remove `;`
  reason: `;` ends a statement in JavaScript, Rust, Go, Java, C#, Kotlin, Swift and SQL; a Jadpo statement or member ends at the end of its line.
  next: Remove `;` (agent)
  rule: syntax.semicolon
  help: docs/language/03-logic.html#syntax-semicolon

check failed: 1 error, 1 file
```

### `syntax.logical_operator`

Logical operators are the words `and`, `or` and `not`, for `&&`, `||` and `!`, and SQL's `AND`, `OR` and `NOT`.

```jadpo check project=e-habit-logic expect=syntax.logical_operator
type Open = Bool {}

type Paid = Bool {}

type Done = Bool {}

function done(open: Open, paid: Paid) -> Done {
    return Done(open && paid)
}
```

```text output
error: Jadpo writes `and`, not `&&`
 --> main.jadpo:8:22
  |
8 |     return Done(open && paid)
  |                      ^^ write `and`
  reason: `&&`, `||` and `!` are the logical operators of JavaScript, Rust, Go, Java, C#, Kotlin and Swift; Jadpo writes them as words: `and`, `or` and `not`.
  next: Use `and` instead of `&&` (agent)
  rule: syntax.logical_operator
  help: docs/language/03-logic.html#syntax-logical-operator

check failed: 1 error, 1 file
```

### `syntax.ternary_operator`

A choice between two values is a `match` on the condition; the edit writes it.

```jadpo check project=e-habit-ternary expect=syntax.ternary_operator
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    return count > 10 ? "large" : "small"
}
```

```text output
error: Jadpo chooses a value with `match`, not `? :`
 --> main.jadpo:8:23
  |
8 |     return count > 10 ? "large" : "small"
  |                       ^ write a `match`
  reason: `condition ? a : b` chooses a value in JavaScript, Java, C#, Kotlin and Swift; Jadpo chooses between two values with a `match` on the condition.
  next: Write `match (count > 10)` with the arms `true -> "large"` and `false -> "small"` (agent)
  rule: syntax.ternary_operator
  help: docs/language/03-logic.html#syntax-ternary-operator

check failed: 1 error, 1 file
```

### `syntax.increment_operator`

A `var mut` changes with `=`, and a named type converts the result back with its constructor.

```jadpo check project=e-habit-increment expect=syntax.increment_operator
type Count = Int {
    min: 0
}

function bumped(count: Count) -> Count {
    var mut next: Count = count
    next++
    return next
}
```

```text output
error: Jadpo writes `next = next + 1`, not `next++`
 --> main.jadpo:7:9
  |
7 |     next++
  |         ^^ write `next = next + 1`
  reason: `++`, `--`, `+=`, `-=` and `*=` change a variable in place in JavaScript, Java, C#, Go, Kotlin, Swift and Python; Jadpo assigns a `var mut` with `=`, and a named type converts the result back with its constructor, as in `count = Count(count + 1)`.
  next: Write `next = next + 1` (agent)
  rule: syntax.increment_operator
  help: docs/language/03-logic.html#syntax-increment-operator

check failed: 1 error, 1 file
```

### `syntax.else_if`

An `if` goes on with `else if`.

```jadpo check project=e-habit-elif expect=syntax.else_if
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    if (count > 100) {
        return "large"
    } elif (count > 10) {
        return "medium"
    }
    return "small"
}
```

```text output
error: Jadpo writes `else if`, not `elif`
 --> main.jadpo:10:7
   |
10 |     } elif (count > 10) {
   |       ^^^^ write `else if`
  reason: `elif` continues an `if` in Python, `elsif` in Ruby and `elseif` in PHP and Lua; Jadpo writes `else if`.
  next: Use `else if` instead of `elif` (agent)
  rule: syntax.else_if
  help: docs/language/03-logic.html#syntax-else-if

check failed: 1 error, 1 file
```

### `syntax.for_loop`

A loop is `for item in items {`, without brackets. A counting loop has no Jadpo form, so its error has no edit.

```jadpo check project=e-habit-for expect=syntax.for_loop
type Pence = Int {
    min: 0
}

type OrderLine = Object {
    subtotal: Pence
}

function order_total(lines: List<OrderLine>) -> Pence {
    var mut total: Pence = 0
    for (const line of lines) {
        total = Pence(total + line.subtotal)
    }
    return total
}
```

```text output
error: Jadpo writes `for line in lines`, not `for (const line of lines)`
 --> main.jadpo:11:9
   |
11 |     for (const line of lines) {
   |         ^^^^^^^^^^^^^^^^^^^^^ write `for line in lines`
  reason: `for (const item of items)` walks a list in JavaScript and TypeScript, `for (item in items)` in Kotlin, `for (Type item : items)` in Java and `foreach (var item in items)` in C#; Jadpo writes `for item in items {`, without brackets.
  next: Write `for line in lines` (agent)
  rule: syntax.for_loop
  help: docs/language/03-logic.html#syntax-for-loop

check failed: 1 error, 1 file
```

### `syntax.switch_statement`

A choice between cases is a `match`, with arms `pattern -> result`: `switch` and `when` are `match`, `case` goes, and `default` or `else` is `_`.

```jadpo check project=e-habit-switch expect=syntax.switch_statement
type TicketStatus = Enum {
    open
    closed
}

type Rank = Int {
    min: 1
}

function rank(status: TicketStatus) -> Rank {
    return switch (status) {
        TicketStatus.open -> 1
        TicketStatus.closed -> 2
    }
}
```

```text output
error: Jadpo writes `match`, not `switch`
 --> main.jadpo:11:12
   |
11 |     return switch (status) {
   |            ^^^^^^ write `match`
  reason: `switch` chooses between cases in JavaScript, Java, C#, Go and Swift, and `when` in Kotlin; Jadpo's `match` does, with one arm per line, `pattern -> result`, covering every case.
  next: Use `match` instead of `switch` (agent)
  rule: syntax.switch_statement
  help: docs/language/03-logic.html#syntax-switch-statement

check failed: 1 error, 1 file
```

### `syntax.try_catch`

A call that can fail is marked where it is made: `attempt` passes its failures on, and a `match` on the call handles them. `try {` and `catch` have no edit, since what to do with each failure is the author's to say.

```jadpo check project=e-habit-try expect=syntax.try_catch
type Count = Int {
    min: 0
}

type CountMissing = Failure {
    kind: FailureKind.not_found
}

function counted_or_missing(count: Count) fails CountMissing -> Count {
    if (count == 0) {
        reject CountMissing
    }
    return count
}

function first(count: Count) fails CountMissing -> Count {
    var result: Count = try counted_or_missing(count)
    return result
}
```

```text output
error: Jadpo writes `attempt`, not `try`
 --> main.jadpo:17:25
   |
17 |     var result: Count = try counted_or_missing(count)
   |                         ^^^ write `attempt`
  reason: `try` marks a call that can fail in Swift; Jadpo writes `attempt`, which passes the call's failures on to the caller.
  next: Use `attempt` instead of `try` (agent)
  rule: syntax.try_catch
  help: docs/language/03-logic.html#syntax-try-catch

check failed: 1 error, 1 file
```

### `syntax.inline_function_arrow`

An inline function is `item -> expression`.

```jadpo check project=e-habit-arrow expect=syntax.inline_function_arrow
type TodoTitle = Text {
    max_length: 200
}

type Todo = Object {
    title: TodoTitle
}

function titles(todos: List<Todo>) -> List<TodoTitle> {
    return collection.map(todos, todo => todo.title)
}
```

```text output
error: Jadpo writes `todo -> todo.title`, not `todo => todo.title`
 --> main.jadpo:10:34
   |
10 |     return collection.map(todos, todo => todo.title)
   |                                  ^^^^^^^ write `todo -> todo.title`
  reason: `x => e` is an arrow function in JavaScript, TypeScript and C#, `lambda x: e` in Python and `|x| e` in Rust; a Jadpo inline function is `item -> expression`.
  next: Write `todo -> todo.title` (agent)
  rule: syntax.inline_function_arrow
  help: docs/language/03-logic.html#syntax-inline-function-arrow

check failed: 1 error, 1 file
```

### `syntax.named_argument_equals`

A named argument is `name: value`.

```jadpo check project=e-habit-named-equals expect=syntax.named_argument_equals
type Price = Decimal {
    scale: 2
    min: 0.00
}

type People = Int {
    min: 1
}

function share(total: Price, people: People) -> Price {
    return Price(decimal.divide(total, people, rounding = Rounding.half_even))
}
```

```text output
error: Jadpo names an argument with `:`, not `=`
 --> main.jadpo:11:57
   |
11 |     return Price(decimal.divide(total, people, rounding = Rounding.half_even))
   |                                                         ^ write `:`
  reason: `name = value` names an argument in Python, Kotlin and C#; Jadpo writes `name: value`.
  next: Use `:` instead of `=` (agent)
  rule: syntax.named_argument_equals
  help: docs/language/03-logic.html#syntax-named-argument-equals

check failed: 1 error, 1 file
```

### `syntax.member_comma`

Each member is on its own line, with no commas between them.

```jadpo check project=e-habit-member-comma expect=syntax.member_comma
type Pence = Int {
    min: 0
}

type PriceRange = Object {
    low: Pence,
    high: Pence
}
```

```text output
error: Jadpo has no `,` between members
 --> main.jadpo:6:15
  |
6 |     low: Pence,
  |               ^ remove `,`
  reason: Commas separate fields in JavaScript, TypeScript, Rust, Go, Swift and JSON; each Jadpo member is on its own line, and the formatter puts it there.
  next: Remove `,` (agent)
  rule: syntax.member_comma
  help: docs/language/03-logic.html#syntax-member-comma

check failed: 1 error, 1 file
```

### `syntax.equality_operator`

```jadpo check project=e-equality expect=syntax.equality_operator
type Count = Int {
    min: 0
}

type Done = Bool {}

function sold_out(count: Count) -> Done {
    return Done(count === 0)
}
```

```text output
error: Jadpo compares with `==`, not `===`
 --> main.jadpo:8:23
  |
8 |     return Done(count === 0)
  |                       ^^^ write `==`
  reason: `===` and `!==` compare without converting types in JavaScript and TypeScript; Jadpo's `==` and `!=` never convert between types, so there is no stricter form to ask for.
  next: Use `==` instead of `===` (agent)
  rule: syntax.equality_operator
  help: docs/language/03-logic.html#syntax-equality-operator

check failed: 1 error, 1 file
```

### `syntax.unsupported_throw`

```jadpo check project=e-throw expect=syntax.unsupported_throw
type Note = Text {}

type NoteRejected = Failure {
    kind: FailureKind.rejected
}

function checked_note(note: Note) fails NoteRejected -> Note {
    if (note == "") {
        throw NoteRejected
    }
    return note
}
```

```text output
error: Jadpo writes `reject`, not `throw`
 --> main.jadpo:9:9
  |
9 |         throw NoteRejected
  |         ^^^^^ write `reject`
  reason: `throw` raises an error in JavaScript, Java, C#, Kotlin and Swift, and `raise` in Python and Ruby; a Jadpo callable raises one of its declared failures with `reject`, and lists it in `fails`.
  next: Use `reject` instead of `throw` (agent)
  rule: syntax.unsupported_throw
  help: docs/language/03-logic.html#syntax-unsupported-throw

check failed: 1 error, 1 file
```

## Review

### Status

The new compiler builds this area: roadmap chunk 1.5, with the failure side
of `attempt` and `match` in chunk 1.6 and the runtime recursion limit in
chunk 2.1. Every example and error above is checked against it on each gate
run.

### Consistency with the rest of the language

- **Two arrows.** `->` for results, `=>` for arms; decided in area 2 to use
   `->` for both.
- **Two `match` constructs.** A value match is a statement with block arms;
   an outcome match is an expression with single-expression arms and a
   `failure` prefix. Neither works in the other's position.
- **Payload patterns break the list decision.** `PaymentOutcome.paid { receipt_id, paid_at }`
   is a comma list in braces, which area 2's list decision rules out.
- **`var` means "immutable".** In JavaScript, C#, Kotlin and Go `var` is the
   mutable form; `var mut` already mirrors Rust's `let mut`.
- **Units are checked by comparison but not by addition.** `Pence + Quantity`
   compiles and `Pence == Quantity` does not, although area 2's rule is that
   named types never mix silently.
- **Division contradicts area 2.** Area 2 decided `Decimal` is exact with a
   scale and that division names a rounding mode; the archived compiler's `/` was
   JavaScript division producing a float.
- **Area 2's examples omit `attempt`.** `Pence(price * quantity)` is a runtime
   construction, which area 2 question 1 made fallible; whether the compiler
   may skip `attempt` when the declared ranges prove it safe is carried to
   area 4.
- **Parentheses are a workaround, not a style.** The decision register limits
   parentheses to calls, constructors and grouping; conditions need them only
   because of the parsing bug.
- **The grammar and the compiler disagree on recursion.** Grammar §16 and
   RM-216 list recursion as unsupported; the compiler accepts it.
- **`reject` is a statement and also an arm.** In an outcome arm it stands
   where an expression would; the proposed `match` keeps this, alongside
   `propagate`, as the two non-value arm forms.

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Whether a runtime construction the declared ranges prove safe (`Pence(price * quantity)` with both minimums at 0 or above and no maximum) still needs `attempt` and a mapping | 4 | Decided in area 4, question 6: no |
| `fails` sets, `reject`, `propagate` and failure arms' mapping rules; how overflow, division by zero and recursion depth faults are reported | 4 | Input to area 4 |
| Calling actions or queries inside a `for` loop (one query per item, transaction scope) | 6 | Input to area 6 |
| Callable settings (`consistency:`, `freshness:`, `guard:`) become defaults; reading a cache or projection is declared on the cache | 6 | Follows from a decision |
| Recursive types (`replies: List<Comment>`) are accepted and were not discussed in area 2; storing them | 5 | Input to area 5 |
| `assert` and `advance clock by` | 13 | Input to area 13 |
| Sections gain a colon under the colon rule (`policy: { }`, `persistence: { }`, `lifecycle: { }`); `set: { }` and `initial: { }` already follow it | 5, 6, 9 | Follows from a decision |
| Route sections follow the colon rule; the syntax review separately proposes removing `path: { }`, `headers: { }` and inline `action: { }` | 7 | Input to area 7 |
| Entities: `identity: id` and one-value `persistence { store: primary role: authority }` become defaults, leaving `entity Customer { id: Uuid }` (syntax review) | 5 | Proposed |

### Open questions

1. **Which arrow do match arms use?** *Decided 2026-10-12 (Richard, carried from area 2):* `->`, the same arrow as result types; `=>` is removed and the formatter rewrites it.
2. **Rename `var` to `let`?** *Decided 2026-10-12 (Richard):* no. Keep `var`
   ("variable"), immutable by default, and `var mut` when it can change. Every
   keyword choice carries baggage from another language (`let` is mutable in
   JavaScript), and `var` plus an explicit `mut` reads clearly on its own.

3. **May a local reuse a name already in scope?** Today a local can shadow a
   parameter or an outer local, and two parameters may share a name.
   *Decided 2026-10-12:* no: a name in scope cannot be declared again, including
   parameters, extending area 1's rule that locals cannot reuse domain and
   library names. Rust encourages shadowing; Go and Kotlin warn about it; for
   a reviewer reading a diff, one name meaning one value is worth more.
4. **Which statements may stand alone?** *Decided 2026-10-12:* an unused
   variable is a compile error, so a value nobody needs is never bound to a
   name. Three forms may stand alone instead: a call whose result is `Unit`;
   `attempt call(...)` with any result, meaning "this must succeed"; and a
   bare `return` in a `Unit` callable. A pure call whose value is thrown away
   is also an error, because it cannot do anything.

5. **How are conditions written?** *Decided 2026-10-12 (Richard):* always in
   brackets, `if (total > limit) { ... }`, with `else if` chains. The brackets
   end the condition before the block, which also removes today's parsing bug
   where `b {` is read as the start of an object. `if` stays a statement;
   choosing a value is a `match`.

6. **Which operand types do operators accept?** Today arithmetic mixes any
   named types while comparison does not, and text is ordered by JavaScript
   string comparison. *Decided 2026-10-12:* `+`, `-` and every comparison need
   both sides to be the same named type after widening (or one a literal);
   `*` and `%` may mix types, since multiplying a price by a quantity is
   meaningful and adding them is not. Ordering applies to numbers and time
   values only; text order depends on locale and belongs in a standard
   function. This is F#'s units-of-measure idea with the types Jadpo already
   has.
7. **How does division work?** *Decided 2026-10-12:* `/` divides two values
   of the same named type, like `+` and `-`. `Int` division drops the
   remainder (`10 / 3` is `3`), as in Rust, Go, Java and C#; `Decimal`
   division is exact and is rounded half to even when the result is stored in
   a type with a fixed `scale`. Dividing values of different types, or
   choosing another rounding, uses `decimal.divide(total, people,
   rounding: Rounding.half_up)` or `integer.divide(...)`. Division by zero is ruled out
   by the compiler, not left to the author: the divisor's type must exclude
   zero (`type People = Int { min: 1 }`), a literal divisor must not be `0`,
   and a computed divisor is constructed into such a type first, which is the
   one place the zero case has to be handled. `%` stays `Int`-only.

8. **One `match` for values and outcomes?** *Decided 2026-10-12:* yes: the
   subject's type decides which patterns apply, every `match` works as a
   statement or an expression, an arm is an expression, `reject`, `propagate`
   or a block, and failure arms drop the `failure` prefix because failure
   names are already unique types. `attempt` remains the short form for
   passing every failure on.
9. **How is a variant's data read in a `match` arm?** A variant can carry
   data (`paid` carries `receipt_id` and `paid_at`); a `match` arm has to
   open it. Today the arm lists field names in braces,
   `PaymentOutcome.paid { receipt_id, paid_at }`, a comma list in braces that
   area 2's list decision rules out, repeated in every arm.
   *Recommendation:* the arm names the variant's data in brackets, the same
   shape as `some(subtitle)` and `success(value)`:
   `PaymentOutcome.paid(payment) -> payment.receipt_id`, where `payment` is a
   name you choose and holds that variant's fields. One rule for every kind of
   match: brackets after a case name the data it carries. The alternative is
   narrowing, as in TypeScript and Kotlin: no name at all, and inside the
   `paid` arm `outcome.receipt_id` simply works.
   *Decided 2026-10-12:* the arm names the data in brackets.

10. **May `_` cover the rest of an enum?** Today it may, so adding a variant
    silently falls into an existing arm. *Decided 2026-10-12:* no: on enums,
    `Bool` and nullable values every case is listed; `_` is only for text and
    numbers. Adding a variant then fails to compile at every `match` that
    must consider it, which is the review prompt a new state needs. Rust and
    Kotlin allow the wildcard; Elixir's convention favours listing cases.
11. **Where may `attempt` appear?** Today anywhere in an expression, and
    silently on calls that cannot fail. *Decided 2026-10-12:* only at the start of
    a statement (`var`, assignment, `return` or on its own) and only on a call
    that can fail. Every early exit is then visible at the start of a line,
    which Rust's `?` (allowed anywhere) does not give, without Go's
    `if err != nil` repetition.
12. **Which loops?** Area 2 decided `for item in items`. *Decided 2026-10-12:*
    `for` over a `List` only, with `return` allowed inside; no `while`,
    `break` or `continue`. Every loop then ends after its list, which keeps
    request deadlines predictable, and searching or stopping early is
    `collection.filter` or a `return`. Go manages with one loop form; Elixir
    with none.
13. **How are functions passed to `collection.map` and `collection.filter`?**
    *Decided 2026-10-12:* inline functions only as arguments to standard-library
    collection operations, written `todo -> todo.title`: one expression, pure,
    parameter type taken from the list. No function types in your own
    signatures and no functions stored in variables. Capturing surrounding
    variables is safe because values never change. Elixir writes the same
    shape (`fn todo -> todo.title end`); TypeScript's general closures are
    more than application code needs.
14. **Is recursion allowed?** It is accepted today with no limit, and
    recursive types such as comment threads need it. *Decided 2026-10-12:* allow
    it in functions only; an action or query in a call cycle is an error,
    because repeating effects has no data reason and an unbounded one never
    finishes. Add a runtime depth limit that ends in a defined fault instead
    of a crash, and correct grammar §16.
15. **Where do a callable's settings go?** *Decided 2026-10-12:* nowhere: the
    safe choice is the default, so an action is only its signature and its
    logic. Every action is all-or-nothing (long-running work became
    subscribers in area 1); a write to a loaded entity always checks it was not
    changed in the meantime; every read is current unless it reads a cache or
    projection, declared where that cache is (area 6). Who may call an action
    is decided in area 9. The cases this does not fit have better answers
    already: a large batch that should keep partial progress is a schedule or
    subscriber handling one item per action (resumable as well); a call to an
    external provider comes before any write in the action and carries an
    idempotency key, so a retry repeats it harmlessly (amended 2026-10-12 by
    area 11 question 7); a counter or "last seen" field that must not conflict is updated by
    the database in one step (`count + 1`) rather than read and rewritten
    (area 6). Illustrative shapes, decided in their own areas:
    `subscriber import_row { on: RowReceived  run: import_product(event) }`;
    `emit OrderPlaced { order_id: order.id }` in the action, with a subscriber calling
    the provider; `set: { view_count: Post.view_count + 1 }`.
16. **Should your own functions be generic, or accept unions?** Area 2 left
    this open. *Decided 2026-10-12:* neither. The standard library stays generic;
    a parameter that accepts several types is an enum whose variants carry
    them, which keeps every value's type named and keeps identifiers apart.
    No user generics also means no type-parameter errors for agents to fight.
    Go managed without them for twelve years; revisit only if the order and
    payment application shows repeated helpers that differ by type alone.

### Decisions

Decided by Richard on 2026-10-12. Built in the new compiler on 2026-10-13 (roadmap chunk 1.5, with the failure side of `attempt` and `match` in chunk 1.6); the runtime recursion limit came with the interpreter on 2026-10-14 (chunk 2.1): a call past 1,000 frames is the `recursion_limit` fault at its site.

| Topic | Decision | Date |
|---|---|---|
| Match arm arrow | Carried from area 2 (Richard, 2026-10-12): `->` for match arms and result types; `=>` removed | 2026-10-12 |
| `var` or `let` | Keep `var`, immutable by default; `var mut` when it can change (Richard) | 2026-10-12 |
| Shadowing | A name in scope cannot be declared again, parameters included | 2026-10-12 |
| Standalone statements | Unused variables are compile errors; a `Unit` call, `attempt call(...)` and a bare `return` may stand alone; a discarded pure call is an error | 2026-10-12 |
| Conditions | Always in brackets, `if (condition) { }`, with `else if`; `if` is a statement (Richard) | 2026-10-12 |
| Operator operand types | `+`, `-` and comparisons need the same named type (or a literal); `*` and `%` may mix; ordering only on numbers and time values | 2026-10-12 |
| Division | `/` on the same named type; `Int` drops the remainder; `Decimal` exact, rounded half to even into a fixed scale; `decimal.divide`/`integer.divide` for mixed types or other rounding; the divisor's type must exclude zero, checked by the compiler | 2026-10-12 |
| One `match` | One `match` for values and call outcomes; failure arms have no `failure` prefix | 2026-10-12 |
| Payload binding | A `match` arm names a variant's data in brackets, `PaymentOutcome.paid(payment) -> payment.receipt_id`, like `some(x)` and `success(value)` | 2026-10-12 |
| Wildcard on closed types | No `_` on enums, `Bool` or nullable values; every case listed | 2026-10-12 |
| `attempt` placement | Only at the start of a statement, and only on a call that can fail | 2026-10-12 |
| Loops | `for` over a `List` only, `return` allowed inside; no `while`, `break` or `continue` | 2026-10-12 |
| Passing functions | Inline `x -> expression` only as an argument to standard-library collection operations; no function types in your own signatures or variables | 2026-10-12 |
| Recursion | Functions only, with a runtime depth limit that ends in a defined fault; actions and queries in a call cycle are errors | 2026-10-12 |
| Callable settings | None: every action is all-or-nothing, writes always check for concurrent change, reads are current unless they read a declared cache; policy is area 9 | 2026-10-12 |
| Generics and unions | None for your own code; a parameter accepting several types takes an `Enum` whose variants carry them | 2026-10-12 |
| Colon rule (language-wide) | Every member inside braces is `name: value`, and a value is one thing or a `{ ... }` block (`paid: { ... }`, `persistence: { ... }`); a variant with no data is just its name; a field's own settings still follow its type (`name: ProductName { optional: true }`); top-level declarations, statements and `match` arms are not members; route entries inside a `routes` group, keyed by method and path, are the one exception (area 7) (Richard; Terraform's block-versus-attribute split is a known source of confusion) | 2026-10-12 |

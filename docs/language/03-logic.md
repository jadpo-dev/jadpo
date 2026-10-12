---
title: Logic
area: 3
status: in-review
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - effect.*
  - failure.attempt_required
  - failure.outcome_*
  - syntax.expected_expression
  - syntax.expected_statement
  - syntax.match_pattern
  - syntax.named_argument_duplicate
  - syntax.positional_after_named
  - syntax.unsupported_throw
  - type.argument_count
  - type.arithmetic_operand
  - type.assign_*
  - type.incomparable
  - type.logical_operand
  - type.match_*
  - type.named_argument_unsupported
  - type.ordering_operand
  - type.unary_operand
  - type.unknown_value
---

# Logic

How behaviour is written: functions and actions, variables, statements,
operators, `match`, `attempt`, loops, and what the language says about passing
functions, recursion and generic code.

> Proposed code on this page assumes every recommendation in the Review is accepted, together with the decisions from areas 1 and 2: `let` instead of `var` (question 2), `->` match arms (question 1), one `match` (question 8) and literals taking the type they go into. Today's compiled syntax is collapsed beneath each proposal.

## Functions and actions

There are three kinds of callable. A **function** is pure: it computes a
result from its arguments and nothing else. An **action** is an application
operation that may read and write data, call integrations and call other
actions. A **query** is a read-only operation, described in area 6.

```jadpo fragment
/// Total for one order line before discounts.
function line_total(price: Pence, quantity: Quantity) -> Pence {
    var total: Pence = Pence(price * quantity)
    return total
}

action complete_todo(todo_id: Todo.id) fails TodoNotFound -> TodoView {
    var todo: Todo = attempt Todo.get(todo_id)
    var view: TodoView = attempt Todo.mark_done(todo)
    return view
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

::: today

```jadpo project=callables
type Pence = Int {
    min: 0
}

type Quantity = Int {
    min: 1
    max: 1000
}

failure QuantityTooLarge {
    kind: InvalidValue
    code: "quantity_too_large"
}

function line_total(price: Pence, quantity: Quantity) -> Pence {
    return Pence(price * quantity)
}

function bulk_quantity(quantity: Quantity)
    fails QuantityTooLarge
    -> Quantity
{
    if quantity > 100 {
        reject QuantityTooLarge
    }
    return quantity
}

action price_line(price: Pence, quantity: Quantity)
    fails QuantityTooLarge
    -> Pence
{
    var checked = attempt bulk_quantity(quantity)
    return line_total(price, checked)
}
```

:::

### Signatures and settings

Parameters are positional and every one has a type; the result type follows
`->`, and `fails` comes before it. A callable that returns nothing returns
`Unit`. Named arguments exist only for standard-library operations
(`temporal.resolve(date: d, zone: z, ...)`); your own callables are called
positionally. Parameters have no defaults.

The proposal (question 15) puts the whole signature on the header line and
every setting at the top of the body, one per line:

```jadpo fragment
action reschedule(todo_id: Todo.id, due_at: DueAt) fails TodoNotFound, DueDateInPast -> TodoView {
    consistency: atomic
    policy {
        TodoRole.owner: [invoke]
    }

    var todo: Todo = attempt Todo.get(todo_id)
    ...
}
```

When a header is too long the formatter puts one parameter per line and keeps
`fails` and `->` after the closing parenthesis.

::: today

Settings sit between the parameters and `fails`, and the formatter moves
`consistency:` to the first column:

```jadpo project=callable-settings
type Count = Int {}

failure TooBig {
    kind: InvalidValue
    code: "too_big"
}

action bump(n: Count)
consistency: atomic
    fails TooBig
    -> Count
{
    if n > 10 {
        reject TooBig
    }
    return Count(n + 1)
}
```

:::

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
var local: Time = temporal.in_zone(at, zone)                       // the standard library
```

A library holds only functions. Today there is no `library` keyword and every
authored callable is called bare.

## Variables

> Question 2 proposes `let` and `var mut` in place of `var` and `var mut`. Area 2 decided that every variable states its type.

```jadpo fragment
var subtotal: Pence = line_total(price, quantity)
var mut total: Pence = subtotal
total = Pence(total + delivery)
```

- `let` names a value that never changes. `var mut` names one that can be
   pointed at a new value of the same type with `name = value`.
- Only a `var mut` local can be assigned. Parameters, fields and
   declarations cannot (area 2: values never change).
- A variable is visible from its declaration to the end of its block.
- Question 3 proposes that a name already in scope, including a parameter,
   cannot be declared again.

::: today

```jadpo project=variables
type Choice = Text {}

function choose(initial: Choice, replacement: Choice) -> Choice {
    var mut selected: Choice = initial
    selected = replacement
    var unchanged = initial
    return selected
}
```

Today `var` is immutable, the type may be left out, and a local can reuse the
name of a parameter or an outer local.

:::

## Statements

A body is a list of statements, one per line, with no semicolons:

| Statement | Form |
|---|---|
| Variable | `var name: T = value`, `var mut name: T = value` |
| Assignment | `name = value` |
| Return | `return value` |
| Rejection | `reject Failure { ... }` (area 4) |
| Condition | `if condition { } else { }` |
| Match | `match value { ... }` |
| Loop | `for item in items { }` (decided in area 2) |
| Test only | `assert`, `advance clock by` (area 13) |

There is no `throw`. Question 4 proposes three more statements, so that a
result nobody needs no longer has to be bound to a name:

```jadpo fragment
action archive_todo(todo_id: Todo.id) fails TodoNotFound -> Unit {
    attempt Todo.get(todo_id)         // fails or carries on; the value is not needed
    if (already_archived(todo_id)) {
        return                        // leaves a Unit action early
    }
    notify_owner(todo_id)             // a Unit action called for its effect
}
```

::: today

A call cannot stand on its own, so the golden app binds results it never
reads (`var existing = attempt Todo.by_id(todo_id)`), and a `Unit` action has
no way to return early.

```jadpo project=unit-calls
type Note = Text {}

action record(note: Note) -> Unit {
}

action record_twice(note: Note) -> Unit {
    var first = record(note)
    var second = record(note)
}
```

:::

### Conditions

```jadpo fragment
if (due_at <= clock.now) {
    reject DueDateInPast
} else if (due_at > latest_due_at) {
    reject DueDateTooFar
}
```

The condition must be a `Bool` (or a type built on `Bool`), and there is no
truthiness. `if` is a statement; a choice between two values is a `match`.
Decided (question 5): conditions are always in brackets, and `else if` chains are allowed.

::: today

There is no `else if`. A condition that ends in a name is read as the start
of an object (`b {`), so `if a == b {` fails with a misleading error; the
golden app wraps such conditions in parentheses, and the formatter keeps them.

```jadpo project=conditions
type Count = Int {}

type Size = Text {}

function size(count: Count, maximum: Count) -> Size {
    if (count > maximum) {
        return Size("large")
    } else {
        if count > 10 {
            return Size("medium")
        }
    }
    return Size("small")
}
```

:::

## Expressions and operators

| Precedence | Operators | Operands | Result |
|---|---|---|---|
| 1 (tightest) | `not`, unary `-` | `Bool`; a number | same |
| 2 | `*`, `/`, `%` | numbers | `Int`, or `Decimal` if either side is `Decimal`; `/` always gives `Decimal` |
| 3 | `+`, `-` | numbers; `+` also joins two `Text` values; time arithmetic is area 10 | as above |
| 4 | `<`, `<=`, `>`, `>=` | two numbers, two `Text` values or two time values of the same type | `Bool` |
| 5 | `==`, `!=` | two values of the same type after widening, or a nullable value and `none` | `Bool` |
| 6 | `and` | `Bool` | `Bool` |
| 7 (loosest) | `or` | `Bool` | `Bool` |

Operators work on the representation, so their result is a plain `Int`,
`Decimal`, `Text` or `Bool`, and turning it back into a named type is a
constructor (area 2). Parentheses group. `and` and `or` stop as soon as the
answer is known. `a < b < c` is an error, because `a < b` is a `Bool`.
Logical operators are words; `!`, `&&` and `||` are not Jadpo.

```jadpo project=operators
type Score = Int {}

type Ratio = Decimal {}

type Phrase = Text {}

type Flag = Bool {}

type Calculation = Object {
    total: Score
    ratio: Ratio
    label: Phrase
    allowed: Flag
}

function calculate(left: Score, right: Score, prefix: Phrase, suffix: Phrase, disabled: Flag) -> Calculation {
    var total = left + right * 2 - left % right
    var ratio = left / right
    var label = prefix + " " + suffix
    var allowed = not disabled and (left < right or left == right)
    return Calculation {
        total: Score(total)
        ratio: Ratio(ratio)
        label: Phrase(label)
        allowed: Flag(allowed)
    }
}
```

Questions 6 and 7 change operator typing and division:

```jadpo fragment
var wrong: Pence = Pence(price + quantity)       // error: adds a Quantity to Pence
var line: Pence = Pence(price * quantity)        // fine: multiplying different units
var earlier: Bool = name < other_name             // error: text has no order; use a standard function
var share: Price = decimal.divide(total, people, rounding: half_even)
var boxes: BoxCount = integer.divide(items, box_size, rounding: up)   // box_size can't be zero
```

## Match

> Decided in area 2 and carried here: match arms use `->`, the same arrow as result types; `=>` is removed.

`match` chooses between the cases of a value. It must cover every case, and it
can be used as a statement or as an expression.

```jadpo fragment
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

function lookup_or_default(key: LookupCode) fails LookupRejected -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "not found"
        LookupRefused -> reject LookupRejected
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

::: today

Two different constructs share the keyword. A **value match** is a statement
only, every arm is a block, and payload fields are bound as a comma list:

```jadpo project=value-match
type ReceiptId = Uuid {}

type OutcomeLabel = Text {}

type ProductName = Text {}

enum PaymentOutcome {
    pending
    paid {
        receipt_id: ReceiptId
    }
    declined
}

type Listing = Object {
    name: ProductName
    subtitle: ProductName?
}

function label(outcome: PaymentOutcome) -> OutcomeLabel {
    match outcome {
        PaymentOutcome.pending => {
            return OutcomeLabel("pending")
        }
        PaymentOutcome.paid { receipt_id } => {
            return OutcomeLabel("paid")
        }
        PaymentOutcome.declined => {
            return OutcomeLabel("declined")
        }
    }
}

function headline(listing: Listing) -> ProductName {
    match listing.subtitle {
        some(subtitle) => {
            return subtitle
        }
        none => {
            return listing.name
        }
    }
}

function shout(name: ProductName) -> ProductName {
    match name {
        "" => {
            return ProductName("unnamed")
        }
        _ => {
            return name
        }
    }
}
```

An **outcome match** is an expression only, its subject must be a direct call
that can fail, and each arm is one expression, `reject` or `propagate`. The
space in `success (value)` is how the formatter writes it today (a bug):

```jadpo project=outcome-match
type LookupCode = Text {}

type LookupResult = Text {}

failure LookupMissing {
    kind: NotFound
    code: "lookup_missing"
}

failure LookupRefused {
    kind: Rejected
    code: "lookup_refused"
}

failure LookupRejected {
    kind: Rejected
    code: "lookup_rejected"
}

function lookup(key: LookupCode)
    fails LookupMissing, LookupRefused
    -> LookupResult
{
    if key == LookupCode("missing") {
        reject LookupMissing
    }
    if key == LookupCode("refused") {
        reject LookupRefused
    }
    return LookupResult("found")
}

function lookup_or_default(key: LookupCode)
    fails LookupRejected
    -> LookupResult
{
    return match lookup(key) {
        success (value) => value
        failure LookupMissing => LookupResult("not found")
        failure LookupRefused => reject LookupRejected
    }
}
```

`return match status { ... }` on an enum fails with an outcome-match error,
and an outcome match written as a statement fails with "invalid match
pattern" (both under Errors). A payload variant pattern must list at least one
field, may leave others out, and `_` is accepted on enums.

:::

## Attempt

A call that can fail must say so at the call site. `attempt` passes the
call's failures on to the caller, whose `fails` list must include them;
`match` on the call handles them instead.

```jadpo fragment
var todo: Todo = attempt Todo.get(todo_id)            // a variable
return attempt Todo.by_id(todo_id)                    // a return
attempt Todo.get(todo_id)                             // on its own (question 4)
var total: Pence = Pence(attempt price(a) + attempt price(b))   // rejected: inside an expression
```

Question 11 proposes that `attempt` only starts a statement, so every point
where a callable can stop early is at the start of a line, and that `attempt`
on a call that cannot fail is an error.

::: today

`attempt` is a prefix allowed anywhere in an expression, and on a call that
cannot fail it is accepted silently:

```jadpo project=attempt
type Count = Int {}

failure TooBig {
    kind: InvalidValue
    code: "too_big"
}

function checked(n: Count)
    fails TooBig
    -> Count
{
    if n > 10 {
        reject TooBig
    }
    return n
}

function plain(n: Count) -> Count {
    return n
}

function doubled(n: Count)
    fails TooBig
    -> Count
{
    var total = attempt checked(n) + attempt checked(n)
    return attempt plain(Count(total))
}
```

:::

## Loops

Decided in area 2: `for item in items` iterates a list, and the loop variable
takes its type from the list.

```jadpo fragment
function order_total(lines: List<OrderLine>) -> Pence {
    var mut total: Pence = 0
    for line in lines {
        total = Pence(total + line.subtotal)
    }
    return total
}
```

Question 12 proposes that `for` over a list is the only loop: no `while`,
`break` or `continue`, with `return` allowed inside the body. Today there are
no loops at all; `for` and `while` are syntax errors (see
`syntax.expected_statement`).

## Passing functions

Area 2 decided on `collection.map` and `collection.filter`, which need a
function as an argument. Question 13 proposes inline functions only as
arguments to standard-library collection operations, using `->` for
"produces":

```jadpo fragment
var open: List<Todo> = collection.filter(todos, todo -> todo.status == TodoStatus.open)
var titles: List<TodoTitle> = collection.map(todos, todo -> todo.title)
```

The inline function is one expression, takes its parameter's type from the
list, may read the variables around it (safe because values never change),
and cannot call actions or use `attempt`. Functions are not values: they
cannot be stored in variables or passed to your own callables. Today a
function's name used as a value is an unknown name.

## Recursion

A function may call itself or form a cycle with other functions. Recursive
types are accepted (`replies: List<Comment>`), and recursion is the only way
to walk them:

```jadpo fragment
function reply_count(comment: Comment) -> ReplyCount {
    var mut count: ReplyCount = 0
    for reply in comment.replies {
        count = ReplyCount(count + 1 + reply_count(reply))
    }
    return count
}
```

Question 14 proposes allowing recursion in functions only, with a runtime
depth limit that ends in a defined fault. Today recursion is accepted
everywhere, including actions, with no limit:

::: today

```jadpo project=recursion
type Count = Int {
    min: 0
}

function countdown(n: Count) -> Count {
    if n == 0 {
        return n
    }
    return countdown(Count(n - 1))
}
```

:::

## Generic code

Only the standard library is generic (`List<T>`, `collection.count`). Your own
callables take named types, and a parameter that accepts several types is an
enum whose variants carry each one (from area 2):

```jadpo fragment
type PartyId = Enum {
    customer {
        id: Customer.id
    }
    supplier {
        id: Supplier.id
    }
}

function party_label(party: PartyId) -> PartyLabel {
    return match party {
        PartyId.customer(customer) -> customer_label(customer.id)
        PartyId.supplier(supplier) -> supplier_label(supplier.id)
    }
}
```

`function first<T>(items: List<T>) -> T` and `Customer.id | Supplier.id` are
syntax errors today; question 16 recommends keeping it that way.

## Errors

> The examples in this section use today's syntax, because they show the compiler's real output; they are rewritten as the rework lands.

Every error this area can produce, with an example that triggers it and the
compiler's actual output.

### `effect.function_calls_action`

```jadpo project=e-fn-action expect=effect.function_calls_action
type Note = Text {}

action save(note: Note) -> Note {
    return note
}

function pretend_pure(note: Note) -> Note {
    return save(note)
}
```

### `effect.function_persistence`

```jadpo project=e-fn-persist expect=effect.function_persistence
entity Customer {
    id: Uuid
    identity: id

    persistence {
        store: primary
        role: authority
    }
}

function invalid_create(id: Customer.id) -> Customer {
    return attempt create Customer {
        id: id
    }
}
```

### `effect.query_calls_action`

```jadpo project=e-query-action expect=effect.query_calls_action
entity Customer {
    id: Uuid
    identity: id

    persistence {
        store: primary
        role: authority
    }

    action register(id: Customer.id) -> Customer {
        return attempt create Customer {
            id: id
        }
    }

    query illegal(id: Customer.id)
    freshness: authoritative
        -> Customer
    {
        return Customer.register(id)
    }
}
```

### `effect.query_mutation`

```jadpo project=e-query-write expect=effect.query_mutation
entity Customer {
    id: Uuid
    identity: id

    persistence {
        store: primary
        role: authority
    }

    query illegal_write(id: Customer.id)
    freshness: authoritative
        -> Customer
    {
        return attempt create Customer {
            id: id
        }
    }
}
```

### `effect.authored_async`

```jadpo project=e-async expect=effect.authored_async
type Note = Text {}

async action save(note: Note) -> Note {
    return note
}
```

### `effect.authored_await`

```jadpo project=e-await expect=effect.authored_await
type Note = Text {}

action save(note: Note) -> Note {
    return note
}

action save_again(note: Note) -> Note {
    return await save(note)
}
```

### `failure.attempt_required`

```jadpo project=e-attempt expect=failure.attempt_required
type Note = Text {}

failure Refused {
    kind: Rejected
    code: "refused"
}

action child(note: Note)
    fails Refused
    -> Note
{
    reject Refused
}

action parent(note: Note)
    fails Refused
    -> Note
{
    return child(note)
}
```

The outcome-match examples below share a hidden file declaring
`lookup(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult`.

### `failure.outcome_missing_arm`

```jadpo project=e-outcome-missing file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

failure LookupMissing {
    kind: NotFound
    code: "lookup_missing"
}

failure LookupRefused {
    kind: Rejected
    code: "lookup_refused"
}

function lookup(key: LookupKey)
    fails LookupMissing, LookupRefused
    -> LookupResult
{
    reject LookupMissing
}
```

```jadpo project=e-outcome-missing expect=failure.outcome_missing_arm
function missing_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) => value
        failure LookupMissing => LookupResult("missing")
    }
}
```

### `failure.outcome_duplicate_arm`

```jadpo project=e-outcome-dup file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

failure LookupMissing {
    kind: NotFound
    code: "lookup_missing"
}

failure LookupRefused {
    kind: Rejected
    code: "lookup_refused"
}

function lookup(key: LookupKey)
    fails LookupMissing, LookupRefused
    -> LookupResult
{
    reject LookupMissing
}
```

```jadpo project=e-outcome-dup expect=failure.outcome_duplicate_arm
function duplicate_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) => value
        failure LookupMissing => LookupResult("missing")
        failure LookupMissing => LookupResult("missing again")
        failure LookupRefused => LookupResult("refused")
    }
}
```

### `failure.outcome_unknown_arm`

```jadpo project=e-outcome-unknown file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

failure LookupMissing {
    kind: NotFound
    code: "lookup_missing"
}

failure LookupRefused {
    kind: Rejected
    code: "lookup_refused"
}

function lookup(key: LookupKey)
    fails LookupMissing, LookupRefused
    -> LookupResult
{
    reject LookupMissing
}
```

```jadpo project=e-outcome-unknown expect=failure.outcome_unknown_arm
failure LookupTimedOut {
    kind: Rejected
    code: "lookup_timed_out"
}

function unknown_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) => value
        failure LookupMissing => LookupResult("missing")
        failure LookupRefused => LookupResult("refused")
        failure LookupTimedOut => LookupResult("timed out")
    }
}
```

### `failure.outcome_wildcard`

```jadpo project=e-outcome-wildcard file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

failure LookupMissing {
    kind: NotFound
    code: "lookup_missing"
}

failure LookupRefused {
    kind: Rejected
    code: "lookup_refused"
}

function lookup(key: LookupKey)
    fails LookupMissing, LookupRefused
    -> LookupResult
{
    reject LookupMissing
}
```

```jadpo project=e-outcome-wildcard expect=failure.outcome_wildcard
function wildcard_failure(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) => value
        failure _ => LookupResult("fallback")
    }
}
```

### `failure.outcome_success_missing`

```jadpo project=e-outcome-no-success file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

failure LookupMissing {
    kind: NotFound
    code: "lookup_missing"
}

failure LookupRefused {
    kind: Rejected
    code: "lookup_refused"
}

function lookup(key: LookupKey)
    fails LookupMissing, LookupRefused
    -> LookupResult
{
    reject LookupMissing
}
```

```jadpo project=e-outcome-no-success expect=failure.outcome_success_missing
function missing_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        failure LookupMissing => LookupResult("missing")
        failure LookupRefused => LookupResult("refused")
    }
}
```

### `failure.outcome_success_duplicate`

```jadpo project=e-outcome-two-success file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

failure LookupMissing {
    kind: NotFound
    code: "lookup_missing"
}

failure LookupRefused {
    kind: Rejected
    code: "lookup_refused"
}

function lookup(key: LookupKey)
    fails LookupMissing, LookupRefused
    -> LookupResult
{
    reject LookupMissing
}
```

```jadpo project=e-outcome-two-success expect=failure.outcome_success_duplicate
function duplicate_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) => value
        success(other) => other
        failure LookupMissing => LookupResult("missing")
        failure LookupRefused => LookupResult("refused")
    }
}
```

### `failure.outcome_success_value_required`

```jadpo project=e-outcome-success-value file=lookup.jadpo hidden
type LookupKey = Text {}

type LookupResult = Text {}

failure LookupMissing {
    kind: NotFound
    code: "lookup_missing"
}

failure LookupRefused {
    kind: Rejected
    code: "lookup_refused"
}

function lookup(key: LookupKey)
    fails LookupMissing, LookupRefused
    -> LookupResult
{
    reject LookupMissing
}
```

```jadpo project=e-outcome-success-value expect=failure.outcome_success_value_required
function invalid_success(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) => propagate
        failure LookupMissing => LookupResult("missing")
        failure LookupRefused => LookupResult("refused")
    }
}
```

### `failure.outcome_infallible`

```jadpo project=e-outcome-infallible expect=failure.outcome_infallible
type LookupKey = Text {}

type LookupResult = Text {}

function plain(key: LookupKey) -> LookupResult {
    return LookupResult("ok")
}

function needless(key: LookupKey) -> LookupResult {
    return match plain(key) {
        success(value) => value
    }
}
```

### `failure.outcome_subject_required`

```jadpo project=e-outcome-subject expect=failure.outcome_subject_required
type LookupKey = Text {}

function not_a_call(key: LookupKey) -> LookupKey {
    return match key {
        success(value) => value
    }
}
```

### `failure.outcome_pattern_invalid`

A value match used as an expression is read as an outcome match, so the
error names the wrong construct:

```jadpo project=e-outcome-pattern expect=failure.outcome_pattern_invalid
enum TicketStatus {
    open
    closed
}

type Rank = Int {}

function rank(status: TicketStatus) -> Rank {
    return match status {
        TicketStatus.open => Rank(1)
        TicketStatus.closed => Rank(2)
    }
}
```

### `syntax.match_pattern`

An outcome match written as a statement:

```jadpo project=e-match-pattern expect=syntax.match_pattern
type LookupKey = Text {}

type LookupResult = Text {}

failure LookupMissing {
    kind: NotFound
    code: "lookup_missing"
}

function lookup(key: LookupKey)
    fails LookupMissing
    -> LookupResult
{
    reject LookupMissing
}

function recover(key: LookupKey) -> LookupResult {
    match lookup(key) {
        success(value) => {
            return value
        }
        failure LookupMissing => {
            return LookupResult("missing")
        }
    }
}
```

The value-match examples below share a hidden file declaring
`DeliveryState { pending, sent, failed }`, `PaymentOutcome { pending, paid { receipt_id } }`
and `Label = Text`.

### `type.match_non_exhaustive`

```jadpo project=e-match-exhaustive file=shapes.jadpo hidden
enum DeliveryState {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

enum PaymentOutcome {
    pending
    paid {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo project=e-match-exhaustive expect=type.match_non_exhaustive
function describe(state: DeliveryState) -> Label {
    match state {
        DeliveryState.pending => {
            return Label("Pending")
        }
        DeliveryState.sent => {
            return Label("Sent")
        }
    }
}
```

### `type.match_duplicate_pattern`

```jadpo project=e-match-duplicate file=shapes.jadpo hidden
enum DeliveryState {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

enum PaymentOutcome {
    pending
    paid {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo project=e-match-duplicate expect=type.match_duplicate_pattern
function describe(state: DeliveryState) -> Label {
    match state {
        DeliveryState.pending => {
            return Label("Pending")
        }
        DeliveryState.pending => {
            return Label("Waiting")
        }
        DeliveryState.sent => {
            return Label("Sent")
        }
        DeliveryState.failed => {
            return Label("Failed")
        }
    }
}
```

### `type.match_unknown_variant`

```jadpo project=e-match-variant file=shapes.jadpo hidden
enum DeliveryState {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

enum PaymentOutcome {
    pending
    paid {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo project=e-match-variant expect=type.match_unknown_variant
function describe(state: DeliveryState) -> Label {
    match state {
        DeliveryState.pending => {
            return Label("Pending")
        }
        DeliveryState.delivered => {
            return Label("Delivered")
        }
        _ => {
            return Label("Other")
        }
    }
}
```

### `type.match_unreachable_pattern`

```jadpo project=e-match-unreachable file=shapes.jadpo hidden
enum DeliveryState {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

enum PaymentOutcome {
    pending
    paid {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo project=e-match-unreachable expect=type.match_unreachable_pattern
function describe(state: DeliveryState) -> Label {
    match state {
        DeliveryState.pending => {
            return Label("Pending")
        }
        _ => {
            return Label("Other")
        }
        DeliveryState.sent => {
            return Label("Sent")
        }
    }
}
```

### `type.match_wildcard_required`

```jadpo project=e-match-wildcard file=shapes.jadpo hidden
enum DeliveryState {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

enum PaymentOutcome {
    pending
    paid {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo project=e-match-wildcard expect=type.match_wildcard_required
function shout(label: Label) -> Label {
    match label {
        "" => {
            return Label("unnamed")
        }
    }
    return label
}
```

### `type.match_pattern_type`

```jadpo project=e-match-type file=shapes.jadpo hidden
enum DeliveryState {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

enum PaymentOutcome {
    pending
    paid {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo project=e-match-type expect=type.match_pattern_type
function describe(state: DeliveryState) -> Label {
    match state {
        true => {
            return Label("Yes")
        }
        _ => {
            return Label("No")
        }
    }
}
```

### `type.match_some_non_optional`

```jadpo project=e-match-some file=shapes.jadpo hidden
enum DeliveryState {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

enum PaymentOutcome {
    pending
    paid {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo project=e-match-some expect=type.match_some_non_optional
function describe(label: Label) -> Label {
    match label {
        some(text) => {
            return text
        }
        _ => {
            return label
        }
    }
}
```

### `type.match_variant_bindings_required`

```jadpo project=e-match-bindings file=shapes.jadpo hidden
enum DeliveryState {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

enum PaymentOutcome {
    pending
    paid {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo project=e-match-bindings expect=type.match_variant_bindings_required
function describe(outcome: PaymentOutcome) -> Label {
    match outcome {
        PaymentOutcome.pending => {
            return Label("Pending")
        }
        PaymentOutcome.paid => {
            return Label("Paid")
        }
    }
}
```

### `type.match_unknown_binding`

```jadpo project=e-match-binding file=shapes.jadpo hidden
enum DeliveryState {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

enum PaymentOutcome {
    pending
    paid {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo project=e-match-binding expect=type.match_unknown_binding
function describe(outcome: PaymentOutcome) -> Label {
    match outcome {
        PaymentOutcome.pending => {
            return Label("Pending")
        }
        PaymentOutcome.paid { receipt } => {
            return Label("Paid")
        }
    }
}
```

### `type.match_duplicate_binding`

```jadpo project=e-match-binding-twice file=shapes.jadpo hidden
enum DeliveryState {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

enum PaymentOutcome {
    pending
    paid {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo project=e-match-binding-twice expect=type.match_duplicate_binding
function describe(outcome: PaymentOutcome) -> Label {
    match outcome {
        PaymentOutcome.pending => {
            return Label("Pending")
        }
        PaymentOutcome.paid { receipt_id, receipt_id } => {
            return Label("Paid")
        }
    }
}
```

### `type.arithmetic_operand`

```jadpo project=e-arithmetic expect=type.arithmetic_operand
type Score = Int {}

type Phrase = Text {}

function combine(score: Score, phrase: Phrase) -> Score {
    var total = score + phrase
    return score
}
```

### `type.ordering_operand`

A nullable value has to be matched before it can be compared:

```jadpo project=e-ordering expect=type.ordering_operand
type Count = Int {}

type Flag = Bool {}

function over_maximum(count: Count?, maximum: Count) -> Flag {
    return Flag(count > maximum)
}
```

### `type.incomparable`

```jadpo project=e-incomparable expect=type.incomparable
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

### `type.logical_operand`

```jadpo project=e-logical expect=type.logical_operand
type Score = Int {}

type Flag = Bool {}

function both(score: Score) -> Flag {
    return Flag(score and score)
}
```

### `type.unary_operand`

```jadpo project=e-unary expect=type.unary_operand
type Phrase = Text {}

type Flag = Bool {}

function negate(phrase: Phrase) -> Flag {
    return Flag(not phrase)
}
```

### `type.assign_immutable`

```jadpo project=e-assign-immutable expect=type.assign_immutable
type Choice = Text {}

function replace(initial: Choice, replacement: Choice) -> Choice {
    var fixed: Choice = initial
    fixed = replacement
    return fixed
}
```

### `type.assign_unknown`

```jadpo project=e-assign-unknown expect=type.assign_unknown
type Count = Int {}

function total(count: Count) -> Count {
    running = count
    return count
}
```

### `type.unknown_value`

```jadpo project=e-unknown-value expect=type.unknown_value
type Name = Text {}

function echo(name: Name) -> Name {
    return nmae
}
```

### `type.argument_count`

```jadpo project=e-argument-count expect=type.argument_count
type Name = Text {}

function choose(first: Name, second: Name) -> Name {
    return first
}

function incomplete(name: Name) -> Name {
    return choose(name)
}
```

### `type.named_argument_unsupported`

```jadpo project=e-named-argument expect=type.named_argument_unsupported
type Number = Int {}

function add(left: Number, right: Number) -> Number {
    return Number(left + right)
}

function three() -> Number {
    return add(left: Number(1), right: Number(2))
}
```

### `syntax.named_argument_duplicate`

```jadpo project=e-named-duplicate expect=syntax.named_argument_duplicate
type Day = CalendarDate {}

function london_morning(date: Day) -> Instant {
    return temporal.resolve(
        date: date,
        at: "09:00",
        zone: Zone.europe_london,
        zone: Zone.europe_london,
        overlap: LocalOverlap.earlier,
        gap: LocalGap.reject
    )
}
```

### `syntax.positional_after_named`

```jadpo project=e-positional-after expect=syntax.positional_after_named
type Day = CalendarDate {}

function london_morning(date: Day) -> Instant {
    return temporal.resolve(
        date: date,
        "09:00",
        zone: Zone.europe_london,
        overlap: LocalOverlap.earlier,
        gap: LocalGap.reject
    )
}
```

### `syntax.expected_statement`

There are no loops yet:

```jadpo project=e-statement expect=syntax.expected_statement
type Tag = Text {}

type Tags = Object {
    items: List<Tag>
}

function first(tags: Tags) -> Tag {
    for tag in tags.items {
        return tag
    }
    return Tag("none")
}
```

### `syntax.expected_expression`

`return` always needs a value, even in a `Unit` action:

```jadpo project=e-expression expect=syntax.expected_expression
type Count = Int {}

action stop_early(count: Count) -> Unit {
    if count > 0 {
        return
    }
}
```

### `syntax.unsupported_throw`

```jadpo project=e-throw expect=syntax.unsupported_throw
type Note = Text {}

function fail_loudly(note: Note) -> Note {
    throw Error("boom")
}
```

## Review

### Status

| Feature | Status |
|---|---|
| Functions, actions and who may call whom | Works in general: function to action, function to persistence, query to action, query writes, `async` and `await` are all rejected. A function calling a query is reported as "can't call action `Customer.find`", naming the wrong kind. |
| Signatures | Works in general. Two gaps: duplicate parameter names (`f(n: Count, n: Count)`) are accepted; `-> R fails F` is still accepted and kept by the formatter, although the decision register puts `fails` first. The formatter moves `consistency:` to column 1. |
| Return paths | **Bug.** A function whose declared result is not `Unit` compiles without returning on every path (`if n > 0 { return n }` and nothing after it); the generated TypeScript returns `undefined` (read from generated code, not run). Code after `return` is accepted. |
| Variables and assignment | Works: immutable `var`, `var mut` rebinding with type checks, block scope. A local may reuse a parameter's or an outer local's name. Types may be left out (area 2 decided to require them). Query words such as `limit` and `missing` cannot be names and fail with unrelated errors ("Expected an expression"); area 1's reserved-word decision covers them. |
| Statements | No call or `attempt` on its own and no bare `return`, so results are bound and never read (`var existing`, `var todo` in the golden app's `patch_todo` and `delete_todo`). `return g(n)` from a `Unit` action works. |
| Conditions | **Bug.** `if` parses its condition allowing object construction, so a condition ending in a name (`if a == b {`) is read as `b { ... }` and fails with four unrelated syntax errors; `match` already parses its subject without construction. Parentheses are the workaround. No `else if`. |
| Operators | Work as the grammar says. `Int / Int` gives a `Decimal` (JavaScript division); `%` works on decimals; `Pence + Quantity` is accepted while `Pence == Quantity` is rejected; text is ordered by JavaScript string comparison (read from generated code). Division by zero is not checked (generated `/`, not run). `and` and `or` short-circuit (read from generated code). |
| `match` | Works for plain and payload variants, nullable values, `Bool`, text and number literals, with exhaustiveness and unreachable-arm checks. Value match is statement-only and outcome match expression-only, each with a misleading error for the other form. No nested (`some(S.a)`), alternative or guarded patterns. `_` is accepted on enums. **Formatter bug:** `success(value)` is rewritten as `success (value)`, so `examples/outcome-sequencing` and pass fixture 113 are not formatter-clean. |
| `attempt` | Works, propagating the exact failure set. Allowed anywhere inside an expression; accepted silently on a call that cannot fail. |
| Loops | None. `for` and `while` are syntax errors. |
| Passing functions | None. A function name used as a value is `type.unknown_value`. |
| Recursion | Accepted for functions and actions, directly and mutually, although grammar §16 lists it as unsupported. Compiles to plain JavaScript recursion with no depth limit, so deep recursion would crash the process (inferred). |
| Generic code | None for authored code: `<T>` and union types are syntax errors. |
| Named arguments | Standard library only, with duplicate and order checks. |

### Consistency with the rest of the language

- **Two arrows.** `->` for results, `=>` for arms; decided in area 2 to use
   `->` for both.
- **Two `match` constructs.** A value match is a statement with block arms;
   an outcome match is an expression with single-expression arms and a
   `failure` prefix. Neither works in the other's position.
- **Payload patterns break the list decision.** `PaymentOutcome.paid { receipt_id, paid_at }`
   is a comma list in braces, which area 2's list decision rules out.
- **`var` means "immutable".** In JavaScript, C#, Kotlin and Go `var` is the
   mutable form; `var mut` already mirrors Rust's `var mut`.
- **Units are checked by comparison but not by addition.** `Pence + Quantity`
   compiles and `Pence == Quantity` does not, although area 2's rule is that
   named types never mix silently.
- **Division contradicts area 2.** Area 2 decided `Decimal` is exact with a
   scale and that division names a rounding mode; today `/` is JavaScript
   division producing a float.
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
| Whether a runtime construction the declared ranges prove safe (`Pence(price * quantity)` with both minimums at 0 or above and no maximum) still needs `attempt ... invalid:` | 4 | Open |
| `fails` sets, `reject`, `propagate` and failure arms' mapping rules; how overflow, division by zero and recursion depth faults are reported | 4 | Input to area 4 |
| Calling actions or queries inside a `for` loop (one query per item, transaction scope) | 6 | Input to area 6 |
| Callable settings' values (`consistency:`, `freshness:`, `guard:`) | 6 | Placement proposed here (question 15) |
| Recursive types (`replies: List<Comment>`) are accepted and were not discussed in area 2; storing them | 5 | Input to area 5 |
| `assert` and `advance clock by` | 13 | Input to area 13 |

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
   rounding: half_up)` or `integer.divide(...)`. Division by zero is ruled out
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

10. **May `_` cover the rest of an enum?** Today it may, so adding a variant
    silently falls into an existing arm. *Decided 2026-10-12:* no: on enums,
    `Bool` and nullable values every case is listed; `_` is only for text and
    numbers. Adding a variant then fails to compile at every `match` that
    must consider it, which is the review prompt a new state needs. Rust and
    Kotlin allow the wildcard; Elixir's convention favours listing cases.
11. **Where may `attempt` appear?** Today anywhere in an expression, and
    silently on calls that cannot fail. *Decided 2026-10-12:* only at the start of
    a statement (`let`, assignment, `return` or on its own) and only on a call
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
15. **Where do a callable's settings go?** Today `consistency:`, `guard:`
    and `freshness:` sit in the header between the parameters and `fails`, and
    `policy { }` starts the body. *Recommendation:* remove them by making the
    safe choice the default, so an action is only its signature and its logic.
    `consistency:` goes: since long-running work became subscribers (area 1),
    the only choice left is all-or-nothing, so every action is atomic.
    `guard:` goes: a write to a loaded entity always checks it was not changed
    in the meantime. `freshness:` goes: every read is current unless it reads
    a cache or projection, which is declared where that cache is (area 6).
    Who may call an action is decided in area 9 (policy).

16. **Should your own functions be generic, or accept unions?** Area 2 left
    this open. *Decided 2026-10-12:* neither. The standard library stays generic;
    a parameter that accepts several types is an enum whose variants carry
    them, which keeps every value's type named and keeps identifiers apart.
    No user generics also means no type-parameter errors for agents to fight.
    Go managed without them for twelve years; revisit only if the order and
    payment application shows repeated helpers that differ by type alone.

### Decisions

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
| Payload binding | | |
| Wildcard on closed types | No `_` on enums, `Bool` or nullable values; every case listed | 2026-10-12 |
| `attempt` placement | Only at the start of a statement, and only on a call that can fail | 2026-10-12 |
| Loops | `for` over a `List` only, `return` allowed inside; no `while`, `break` or `continue` | 2026-10-12 |
| Passing functions | Inline `x -> expression` only as an argument to standard-library collection operations; no function types in your own signatures or variables | 2026-10-12 |
| Recursion | Functions only, with a runtime depth limit that ends in a defined fault; actions and queries in a call cycle are errors | 2026-10-12 |
| Callable settings | | |
| Generics and unions | None for your own code; a parameter accepting several types takes an `Enum` whose variants carry them | 2026-10-12 |

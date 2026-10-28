---
title: Logic
area: 3
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Logic
<!-- topic: logic -->

How behaviour is written: functions and actions, variables and constants,
statements, operators, `match`, `attempt`, loops, and what the language says
about passing functions, recursion and generic code.

## Functions and actions
<!-- topic: functions-and-actions -->

There are three kinds of callable. A **function** is pure: it computes a
result from its arguments and nothing else. An **action** is an application
operation that may read and write data, call integrations and call other
actions. A **query** is a read-only operation, described in area 6.

```jadpo check project=line-total file=numbers.jadpo hidden
type Stock = Int {
    min: 0
}

type Quantity = Int {
    min: 1
    max: 1000
}
```

```jadpo check project=line-total
/// Total for one order line before discounts.
function stock_needed(per_pack: Stock, packs: Quantity) -> Stock {
    var total: Stock = Stock(per_pack * packs)
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

The whole signature is on the header line and an action has no settings
(question 15), because the safe choice is the default: every action is
all-or-nothing, writes check for concurrent change, and reads are current.
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

The dot names the owner of what you call (area 1). Your own
domain's actions and queries, and a helper function used only in its own
file, are called bare. A helper shared by several files lives in a `library`
and is called through it like the standard library, and another domain's
public actions are called through that domain.

```jadpo fragment
library rounding {
    function half_even(amount: ItemPrice) -> ItemPrice { ... }
}

var total: ItemPrice = rounding.half_even(subtotal)                  // a library
var line: Stock = stock_needed(per_pack, packs)                      // a helper in this file
var stock: Reservation = attempt inventory.reserve_for_cart(cart)  // another domain
var todo: Todo = attempt Todo.get(todo_id)                         // an entity
var local: ZonedTime = temporal.in_zone(at, zone)                  // the standard library
```

A library holds only functions and constants (see [Constants](#constants)).

## Variables
<!-- topic: variables -->

A variable is immutable by default, `var mut` when it can change
(question 2), and every variable states its type (area 2).

```jadpo fragment
var needed: Stock = stock_needed(per_pack, packs)
var mut total: Stock = needed
total = Stock(total + holdback)
```

- `var` names a value that never changes. `var mut` names one that can be
   pointed at a new value of the same type with `name = value`.
- Only a `var mut` local can be assigned. Parameters, fields and
   declarations cannot (area 2: values never change).
- A variable is visible from its declaration to the end of its block.
- A name already in scope, including a parameter, cannot be declared again
   (question 3), and an unused variable is an error (question 4).

## Constants
<!-- topic: constants -->

A `constant` names a fixed value at the top level of a file: the same in
every deployment, worked out while compiling, and never changed.

```jadpo check project=constants
type Stock = Int {
    min: 0
}

constant bulk_from: Stock = 5000
constant small_batch: Stock = 250

function batch_for(needed: Stock) -> Stock {
    if (needed >= bulk_from) {
        return 0
    }
    return small_batch
}
```

- **Form.** `constant name: Type = value`. The name is lower_snake_case like
  any value, and there is no mutable form: `constant mut` is
  `syntax.expected_name`, whose edit removes `mut`.
- **Only at the top of a file.** Inside a body a fixed value is always
  `var`, and at the top of a file always `constant`, so each place has one
  way to write it. `constant` in a body is `syntax.expected_statement`,
  whose edit writes `var`. A top-level `var` stays
  `syntax.expected_declaration`, whose `next:` says to write `constant`
  (area 1). `const` at the top of a file is `syntax.variable_keyword`,
  whose edit writes `constant`; in a body it stays
  [`syntax.variable_keyword`](../errors/syntax.variable_keyword.md), whose edit writes
  `var`.
- **A named type.** A constant's type follows the rule signatures follow
  (area 2, "Signatures use named types"): `constant limit: Int = 5` is
  `type.primitive_signature`, as `limit: Int` is in a parameter. A list of a named type, an
  object, an enum, `Email`, `Url` and the other types a signature may use
  are fine.
- **A value the compiler can work out.** Literals, enum values, other
  constants, construction from those (`Stock(5000)`, an object, a list),
  and the operators of [Expressions and operators](#expressions-and-operators)
  on them. No call of any kind (functions, library functions, the standard
  library), no `config`, no `clock`, and no constant that depends on
  itself ([`constant.value_not_fixed`](../errors/constant.value_not_fixed.md),
  [`constant.cycle`](../errors/constant.cycle.md)). Arithmetic gives the
  representation, as it does in a body, so `Stock(bulk_from * 2)`
  constructs it back.
- **Checked while compiling.** The value is checked against every rule of
  its type and the types it is built on, as a literal is
  ([`type.invalid_literal`](../errors/type.invalid_literal.md),
  naming the value worked out), so a construction in a constant never
  needs `attempt`. The executors run the constant's typed body, which
  can't fail, where it is read (docs/compiler-runtime.md §2.5).
- **Kept together.** `jadpo fmt` keeps constants written one after
  another on consecutive lines, as the source groups them, and puts a
  blank line between every other pair of declarations.
- **Unused is an error**, as for any other declaration: beside a primary
  declaration it is `layout.unused_declaration`, and in a library a
  constant nothing reads is unused as a function nothing calls is.

```jadpo check project=constants-built
type Stock = Int {
    min: 0
    max: 100000
}

type LargeOrder = Bool {}

type BatchCharged = Bool {}

constant small_batch: Stock = 250
constant large_batch: Stock = Stock(small_batch * 2)
constant batch_sizes: List<Stock> = [small_batch, large_batch]

function batch_chosen(large: LargeOrder) -> Stock {
    if (large) {
        return large_batch
    }
    return small_batch
}

function charged(batch: Stock) -> BatchCharged {
    return BatchCharged(batch in batch_sizes)
}
```

Each of these is one error with its edit:

```jadpo check project=e-constant-in-body expect=syntax.expected_statement
type Stock = Int {
    min: 0
}

function batch() -> Stock {
    constant small_batch: Stock = 250
    return small_batch
}
```

```text output
error: `constant` doesn't start a statement
 --> main.jadpo:6:5
  |
6 |     constant small_batch: Stock = 250
  |     ^^^^^^^^ write `var`
  reason: A constant lives at the top of a file or in a library; inside a body a value that never changes is a `var`.
  next: Use `var` instead of `constant` (agent)
  rule: syntax.expected_statement
  help: docs/errors/syntax.expected_statement.md

check failed: 1 error, 1 file
```

```jadpo check project=e-constant-mut expect=syntax.expected_name
type Stock = Int {
    min: 0
}

constant mut small_batch: Stock = 250
```

```text output
error: A constant can't be `mut`
 --> main.jadpo:5:10
  |
5 | constant mut small_batch: Stock = 250
  |          ^^^ remove `mut`
  reason: A constant never changes, so it has no mutable form; a value that changes is a `var mut` inside a body.
  next: Remove `mut` (agent)
  rule: syntax.expected_name
  help: docs/errors/syntax.expected_name.md

check failed: 1 error, 1 file
```

```jadpo check project=e-const-keyword expect=syntax.variable_keyword
type Stock = Int {
    min: 0
}

const small_batch: Stock = 250
```

```text output
error: Jadpo writes `constant`, not `const`
 --> main.jadpo:5:1
  |
5 | const small_batch: Stock = 250
  | ^^^^^ write `constant`
  reason: `const` declares a value that never changes in JavaScript, TypeScript, Rust, Go and C#; at the top of a Jadpo file a fixed value is `constant name: Type = value`.
  next: Use `constant` instead of `const` (agent)
  rule: syntax.variable_keyword
  help: docs/errors/syntax.variable_keyword.md

check failed: 1 error, 1 file
```

```jadpo check project=e-constant-built-in expect=type.primitive_signature
constant max_login_attempts: Int = 5
```

```text output
error: This constant uses `Int` directly
 --> main.jadpo:1:30
  |
1 | constant max_login_attempts: Int = 5
  |                              ^^^ a built-in type
  reason: A constant's type is named as a signature's is, so its value says what it means and carries its rules; `Int` says neither.
  next: Declare a named type built on `Int` and use it here (agent)
  rule: type.primitive_signature
  help: docs/errors/type.primitive_signature.md

check failed: 1 error, 1 file
```

### Where a constant lives

Placement and sharing follow helper functions exactly (area 1, "Sharing and
promotion"). A constant used by one file lives at the top of that file,
beside the declaration that uses it; it is never a file's primary
declaration, so there is no `constants/` folder, and its name never clashes
with a file name (`constant vat_rate: VatRate` is fine;
`naming.folded_collision` leaves constants out). When a second file
uses it, `layout.promotion_required` moves it into a library, and it is read
through the library like a library function, without brackets:

```jadpo check project=constants-library
type Stock = Int {
    min: 0
}

type HoldbackRate = Decimal {
    scale: 4
    min: 0.0000
    max: 1.0000
}

library stocking {
    /// The standard share held back.
    constant holdback_rate: HoldbackRate = 0.2000

    function holdback_of(per_pack: Stock) -> Stock {
        return Stock(per_pack / 5)
    }
}

function standard_holdback() -> HoldbackRate {
    var uk_rate: HoldbackRate = stocking.holdback_rate
    return uk_rate
}

function holdback(per_pack: Stock) -> Stock {
    return stocking.holdback_of(per_pack)
}
```

A library's own functions and constants read its constants bare, as they
call its functions. A library used by a second domain moves to
`shared/libraries/`, or stays and becomes `public` when it depends on its
domain's types, as for functions. A private constant needs a name unique
only within its domain, as private functions and types do (area 1, "Files
and names"), and its identity key carries its domain,
`constant billing.free_delivery_from` (area 18).

### Where a constant can be read

Anywhere a value can, functions included, because it never changes. It is
also what "constants" already means in the places that allow them:

- **Queries** accept a constant wherever they accept a literal (area 6,
  `query.value_unsupported`).
- **An update by `id`** may use parameters, constants, `clock.now` and the
  stored fields (area 6).
- **A computed backfill** may use the row's stored fields, constants and
  pure functions (area 5).

A setting's `default:` and `test:` take a literal but never a constant
(see [A constant or a setting](#a-constant-or-a-setting)).

### Two constants holding one value

Agents working in parallel tend to invent the same constant twice, each
writing its own `vat_rate`. Two constants of the same type holding the same
value are an error, [`constant.similar_exists`](../errors/constant.similar_exists.md),
modelled on
[`library.similar_exists`](../errors/library.similar_exists.md),
naming the other constant and its file so the agent reads that one and
removes its own. The first in file and source order is the one named; the
others are reported. It is declinable (area 1): a VAT rate and a fee can share a value by coincidence, so a
`decline constant.similar_exists: "<why>"` line above the constant
answers it instead. The library catalogue
(area 14) lists constants with their type, value, `///` description and
readers.

### A constant or a setting

If a deployment could ever need a different value, it is a setting in
`config.jadpo` (area 10); otherwise it is a constant. A page size an
operator might tune is `config`; the number of pence in a pound is a
constant. The agent skill gives the same rule (area 14).

A value is one or the other, never both, so it has one source and one way
to read it: a constant is fixed and read as `security.max_login_attempts`,
a setting is chosen per deployment and read as `config.max_login_attempts`.
A setting's `default:` therefore never names a constant (area 10,
`configuration.default_literal_required`). If
a constant could seed a setting, code reading the constant directly would
quietly ignore the environment: with `security.standard_attempts` (5) as
`max_login_attempts`'s default, someone sets `MAX_LOGIN_ATTEMPTS=10`, and
code reading `security.standard_attempts` still allows 5. That is the
familiar `DEFAULT_PORT` bug of TypeScript and pydantic code, and Jadpo has
no need for the pattern, because `default:` on the field already names the
default.

The three tiers, from fixed to live: a `constant` is fixed in code; a
`config` setting is chosen per deployment (area 10); a flag in `flags { }`
changes while the app runs (area 16).

**Making a constant a setting** is a move a tool makes, `jadpo config move
<constant>` or the editor's "Make this a setting" (area 14). It adds a field
of the same name and type to `config.jadpo` whose `default:` is the
constant's value, deletes the constant, and rewrites every read
(`security.max_login_attempts` becomes `config.max_login_attempts`), so the
reviewer sees each place whose behaviour can now differ per deployment. It
refuses, editing nothing, a constant whose type a setting can't hold, one
whose name a setting already has, and one read where a setting can't be (a
query, an update by `id`, a backfill or another constant's value). The
details are in area 10, [A constant or a setting](10-configuration-and-time.md#a-constant-or-a-setting).

### Values that change on a known date

Guidance, not built into the language (there is no `effective:` setting).
A value that changes on a known date, such as the UK's VAT rate, is a
constant holding a dated list, read through a library function:

```jadpo check project=vat-bands
type VatRate = Decimal {
    scale: 4
    min: 0.0000
    max: 1.0000
}

type VatDate = CalendarDate {}

type VatBand = Object {
    valid_from: VatDate
    band_rate: VatRate
}

type VatBands = List<VatBand> {
    min_length: 1
}

library vat {
    constant uk_bands: VatBands = [
        VatBand {
            valid_from: "2010-01-01"
            band_rate: 0.1750
        },
        VatBand {
            valid_from: "2011-01-04"
            band_rate: 0.2000
        }
    ]

    /// The UK standard rate on a date.
    function rate_on(day: VatDate) -> VatRate {
        var mut found: VatRate = 0.0000
        for band in uk_bands {
            if (band.valid_from <= day) {
                found = band.band_rate
            }
        }
        return found
    }
}

function vat_rate_on(day: VatDate) -> VatRate {
    var vat_due_rate: VatRate = vat.rate_on(day)
    return vat_due_rate
}
```

- A change of rate is an edit to the list, reviewed as a diff, and tested
  with the test clock (area 13) on each side of the date.
- Make it an entity only when someone who isn't a developer must change it
  while the app runs.
- A record stores the rate it applied (an invoice's `vat_rate` field), so
  a later change never rewrites what was charged.

### Rejected

- **A top-level `var`.** `var` would mean worked out while running in a
  body and while compiling at the top of a file.
- **Functions with no arguments.** Queries and updates by `id` refuse
  calls, and a call looks like a calculation, not a fixed value.
- **Values attached to their type, `Rate.standard_vat`.** A one-off value
  would need a type of its own, and one block per type clashes when agents
  work in parallel.
- **A root `constants { }` block.** A grab-bag, the one file every change
  touches, and against domains owning their code.

## Statements
<!-- topic: statements -->

A body is a list of statements, one per line, with no semicolons:

| Statement | Form |
|---|---|
| Variable | `var name: T = value`, `var mut name: T = value` |
| Assignment | `name = value` |
| Return | `return value` |
| Rejection | `reject Failure { ... }` (area 4) |
| Condition | `if (condition) { } else if (condition) { } else { }` |
| Match | `match value { ... }` |
| Loop | `for item in items { }` (area 2) |
| Test only | `assert`, `advance clock by` (area 13) |

There is no `throw`. Three more statements may stand alone (question 4), so
a result nobody needs never has to be bound to a name:

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
Conditions are always in brackets, and `else if` chains are allowed
(question 5).

## Expressions and operators
<!-- topic: expressions-and-operators -->

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
same named type after widening, or one is a literal, as `==` compares
them. `*` and `%` still mix.

Operators work on the representation, so their result is a plain `Int`,
`Decimal`, `Text` or `Bool`, and turning it back into a named type is a
constructor (area 2). Parentheses group. `and` and `or` stop as soon as the
answer is known. `a < b < c` is an error, because `a < b` is a `Bool`.
Logical operators are words; `!`, `&&` and `||` are not Jadpo, and each is
one error whose edit writes the word (`syntax.logical_operator`). Nor are
`===` and `!==`: `==` and `!=` already compare values of one type, so each
is one error whose edit writes `==` or `!=` (`syntax.equality_operator`),
as are SQL's `<>`, `=` and `IS NULL` ([Habits from other
languages](#habits-from-other-languages)).

Operators on standard types follow area 19's standard operator table
(`price * 20%`, `distance / 2h`), and anything not in it, or in the table
above, is a compile error naming the two types (area 19, "`Money` and the operator table").
`20%` is a `Percentage` literal; remainder keeps `%` written with spaces,
as the formatter writes every binary operator, and `20%3` is
`syntax.percent_ambiguous`, naming both readings. `Rounding` has `down`,
toward zero (area 19, "`Rounding.down`").

Operator typing and division (questions 6 and 7):

```jadpo fragment
var wrong: Stock = Stock(per_pack + packs)       // error: adds a Quantity to Stock
var line: Stock = Stock(per_pack * packs)        // fine: multiplying different units
var earlier: Bool = name < other_name             // error: text has no order; use a standard function
var share: ItemPrice = decimal.divide(total, people, rounding: Rounding.half_even)
var boxes: BoxCount = integer.divide(items, box_size, rounding: Rounding.up)   // box_size can't be zero
```

## Match
<!-- topic: match -->

`match` chooses between the cases of a value. It must cover every case, and it
can be used as a statement or as an expression. Its arms use `->`, the same
arrow as result types (area 2).

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
<!-- topic: attempt -->

A call that can fail must say so at the call site. `attempt` passes the
call's failures on to the caller, whose `fails` list must include them;
`match` on the call handles them instead.

```jadpo fragment
var todo: Todo = attempt Todo.get(todo_id)            // a variable
return attempt todo_by_id(todo_id)                    // a return
attempt Todo.get(todo_id)                             // on its own (question 4)
var total: Stock = Stock(attempt stock_of(a) + attempt stock_of(b))   // rejected: inside an expression
```

`attempt` only starts a statement (question 11), so every point where a
callable can stop early is at the start of a line, and `attempt` on a call
that cannot fail is an error. `reject` and `propagate` follow the same
rule: each starts a statement or is the result of a `match` arm, never part
of a value (`failure.reject_placement`, `failure.propagate_placement`).

## Loops
<!-- topic: loops -->

`for item in items` iterates a list, and the loop variable takes its type
from the list (area 2).

```jadpo check project=order-total file=lines.jadpo hidden
type Stock = Int {
    min: 0
}

type OrderLine = Object {
    needed: Stock
}
```

```jadpo check project=order-total
function stock_total(lines: List<OrderLine>) -> Stock {
    var mut total: Stock = 0
    for line in lines {
        total = Stock(total + line.needed)
    }
    return total
}
```

`for` is the only loop (question 12): no `while`, `break` or `continue`,
with `return` allowed inside the body. It also walks a set, in area 2's
canonical order, the same on every target, and a map through its keys,
`for loaf in map.keys(stock)` (area 19, "Sets and maps"). A map itself is not
walked, since its keys and values would need a pair; the error's edit
writes `map.keys`:

```jadpo check project=e-loop-over-map expect=type.mismatch
type Loaf = Text {
    min_length: 1
}

type Stock = Int {
    min: 0
}

function loaves(stock: Map<Loaf, Stock>) -> List<Loaf> {
    var mut out: List<Loaf> = []
    for loaf in stock {
        out = list.append(out, loaf)
    }
    return out
}
```

```text output
error: `Map<Loaf, Stock>` can't be looped over
 --> main.jadpo:11:17
   |
11 |     for loaf in stock {
   |                 ^^^^^ this is `Map<Loaf, Stock>`
  reason: `for` loops over a `List` or a `Set`; a map is walked through its keys, a `Set<Loaf>`, and each key's value read with `map.get`.
  next: Loop over `map.keys(stock)` (agent)
  rule: type.mismatch
  help: docs/errors/type.mismatch.md

check failed: 1 error, 1 file
```

**No outside calls inside `for`** (area 12's multi-step calls). A provider operation, a package action, a file
store's `put` or `read`, or a `documents` render inside a loop's body is a
compile error, in any action, as a query inside one already is (area 6).
A loop of calls is `batches.each` (area 12), the one way; a provider that
takes a list is called once with the list. Keeping calls out of
loops means the steps of an action that makes several changing calls lie
along a path whose length the compiler knows. The error is
[`effect.call_in_loop`](../errors/effect.call_in_loop.md), at the call: any outside
call, one that only reads included, made in a loop's body or by an action
called there, in an action or a fixture.

## Passing functions
<!-- topic: passing-functions -->

Inline functions are allowed only as arguments to standard-library
operations, such as `list.map` and `list.filter`, using `->` for
"produces" (question 13):

```jadpo fragment
var open: List<Todo> = list.filter(todos, todo -> todo.status == TodoStatus.open)
var titles: List<TodoTitle> = list.map(todos, todo -> todo.title)
```

The inline function is one expression, takes its parameter's type from the
list, may read the variables around it (safe because values never change),
and cannot call actions or use `attempt`. Functions are not values: they
cannot be stored in variables or passed to your own callables.

The collection operations are `list.`, `set.` and `map.`, and those that
take an inline function include `any`, `all`, `find`, `min_by`, `max_by`,
`sort_by` and `group_by` (area 19, "One namespace per type", "Sets and maps" and "Language gaps this page depends on"). Text is put in
order with `list.sort_by` and a text key. For standard operations only, an
inline function may take two parameters, `(key, value) -> ...`
(`list.zip_with`, `map.filter`, `map.map_values`). The parameters are
written in parentheses, separated by commas, and each takes the type of the
value the operation gives it in that place, `list.zip_with` an item from
each list and `map.filter` and `map.map_values` an entry's key and its
value. An inline function with a different number of parameters than its
operation gives is `type.inline_function_parameters`, and one with two
anywhere else is `type.inline_function_placement`, as for one:

```jadpo check project=inline-two-parameters
type Loaf = Text {
    min_length: 1
}

type Stock = Int {
    min: 0
}

type Sold = Int {}

/// The loaves with some left, given each loaf and its stock.
function in_stock(stock: Map<Loaf, Stock>) -> Map<Loaf, Stock> {
    return map.filter(stock, (loaf, count) -> count > 0)
}

/// How many of each loaf sold, from the morning's and the evening's
/// counts, taken in the same order.
function sold(morning: List<Stock>, evening: List<Stock>) -> List<Sold> {
    return list.zip_with(morning, evening, (before, after) -> Sold(before - after))
}
```

No declaration's name is a value. A type, enum, failure, entity,
function, action, query, library or route group, or a standard namespace such
as `list`, written where a value is read is `type.not_a_value`: in a
variable's value, an argument, a returned value, a field or list item, an
operand, a condition, a `match` subject, a `set:`, a route's `run:`, a query's
condition, an input's `default:` or a test's expectation. A value is built
(`Placed { ... }`, `Count(5)`), is one of an enum's variants (`Status.open`),
is read from a variable, a setting (`config.page_size`) or the clock
(`clock.now`), or is called (`helper(...)`); the error's `next:` gives the one
that fits the name.

**The batch helpers take the same form** (area 12): `each:` and `finished:` of `csv.read`, `csv.write`, `tsv`,
`json_lines`, `json.read`, `json.write` and `batches.each` are inline
functions written with `->`. They differ from the collection operations in
two ways, because they run later, in the background, after the calling
action commits:

```jadpo fragment
csv.read(
    import.sheet,
    as: ProductRow,
    each: row -> save_product(row, account: import.account_id),
    finished: summary -> import_finished(summary, import: import.id)
)
```

- Where there is no destination (`each:` when reading, `batches.each`'s
  `each:`, and every `finished:`), the body is one call to an action in the
  same domain, as a subscriber's `run:` is; where the helper writes,
  `each:` is a pure expression giving the row to write
  (`product -> ProductExportRow { ... }`).
- Values the body captures from around it are saved with the run, so only
  storable values may be captured (those an entity field could hold).
  Capturing anything else is a compile error naming the value.

For `batches.each` an inline function is a named argument's value too, and
the body's arguments are the item, a field inside it, or a captured value,
which `type.batch_each_invalid` and `type.batch_capture_not_storable` hold
it to (area 12, "How a run is kept").

## Recursion
<!-- topic: recursion -->

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

Recursion is allowed in functions only, with a runtime depth limit that
ends in a defined fault (question 14). The limit is the same for every
executor and target: a thousand frames, the test's or the route's action's
own among them, and four hundred inline functions running at once, one
inside another's body through the functions it calls. Past either, the
call or the operation that would go deeper is the `recursion_limit` fault
at its site, in `jadpo test` and `jadpo dev` as in a deployable, never a
crash. Inline functions have the lower limit because compiled code runs
one inside its operation, several of the runtime's frames deep, and on
Workers those take V8's own stack, about a megabyte that nothing in the
module can grow. So `reply_count` above walks a thread a thousand replies
deep, and the same walk written as `list.map(comment.replies, reply -> ...)`
four hundred. A native deployable's threads have the stack `jadpo test`'s
have, 64 MiB, and a Workers module is linked with 8 MiB of its own, which a
thousand frames of a large function fit; in a module, a callable entered
with less than 256 KiB of it left is the `recursion_limit` fault too, so a
frame larger still stops there rather than writing past the stack's end.

## Generic code
<!-- topic: generic-code -->

Only the standard library is generic (`List<T>`, `list.count`). Your own
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
not Jadpo: your own code has no generics or unions (question 16). For the
same reason, an application's types never declare operators, so every
`a + b` is a row of the operator tables a reviewer can look up, and never an
overload (area 19, "`Money` and the operator table").

## What is built
<!-- topic: logic-built -->

| Feature | Status |
|---|---|
| Everything on this page, [Constants](#constants) included | Built: every example on this page and every error's example is checked against the compiler on each gate run. |

## Errors

Every error this area can produce, with an example that triggers it and the
compiler's actual output.

- [`constant.value_not_fixed`](../errors/constant.value_not_fixed.md)
- [`constant.cycle`](../errors/constant.cycle.md)
- [`constant.similar_exists`](../errors/constant.similar_exists.md)
- [`effect.call_in_loop`](../errors/effect.call_in_loop.md)
- [`effect.function_calls_action`](../errors/effect.function_calls_action.md)
- [`effect.function_persistence`](../errors/effect.function_persistence.md)
- [`effect.recursive_action`](../errors/effect.recursive_action.md)
- [`failure.attempt_required`](../errors/failure.attempt_required.md)
- [`failure.attempt_infallible`](../errors/failure.attempt_infallible.md)
- [`failure.outcome_missing_arm`](../errors/failure.outcome_missing_arm.md)
- [`failure.outcome_duplicate_arm`](../errors/failure.outcome_duplicate_arm.md)
- [`failure.outcome_unknown_arm`](../errors/failure.outcome_unknown_arm.md)
- [`failure.outcome_wildcard`](../errors/failure.outcome_wildcard.md)
- [`failure.outcome_success_missing`](../errors/failure.outcome_success_missing.md)
- [`failure.outcome_success_duplicate`](../errors/failure.outcome_success_duplicate.md)
- [`failure.outcome_success_value_required`](../errors/failure.outcome_success_value_required.md)
- [`failure.outcome_pattern_invalid`](../errors/failure.outcome_pattern_invalid.md)
- [`failure.outcome_infallible`](../errors/failure.outcome_infallible.md)
- [`failure.outcome_subject_required`](../errors/failure.outcome_subject_required.md)
- [`failure.propagate_placement`](../errors/failure.propagate_placement.md)
- [`failure.reject_placement`](../errors/failure.reject_placement.md)
- [`syntax.match_arrow`](../errors/syntax.match_arrow.md)
- [`syntax.match_pattern`](../errors/syntax.match_pattern.md)
- [`type.match_non_exhaustive`](../errors/type.match_non_exhaustive.md)
- [`type.match_duplicate_pattern`](../errors/type.match_duplicate_pattern.md)
- [`type.match_unknown_variant`](../errors/type.match_unknown_variant.md)
- [`type.match_unreachable_pattern`](../errors/type.match_unreachable_pattern.md)
- [`type.match_wildcard_required`](../errors/type.match_wildcard_required.md)
- [`type.match_wildcard_closed`](../errors/type.match_wildcard_closed.md)
- [`type.match_pattern_type`](../errors/type.match_pattern_type.md)
- [`type.match_some_non_optional`](../errors/type.match_some_non_optional.md)
- [`type.match_unknown_binding`](../errors/type.match_unknown_binding.md)
- [`type.match_arm_value`](../errors/type.match_arm_value.md)
- [`type.arithmetic_operand`](../errors/type.arithmetic_operand.md)
- [`type.ordering_operand`](../errors/type.ordering_operand.md)
- [`type.incomparable`](../errors/type.incomparable.md)
- [`type.logical_operand`](../errors/type.logical_operand.md)
- [`type.unary_operand`](../errors/type.unary_operand.md)
- [`type.divisor_zero`](../errors/type.divisor_zero.md)
- [`type.assign_immutable`](../errors/type.assign_immutable.md)
- [`type.assign_unknown`](../errors/type.assign_unknown.md)
- [`type.unknown_value`](../errors/type.unknown_value.md)
- [`type.not_a_value`](../errors/type.not_a_value.md)
- [`semantic.unused_variable`](../errors/semantic.unused_variable.md)
- [`semantic.unused_result`](../errors/semantic.unused_result.md)
- [`semantic.unreachable_code`](../errors/semantic.unreachable_code.md)
- [`semantic.missing_return`](../errors/semantic.missing_return.md)
- [`type.argument_count`](../errors/type.argument_count.md)
- [`type.named_argument_unsupported`](../errors/type.named_argument_unsupported.md)
- [`type.named_argument_unknown`](../errors/type.named_argument_unknown.md)
- [`type.inline_function_parameters`](../errors/type.inline_function_parameters.md)
- [`type.inline_function_placement`](../errors/type.inline_function_placement.md)
- [`toolchain.not_built_yet`](../errors/toolchain.not_built_yet.md)
- [`syntax.named_argument_duplicate`](../errors/syntax.named_argument_duplicate.md)
- [`syntax.positional_after_named`](../errors/syntax.positional_after_named.md)
- [`syntax.expected_statement`](../errors/syntax.expected_statement.md)
- [`syntax.attempt_placement`](../errors/syntax.attempt_placement.md)
- [`syntax.colon_required`](../errors/syntax.colon_required.md)
- [`syntax.condition_brackets`](../errors/syntax.condition_brackets.md)
- [`syntax.expected_expression`](../errors/syntax.expected_expression.md)
### Habits from other languages

People and agents write constructs from other languages out of habit. Each has an error of its own that names
the habit and the Jadpo form, with an edit, and parsing carries on as if
the Jadpo form had been written. A syntax error never brings a second on
its line: anything else the line holds is reported once the first is
fixed, so `pub fn total(): Stock {` shows `pub`, then `fn`, then `:`.
When the Jadpo form only renames or drops words (`let` for `var`, `&&` for
`and`, a `;` at the end of a line), the tree is the one the Jadpo form
gives and the rest of the project is checked too, leaving out only
problems on the habit's line. The catalogue is `jadpo_syntax::habits`,
and a corpus of broken files checks one error a line
(`crates/jadpo-syntax/tests/broken/`).

- [`syntax.variable_keyword`](../errors/syntax.variable_keyword.md)
- [`syntax.function_keyword`](../errors/syntax.function_keyword.md)
- [`syntax.return_type_arrow`](../errors/syntax.return_type_arrow.md)
- [`syntax.async_await`](../errors/syntax.async_await.md)
- [`syntax.semicolon`](../errors/syntax.semicolon.md)
- [`syntax.logical_operator`](../errors/syntax.logical_operator.md)
- [`syntax.ternary_operator`](../errors/syntax.ternary_operator.md)
- [`syntax.increment_operator`](../errors/syntax.increment_operator.md)
- [`syntax.else_if`](../errors/syntax.else_if.md)
- [`syntax.for_loop`](../errors/syntax.for_loop.md)
- [`syntax.switch_statement`](../errors/syntax.switch_statement.md)
- [`syntax.try_catch`](../errors/syntax.try_catch.md)
- [`syntax.inline_function_arrow`](../errors/syntax.inline_function_arrow.md)
- [`syntax.named_argument_equals`](../errors/syntax.named_argument_equals.md)
- [`syntax.member_comma`](../errors/syntax.member_comma.md)
- [`syntax.equality_operator`](../errors/syntax.equality_operator.md)
- [`syntax.unsupported_throw`](../errors/syntax.unsupported_throw.md)

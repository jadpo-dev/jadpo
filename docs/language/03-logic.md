---
title: Logic
area: 3
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Logic
<!-- topic: logic -->

How you write what your app does: functions and actions, variables and
constants, statements, operators, `match`, `attempt` and loops, and what
Jadpo allows for passing functions, recursion and generic code.

## Functions and actions
<!-- topic: functions-and-actions -->

You write behaviour in three kinds of callable:

- A **function** is pure: it works out a result from its arguments and
  nothing else.
- An **action** is an operation of your app. It may read and write stored
  data, call outside services and call other actions.
- A **query** reads stored data and never changes it
  ([Named queries](06-reads-and-writes.md#named-queries)).

Here is a function:

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

And an action, which loads a todo, changes it and returns a view of it:

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

Each kind may call only some of the others:

| Caller | May call | May not |
|---|---|---|
| `function` | functions, library functions, the standard library | actions, queries, stored data, outside services, `clock.now` |
| `query` | functions and queries | actions, writes |
| `action` | functions, queries and actions | |
| A route's `run:` | one action | a function or query directly ([What a route runs](07-http.md#what-a-route-runs)) |

A function may still fail: it can declare `fails` and `reject`
([Failures](04-failures.md)). There is no `async`, `await` or promise:
every call finishes before the caller carries on.

### Signatures and settings

Parameters are positional, and every one has a type. The result type
follows `->`, and `fails` comes before it. A callable that returns nothing
returns `Unit`. Parameters have no defaults.

```jadpo fragment
action reschedule(todo_id: Todo.id, due_at: DueAt) fails TodoNotFound, DueDateInPast -> TodoView {
    var todo: Todo = attempt Todo.get(todo_id)
    ...
}
```

Your own callables are called with positional arguments. Named arguments
exist only for standard-library operations, such as
`temporal.resolve(date: d, zone: z, ...)`.

The whole signature is on the header line, and an action has no settings,
because the safe choice is already the default:

- every action is all-or-nothing;
- every write checks that no one else changed the record since it was
  read;
- every read is current.

[All-or-nothing actions and concurrent changes](06-reads-and-writes.md#all-or-nothing-actions-and-concurrent-changes)
has the details. Who may call an action is set by
[Policy](09-policy.md), not by the action.

When a header is too long, the formatter puts one parameter on each line
and keeps `fails` and `->` after the closing bracket.

### Calling

What comes before the dot names the owner of what you call. Inside your
own domain (a business area of your app, a folder under `domains/`; see
[Project layout](01-project-layout-and-modules.md#project-layout)), you
call its actions and queries by name alone. A helper function used only
in its own file is called the same way.

A helper that several files share lives in a `library`, a named group of
functions and constants, and is called through it, as the standard
library is. Another domain's public actions are called through that
domain's name.

```jadpo fragment
library rounding {
    function half_even(amount: ItemPrice) -> ItemPrice { ... }
}

var total: ItemPrice = rounding.half_even(subtotal)
var line: Stock = stock_needed(per_pack, packs)
var stock: Reservation = attempt inventory.reserve_for_cart(cart)
var todo: Todo = attempt Todo.get(todo_id)
var local: ZonedTime = temporal.in_zone(at, zone)
```

In order, those calls are: a function in the `rounding` library, a helper
in this file, an action of the `inventory` domain, an operation every
entity gets, and the standard library.

A library holds only functions and constants (see [Constants](#constants)).

## Variables
<!-- topic: variables -->

A variable never changes unless you write `var mut`, and every variable
states its type ([Values and types](02-values-and-types.md)).

```jadpo fragment
var needed: Stock = stock_needed(per_pack, packs)
var mut total: Stock = needed
total = Stock(total + holdback)
```

- `var` names a value that never changes. `var mut` names one you can
  point at a new value of the same type, with `name = value`.
- Only a `var mut` local can be assigned. Parameters, fields and
  declarations can't, because values never change
  ([Values never change](02-values-and-types.md#values-never-change)).
- A variable is visible from where it is declared to the end of its block.
- A name already in scope, a parameter included, can't be declared again.
- A variable nothing reads is an error.

## Constants
<!-- topic: constants -->

A `constant` names a fixed value at the top of a file. It is the same in
every deployment, worked out while compiling, and never changes.

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

- **Form.** `constant name: Type = value`. The name is lower_snake_case,
  like any value. There is no mutable form: `constant mut` is
  `syntax.expected_name`, whose edit removes `mut`.
- **Only at the top of a file.** Inside a body a fixed value is always a
  `var`, and at the top of a file always a `constant`, so each place has
  one way to write it. `constant` in a body is
  `syntax.expected_statement`, whose edit writes `var`. A `var` at the top
  of a file is `syntax.expected_declaration`, whose `next:` says to write
  `constant`. `const` at the top of a file is `syntax.variable_keyword`,
  whose edit writes `constant`; in a body it is also
  [`syntax.variable_keyword`](../errors/syntax.variable_keyword.md), whose
  edit writes `var`.
- **A named type.** A constant's type follows the same rule as a
  signature's ([Signatures use named types](02-values-and-types.md#signatures-use-named-types)):
  `constant limit: Int = 5` is `type.primitive_signature`, as `limit: Int`
  is in a parameter. A list of a named type, an object, an enum, `Email`,
  `Url` and the other types a signature may use are all fine.
- **A value the compiler can work out.** Literals, enum values, other
  constants, values constructed from those (`Stock(5000)`, an object, a
  list), and the operators of
  [Expressions and operators](#expressions-and-operators) on them. No call
  of any kind (functions, library functions, the standard library), no
  `config`, no `clock`, and no constant that depends on itself
  ([`constant.value_not_fixed`](../errors/constant.value_not_fixed.md),
  [`constant.cycle`](../errors/constant.cycle.md)). Arithmetic gives the
  built-in type underneath, as it does in a body, so `Stock(bulk_from * 2)`
  constructs a `Stock` from it again.
- **Checked while compiling.** The value is checked against every rule of
  its type and of the types it is built on, as a literal is
  ([`type.invalid_literal`](../errors/type.invalid_literal.md), which
  names the value worked out). So constructing a value in a constant never
  needs `attempt`. When the app runs, the constant's value is worked out
  where it is read, and that can't fail.
- **Kept together.** `jadpo fmt` keeps constants written one after another
  on consecutive lines, as you grouped them, and puts a blank line between
  every other pair of declarations.
- **Unused is an error**, as for any other declaration. Beside a file's
  primary declaration (the one the file is named after) it is
  `layout.unused_declaration`. In a library, a constant nothing reads is
  unused, as a function nothing calls is.

Constants built from other constants, a list of them, and both read in
functions:

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

Each of these mistakes is one error, with its edit:

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
  help: https://jadpo.dev/docs/errors/syntax.expected_statement

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
  help: https://jadpo.dev/docs/errors/syntax.expected_name

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
  help: https://jadpo.dev/docs/errors/syntax.variable_keyword

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
  help: https://jadpo.dev/docs/errors/type.primitive_signature

check failed: 1 error, 1 file
```

### Where a constant lives

A constant goes where a helper function would
([Sharing and promotion](01-project-layout-and-modules.md#sharing-and-promotion)).
A constant one file uses lives at the top of that file, beside the
declaration that uses it. It is never a file's primary declaration, so
there is no `constants/` folder, and its name never clashes with a file's
name: `constant vat_rate: VatRate` is fine, because
`naming.folded_collision` leaves constants out.

When a second file uses it, `layout.promotion_required` asks you to move
it into a library. Code then reads it through the library, like a library
function but without brackets:

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

A library's own functions and constants read its constants by name alone,
as they call its functions.

A library a second domain uses moves to `shared/libraries/`, or, when it
depends on its domain's types, stays and becomes `public`, as it would for
functions. A private constant's name must be unique only within its
domain, as private functions' and types' names are
([Files and names](01-project-layout-and-modules.md#files-and-names)). Its
identity key, the name Jadpo's records know it by
([Stable identity](18-records-and-manifest.md#stable-identity)), carries
its domain: `constant billing.free_delivery_from`.

### Where a constant can be read

Anywhere a value can, functions included, because it never changes. It is
also what "constants" means in the places that allow only fixed values:

- **A query** accepts a constant wherever it accepts a literal; anything
  else there is `query.value_unsupported`
  ([Named queries](06-reads-and-writes.md#named-queries)).
- **An update by `id`** may use parameters, constants, `clock.now` and the
  stored fields
  ([The operations every entity gets](06-reads-and-writes.md#the-operations-every-entity-gets)).
- **A computed backfill** may use the record's stored fields, constants
  and pure functions ([Computed backfills](05-data.md#computed-backfills)).

A setting's `default:` and `test:` take a literal, never a constant (see
[A constant or a setting](#a-constant-or-a-setting)).

### Two constants holding one value

Agents working at the same time tend to invent the same constant twice,
each writing its own `vat_rate`. Two constants of the same type holding
the same value are an error,
[`constant.similar_exists`](../errors/constant.similar_exists.md), as two
alike functions are
[`library.similar_exists`](../errors/library.similar_exists.md). The error
names the other constant and its file, so the agent reads that one and
removes its own. The first in file and source order is the one named, and
the others are reported.

A VAT rate and a fee can share a value by coincidence, so the error is a
judgement call you can decline: a `decline constant.similar_exists:
"<why>"` line above the constant answers it
([Declining a judgement call](01-project-layout-and-modules.md#declining-a-judgement-call)).

The library catalogue, the list of shared code that `jadpo check` writes
into `build/catalogue/` ([Tooling](14-tooling.md)), lists each constant
with its type, value, `///` description and the code that reads it.

### A constant or a setting

If a deployment could ever need a different value, make it a setting in
`config.jadpo` ([Configuration and time](10-configuration-and-time.md));
otherwise make it a constant. A page size an operator might tune is a
setting. The number of pence in a pound is a constant. The Jadpo agent
skill gives agents the same rule
([Tools for agents](14-tooling.md#tools-for-agents)).

A value is one or the other, never both, so it has one source and one way
to read it. A constant is fixed and read as
`security.max_login_attempts`. A setting is chosen per deployment and read
as `config.max_login_attempts`.

So a setting's `default:` never names a constant
(`configuration.default_literal_required`). If it could, code reading the
constant directly would quietly ignore the deployment's value. Say
`security.standard_attempts` (5) were `max_login_attempts`'s default, and
someone set `MAX_LOGIN_ATTEMPTS=10`: code reading
`security.standard_attempts` would still allow 5. You don't need the
constant anyway, because the setting's `default:` already holds the
default.

From fixed to live, there are three kinds of value:

- a `constant` is fixed in the code;
- a `config` setting is chosen per deployment;
- a flag in `flags { }` changes while the app runs
  ([Feature flags](16-built-in-services.md#feature-flags)).

**Making a constant a setting** is a job for a tool: `jadpo config move
<constant>`, or the editor's "Make this a setting"
([Tooling](14-tooling.md)). It adds a field of the same name and type to
`config.jadpo` whose `default:` is the constant's value, deletes the
constant, and rewrites every read (`security.max_login_attempts` becomes
`config.max_login_attempts`). The reviewer then sees each place whose
behaviour can now differ per deployment.

It refuses, editing nothing, a constant:

- whose type a setting can't hold;
- whose name a setting already has;
- read where a setting can't be: in a query, an update by `id`, a backfill
  or another constant's value.

The details are in
[Making a constant a setting](10-configuration-and-time.md#making-a-constant-a-setting).

### Values that change on a known date

Some values change on a date you know in advance, such as the UK's VAT
rate. Jadpo has no setting for this (there is no `effective:`). Write a
constant holding a dated list, and read it through a library function:

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

- A change of rate is an edit to the list, reviewed as a diff. Test it
  with the test clock on each side of the date ([Testing](13-testing.md)).
- Make it an entity only when someone who isn't a developer must change it
  while the app runs.
- A record stores the rate it used (an invoice's `vat_rate` field), so a
  later change never rewrites what was charged.

## Statements
<!-- topic: statements -->

A body is a list of statements, one on each line, with no semicolons:

| Statement | Form |
|---|---|
| Variable | `var name: T = value`, `var mut name: T = value` |
| Assignment | `name = value` |
| Return | `return value` |
| Rejection | `reject Failure { ... }` ([Raising a failure: `reject`](04-failures.md#raising-a-failure-reject)) |
| Condition | `if (condition) { } else if (condition) { } else { }` |
| Match | `match value { ... }` |
| Loop | `for item in items { }` ([Loops](#loops)) |
| Test only | `assert`, `advance clock by` ([Testing](13-testing.md)) |

There is no `throw`.

A variable nothing reads is an error, so a result nobody needs never has
to be given a name. Instead, three more statements may stand alone:

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

- `attempt Todo.get(todo_id)` on its own means "this must succeed". If it
  fails, the failure is passed on; if not, the action carries on, and the
  value isn't needed.
- A bare `return` leaves an action or function that returns `Unit` early.
- A call to an action that returns `Unit`, such as `notify_owner`, is made
  for what it does.

A function call standing alone is an error
([`semantic.unused_result`](../errors/semantic.unused_result.md)): its
value is thrown away, so it does nothing.

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

The condition must be a `Bool`, or a type built on `Bool`. Nothing else
counts as true or false. A condition is always in brackets, and `else if`
chains are allowed.

`if` is a statement, not a value. To choose between two values, use a
[`match`](#match).

## Expressions and operators
<!-- topic: expressions-and-operators -->

| Precedence | Operators | Operands | Result |
|---|---|---|---|
| 1 (tightest) | `not`, unary `-` | `Bool`; a number | same |
| 2 | `*`, `/`, `%` | `*` and `%` may mix named types; `/` needs the same named type: `Int` drops the remainder, `Decimal` is exact | `Int`, or `Decimal` if either side is `Decimal` |
| 3 | `+`, `-` | the same named type after widening, or a literal; `+` also joins two `Text` values | as above |
| 4 | `<`, `<=`, `>`, `>=` | two numbers or two time values of the same named type; text has no order | `Bool` |
| 5 | `==`, `!=` | two values of the same type after widening, or a nullable value and `none` | `Bool` |
| 6 | `and` | `Bool` | `Bool` |
| 7 (loosest) | `or` | `Bool` | `Bool` |

"Widening" is one type standing in for another it is built on
([When one type can stand in for another](02-values-and-types.md#when-one-type-can-stand-in-for-another)).

Adding two values needs the same type on each side, and multiplying may
mix them, because multiplying a price by a quantity means something and
adding them doesn't:

```jadpo fragment
var wrong: Stock = Stock(per_pack + packs)
var line: Stock = Stock(per_pack * packs)
var earlier: Bool = name < other_name
var share: ItemPrice = decimal.divide(total, people, rounding: Rounding.half_even)
var boxes: BoxCount = integer.divide(items, box_size, rounding: Rounding.up)
```

- `per_pack + packs` is an error: it adds a `Quantity` to a `Stock`.
- `per_pack * packs` is fine: it multiplies different units.
- `name < other_name` is an error: text has no order, because the order
  depends on the language. Sort text with `list.sort_by` and a text key.
- `decimal.divide` and `integer.divide` divide values of different types,
  or round another way, with the rounding you name.
- `box_size`'s type excludes zero. A divisor's type must, so dividing by
  zero is ruled out while compiling
  ([`type.divisor_zero`](../errors/type.divisor_zero.md)).

In `+`, `-` and `/`, a literal takes the other side's type, exactly as it
does in `==`. So `price + 3` with a `Decimal` price is an error, as
`price == 3` is: write `3.00`. `+` joins two `Text` values only when they
are the same named type after widening, or one is a literal, as `==`
compares them. `*` and `%` still mix.

Operators work on the built-in type a named type is built on, so the
result is a plain `Int`, `Decimal`, `Text` or `Bool`. To turn it back into
a named type, construct it: `Stock(total + holdback)`
([Creating values](02-values-and-types.md#creating-values)). Brackets
group. `and` and `or` stop as soon as the answer is known. `a < b < c` is
an error, because `a < b` is a `Bool`.

Logical operators are words. `!`, `&&` and `||` are not Jadpo, and each is
one error whose edit writes the word (`syntax.logical_operator`). Nor are
`===` and `!==`: `==` and `!=` already compare values of one type, so each
is one error whose edit writes `==` or `!=` (`syntax.equality_operator`).
So are SQL's `<>`, `=` and `IS NULL`
([Habits from other languages](#habits-from-other-languages)).

Operators on standard types, such as `price * 20%` or `distance / 2h`,
follow [the standard operator table](19-standard-library.md#the-standard-operator-table).
Anything in neither that table nor the one above is a compile error naming
the two types. `20%` is a `Percentage` literal. The remainder operator
keeps `%` with spaces round it, as the formatter writes every binary
operator, and `20%3` is `syntax.percent_ambiguous`, naming both readings.
`Rounding.down` rounds toward zero ([Numbers](19-standard-library.md#numbers)).

## Match
<!-- topic: match -->

`match` chooses between the cases of a value. It must cover every case,
and it works as a statement or as a value. Its arms use `->`, the same
arrow as result types.

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

A `match` on a call that can fail names its success value and each
failure ([Handling a failure in `match`](04-failures.md#handling-a-failure-in-match)):

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
| `Enum.variant` | a plain variant, or a variant whose fields this arm doesn't read |
| `Enum.variant(name)` | a variant with fields, naming them `name`; `name.field` has the field's type, `Enum.variant.field` |
| `some(name)`, `none` | a nullable value |
| `true`, `false`, `"text"`, `42` | a literal |
| `_` | anything else; only where the cases can't be listed, such as text and numbers |
| `success(name)`, `FailureName` | the result of a call that can fail: its value, or one of its failures |

There is no `_` on an enum, a `Bool` or a nullable value: every case is
listed. So adding a variant to an enum makes every `match` that must
handle it fail to compile until it does.

An arm is one of:

- `pattern -> expression`;
- `pattern -> reject F`, which maps the failure to another;
- `pattern -> propagate`, which passes the failure on;
- `pattern -> { statements }`.

A failure arm can recover with a value, map to another failure with
`reject`, or pass the failure on with `propagate`. Every failure the call
can produce needs an arm ([Failures](04-failures.md)).

## Attempt
<!-- topic: attempt -->

A call that can fail must say so where it is called. `attempt` passes the
call's failures on to the caller, whose `fails` list must include them. A
`match` on the call handles them instead.

`attempt` starts a statement: a variable, a `return`, or a statement on
its own.

```jadpo fragment
var todo: Todo = attempt Todo.get(todo_id)
return attempt todo_by_id(todo_id)
attempt Todo.get(todo_id)
```

It can't stand inside an expression, so this is refused:

```jadpo fragment
var total: Stock = Stock(attempt stock_of(a) + attempt stock_of(b))
```

So every point where a callable can stop early is at the start of a line.
`attempt` on a call that can't fail is an error.

`reject` and `propagate` follow the same rule: each starts a statement or
is the result of a `match` arm, never part of a value
(`failure.reject_placement`, `failure.propagate_placement`). More is in
[Passing a failure on: `attempt`](04-failures.md#passing-a-failure-on-attempt).

## Loops
<!-- topic: loops -->

`for item in items` goes through a list, and the loop variable takes its
type from the list.

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

`for` is the only loop. There is no `while`, `break` or `continue`, and
`return` is allowed inside the body.

`for` also goes through a set, in its canonical order, one fixed order that is
the same on every target ([Collections](02-values-and-types.md#collections)). It
goes through a map's keys with `for loaf in map.keys(stock)`
([Maps](19-standard-library.md#maps)). A map itself can't be looped over,
since each step would need a key and a value together. The error's edit
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
  help: https://jadpo.dev/docs/errors/type.mismatch

check failed: 1 error, 1 file
```

### No outside calls inside a loop

An outside call is one that leaves your app: a provider operation (an
outside service, reached through its package;
[Integrations](11-integrations.md)), a package action, a file store's
`put` or `read`, or a `documents` render. Inside a loop's body, any of
these is a compile error, in any action. A query inside a loop already is
([Reading in a loop](06-reads-and-writes.md#reading-in-a-loop)).

To make a call for each item, use `batches.each`, the one way
([Batches: work over many items](12-jobs-and-events.md#batches-work-over-many-items)).
A provider that takes a list is called once, with the list.

Keeping calls out of loops means the steps of an action that makes several
changing calls lie along a path whose length the compiler knows
([Several changing calls in one action](12-jobs-and-events.md#several-changing-calls-in-one-action)).

The error is [`effect.call_in_loop`](../errors/effect.call_in_loop.md), at
the call. It covers any outside call, one that only reads included, made
in a loop's body or by an action called there, in an action or a fixture.

## Passing functions
<!-- topic: passing-functions -->

You can pass an inline function only as an argument to a standard-library
operation, such as `list.map` or `list.filter`. It is written with `->`,
read as "produces":

```jadpo fragment
var open: List<Todo> = list.filter(todos, todo -> todo.status == TodoStatus.open)
var titles: List<TodoTitle> = list.map(todos, todo -> todo.title)
```

An inline function:

- is one expression;
- takes its parameter's type from the list;
- may read the variables around it, which is safe because values never
  change;
- can't call actions or use `attempt`.

Functions are not values: you can't store one in a variable or pass one to
your own callables.

The collection operations are under `list.`, `set.` and `map.`
([Lists](19-standard-library.md#lists), [Sets](19-standard-library.md#sets),
[Maps](19-standard-library.md#maps)). Those that take an inline function
include `any`, `all`, `find`, `min_by`, `max_by`, `sort_by` and
`group_by`. Text is put in order with `list.sort_by` and a text key.

### Two parameters

For a few standard operations, an inline function takes two parameters:
`list.zip_with`, `map.filter` and `map.map_values`. Write them in
brackets, separated by a comma: `(key, value) -> ...`. Each takes the type
of what the operation gives it in that place: for `list.zip_with`, an item
from each list; for `map.filter` and `map.map_values`, an entry's key and
its value.

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

An inline function with a different number of parameters from what its
operation gives is `type.inline_function_parameters`. One with two
parameters anywhere else is `type.inline_function_placement`, as one with
a single parameter is.

### Names that aren't values

No declaration's name is a value. A type, enum, failure, entity, function,
action, query, library or route group, or a standard namespace such as
`list`, written where a value is read is `type.not_a_value`. The places a
value is read are: a variable's value, an argument, a returned value, a
field or list item, an operand, a condition, a `match` subject, a `set:`,
a route's `run:`, a query's condition, an input's `default:` and a test's
expectation.

A value is one of these:

- built: `Placed { ... }`, `Count(5)`;
- one of an enum's variants: `Status.open`;
- read from a variable, a setting (`config.page_size`) or the clock
  (`clock.now`);
- the result of a call: `helper(...)`.

The error's `next:` gives the one that fits the name.

### Batch helpers

The batch helpers take inline functions written with `->` too: `each:` and
`finished:` of `csv.read`, `csv.write`, `tsv`, `json_lines`, `json.read`,
`json.write` and `batches.each`
([Batches: work over many items](12-jobs-and-events.md#batches-work-over-many-items)).
They run later, in the background, after the action that started them
commits, so they differ from the collection operations in two ways:

```jadpo fragment
csv.read(
    import.sheet,
    as: ProductRow,
    each: row -> save_product(row, account: import.account_id),
    finished: summary -> import_finished(summary, import: import.id)
)
```

- Where there is nothing to write to (`each:` when reading,
  `batches.each`'s `each:`, and every `finished:`), the body is one call to
  an action in the same domain, as a subscriber's `run:` is. Where the
  helper writes, `each:` is a pure expression giving the row to write
  (`product -> ProductExportRow { ... }`).
- Values the body reads from around it are saved with the run, the record
  Jadpo keeps of the background work, so only values an entity field could
  hold may be read. Reading anything else is a compile error naming the
  value.

For `batches.each`, an inline function is also a named argument's value.
The body's arguments are the item, a field inside it, or a value read from
around it, which `type.batch_each_invalid` and
`type.batch_capture_not_storable` enforce
([How a run is kept](12-jobs-and-events.md#how-a-run-is-kept)).

## Recursion
<!-- topic: recursion -->

A function may call itself, or form a cycle with other functions.
Recursive types are allowed (`replies: List<Comment>`), and recursion is
the only way to walk them:

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

Only functions may recurse. An action or query in a cycle of calls is an
error (`effect.recursive_action`).

How deep recursion can go is limited while the app runs, and going past
the limit ends in a defined fault rather than a crash. The limits are the
same in the interpreter and in a compiled build, on every target:

- **A thousand frames**, the test's own or the route's action's among them.
- **Four hundred inline functions running at once**, one inside another's
  body through the functions it calls.

Past either, the call or operation that would go deeper is the
`recursion_limit` fault, at its place in the code. That holds in
`jadpo test` and `jadpo dev` as in a deployable, a compiled build of your
app. So `reply_count` above walks a thread a thousand replies deep, and
the same walk written as `list.map(comment.replies, reply -> ...)` four
hundred.

Inline functions have the lower limit because compiled code runs each one
inside its operation, several of the runtime's frames deep. On Workers
those frames use V8's own stack, about a megabyte that nothing in the
module can grow.

The stacks behind the limits:

- A native deployable's threads have the same stack as `jadpo test`'s,
  64 MiB.
- A Workers module is linked with 8 MiB of its own stack, which a thousand
  frames of a large function fit. In a module, a callable entered with less
  than 256 KiB of it left is also the `recursion_limit` fault, so an even
  larger frame stops there rather than writing past the stack's end.

## Generic code
<!-- topic: generic-code -->

Only the standard library is generic (`List<T>`, `list.count`). Your own
callables take named types. A parameter that accepts several types is an
[enum](02-values-and-types.md#enums) whose variants carry each one:

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

`function first<T>(items: List<T>) -> T` and `Customer.id | Supplier.id`
are not Jadpo: your own code has no generics or unions.

For the same reason, your app's types never declare operators. Every
`a + b` is a row of the operator tables a reviewer can look up, and never
an overload ([The standard operator table](19-standard-library.md#the-standard-operator-table)).

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


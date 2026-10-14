---
title: Values and types
area: 2
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
---

# Values and types

What kinds of value a Jadpo program works with, how you give a value a domain
meaning, and when one type can stand in for another.

The rule behind all of it: **representation is not meaning**. An email address
and a username are both text underneath, and an order ID and a customer ID are
both UUIDs, but Jadpo treats them as different types. A value only becomes a
type by being validated against it, and the compiler never converts between
types silently.

## Built-in types

These types exist without being declared, and their names cannot be reused.

| Kind | Types | Notes |
|---|---|---|
| Representation | `Bool`, `Int`, `Decimal`, `Text`, `Bytes`, `Uuid` | The raw material for your own types. `Int` and `Decimal` are currently JavaScript numbers at runtime (see Review). |
| Validated text | `Email`, `Url`, `IpAddress` | Ready-made named types built on `Text`. |
| Time | `Instant`, `CalendarDate`, `Time`, `Duration`, `Zone`, `Locale`, `Weekday` and the formatting types | Described in area 10, Configuration and time. |
| Containers | `List<T>`, `Set<T>`, `Map<K, V>` | Described below. |
| Bases | `Object`, `Enum`, `Input`, `Output` | Used only after `=` in a `type` declaration (decided 2026-10-12). |
| Nothing | `Unit` | What an action returns when it returns nothing. |

Literals are `true` and `false`, whole numbers (`25`, `-3`), decimals (`1.25`),
strings (`"Hello"`, with `\"`, `\\`, `\n`, `\r` and `\t` escapes) and `none`.

## Named types

Most of a program's types are named types: a built-in representation, a name
that says what the value means, and the rules a valid value must follow.

```jadpo check project=shop
type Pence = Int {
    min: 0
}

type Quantity = Int {
    min: 1
    max: 1000
}

type ProductName = Text {
    min_length: 1
    max_length: 120
}

type OrderId = Uuid {}
```

`OrderId` has no rules, but it is still its own type: an `OrderId` cannot be
passed where another UUID-based type is expected. Every `type` declaration
creates a new type; there are no aliases.

A named type can build on another named type, adding rules. The narrower type
can be used wherever its parent is expected, but not the other way round.

```jadpo check project=refine
type ProductName = Text {
    min_length: 1
    max_length: 120
}

type ShortProductName = ProductName {
    max_length: 40
}

function catalogue_name(name: ShortProductName) -> ProductName {
    return name
}
```

### Constraints

| Constraint | For | Value | Meaning |
|---|---|---|---|
| `min`, `max` | `Int`, `Decimal` | number | Inclusive bounds |
| `min_length`, `max_length` | `Text`, `Bytes`, `List<T>` | whole number | Inclusive bounds; text counts Unicode characters, lists count elements |
| `pattern` | `Text` | string | A regular expression the text must match |
| `format` | `Text` | `email` | The text must be an email address |

Each constraint is written `name: value`, one per line. There are no other
constraints and no custom validation functions.

Built in chunk 1.9: a refinement can only add to the rules of the type it
builds on, so a `max` below an inherited `min` (or the other way round, for
lengths and durations too) is `type.constraint_contradiction`, and so is a
`Decimal` refinement whose `scale` differs from the one it inherits, since
the scale decides how the value is stored and sent. The same settings rules
hold for the fields of every type with fields, entities and events included;
the settings later areas add to an entity's or event's fields (`generated`,
`on_delete`, `unique`, `immutable`, `role`, `personal`, `erase`,
`retention`) are left to those areas. A number literal with more digits than
an exact number holds (38) is `type.invalid_literal`.

Today the compiler accepts constraints in some places where they have no
effect: `min` on text, `min_length` on a number, a `pattern` other than
`[a-z0-9_]+`, and any `format` other than `email` are all accepted but not
checked at runtime. The Review lists these.

## Creating values

Applying a type to a value, `Type(value)`, creates a value of that type by
checking every rule on it and on the types it builds on. It is validation,
never a cast.

```jadpo check project=literals
type Quantity = Int {
    min: 1
    max: 1000
}

type Price = Decimal {
    scale: 2
    min: 0.00
}

function starting_quantity() -> Quantity {
    return 1
}

function free() -> Price {
    return 0.00
}
```

::: today

```jadpo project=construct
type Quantity = Int {
    min: 1
    max: 1000
}

type Price = Decimal {
    min: 0.0
}

function starting_quantity() -> Quantity {
    return Quantity(1)
}

function free() -> Price {
    return Price(0.0)
}
```

:::

When the value is a literal, the compiler checks it while compiling:
`Quantity(0)` is a compile error (see `type.invalid_literal` below). When the
value is computed, the check runs when the program runs.

A constructor accepts a value with the same representation as the type: `Int`
or any `Int`-based type for `Quantity`, a decimal literal such as `0.0` (not
`0`) for a `Decimal`-based type. This is also how you deliberately convert
between two named types: the value is re-checked against the target's rules.

```jadpo check project=convert
type Pence = Int {
    min: 0
}

type Quantity = Int {
    min: 1
    max: 1000
}

function line_total(price: Pence, quantity: Quantity) -> Pence {
    return Pence(price * quantity)
}
```

Arithmetic works on the representation, so `price * quantity` is a plain `Int`
and has to be turned back into `Pence`. A plain `Int` can never be returned
where `Pence` is expected.

Here `price` and `quantity` are never below zero and `Pence` has no
maximum, so the declared ranges prove `Pence(price * quantity)` always
follows its rules, and it cannot fail.

### Construction failures by rule

When neither the declared ranges nor the conditions around it (below)
prove a computed value follows the target's rules, building it can fail. The statement starts with `attempt`, and a
block after the call maps each rule that can break to the failure the
caller gets, one arm per line: the same block form writes and integration
calls use (area 4).

```jadpo check project=by-rule file=types.jadpo hidden
type Count = Int {}

type CakeCount = Int {
    min: 1
    max: 12
}

type OrderEmpty = Failure {
    kind: FailureKind.invalid_value
}

type OrderTooLarge = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=by-rule
function doubled(count: CakeCount) fails OrderTooLarge -> CakeCount {
    var result: CakeCount = attempt CakeCount(count * 2) {  // at least 2, so only `max` can break
        max: OrderTooLarge
    }
    return result
}

function from_count(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {
    var result: CakeCount = attempt CakeCount(n) {  // anything, so both can break
        max: OrderTooLarge
        invalid: OrderEmpty
    }
    return result
}
```

- **The keys are rules.** Each key names a rule of the type, or of a type it
  builds on, that neither the declared ranges nor a condition proves holds: `min`, `max`,
  `min_length`, `max_length`, `pattern`, `format` or `version`. Each such
  rule needs exactly one arm.
- **`invalid:` is the catch-all.** It covers every rule that can break and
  has no arm of its own, so `{ invalid: OrderTooLarge }` alone covers them
  all. It covers only a construction's rules: a write's conditions
  (`missing:`, `conflict:`, area 6) and an integration call's (area 11)
  each still need their own arm, and `invalid:` on them is an error.
- **Every arm can run.** An arm for a rule that can't break, an `invalid:`
  left covering nothing, a key that isn't one of the type's rules and a
  key given twice are errors (area 4).
- **What the compiler can't reason about can always break.** A `pattern`
  other than the value's own always counts as able to break. `scale` never
  does: it rounds the value as it is kept.
- **Every failure is `invalid_value`.** At runtime the failure records the
  rule that broke, such as `max: 12`, for the log, never the value.

### Conditions as proof

An `if` condition counts as proof. After a check that leaves when the value
is out of range, the construction it guards is plain:

```jadpo check project=conditions file=types.jadpo hidden
type Count = Int {}

type CakeCount = Int {
    min: 1
    max: 12
}

type OrderEmpty = Failure {
    kind: FailureKind.invalid_value
}

type OrderTooLarge = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=conditions
function doubled_checked(count: CakeCount) fails OrderTooLarge -> CakeCount {
    if (count * 2 > 12) {
        reject OrderTooLarge
    }
    return CakeCount(count * 2)  // at most 12 here, and at least 2
}

function from_count(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {
    if (n < 1) {
        reject OrderEmpty
    } else if (n > 12) {
        reject OrderTooLarge
    }
    return CakeCount(n)  // neither condition held, so 1 to 12
}

function inside_range(n: Count) fails OrderTooLarge -> CakeCount {
    if (n >= 1 and n <= 12) {
        return CakeCount(n)  // both hold here
    }
    reject OrderTooLarge
}
```

What counts as proof, exactly (built in chunk 2.6):

- **A comparison bounds a number.** `<`, `<=`, `>`, `>=`, `==` and `!=`
  between two numbers bound each side by the other side's range: a
  literal's value, or the range another value has at that point. A side is
  bounded when it is made of locals (parameters, `var` and `var mut`
  variables, loop items, names a `match` arm binds), number literals, their
  fields (`line.quantity`), standard-library calls on them
  (`collection.count(items)`) and arithmetic. A bound on `collection.count`
  of a list bounds that list's `min_length` and `max_length` too.
- **The same number is written the same way.** A bound on `count * 2`
  proves `CakeCount(count * 2)`, brackets aside, and a bound on `count`
  carries through arithmetic: after `count > 6` leaves, `count * 2` is at
  most 12. A bound on `2 * count` says nothing about `count * 2`, and one
  on `count * 2` says nothing about `count` alone.
- **Whole numbers step.** Between two `Int`-based values, `n > 12` leaves
  `n` at least 13 where it holds and at most 12 where it doesn't, and after
  `n == 0` leaves, a range starting at 0 starts at 1. A `Decimal` can come
  as close to a value as it likes, so `!=` bounds no decimal.
- **`and`, `or` and `not` combine.** Inside `if (a and b)` both hold, and
  after `if (a or b) { reject ... }` neither does; `not` turns a condition
  round. Where only one of two may hold, as inside `if (a or b)`, a bound
  survives only when both give one, widened to cover both. The right side
  of `and` is read knowing its left side holds, and of `or` knowing its
  left side doesn't, so `n >= 1 and is_small(CakeCount(n))` is plain too.
- **Where it holds.** Inside the branch where the condition holds. An
  `else if` and the `else` know every earlier condition is false. After the
  `if`, what holds at the end of every branch that can reach the next
  line: a branch ending in `return`, `reject`, or a `match` whose every arm
  leaves, never does, and with no `else`, the way past the chain knows
  every condition is false.
- **What ends a bound.** Assigning a `var mut` ends every bound that
  mentions it. A `for` loop ends the bounds of every `var mut` it assigns,
  before it and after it, since a later pass sees what an earlier one
  assigned. What a block finds out ends with the block, so a check inside a
  `match` arm, a loop or a branch that goes on proves nothing after it.
- **Nothing else is proof.** A variable's starting value is not:
  `var doubled: Int = count * 2` is an `Int` with no range, until a
  condition bounds `doubled` itself. Nor is an assignment, a `match`
  pattern, an `assert`, or a call to your own function.
- **It proves every construction.** `Type(value)`, `Owner.field(value)`
  and an object literal's field (`type.field_unproved`) all read it. A
  divisor still needs a type that excludes zero (area 3, question 7),
  which a condition can prove by building that type first.
- **A fixed `scale` rounds first.** A type with a fixed `scale` keeps a
  value rounded half to even, so its bounds are compared after rounding: a
  value of at most 10.004 fits `max: 10.00` with `scale: 2`, and one of at
  most 10.07 doesn't fit `max: 10.07` with `scale: 1`, since 10.07 is kept
  as 10.1.

Changing a range or a condition flips call sites, and each one says so
where it is written (area 4, question 6): a construction that can now fail
needs `attempt` and an arm for each rule that can break
(`failure.attempt_required`, `failure.rule_unmapped`), and one that no
longer can takes none (`failure.attempt_infallible`,
`failure.rule_cannot_break`, `failure.invalid_covers_nothing`). When a
condition is what proves it, the error points at the comparison:

```jadpo check project=e-condition-proves expect=failure.attempt_infallible
type CakeCount = Int {
    min: 1
    max: 12
}

type OrderTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled_checked(count: CakeCount) fails OrderTooLarge -> CakeCount {
    if (count * 2 > 12) {
        reject OrderTooLarge
    }
    var doubled: CakeCount = attempt CakeCount(count * 2) {
        max: OrderTooLarge
    }
    return doubled
}
```

```text output
error: `CakeCount(...)` can't fail, so it takes no `attempt`
 --> main.jadpo:14:30
   |
11 |     if (count * 2 > 12) {
   |         -------------- checked here
14 |     var doubled: CakeCount = attempt CakeCount(count * 2) {
   |                              ^^^^^^^ nothing to pass on
  reason: The declared ranges and the condition `count * 2 > 12` prove `CakeCount(...)` always follows its type's rules, so building it never fails; `attempt` marks only what can.
  next: Remove `attempt` (agent)
  rule: failure.attempt_infallible
  help: docs/language/03-logic.html#failure-attempt-infallible

check failed: 1 error, 1 file
```

```jadpo check project=e-condition-proves-rule expect=failure.rule_cannot_break
type Count = Int {}

type CakeCount = Int {
    min: 1
    max: 12
}

type OrderEmpty = Failure {
    kind: FailureKind.invalid_value
}

type OrderTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function from_count(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {
    if (n > 12) {
        reject OrderTooLarge
    }
    var count: CakeCount = attempt CakeCount(n) {
        min: OrderEmpty
        max: OrderTooLarge
    }
    return count
}
```

```text output
error: `CakeCount(...)` can't break `max`, so it takes no arm
 --> main.jadpo:22:9
   |
17 |     if (n > 12) {
   |         ------ checked here
22 |         max: OrderTooLarge
   |         ^^^ never runs
  reason: The declared ranges and the condition `n > 12` prove the value always follows `max: 12`, so this arm could never run.
  next: Remove `max: ...` (agent)
  rule: failure.rule_cannot_break
  help: docs/language/04-failures.html#failure-rule-cannot-break

check failed: 1 error, 1 file
```

## Signatures use named types

Functions, actions and queries cannot take or return a built-in representation
type directly. `Text`, `Int`, `Bool`, `Uuid`, the time types and their nullable
forms all have to be wrapped in a named type that says what they mean.

```jadpo check project=signature
type CustomerNote = Text {
    max_length: 500
}

type NoteWasEdited = Bool {}

function edited(before: CustomerNote, after: CustomerNote) -> NoteWasEdited {
    return NoteWasEdited(before != after)
}
```

`Email`, `Url`, `IpAddress`, objects, enums, field types and `Unit` are
allowed. Inside a body, locals can still hold built-in types.

## Objects

> **Decided on 2026-10-12.** Every object type is `type Name = Base { ... }`. Today's `value`, `input` and `output` keywords are shown collapsed below until the rework lands.

An object is a fixed set of named fields. The base after `=` says what the
object is for:

```jadpo check project=objects file=names.jadpo hidden
type ProductName = Text {
    min_length: 1
    max_length: 120
}

type Pence = Int {
    min: 0
}
```

```jadpo check project=objects file=main.jadpo
type Money = Object {  // used inside the application
    amount: Pence
}

type CreateProduct = Input {  // arrives from outside, such as a request body
    name: ProductName
    price: Pence
}

type ProductView = Output {  // sent back, such as a response
    name: ProductName
    price: Money
}
```

Only an `Input` may leave fields out or give them defaults. A route accepts
only an `Input` and returns only an `Output`. There is no unnamed object: a
nested shape is declared once, with a name, and used by that name.

An object value is written with its type name and every field:

```jadpo check project=objects file=view.jadpo
function view(name: ProductName, price: Pence) -> ProductView {
    return ProductView {
        name: name
        price: Money {
            amount: price
        }
    }
}
```

Every declared field must be given, and unknown fields are errors. Fields can
hold other objects and lists, nested as deeply as needed.

An object literal never fails. A field accepts its declared type or anything
that widens to it, and when the field adds rules of its own, the declared
ranges must prove the value follows them (`type.field_unproved`). A value
that might break them is built first, on its own line, as the field's type
(below), where its failure is mapped:

```jadpo check project=field-first file=profiles.jadpo hidden
type Profile = Object {
    display_name: Text {
        max_length: 80
    }
}

type BadgeNameTooLong = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=field-first
type Badge = Object {
    name: Text {
        max_length: 12
    }
}

function make_badge(profile: Profile) fails BadgeNameTooLong -> Badge {
    var name: Badge.name = attempt Badge.name(profile.display_name) {
        max_length: BadgeNameTooLong
    }
    return Badge {
        name: name
    }
}
```

::: today

```jadpo project=records
type ProductName = Text {
    min_length: 1
    max_length: 120
}

type Pence = Int {
    min: 0
}

value Money {
    amount: Pence
}

input CreateProduct {
    name: ProductName
    price: Pence
}

output ProductView {
    name: ProductName
    price: Money
}

type Dimensions = Object {
    width: Pence
    height: Pence
}
```

Today `value`, `input`, `output` and `type X = Object` all declare the same
thing, and any of them can be used at either boundary.

```jadpo project=records-build
type ProductName = Text {
    min_length: 1
    max_length: 120
}

type Pence = Int {
    min: 0
}

output ProductView {
    name: ProductName
    price: Pence
}

function view(name: ProductName, price: Pence) -> ProductView {
    return ProductView {
        name: name
        price: price
    }
}
```

:::

### Field types

Every field of an object, entity or enum variant is a type of its own, written
`Owner.field`. `Customer.email` is a refinement of `Email`: it can be used
wherever an `Email` is expected, but an `Email` is not automatically a
`Customer.email`, and `Supplier.email` is never one.

```jadpo check project=fields
entity Customer {
    id: Uuid
    email: Email
}

entity Supplier {
    id: Uuid
    email: Email
}

function contact_address(email: Email) -> Email {
    return email
}

function customer_contact(customer: Customer) -> Email {
    return contact_address(customer.email)
}

function supplier_contact(supplier: Supplier) -> Email {
    return contact_address(supplier.email)
}
```

::: today

```jadpo project=field-types
entity Customer {
    id: Uuid
    email: Email
    identity: id
}

entity Supplier {
    id: Uuid
    email: Email
    identity: id
}

function contact_address(email: Email) -> Email {
    return email
}

function customer_contact(customer: Customer) -> Email {
    return contact_address(customer.email)
}

function supplier_contact(supplier: Supplier) -> Email {
    return contact_address(supplier.email)
}
```

:::

Field types are how identifiers stay apart: `Customer.id` and `Supplier.id`
are both UUIDs, and passing one where the other is expected is
`type.sibling_mismatch`. To move a value into a field type on purpose, use the
field as a constructor, as in `Customer.email(email)`.

A field can add rules of its own in a block after its type:

```jadpo check project=badge file=names.jadpo hidden
type ProductName = Text {
    min_length: 1
    max_length: 120
}
```

```jadpo check project=badge file=main.jadpo
type Badge = Object {
    label: ProductName {
        max_length: 12
    }
}
```

::: today

```jadpo project=field-rules
type ProductName = Text {
    min_length: 1
    max_length: 120
}

value Badge {
    label: ProductName {
        max_length: 12
    }
}
```

:::

Reading through a nested object gives the inner object's field type:
`customer.billing_address.postal_code` has type `Address.postal_code`, whether
it came from the billing or the shipping address.

## Absent values

> **Decided on 2026-10-12.** Objects and inputs use `type Name = Object { ... }` and `type Name = Input { ... }`; today's spelling is collapsed below. Decided (question 20): `optional` and `default` go in a settings block after the field's type.

There is no `null` or `undefined`. A type followed by `?` may hold `none`, and
reading it means handling both cases, usually with `match` (area 3).

```jadpo check project=absent file=names.jadpo hidden
type ProductName = Text {
    min_length: 1
    max_length: 120
}
```

```jadpo check project=absent file=main.jadpo
type Listing = Object {
    name: ProductName
    subtitle: ProductName?
}

function headline(listing: Listing) -> ProductName {
    return match listing.subtitle {
        some(subtitle) -> subtitle
        none -> listing.name
    }
}
```

::: today

```jadpo project=nullable
type ProductName = Text {
    min_length: 1
    max_length: 120
}

value Listing {
    name: ProductName
    subtitle: ProductName?
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
```

:::

A nullable field is still required: a `Listing` must say `subtitle: none`
explicitly. Being allowed to leave a field out is a separate idea that belongs
to inputs:

```jadpo check project=input-settings file=names.jadpo hidden
type ProductName = Text {
    min_length: 1
    max_length: 120
}

type PageSize = Int {
    min: 1
    max: 100
}
```

```jadpo check project=input-settings file=main.jadpo
type ListProducts = Input {
    name: ProductName {
        optional: true
    }
    page_size: PageSize {
        default: 25
    }
}

type RenameProduct = Input {
    name: ProductName
    subtitle: ProductName? {
        default: none
    }
}
```

::: today

```jadpo project=inputs
type ProductName = Text {
    min_length: 1
    max_length: 120
}

type PageSize = Int {
    min: 1
    max: 100
}

input ListProducts {
    name: ProductName optional
    page_size: PageSize default 25
}

input RenameProduct {
    name: ProductName
    subtitle: ProductName? default none
}
```

:::

- `optional: true` lets the request leave the field out; reading it gives
  `T?`, and a field left out reads as `none`. Only an update's `patch:` tells
  "left out" from "sent as `none`" (area 6).
- `default: 25` fills in a value when the request leaves the field out,
  checked against the field's rules while compiling.
- `default: none` fills in `none` for a nullable field.

A value can widen through `?`: a `Customer.email?` can be used where an
`Email?` is expected.

## Enums

> **Decided on 2026-10-12.** Enums are `type Name = Enum { ... }`; today's `enum Name { ... }` is collapsed below.

An enum is a closed list of named alternatives. Variants are always written
with their enum's name.

```jadpo check project=enum-values
type TicketStatus = Enum {
    open
    waiting
    closed
}

function reopen() -> TicketStatus {
    return TicketStatus.open
}
```

::: today

```jadpo project=enums
enum TicketStatus {
    open
    waiting
    closed
}

function reopen() -> TicketStatus {
    return TicketStatus.open
}
```

:::

A variant can carry its own fields, which makes an enum a tagged union: each
value is exactly one variant and has only that variant's fields.

```jadpo check project=tagged-values file=names.jadpo hidden
type ReceiptId = Uuid {}

type DeclineReason = Text {
    min_length: 1
    max_length: 200
}

type OutcomeLabel = Text {
    max_length: 20
}
```

```jadpo check project=tagged-values file=main.jadpo
type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
        paid_at: Instant
    }
    declined: {
        reason: DeclineReason
    }
}

function decline(reason: DeclineReason) -> PaymentOutcome {
    return PaymentOutcome.declined {
        reason: reason
    }
}

function label(outcome: PaymentOutcome) -> OutcomeLabel {
    return match outcome {
        PaymentOutcome.pending -> "pending"
        PaymentOutcome.paid -> "paid"
        PaymentOutcome.declined -> "declined"
    }
}
```

::: today

```jadpo project=tagged
type ReceiptId = Uuid {}

type DeclineReason = Text {
    min_length: 1
    max_length: 200
}

type OutcomeLabel = Text {
    max_length: 20
}

enum PaymentOutcome {
    pending
    paid {
        receipt_id: ReceiptId
        paid_at: Instant
    }
    declined {
        reason: DeclineReason
    }
}

function decline(reason: DeclineReason) -> PaymentOutcome {
    return PaymentOutcome.declined {
        reason: reason
    }
}

function label(outcome: PaymentOutcome) -> OutcomeLabel {
    match outcome {
        PaymentOutcome.pending => {
            return OutcomeLabel("pending")
        }
        PaymentOutcome.paid { receipt_id, paid_at } => {
            return OutcomeLabel("paid")
        }
        PaymentOutcome.declined { reason } => {
            return OutcomeLabel("declined")
        }
    }
}
```

:::

The only way to read a variant's fields is to `match` on it. Payload fields
have field types too, written `PaymentOutcome.declined.reason`.

An enum cannot be created from text: `TicketStatus("open")` is an error. Text
from outside becomes a variant only by being decoded at a boundary. On the
wire a plain variant is its name as a string (`"open"`), and a variant with
fields is an object with a `tag` naming the variant plus its fields. Enums have
no methods or display labels.

## Collections

`List<T>`, `Set<T>` and `Map<K, V>` can be used as field types, parameter
types and boundary types, nested in each other or holding objects.

```jadpo check project=lists file=names.jadpo hidden
type Tag = Text {
    min_length: 1
    max_length: 30
}

type Pence = Int {
    min: 0
}
```

```jadpo check project=lists file=main.jadpo
type BasketLine = Object {
    tag: Tag
    price: Pence
}

type Basket = Output {
    lines: List<BasketLine>
    tags: List<Tag> {
        max_length: 10
    }
}
```

::: today

```jadpo project=collections
type Tag = Text {
    min_length: 1
    max_length: 30
}

type Pence = Int {
    min: 0
}

value BasketLine {
    tag: Tag
    price: Pence
}

output Basket {
    lines: List<BasketLine>
    tags: List<Tag> {
        max_length: 10
    }
}
```

:::

A collection keeps its element type: a `List<Customer.email>` is not a
`List<Email>`. Collections are written as types today, but there are no list
literals, no iteration and no collection functions yet; lists come from
queries and request bodies (Review, question 4).

## When one type can stand in for another

| You have | Where `Customer.email` is expected | Where `Email` is expected | Where `Text` is expected |
|---|---|---|---|
| `Customer.email` | yes | yes, it widens | no |
| `Supplier.email` | no | yes, it widens | no |
| `Email` | only through `Customer.email(...)` | yes | no |
| `Text` | only through `Customer.email(...)` | only through `Email(...)` | yes |

- **Widening** follows declared steps only: a field type to its declared type,
  a named type to the named type it builds on.
- **No silent narrowing.** Going from a general type to a specific one always
  goes through a constructor, which re-checks the rules.
- **Same shape is not same type.** Two objects with identical fields are still
  different types.
- **Nothing unwraps.** A named type never becomes its built-in representation
  except through arithmetic and comparison operators.

## Values never change

Every value is immutable. Passing a value to a function or returning it never
lets anyone else change it. A local declared with `var mut` can be pointed at a
new value of the same type, but fields cannot be assigned: to change an object,
build a new one.

```jadpo check project=values file=names.jadpo hidden
type ProductName = Text {
    min_length: 1
    max_length: 120
}

type Pence = Int {
    min: 0
}
```

```jadpo check project=values file=main.jadpo
type Product = Object {
    name: ProductName
    price: Pence
}

function reprice(product: Product, price: Pence) -> Product {
    return Product {
        name: product.name
        price: price
    }
}
```

::: today

```jadpo project=immutable
type ProductName = Text {
    min_length: 1
    max_length: 120
}

type Pence = Int {
    min: 0
}

value Product {
    name: ProductName
    price: Pence
}

function reprice(product: Product, price: Pence) -> Product {
    return Product {
        name: product.name
        price: price
    }
}
```

:::

## Errors

Every error this area can produce, with an example that triggers it and the
new compiler's actual output. Six syntax entries near the end still show the
archived compiler's error, because the new compiler reports those mistakes
under another rule, named in each note.

### `type.mismatch`

```jadpo check project=e-mismatch expect=type.mismatch
type Pence = Int {
    min: 0
}

function add(a: Pence, b: Pence) -> Pence {
    return a + b
}
```

```text output
error: `Int` can't be used where `Pence` is required
 --> main.jadpo:6:12
  |
6 |     return a + b
  |            ^^^^^ this is `Int`
  reason: Arithmetic works on the representation, so this is a plain `Int`; only `Pence(...)` makes it a `Pence`, checking its rules.
  next: Convert it with `Pence(a + b)` (agent)
  rule: type.mismatch
  help: docs/language/02-values-and-types.html#type-mismatch

check failed: 1 error, 1 file
```

### `type.sibling_mismatch`

```jadpo check project=e-sibling expect=type.sibling_mismatch
entity Customer {
    id: Uuid
}

entity Order {
    id: Uuid
}

function customer_reference(id: Customer.id) -> Customer.id {
    return id
}

function misuse(order: Order) -> Customer.id {
    return customer_reference(order.id)
}
```

```text output
error: `Order.id` can't be used where `Customer.id` is required
 --> main.jadpo:14:31
   |
14 |     return customer_reference(order.id)
   |                               ^^^^^^^^ this is `Order.id`
  reason: Both refine `Uuid`, but a field type widens only to its own declared type, so one type's values never stand in for another's.
  next: Pass a `Customer.id` here (agent)
  rule: type.sibling_mismatch
  help: docs/language/02-values-and-types.html#type-sibling-mismatch

check failed: 1 error, 1 file
```

### `type.primitive_signature`

```jadpo check project=e-primitive expect=type.primitive_signature
type Title = Text {
    max_length: 200
}

function normalise(value: Text) -> Title {
    return Title(value)
}
```

```text output
error: This parameter uses `Text` directly
 --> main.jadpo:5:27
  |
5 | function normalise(value: Text) -> Title {
  |                           ^^^^ a built-in type
  reason: Functions, actions and queries take and return named types, so each value says what it means and carries its rules; `Text` says neither.
  next: Declare a named type built on `Text` and use it here (agent)
  rule: type.primitive_signature
  help: docs/language/02-values-and-types.html#type-primitive-signature

check failed: 1 error, 1 file
```

### `type.invalid_literal`

A literal takes the type it goes into and is checked against its rules while compiling, whether it is returned, assigned, passed or written as `Quantity(0)`:

```jadpo check project=e-literal expect=type.invalid_literal
type Quantity = Int {
    min: 1
    max: 1000
}

function starting_quantity() -> Quantity {
    return 0
}
```

```text output
error: `0` isn't a valid `Quantity`
 --> main.jadpo:7:12
  |
7 |     return 0
  |            ^ breaks `min: 1`
  reason: `Quantity` declares `min: 1`, and the value is below 1. A literal is checked against every rule while compiling.
  next: Use a value that `Quantity` allows (agent)
  rule: type.invalid_literal
  help: docs/language/02-values-and-types.html#type-invalid-literal

check failed: 1 error, 1 file
```

### `type.constructor_input`

```jadpo check project=e-ctor-input expect=type.constructor_input
type OrderId = Uuid {}

type ProductName = Text {
    max_length: 120
}

function order_for(name: ProductName) -> OrderId {
    return OrderId(name)
}
```

```text output
error: `OrderId` can't be built from `ProductName`
 --> main.jadpo:8:20
  |
8 |     return OrderId(name)
  |                    ^^^^ this is `ProductName`
  reason: `OrderId` is built on `Uuid` and `ProductName` is built on `Text`; a constructor converts only between types with the same representation, re-checking the rules.
  next: Pass a value built on `Uuid` (agent)
  rule: type.constructor_input
  help: docs/language/02-values-and-types.html#type-constructor-input

check failed: 1 error, 1 file
```

### `type.constructor_argument_count`

```jadpo check project=e-ctor-count expect=type.constructor_argument_count
type Quantity = Int {
    min: 1
}

function pair() -> Quantity {
    return Quantity(1, 2)
}
```

```text output
error: `Quantity(...)` takes one value, not 2
 --> main.jadpo:6:12
  |
6 |     return Quantity(1, 2)
  |            ^^^^^^^^^^^^^^ 2 given
  reason: Constructing a `Quantity` checks one value against its rules.
  next: Pass one value to `Quantity(...)` (agent)
  rule: type.constructor_argument_count
  help: docs/language/02-values-and-types.html#type-constructor-argument-count

check failed: 1 error, 1 file
```

### `type.missing_field`

```jadpo check project=e-missing-field expect=type.missing_field
type Receipt = Output {
    email: Email
}

function incomplete_receipt() -> Receipt {
    return Receipt {}
}
```

```text output
error: `Receipt` is missing `email`
 --> main.jadpo:6:12
  |
6 |     return Receipt {}
  |            ^^^^^^^ fields missing
  reason: Every field of `Receipt` is given when a value is built; a field that may be absent is declared `T?` and given as `none`.
  next: Add `email: ...` to the block (agent)
  rule: type.missing_field
  help: docs/language/02-values-and-types.html#type-missing-field

check failed: 1 error, 1 file
```

### `type.field_unproved`

An object literal never fails, so a field value the declared ranges don't
prove follows the field's own rules is built first:

```jadpo check project=e-field-unproved expect=type.field_unproved
type Profile = Object {
    display_name: Text {
        max_length: 80
    }
}

type Badge = Object {
    name: Text {
        max_length: 12
    }
}

function make_badge(profile: Profile) -> Badge {
    return Badge {
        name: profile.display_name
    }
}
```

```text output
error: `profile.display_name` might not fit `Badge.name`
 --> main.jadpo:15:15
   |
15 |         name: profile.display_name
   |               ^^^^^^^^^^^^^^^^^^^^ can break `max_length: 12`
  reason: An object literal never fails, so each field gets a value already known to fit; neither the declared ranges nor a condition before it prove this one follows `max_length: 12`, which `Badge.name` adds.
  next: Build it first on its own line, `var name: Badge.name = attempt Badge.name(profile.display_name) { max_length: SomeFailure }`, then write `name: name` in the object (agent)
  rule: type.field_unproved
  help: docs/language/02-values-and-types.html#type-field-unproved

check failed: 1 error, 1 file
```

### `type.unknown_field`

```jadpo check project=e-unknown-field expect=type.unknown_field
type ProductName = Text {
    max_length: 120
}

type Product = Object {
    name: ProductName
}

function build_product(name: ProductName) -> Product {
    return Product {
        name: name
        title: name
    }
}
```

```text output
error: `Product` has no field `title`
 --> main.jadpo:12:9
   |
12 |         title: name
   |         ^^^^^ not declared
  reason: `Product`'s fields are `name`.
  next: Remove `title`, or declare it on `Product` (agent)
  rule: type.unknown_field
  help: docs/language/02-values-and-types.html#type-unknown-field

check failed: 1 error, 1 file
```

### `type.duplicate_field`

A field is given once when a value is built, and declared once in a type:

```jadpo check project=e-duplicate-field expect=type.duplicate_field
type ProductName = Text {
    max_length: 120
}

type Product = Object {
    name: ProductName
}

function build_product(name: ProductName) -> Product {
    return Product {
        name: name
        name: name
    }
}
```

```text output
error: `name` is given twice
 --> main.jadpo:12:9
   |
11 |         name: name
   |         ---- first given here
12 |         name: name
   |         ^^^^ given again
  reason: Each field of `Product` is given once, so there is never a question of which value counts.
  next: Remove one `name` (agent)
  rule: type.duplicate_field
  help: docs/language/02-values-and-types.html#type-duplicate-field

check failed: 1 error, 1 file
```

### `type.not_record`

```jadpo check project=e-not-record expect=type.not_record
type ProductName = Text {
    max_length: 120
}

function blank() -> ProductName {
    return ProductName {}
}
```

```text output
error: `ProductName` isn't built with `{ ... }`
 --> main.jadpo:6:12
  |
6 |     return ProductName {}
  |            ^^^^^^^^^^^^^^ has no fields
  reason: Braces build objects and variants with fields; `ProductName`, built on `Text`, takes one value in brackets.
  next: Write `ProductName(...)` with one value (agent)
  rule: type.not_record
  help: docs/language/02-values-and-types.html#type-not-record

check failed: 1 error, 1 file
```

### `type.field_on_non_record`

```jadpo check project=e-field-scalar expect=type.field_on_non_record
type ProductName = Text {
    max_length: 120
}

function raw(name: ProductName) -> ProductName {
    return name.value
}
```

```text output
error: `ProductName` has no fields
 --> main.jadpo:6:17
  |
6 |     return name.value
  |                 ^^^^^ no such field
  reason: `name` is a `ProductName`, built on `Text`; only objects, inputs, outputs, events, failures and entities have fields.
  next: Use `name` itself (agent)
  rule: type.field_on_non_record
  help: docs/language/02-values-and-types.html#type-field-on-non-record

check failed: 1 error, 1 file
```

### `type.nullable_selection`

```jadpo check project=e-nullable expect=type.nullable_selection
type ProductName = Text {
    max_length: 120
}

type Product = Object {
    name: ProductName
}

function listed_name(product: Product?) -> ProductName {
    return product.name
}
```

```text output
error: `product` may be `none`, so `name` can't be read yet
 --> main.jadpo:10:20
   |
10 |     return product.name
   |                    ^^^^ may be `none`
  reason: `product` is `Product?`; a field is read once `match` has found a value.
  next: Match on `product` and read `name` inside `some(...)` (agent)
  rule: type.nullable_selection
  help: docs/language/02-values-and-types.html#type-nullable-selection

check failed: 1 error, 1 file
```

### `type.redundant_nullability`

```jadpo check project=e-redundant expect=type.redundant_nullability
type ProductName = Text {
    max_length: 120
}

type Listing = Object {
    subtitle: ProductName?
}

type ListingPreview = Object {
    subtitle: Listing.subtitle?
}
```

```text output
error: `Listing.subtitle` already permits `none`
 --> main.jadpo:10:15
   |
10 |     subtitle: Listing.subtitle?
   |               ^^^^^^^^^^^^^^^^^ nullable twice
  reason: `Listing.subtitle` takes `none` from its declared type; a second `?` adds nothing.
  next: Remove the extra `?` (agent)
  rule: type.redundant_nullability
  help: docs/language/02-values-and-types.html#type-redundant-nullability

check failed: 1 error, 1 file
```

### `type.unknown_enum_variant`

```jadpo check project=e-variant expect=type.unknown_enum_variant
type TicketStatus = Enum {
    open
    closed
}

function reopen() -> TicketStatus {
    return TicketStatus.opne
}
```

```text output
error: `TicketStatus` has no variant `opne`
 --> main.jadpo:7:25
  |
7 |     return TicketStatus.opne
  |                         ^^^^ not a variant
  reason: `TicketStatus`'s variants are `open` and `closed`. Did you mean `open`?
  next: Use `TicketStatus.open` (agent)
  rule: type.unknown_enum_variant
  help: docs/language/02-values-and-types.html#type-unknown-enum-variant

check failed: 1 error, 1 file
```

### `type.missing_variant_field`

```jadpo check project=e-variant-missing expect=type.missing_variant_field
type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

function paid() -> PaymentOutcome {
    return PaymentOutcome.paid {}
}
```

```text output
error: `PaymentOutcome.paid` is missing `receipt_id`
 --> main.jadpo:11:12
   |
11 |     return PaymentOutcome.paid {}
   |            ^^^^^^^^^^^^^^^^^^^ fields missing
  reason: Every field of `PaymentOutcome.paid` is given when a value is built; a field that may be absent is declared `T?` and given as `none`.
  next: Add `receipt_id: ...` to the block (agent)
  rule: type.missing_variant_field
  help: docs/language/02-values-and-types.html#type-missing-variant-field

check failed: 1 error, 1 file
```

### `type.unknown_variant_field`

```jadpo check project=e-variant-unknown expect=type.unknown_variant_field
type DeclineReason = Text {
    max_length: 200
}

type PaymentOutcome = Enum {
    declined: {
        reason: DeclineReason
    }
}

function decline(reason: DeclineReason) -> PaymentOutcome {
    return PaymentOutcome.declined {
        reason: reason
        note: reason
    }
}
```

```text output
error: `PaymentOutcome.declined` has no field `note`
 --> main.jadpo:14:9
   |
14 |         note: reason
   |         ^^^^ not declared
  reason: `PaymentOutcome.declined`'s fields are `reason`.
  next: Remove `note`, or declare it on `PaymentOutcome.declined` (agent)
  rule: type.unknown_variant_field
  help: docs/language/02-values-and-types.html#type-unknown-variant-field

check failed: 1 error, 1 file
```

### `type.duplicate_variant_field`

```jadpo check project=e-variant-duplicate expect=type.duplicate_variant_field
type DeclineReason = Text {
    max_length: 200
}

type PaymentOutcome = Enum {
    declined: {
        reason: DeclineReason
    }
}

function decline(reason: DeclineReason) -> PaymentOutcome {
    return PaymentOutcome.declined {
        reason: reason
        reason: reason
    }
}
```

```text output
error: `reason` is given twice
 --> main.jadpo:14:9
   |
13 |         reason: reason
   |         ------ first given here
14 |         reason: reason
   |         ^^^^^^ given again
  reason: Each field of `PaymentOutcome.declined` is given once, so there is never a question of which value counts.
  next: Remove one `reason` (agent)
  rule: type.duplicate_variant_field
  help: docs/language/02-values-and-types.html#type-duplicate-variant-field

check failed: 1 error, 1 file
```

### `type.enum_empty`

```jadpo check project=e-enum-empty-type expect=type.enum_empty
type TicketStatus = Enum {}
```

```text output
error: `TicketStatus` has no variants
 --> main.jadpo:1:6
  |
1 | type TicketStatus = Enum {}
  |      ^^^^^^^^^^^^ an empty enum
  reason: An enum is a closed list of named alternatives, so it needs at least one for a value to be.
  next: Add the enum's variants, one name per line (agent)
  rule: type.enum_empty
  help: docs/language/02-values-and-types.html#type-enum-empty

check failed: 1 error, 1 file
```

### `type.duplicate_variant`

```jadpo check project=e-duplicate-variant expect=type.duplicate_variant
type TicketStatus = Enum {
    open
    closed
    open
}
```

```text output
error: `TicketStatus.open` is declared twice
 --> main.jadpo:4:5
  |
2 |     open
  |     ---- first declared here
4 |     open
  |     ^^^^ declared again
  reason: Each variant of an enum has its own name, so a value and a `match` arm name exactly one.
  next: Remove one `open`, or rename it (agent)
  rule: type.duplicate_variant
  help: docs/language/02-values-and-types.html#type-duplicate-variant

check failed: 1 error, 1 file
```

### `type.unknown_setting`

A type's block holds only the constraints in the table above:

```jadpo check project=e-unknown-setting expect=type.unknown_setting
type ProductName = Text {
    min_length: 1
    maxlength: 120
}
```

```text output
error: `maxlength` isn't a setting Jadpo knows
 --> main.jadpo:3:5
  |
3 |     maxlength: 120
  |     ^^^^^^^^^ unknown setting
  reason: A type's block holds the constraints `min`, `max`, `min_length`, `max_length`, `pattern`, `format`, `scale` and `version`. Did you mean `max_length`?
  next: Use `max_length` instead of `maxlength` (agent)
  rule: type.unknown_setting
  help: docs/language/02-values-and-types.html#type-unknown-setting

check failed: 1 error, 1 file
```

### `type.setting_value`

A setting with a fixed set of options takes an enum value, `Type.value`; quantities take numbers of the right kind:

```jadpo check project=e-setting-value expect=type.setting_value
type ContactEmail = Text {
    format: email
}
```

```text output
error: `format` takes a `TextFormat` value, such as `TextFormat.email`
 --> main.jadpo:2:13
  |
2 |     format: email
  |             ^^^^^ a name here
  reason: `format` is a constraint, and its value must be a `TextFormat` value, such as `TextFormat.email`.
  next: Write `TextFormat.email` (agent)
  rule: type.setting_value
  help: docs/language/02-values-and-types.html#type-setting-value

check failed: 1 error, 1 file
```

### `type.constraint_base`

```jadpo check project=e-constraint-base expect=type.constraint_base
type Nickname = Text {
    min: 1
}
```

```text output
error: `min` doesn't apply to a `Text` type
 --> main.jadpo:2:5
  |
2 |     min: 1
  |     ^^^ doesn't fit the base
  reason: `min` constrains `Int`, `Decimal` and `Duration` types.
  next: Remove `min`, or use a constraint that fits (agent)
  rule: type.constraint_base
  help: docs/language/02-values-and-types.html#type-constraint-base

check failed: 1 error, 1 file
```

### `type.constraint_repeated`

```jadpo check project=e-constraint-repeated expect=type.constraint_repeated
type Nickname = Text {
    max_length: 20
    max_length: 30
}
```

```text output
error: `max_length` is given twice
 --> main.jadpo:3:5
  |
2 |     max_length: 20
  |     ---------- first given here
3 |     max_length: 30
  |     ^^^^^^^^^^ given again
  reason: Each setting is given once, so there is never a question of which one counts.
  next: Remove one `max_length` (agent)
  rule: type.constraint_repeated
  help: docs/language/02-values-and-types.html#type-constraint-repeated

check failed: 1 error, 1 file
```

### `type.constraint_contradiction`

```jadpo check project=e-contradiction expect=type.constraint_contradiction
type Nickname = Text {
    min_length: 10
    max_length: 3
}
```

```text output
error: `max_length: 3` is below `min_length: 10`
 --> main.jadpo:3:5
  |
2 |     min_length: 10
  |     ---------- the minimum
3 |     max_length: 3
  |     ^^^^^^^^^^ below the minimum
  reason: No value could follow both rules, so the type could never be built.
  next: Correct `min_length` or `max_length` (agent)
  rule: type.constraint_contradiction
  help: docs/language/02-values-and-types.html#type-constraint-contradiction

check failed: 1 error, 1 file
```

### `type.invalid_pattern`

A pattern must match the whole text and uses a subset of regular expressions without backreferences or lookaround (question 9):

```jadpo check project=e-pattern expect=type.invalid_pattern
type Doubled = Text {
    pattern: "(a+)+\\1"
}
```

```text output
error: This pattern can't be used
 --> main.jadpo:2:14
  |
2 |     pattern: "(a+)+\\1"
  |              ^^^^^^^^^^ backreferences are not allowed
  reason: A pattern is a regular expression that must match the whole text, without backreferences or lookaround, so it runs in linear time and the same way on every target.
  next: Rewrite the pattern without the unsupported part (agent)
  rule: type.invalid_pattern
  help: docs/language/02-values-and-types.html#type-invalid-pattern

check failed: 1 error, 1 file
```

### `type.decimal_scale_required`

```jadpo check project=e-scale expect=type.decimal_scale_required
type Price = Decimal {
    min: 0.00
}
```

```text output
error: `Price` needs a `scale`
 --> main.jadpo:1:6
  |
1 | type Price = Decimal {
  |      ^^^^^ a `Decimal` type
  reason: A `Decimal` is exact with a fixed number of decimal places, its `scale`, which sets how it is stored and sent.
  next: Add `scale:` with the number of decimal places, such as `scale: 2` (agent)
  rule: type.decimal_scale_required
  help: docs/language/02-values-and-types.html#type-decimal-scale-required

check failed: 1 error, 1 file
```

### `type.optional_outside_input`

```jadpo check project=e-optional expect=type.optional_outside_input
type Nickname = Text {
    max_length: 30
}

type Profile = Object {
    nickname: Nickname {
        optional: true
    }
}
```

```text output
error: `Profile` isn't an `Input`, so its fields are always given
 --> main.jadpo:7:9
  |
7 |         optional: true
  |         ^^^^^^^^ only for an `Input`
  reason: Only a request arriving from outside may leave fields out or have them filled by default; an object built inside the application states every field, using `none` for a field declared `T?`.
  next: Remove the setting, or make the field's type `T?` if it may be absent (agent)
  rule: type.optional_outside_input
  help: docs/language/02-values-and-types.html#type-optional-outside-input

check failed: 1 error, 1 file
```

### `type.reserved_field_name`

```jadpo check project=e-reserved expect=type.reserved_field_name
type Flag = Bool {}

type Settings = Object {
    optional: Flag
}
```

```text output
error: `optional` can't be a field's name
 --> main.jadpo:4:5
  |
4 |     optional: Flag
  |     ^^^^^^^^ reserved
  reason: `include`, `pick`, `omit` and `optional` are lines that build a type's fields, so no field may take their names.
  next: Rename the field (agent)
  rule: type.reserved_field_name
  help: docs/language/02-values-and-types.html#type-reserved-field-name

check failed: 1 error, 1 file
```

### `type.base_outside_declaration`

There is no unnamed object: `Object`, `Enum`, `Input` and `Output` appear only after `=` (question 19):

```jadpo check project=e-base expect=type.base_outside_declaration
type Basket = Object {
    details: Object
}
```

```text output
error: `Object` is used as a type
 --> main.jadpo:2:14
  |
2 |     details: Object
  |              ^^^^^^ only a base
  reason: `Object` is written only after `=` in a `type` declaration; every shape has a name, so a field or signature names that type.
  next: Declare `type Name = Object { ... }` and use its name here (agent)
  rule: type.base_outside_declaration
  help: docs/language/02-values-and-types.html#type-base-outside-declaration

check failed: 1 error, 1 file
```

### `type.type_argument_count`

```jadpo check project=e-type-arguments expect=type.type_argument_count
type Tag = Text {
    max_length: 30
}

type Basket = Object {
    tags: List<Tag, Tag>
}
```

```text output
error: `List` takes one type argument, as in `List<Tag>`
 --> main.jadpo:6:11
  |
6 |     tags: List<Tag, Tag>
  |           ^^^^^^^^^^^^^^ 2 given
  reason: `List` holds values of the type in `< >`, so it needs exactly 1.
  next: Write `List` with one type argument, as in `List<Tag>` (agent)
  rule: type.type_argument_count
  help: docs/language/02-values-and-types.html#type-type-argument-count

check failed: 1 error, 1 file
```

### `type.copy_into_input`

```jadpo check project=e-copy-input file=names.jadpo hidden
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
}
```

```jadpo check project=e-copy-input file=main.jadpo expect=type.copy_into_input
type CreateTodo = Input {
    include: [Todo]
}
```

```text output
error: `CreateTodo` is an `Input` and can't use `include`
 --> main.jadpo:2:5
  |
2 |     include: [Todo]
  |     ^^^^^^^ copies every field
  reason: `include` would also accept every field added to the source type later, so a request could set fields nobody chose to expose.
  next: Name the fields this input accepts with `pick: [...]` (human)
  rule: type.copy_into_input
  help: docs/language/02-values-and-types.html#type-copy-into-input

check failed: 1 error, 2 files
```

### `type.absent_field`

```jadpo check project=e-absent file=names.jadpo hidden
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
}
```

```jadpo check project=e-absent file=main.jadpo expect=type.absent_field
type TodoView = Output {
    pick: [Todo.id, Todo.title]
    omit: [Todo.notes]
}
```

```text output
error: `TodoView` has no field `notes` at this point
 --> main.jadpo:3:12
  |
3 |     omit: [Todo.notes]
  |            ^^^^^^^^^^ not present
  reason: `omit` names a field the lines above bring in, so a typo or a field removed since is caught; none of them brings in `notes`.
  next: Remove `Todo.notes` from the list (agent)
  rule: type.absent_field
  help: docs/language/02-values-and-types.html#type-absent-field

check failed: 1 error, 2 files
```

### `type.copy_clash`

```jadpo check project=e-copy-clash file=names.jadpo hidden
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
}

entity Note {
    id: Uuid
    title: TodoTitle
}
```

```jadpo check project=e-copy-clash file=main.jadpo expect=type.copy_clash
type TodoView = Output {
    pick: [Todo.id, Todo.title, Note.title]
}
```

```text output
error: `title` is already in `TodoView`, from `Todo`
 --> main.jadpo:2:33
  |
2 |     pick: [Todo.id, Todo.title, Note.title]
  |                                 ^^^^^^^^^^ copied again
  reason: A copied field never replaces another field, because a field added to a source type would then change this type unseen.
  next: Remove `title` from one of them with `omit: [...]`, or write `title` as a field below them (agent)
  rule: type.copy_clash
  help: docs/language/02-values-and-types.html#type-copy-clash

check failed: 1 error, 2 files
```

### `type.copy_cycle`

An `include` that leads back to its own type would copy its fields from
themselves:

```jadpo check project=e-copy-cycle expect=type.copy_cycle
type Title = Text {}

type TodoView = Output {
    include: [ListView]
    title: Title
}

type ListView = Output {
    include: [TodoView]
}
```

```text output
error: `ListView` includes its own fields
 --> main.jadpo:9:15
  |
9 |     include: [TodoView]
  |               ^^^^^^^^ leads back here
  reason: `TodoView` leads back to `ListView` through its own `include`, so its fields would be copied from themselves and the type could never be written out.
  next: Remove `TodoView` from `include`, or write the fields it needs (agent)
  rule: type.copy_cycle
  help: docs/language/02-values-and-types.html#type-copy-cycle

check failed: 1 error, 1 file
```

### `type.map_key_boundary`

A map in an `Input` or an `Output` is a JSON object, whose keys are text, so
its key type is built on `Text` or `Uuid` (question 5):

```jadpo check project=e-map-key expect=type.map_key_boundary
type DueAt = Instant {}

type Weight = Int {
    min: 0
}

type DueWeights = Output {
    weights: Map<DueAt, Weight>
}
```

```text output
error: `DueWeights` holds a map keyed by `DueAt`
 --> main.jadpo:8:14
  |
8 |     weights: Map<DueAt, Weight>
  |              ^^^^^^^^^^^^^^^^^^ keys are `DueAt`
  reason: `DueWeights` crosses a boundary as JSON, where a map is an object and its keys are text; `DueAt` is built on `Instant`.
  next: Key the map by a type built on `Text` or `Uuid`, or send a list of objects instead (agent)
  rule: type.map_key_boundary
  help: docs/language/02-values-and-types.html#type-map-key-boundary

check failed: 1 error, 1 file
```

### `syntax.type_parent_required`

```jadpo check project=e-parent expect=syntax.type_parent_required
type Dimensions = {
    width: Int
}
```

```text output
error: `Dimensions` needs a base type
 --> main.jadpo:1:19
  |
1 | type Dimensions = {
  |                   ^ expected a base before `{`
  reason: Every type is written `type Name = Base { ... }`; the base says what it is built from, such as `Object`, `Enum`, `Input` or `Text`.
  next: Add the base before `{`, for example `type Dimensions = Object {` (agent)
  rule: syntax.type_parent_required
  help: docs/language/02-values-and-types.html#syntax-type-parent-required

check failed: 1 error, 1 file
```

### `syntax.type_block_required`

A type always has a block, even an empty one, so it never reads like an alias:

```jadpo check project=e-type-block expect=syntax.type_block_required
type OrderId = Uuid
```

```text output
error: `OrderId` needs a block
 --> main.jadpo:1:16
  |
1 | type OrderId = Uuid
  |                ^^^^ expected `{}` after the base
  reason: Every type is written `type Name = Base { ... }`. Without a block it reads like an alias, and Jadpo has no aliases: every type is a new type.
  next: Add `{}` after `Uuid` (agent)
  rule: syntax.type_block_required
  help: docs/language/02-values-and-types.html#syntax-type-block-required

check failed: 1 error, 1 file
```

### `syntax.variable_type_required`

Every variable states its type, and a literal takes it:

```jadpo check project=e-variable-type expect=syntax.variable_type_required
type Price = Decimal {
    scale: 2
    min: 0.00
}

function free() -> Price {
    var price = Price(0.00)
    return price
}
```

```text output
error: `price` needs a type
 --> main.jadpo:7:9
  |
7 |     var price = Price(0.00)
  |         ^^^^^ expected `: Type` after the name
  reason: Every variable states its type, so a reader sees it where the value is introduced.
  next: Write `var price: Price = 0.00` (agent)
  rule: syntax.variable_type_required
  help: docs/language/02-values-and-types.html#syntax-variable-type-required

check failed: 1 error, 1 file
```

### `syntax.expected_constraint`

> The archived compiler's error: a misspelt setting is now `type.unknown_setting`.

```jadpo project=e-constraint expect=syntax.expected_constraint
type ProductName = Text {
    min_length: 1
    maxlength: 120
}
```

### `syntax.constraint_colon_required`

> The archived compiler's error: the new parser reports a missing colon as `syntax.colon_required` (area 3).

```jadpo project=e-constraint-colon expect=syntax.constraint_colon_required
type ContactEmail = Text {
    format email
}
```

### `syntax.expected_constraint_value`

> The archived compiler's error: a setting's value is now checked as `type.setting_value`.

```jadpo project=e-constraint-value expect=syntax.expected_constraint_value
type Price = Decimal {
    min: zero
}
```

### `syntax.expected_literal`

> The archived compiler's error: settings go in a block after the field's type, and a missing value is `syntax.expected_expression` (area 3).

```jadpo project=e-literal-value expect=syntax.expected_literal
type PageSize = Int {
    min: 1
}

input ListProducts {
    page_size: PageSize default
}
```

### `syntax.expected_field_initialiser`

> The archived compiler's error: the new parser reports `Product { name }` as `syntax.colon_required` (area 3).

```jadpo project=e-initialiser expect=syntax.expected_field_initialiser
type ProductName = Text {
    max_length: 120
}

value Product {
    name: ProductName
}

function product(name: ProductName) -> Product {
    return Product { name }
}
```

### `syntax.enum_empty`

> The archived compiler's error: `type X = Enum {}` is now `type.enum_empty`.

```jadpo project=e-enum-empty expect=syntax.enum_empty
enum TicketStatus {}
```

## Review

### Status

| Feature | Status |
|---|---|
| Built-in types | Works in general. `Int` is a JavaScript number at runtime, so values beyond 2^53 lose precision without an error (`Big(9007199254740993)` compiles). `Decimal` is a binary floating-point number, not an exact decimal. |
| Named types and nominal identity | Works in general. `type OrderId = Uuid` without a block is also accepted and formatted as-is, so there are two spellings. |
| Constraint checking | Partial. Literals are checked at compile time for `min`, `max`, `min_length`, `max_length`, `format: email` and the time types, but not for `pattern`. At runtime, `pattern` is enforced only for the exact text `[a-z0-9_]+` and `format` only for `email`; every other pattern or format is silently dropped from the generated validator. Constraints that do not fit the base (`min` on `Text` becomes a string-to-number comparison at runtime), contradictory bounds (`min_length: 10`, `max_length: 3`), repeated constraints, negative lengths and invalid regular expressions are all accepted. Duration bounds cannot be written (`min: 1s` is a syntax error). |
| `Type(value)` construction | Works for literals. Runtime construction validates, but its failure is undeclared: the generated code throws a `ValidationError` that route handlers only catch around input decoding, so a failure inside behaviour escapes as an internal fault (inferred from generated code, not run). `Uuid`-based types cannot be built from any literal, while `Instant`-based ones can. `TicketStatus("open")` is rejected, but with the generic "can't be constructed from `Text`" message rather than "enums are not constructed from text". |
| Primitive-signature rule | Works for bare and nullable built-ins. Gaps: `List<Text>` and inline `Object { a: Int }` parameters pass, and a field declared as `Text` gives a usable `Owner.field` type. Prelude enums (`Zone`, `Weekday`, `Locale`) and `Instant` are rejected and need wrapping. |
| Objects | Works in general. Two bugs: object construction checks each field against its declared type rather than its field type, so `Badge { label: name }` skips the field's own `max_length: 12` (no runtime check is generated), while entity `create` demands the exact field type; and a field given twice in an object (`Product { name: a name: b }`) is accepted, although the same mistake in an enum variant is an error. |
| `optional` and `default` | Partial and unsound outside inputs. `optional` is accepted on `value` and `output` fields; such a field can be left out at construction and then read as a non-nullable `T`, which is `undefined` at runtime. On an input, an `optional` field reads as `T` and cannot be matched with `some`/`none`. `default` on a `value` is accepted but construction still requires the field. `name: Name default none` on a non-nullable field reports "`none` can't be used where `Name?` is required", which contradicts itself. |
| Enums and tagged unions | Works in general for typing, matching and boundary decoding. Storage and migration of payload variants is designed in area 5. `type X = Enum { ... }` is a second accepted spelling. |
| Collections | Types only. `List<T>` works at boundaries and in objects. `Set<T>` and `Map<K, V>` compile at HTTP boundaries, but the generated decoder requires a JavaScript `Set`/`Map`, which JSON never produces, so every request containing one is rejected with 422 (read from generated code, not run). No literals, iteration or `collection.*` functions exist; the `collection` namespace is reserved but empty. The formatter writes `List<Object { ... } >` with a stray space. |
| Compatibility rules | Works in general: widening, sibling rejection, invariance and nullable lifting all behave as documented. |
| Equality | Bug. `==` on objects and lists compiles to JavaScript `===`, which compares object identity, so two equal objects built separately are unequal. |
| Value semantics | Works in general: no field assignment, `var mut` rebinding only. |

### Consistency with the rest of the language

- **Two spellings for objects and enums.** `value X { }` and
  `type X = Object { }` produce the same thing, as do `enum X { }` and
  `type X = Enum { }`. The golden app uses only the keyword forms.
- **`input` and `output` are keywords without a meaning.** The decision register
  says boundary roles "are not type categories", yet `input` and `output`
  declare objects, and nothing stops an `output` being a route's input.
- **`=` appears only in `type`.** `type Pence = Int { }` uses `=`; `value`,
  `enum`, `entity` and `failure` do not. Flagged in area 1 for the syntax pass.
- **The block after a field's type changes vocabulary.** On a `value`,
  `label: ProductName { max_length: 12 }` holds constraints; on an entity,
  `id: Uuid { generated: identity }` and `owner_id: User.id { role: ... }` hold
  settings in the same position, and a generated timestamp is written
  `created_at: Instant generated { on: create }` instead.
- **Field facts are split between trailing words and blocks.** `optional`,
  `default 25` and `generated { ... }` trail the type; `immutable: true` and
  `generated: identity` go in the block.
- **Constraint values are spelled three ways.** `format: email` is a bare
  name, `pattern: "..."` a string, `min: 0` a number. List size reuses
  `max_length`.
- **Five ways to make a value.** `Quantity(1)` for named types,
  `Customer.email(x)` for field types, `Product { ... }` for objects,
  `TicketStatus.open` for plain variants and `PaymentOutcome.paid { ... }` for
  payload variants. Each is deliberate, but `Type(...)` works for every named
  type except enums.
- **Braces hold one item per line, except in two places.** Match patterns
  bind payload fields as a comma list, `PaymentOutcome.paid { receipt_id, paid_at }`,
  while the same shorthand in construction, `Product { name }`, is an error.
  Import lists are the other comma list (area 1).
- **Dotted names mean different things by position.** `Customer.email` is a
  type in a signature, a field read in an expression and a constructor when
  called. Variant field types take three parts, `PaymentOutcome.declined.reason`.
- **Duration literals exist only in configuration.** `default: 30d` is valid in
  a config block but not in an expression or a constraint.
- **Narrowing is strict in one place and lax in another.** Entity `create`
  requires `Todo.status(TodoStatus.open)`; building a `value` accepts
  `TicketStatus.open` directly.

### Decided: one form for every type

Decided on 2026-10-12 (questions 10, 18 and 19). Every type is declared the
same way: `type Name = Base { ... }`. The base says what the type is built
from, and the block holds the details: rules for a number or text, fields for
an object, variants for an enum. `entity` stays separate because an entity is
more than a type (storage, actions, queries and policy).

```jadpo check project=one-form file=names.jadpo hidden
type Currency = Enum {
    gbp
    eur
}

type ReceiptId = Uuid {}

type DeclineReason = Text {
    max_length: 200
}

type OrderNote = Text {
    max_length: 500
}

type OrderLineRequest = Input {
    sku: Sku
}

entity Order {
    id: Uuid
    total: Price
}
```

```jadpo check project=one-form file=main.jadpo
type Price = Decimal {  // rules on a number
    scale: 2
    min: 0.00
}

type Sku = Text {  // rules on text
    pattern: "[A-Z]{3}-[0-9]{4}"
}

type OrderId = Uuid {}  // a new identity type with no extra rules

type Money = Object {  // fields
    amount: Price
    currency: Currency
}

type OrderStatus = Enum {  // variants
    pending
    paid
    shipped
}

type PaymentOutcome = Enum {  // variants that carry fields
    pending
    paid: {
        receipt_id: ReceiptId
    }
    declined: {
        reason: DeclineReason
    }
}

type CreateOrder = Input {  // an object arriving from outside
    lines: List<OrderLineRequest>
    note: OrderNote {
        optional: true
    }
}

type OrderView = Output {  // an object sent back
    id: Order.id
    total: Price
}
```

- `Object`, `Enum`, `Input` and `Output` are only ever used as a base here.
  There is no anonymous object written inline; every shape has a name.
- `Input` objects may have `optional` and `default` fields and are the only
  objects a route accepts. `Output` objects are the only objects a route
  returns, and when derived they may only `pick`.
- `value`, `enum`, `input` and `output` disappear as keywords, and the
  `types/` folder holds only `type` declarations.

### Decided: building a type from other types

*Revised 2026-10-13 (Richard): replaces `Output from Todo { pick: [...] }`.*

Most object types in a real application are slices of an entity or of another
object: a create input, a patch input where every field is optional, a public
view, a list summary, a v2 response that adds a field. Instead of writing each
field out (`title: Todo.title`), a type's body can copy fields from other
types. The body keeps its ordinary `Output { }` form; four kinds of line build
it, **read from top to bottom**, each changing the result of the lines above:

| Line | Does |
|---|---|
| `include: [Todo]` | adds every field of each listed type (types only; never on an `Input`) |
| `pick: [Todo.id, Todo.title]` | adds just the listed fields, each keeping its own name |
| `omit: [Todo.internal_note]` | removes fields present at that point |
| `bar: User.bar` | a written field: replaces a field of the same name above, or adds a new one |

```jadpo check project=building file=names.jadpo hidden
type TodoTitle = Text {
    max_length: 200
}

type TodoStatus = Enum {
    open
    done
}

type DayCount = Int {
    min: 0
}

type Quantity = Int {
    min: 1
}

entity User {
    id: Uuid
    email: Email
}

entity Todo {
    id: Uuid
    title: TodoTitle
    status: TodoStatus
    due_at: Instant?
    deleted_at: Instant?
}

type OrderLineView = Output {
    quantity: Quantity
}

entity Order {
    id: Uuid
    lines: List<OrderLineView>
}
```

```jadpo check project=building file=main.jadpo
type TodoView = Output {
    pick: [Todo.id, Todo.title, Todo.status, Todo.due_at]
}

type TodoDetail = Output {
    include: [Todo]
    omit: [Todo.deleted_at]
    owner_email: User.email
}

type PatchTodo = Input {
    pick: [Todo.title, Todo.due_at]
    optional: all
}

type OrderView = Output {
    include: [Order]
    lines: List<OrderLineView>  // replaces Order.lines with a view of each line
}

type TodoReminder = Object {
    pick: [Todo.id, Todo.title, Todo.due_at]
    owner_email: User.email
    days_overdue: DayCount
}
```

Rules:

- **Top to bottom.** The same kind of line may appear more than once, and the
  formatter keeps the order written, because order matters here.
- **Lists are always in brackets**, even with one entry: `include: [Todo]`.
- **`omit` removes only what is there.** Omitting a field that is not present
  at that point is an error, which catches typos and fields since removed.
- **Written fields win.** A field you write replaces any field of the same
  name above it, visibly; otherwise it is added.
- **Copied fields never silently replace copied fields.** An `include` or
  `pick` that would replace a field brought in by an earlier `include` or
  `pick` is an error: resolve it with `omit` or by writing the field. Copied
  fields are not visible in the declaration, so a clash there would change a
  type unseen when a source type gains a field.
- **No `include` on an `Input`.** Taking every field of `User` would accept
  any field added to `User` later (the mass-assignment bug). Inputs `pick` or
  write their fields. Generated fields such as `id` and `created_at` can never
  appear in an `Input`.
- **`optional`** makes the listed fields, or `all`, optional, only on an
  `Input`.
- **Copied fields follow their source.** Renaming `Todo.title` renames it in
  every type that copies it, and its validation rules come with it.
- **No type includes itself.** An `include` that leads back to its own type,
  directly or through another, is `type.copy_cycle` (chunk 1.9).
- **Every built type has a name;** there are no inline types in signatures.
  Hover and `jadpo inspect TodoDetail` show the final fields and where each
  came from (`title ← Todo.title`, `lines ← written, replaces Order.lines`).
- **Outputs may `include`.** A field added to `Todo` later reaches
  `TodoDetail`, and the committed API record (area 7) shows the addition in
  review; fields policy hides from clients stay hidden at runtime (area 9).
  This replaces the earlier "an `Output` may only `pick`" rule.
- `include`, `pick`, `omit` and `optional` are reserved and cannot be field
  names.

### Decided changes in examples

Every decision on this page, shown as code with all the others applied.
They describe the language after the rework, so they are shown without being
compiled; exact syntax for logic is settled in area 3.

#### Q1: a runtime check that can fail

```jadpo fragment
type QuantityOutOfRange = Failure {
    kind: FailureKind.invalid_value
}

action double_quantity(quantity: Quantity) fails QuantityOutOfRange -> Quantity {
    var doubled: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityOutOfRange
    }
    return doubled
}
```

A literal needs no `attempt`: `var quantity: Quantity = 0` is rejected while
compiling.

#### Q2: an object's fields follow their own rules

```jadpo check project=q2-badge file=users.jadpo hidden
entity User {
    id: Uuid
    display_name: Text {
        max_length: 80
    }
}

type BadgeNameTooLong = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=q2-badge
type Badge = Object {
    name: Text {
        max_length: 12
    }
}

action make_badge(user: User) fails BadgeNameTooLong -> Badge {
    // user.display_name allows 80 characters and the field 12, so the
    // name is built first, where its failure is mapped; the object
    // literal never fails.
    var name: Badge.name = attempt Badge.name(user.display_name) {
        max_length: BadgeNameTooLong
    }
    return Badge {
        name: name
    }
}
```

#### Q3: `optional` only on inputs, read as nullable

```jadpo check project=q3-optional file=names.jadpo hidden
type Nickname = Text {
    max_length: 30
}
```

```jadpo check project=q3-optional file=update_profile.jadpo
type UpdateProfile = Input {
    nickname: Nickname {
        optional: true
    }
}

action update_profile(request: UpdateProfile) -> Nickname? {
    var nickname: Nickname? = request.nickname  // left out reads as none
    return nickname
}
```

`optional: true` on a field of a `type Profile = Object { ... }` is an error: an object
built inside the application always states every field.

#### Q4: lists you can build, count and loop over

```jadpo fragment
var tags: List<TagName> = ["urgent", "home"]
var count: TagCount = TagCount(collection.count(tags))
var has_urgent: Bool = collection.contains(tags, "urgent")

for tag in tags {
    // tag is a TagName
}
```

`collection.map` and `collection.filter` take a function as an argument, which
depends on area 3's answer on passing functions.

#### Q5: sets and maps in JSON

```jadpo fragment
type RetagTodo = Input {
    tags: Set<TagName>
    weights: Map<TagName, Weight>
}
```

```text
{ "tags": ["urgent", "home"], "weights": { "urgent": 3, "home": 1 } }
```

`["urgent", "urgent"]` is rejected as a duplicate. `Map<Instant, Weight>` at a
boundary is a compile error, because JSON object keys are text. Built in
chunk 1.9: a map in an `Input` or `Output`, however nested, is keyed by a type
built on `Text` or `Uuid` (`type.map_key_boundary`), and a set literal that
repeats a value is `type.invalid_literal`.

#### Q6: no transparent aliases

```jadpo fragment
type OrderId = Uuid {}    // a new type: an OrderId is not a CustomerId
type OrderId = Uuid       // rejected: reads like an alias in other languages
```

#### Q7: converting between types only by constructing

```jadpo fragment
var total: Pence = Pence(price * quantity)    // the only way across
var raw: Int = price as Int                    // rejected: there is no cast
```

#### Q8: numbers by range and scale

```jadpo fragment
type Quantity = Int {
    min: 1
    max: 1000          // fits a smallint column; JSON number
}

type LedgerAmount = Int {
    min: 0             // no max: bigint column; JSON string when large
}

type Price = Decimal {
    scale: 2
    min: 0.00          // numeric(p, 2); JSON "12.50"
}

var share: Price = decimal.divide(total, people, rounding: Rounding.half_even)
```

#### Q9: patterns match the whole text and are checked while compiling

```jadpo fragment
type Sku = Text {
    pattern: "[A-Z]{3}-[0-9]{4}"
}

var sku: Sku = "ABC-1234"       // checked while compiling
var bad: Sku = "ABC-1234x"      // compile error: the whole text must match

type Doubled = Text {
    pattern: "(a+)+\\1"         // rejected: backreferences are not allowed
}
```

#### Q11: which built-in types a signature may use

```jadpo fragment
type ReminderTime = Instant {}

function opening_hours(zone: Zone) -> OpeningHours { ... }      // allowed: closed enum
function remind_at(at: Instant) -> Reminder { ... }             // rejected: what instant?
function remind_at(at: ReminderTime) -> Reminder { ... }        // allowed: named
function tag_all(tags: List<Text>) -> TagSet { ... }            // rejected: element is raw Text
```

#### Q12: `==` compares contents

```jadpo fragment
var a: Money = Money {
    amount: 5.00
    currency: Currency.gbp
}
var b: Money = Money {
    amount: 5.00
    currency: Currency.gbp
}
var same: Bool = a == b         // true: same contents (false today)
```

#### Q13: UUID versions

```jadpo check project=q13-uuid-versions
entity Order {
    id: Uuid  // generated as version 7
}

type PartnerRef = Uuid {
    version: UuidVersion.v4  // accepts and generates only version 4
}
```

#### Q15: literals take the type they go into

```jadpo fragment
var price: Price = 0.00                          // checked while compiling
var line: OrderLine = OrderLine {
    quantity: 1                                  // 1 becomes a Quantity
}

function first_quantity() -> Quantity {
    return 1
}
```

#### Q16: every variable states its type

```jadpo fragment
var total: Pence = Pence(price * quantity)
var todo: Todo = attempt Todo.get(id)

for line in order.lines {       // a loop binding takes its type from the list
    var subtotal: Pence = line.total
}
```

#### Q17: fixed options are written `Type.value`

```jadpo check project=q17-options
type ContactEmail = Text {
    format: TextFormat.email
}

type PartnerRef = Uuid {
    version: UuidVersion.v4
}

type TodoNotFound = Failure {
    kind: FailureKind.not_found  // was: kind: NotFound
}
```

#### Q20: field settings in a block

```jadpo fragment
type ListProducts = Input {
    name: ProductName {
        optional: true
    }
    page_size: PageSize {
        default: 25
    }
}

type RenameProduct = Input {
    name: ProductName
    subtitle: ProductName? {
        default: none
    }
}
```

This is the form configuration already uses:

```jadpo fragment
mail_sender: Email {
    binding: "MAIL_SENDER"
}
```

#### Deferred to area 3: one argument, several types

`Customer.id` and `Supplier.id` stay distinct even when both are version 4
UUIDs, so neither can be passed where the other is expected. A function that
genuinely accepts either would take an enum whose variants carry each one,
which keeps the choice explicit:

```jadpo fragment
type PartyId = Enum {
    customer: {
        id: Customer.id
    }
    supplier: {
        id: Supplier.id
    }
}
```

Whether functions should also accept several types directly (generics or
unions) is a logic question and is settled in area 3.

### Carried to later areas

Agreed or raised while reviewing this area; recorded in the area it belongs
to when that page is written.

| Item | Area | Status |
|---|---|---|
| Match arms use `->`, the same arrow as return types; `=>` is removed | 3 | Richard's call, 2026-10-12 |
| A function argument that accepts several types: today an `Enum` whose variants carry each type; whether to add generics or unions | 3 | Open |
| Failure `kind` values become lower_snake_case (`kind: FailureKind.not_found`), following the setting-options decision | 4 | Follows from a decision |
| Entity field settings (`identity`, `unique`, `references`, `generated`) go in the field's block, following question 20 | 5 | Follows from a decision |
| The remaining syntax-review findings (`docs/language/research/syntax-review.md`) | 3 to 14 | Input to each page |

### Open questions

1. **How does a runtime `Type(value)` failure reach the caller?** Today it
   escapes as an internal fault. The language already maps expected failures
   with clauses on fallible operations (`missing: TodoNotFound`,
   `conflict: TodoMutationConflict`). *Decided 2026-10-12:* make runtime
   construction a fallible operation that needs `attempt` and an
   `invalid: SomeFailure` clause naming an `InvalidValue`-kind failure, as in
   `attempt Quantity(count) invalid: QuantityOutOfRange`. Literal construction
   stays infallible because it is checked while compiling. This mirrors Rust's
   `TryFrom` (failure is in the signature) rather than Kotlin's `require`
   (an exception), and closes TYPE-001. *Revised 2026-10-14:* the failure
   is mapped by rule in a block after the call,
   `attempt Quantity(count) { max: QuantityTooLarge }`, with `invalid:`
   covering every rule without an arm of its own ("Construction failures
   by rule").
2. **Should building an object check fields against their declared type or
   their field type?** Object construction uses the declared type and skips
   field rules; `create` demands the exact field type, which is why the golden
   app writes `Todo.status(TodoStatus.open)`. *Decided 2026-10-12:* one rule for
   both: accept the declared type or anything that widens to it, and run the
   field's own rules at that point (fallible under question 1 only when the
   field adds rules). *Revised 2026-10-14:* an object literal never fails; a
   field whose value might break its rules is built first, on its own line. Identifier safety is unaffected, because a reference
   field's declared type is itself a field type (`customer_id: Customer.id`
   still rejects `supplier.id`), and the noise disappears from the golden app.
3. **Where may `optional` be used, and what type does reading it give?**
   *Decided 2026-10-12:* only on `input` fields, and reading one gives `T?`
   (absent reads as `none`). Only the update `patch:` form keeps the
   omitted-versus-`none` distinction. `default` is likewise input-only. This
   removes the `undefined` leak; TypeScript's `field?: T` reading as
   `T | undefined` is the same idea.
4. **Which collection operations come first?** Collections are declared but
   cannot be built, counted or iterated. *Decided 2026-10-12:* build list
   literals (`[a, b]`), `for item in items` (already the recommended loop form
   in the decision register) and a small namespaced set:
   `collection.count`, `collection.contains`, `collection.map`,
   `collection.filter`. Keep collections invariant (Kotlin and Rust do;
   TypeScript's covariant arrays are unsound). Closes TYPE-002 for lists.
5. **How are `Set` and `Map` written in JSON?** They compile at boundaries but
   are always rejected. *Decided 2026-10-12:* a `Set<T>` is a JSON array that is
   rejected if it has duplicates; a `Map<K, V>` is a JSON object and is allowed
   only when `K` is text-based or `Uuid`-based. Other key types are a compile
   error at a boundary.
6. **Transparent aliases (TYPE-003)?** *Decided 2026-10-12:* no, and close the
   issue. Every `type` stays a new type. Also reject the block-less
   `type OrderId = Uuid`: in TypeScript, Rust and Kotlin that shape is a
   transparent alias, so an agent or reviewer will misread it. Require
   `type OrderId = Uuid {}`.
7. **Is explicit construction the only way to reach a value's
   representation (TYPE-004)?** Today any named type converts to any other
   with the same representation through its constructor, re-checking rules,
   and operators work on the representation. *Decided 2026-10-12:* yes, and close
   the issue: no `as` cast and no unwrap escape hatch. The golden app has not
   needed one.
8. **Which number types, and what are they at runtime?** `Int` silently
   loses precision past 2^53 and `Decimal` is a JavaScript float, which is
   wrong for money (JavaScript has one number type, a 64-bit float, plus
   `BigInt` since 2020, and no decimal). *Decided 2026-10-12 (agreed in discussion):* no sized types
   such as `Int32` or `UInt8`. Size is a storage detail; the business rule is
   the range you declare with `min` and `max`, and the compiler picks the
   database column from it (`smallint`, `integer`, `bigint`). `Int` is an
   exact 64-bit integer where overflow is an error, never a silent wrap; JSON
   carries it as a number when the declared range fits JavaScript's safe range
   and as a string otherwise. `Decimal` is exact, with a required `scale:`
   (`type Price = Decimal { scale: 2 }`); operations that can produce extra
   places, such as division or percentages, must name a rounding mode from
   the standard library. JSON carries a `Decimal` as a string (`"12.50"`).
   PostgreSQL stores `numeric(p, s)`; SQLite has no exact decimal, so it stores
   a scaled integer. No `Float` until an application needs approximate
   measurement, and then as its own explicit type so money can never use it
   by accident. Compare C#'s `decimal`, Java's `BigDecimal` and Python's
   unbounded `int`.
9. **What do `pattern` and `format` mean exactly?** *Decided 2026-10-12:* a
   pattern must match the whole text, uses a regular-expression subset with
   no backreferences or lookaround (RE2-style, so it runs in linear time and
   behaves the same on every target), is compiled and checked at compile time,
   and is applied to literals. `format` accepts only `email`; `Url` and
   `IpAddress` are types, not formats. Every constraint must fit its base type,
   and contradictory or repeated bounds are errors.
10. **One spelling per kind of type?** *Decided 2026-10-12:* every type is
    `type Name = Base { ... }`, including `Object`, `Enum`, `Input` and
    `Output`; see "One form for every type" above. Only `Input` objects may
    use `optional` and `default`, a route accepts only an `Input` and returns
    only an `Output`. This amends the decision-register line that boundary
    roles are not type categories.

11. **Which built-in types does the signature rule cover?** *Decided 2026-10-12:*
    keep it for representation types and `Instant`, `CalendarDate`, `Time` and
    `Duration` (a bare `Instant` parameter has the same ambiguity as a bare
    `Int`), allow the closed prelude enums `Zone`, `Locale` and `Weekday`, and
    close the gaps: check collection element types and reject inline `Object`
    parameters.
12. **What does `==` mean on objects, lists and payload variants?**
    *Decided 2026-10-12:* structural equality, field by field, with sets and maps
    compared as sets, as in Rust's derived `PartialEq`, Kotlin data classes and
    Elixir. The alternative of rejecting `==` on objects would force agents to
    write field-by-field comparisons by hand.

13. **Which UUIDs?** *Decided 2026-10-12:* any version is accepted by
    default and normalised to lowercase; new identities are version 7, which
    is time-ordered and keeps indexes compact; a named type can pin a version,
    `type PartnerRef = Uuid { version: UuidVersion.v4 }`, which governs both what is
    accepted and what is generated (for example where a version 7 ID would
    reveal sensitive creation times, or another system dictates the version).

14. **Adopt derived types?** See "Building a type from other types" above. *Decided 2026-10-12 (agreed in discussion, with new fields listed directly and lists in brackets):*
    yes, with exactly `pick`, `omit` and `optional` plus listed new fields,
    named results only, and outputs restricted to `pick`. This covers create,
    patch, view, summary and versioned shapes without TypeScript's anonymous
    type algebra, whose expanded types reviewers and error messages cannot
    name.
    *Revised 2026-10-13 (Richard):* the syntax changed to body lines read top
    to bottom (`include:`, `pick:`, `omit:`, written fields), keeping the
    plain `Output { }` header; see "Building a type from other types".

15. **Should a literal take on the type it is going into?** Today
    `var price: Price = 0.0`, `return 0.0` and `Item { price: 0.0 }` are all
    errors; you must write `Price(0.0)`. *Decided 2026-10-12 (agreed in discussion):* yes, for literals
    only. Where the expected type is known (a variable's declared type, a
    return, a field or an argument), a literal becomes that type and its rules
    are checked while compiling, so `var price: Price = 0.0` and `return 1`
    from a function returning `Quantity` just work. Computed values still need
    `Price(x)`, so a constructor in code always means a real check or a change
    of type. Write the type once: `var price: Price = 0.0`, never
    `var price: Price = Price(0.0)`, and the formatter rewrites
    `var price = Price(0.0)` to that form. Rust adapts literals to built-in
    integer types but not to its newtypes; this goes one step further because
    the check still happens at compile time.

16. **Must every variable declare its type?** Today `var total = price * quantity`
    is allowed and infers a plain `Int`; the mismatch only surfaces later,
    where `total` is returned as `Pence`. *Decided 2026-10-12 (agreed in discussion):* yes: every `var`
    states its type (`var total: Pence = Pence(price * quantity)`), while
    loop and `match` bindings take theirs from what they bind. The reader sees
    each value's type where it is introduced, a wrong type is reported on the
    line that made it, and there is one rule with no exceptions to remember.
    The cost is verbosity, which falls on the agent writing the code rather
    than the person reviewing it, and literal typing (question 15) removes
    most of the repetition: `var price: Price = 0.0`. Most modern languages
    (TypeScript, Rust, Go, Kotlin) infer local types instead; Jadpo's
    audience, code written by agents and reviewed by people, favours the
    explicit form.

17. **How is a setting with a fixed set of options written?** *Decided
    2026-10-12, revised the same day (Richard):* always as an enum value,
    `Type.value`, the same form as in code: `version: UuidVersion.v7`,
    `format: TextFormat.email`, `kind: FailureKind.not_found`,
    `on_delete: OnDelete.cascade`. Genuine quantities such as `min`, `max`
    and `scale` stay numbers. A bare word in a setting is therefore always a
    name you declared (`store: cache`), and a fixed option is unmistakable;
    the type name says where the choices are listed. Value names read as a
    value or a moment, never a verb (`Generated.on_create`, not
    `Generated.create`). This replaces the first version of this decision,
    which wrote bare names in settings.

18. **What replaces `value`?** *Decided 2026-10-12:* `value` is dropped. An
    object used inside the application is `type Name = Object { fields }`,
    the same form as every other type (question 10).

19. **Remove anonymous `Object`?** *Decided 2026-10-12:* yes. `Object` is
    only used as a base in a `type` declaration; an unnamed shape written
    inline as a field type or inside `List<...>` is an error.

20. **Should field settings go in a block?** Today an input field's settings
    trail its type separated by spaces (`name: ProductName optional`,
    `page_size: PageSize default 25`), while configuration fields already use
    a block (`mail_sender: Email { binding: "MAIL_SENDER" }`).
    *Decided 2026-10-12 (agreed in discussion):* one rule everywhere: a field's settings go in a block
    after its type, one `name: value` per line (`optional: true`,
    `default: 25`, `default: none`). It reads like every other block in the
    language and gives later settings an obvious place. Entity field settings
    (`identity`, `unique`, `references`, `generated`) follow the same rule,
    to be confirmed in area 5.

### Decisions

Decided by Richard on 2026-10-12. Nothing here is built yet; the rework is listed in the roadmap.

| Topic | Decision | Date |
|---|---|---|
| Runtime construction failure | A runtime `Type(value)` check needs `attempt ... invalid: SomeFailure` naming an `invalid_value` failure; literal construction is checked while compiling and never fails at runtime. The mapping's form was revised on 2026-10-14 (below) | 2026-10-12 |
| Object construction | Building an object or creating an entity accepts the declared type or anything that widens to it, and runs each field's own rules at that point. Since 2026-10-14 an object literal never fails: a field whose value might break its rules is built first (built in chunk 2.5) | 2026-10-12 |
| Construction failures by rule | A runtime construction maps its failures in a block, the form writes and integration calls use: `attempt CakeCount(n) { max: OrderTooLarge }`. Keys are the rules of the type and the types it builds on that the declared ranges can't prove hold; each needs an arm, or `invalid:` covers every rule without its own; an arm for a rule that can't break, or an `invalid:` covering nothing, is an error; every failure is `invalid_value`; a rule the compiler can't reason about (`pattern`) can always break. An `if` condition counts as proof, so after `if (n > 12) { reject OrderTooLarge }` `CakeCount(n)` is plain. Object literals never fail: a field that might break its rules is built first, on its own line. Replaces the trailing `invalid:` clause (Richard). `invalid:` covers only a construction's rules, never a write's or an integration call's conditions (Richard). Built in chunk 2.5, and conditions as proof in chunk 2.6, as "Conditions as proof" describes | 2026-10-14 |
| `optional` and `default` | Only on `Input` fields; reading an optional field gives `T?`; only the update `patch:` form keeps omitted and `none` apart | 2026-10-12 |
| Collection operations | List literals, `for item in items`, `collection.count` and `collection.contains` first; `map` and `filter` follow area 3's answer on passing functions; collections stay invariant | 2026-10-12 |
| `Set` and `Map` encoding | A `Set` is a JSON array rejected if it has duplicates; a `Map` is a JSON object with text- or UUID-based keys only | 2026-10-12 |
| Transparent aliases | None; every `type` is a new type; `type X = Uuid` without a block is rejected | 2026-10-12 |
| Representation access | No cast and no unwrap; constructing is the only conversion | 2026-10-12 |
| Number types | No sized types; ranges set storage; `Int` is exact 64-bit with overflow an error; `Decimal` is exact with a required `scale`; JSON carries large `Int` and every `Decimal` as strings; no `Float` until needed | 2026-10-12 |
| `pattern` and `format` | A pattern matches the whole text, uses a linear-time subset with no backreferences or lookaround, is checked while compiling and applies to literals; `format` accepts only `email`; constraints must fit their base and contradictions are errors | 2026-10-12 |
| Type spellings | Every type is `type Name = Base { ... }`, including `Object`, `Enum`, `Input` and `Output`; `value`, `enum`, `input` and `output` keywords are removed; only `Input` may have optional or default fields; routes accept `Input` and return `Output` | 2026-10-12 |
| Signature rule scope | Applies to representation types and `Instant`, `CalendarDate`, `Time`, `Duration`; `Zone`, `Locale` and `Weekday` may be used directly; list element types are checked | 2026-10-12 |
| Equality | `==` compares contents field by field on objects, lists and enum payloads; sets and maps compare as sets | 2026-10-12 |
| UUID versions | Any version accepted by default, normalised to lowercase; new identities are version 7; a named type can pin one, `type PartnerRef = Uuid { version: v4 }`, governing what is accepted and generated (Written `UuidVersion.v4` under the revised question 17.) | 2026-10-12 |
| Derived types | `type X = Base from Source { pick/omit/optional, then new fields }`; named results only; `Output` may only `pick`; generated fields never in an `Input` (Superseded 2026-10-13: see "Building types".) | 2026-10-12 |
| Building types | Body lines read top to bottom: `include: [Type]` (all fields; never on an `Input`), `pick: [Type.field]`, `omit: [Type.field]` (only what is present), written fields replace or add; copied fields never silently replace copied fields; lists always bracketed; `optional` on `Input` only (Richard) | 2026-10-13 |
| Literal typing | A literal takes the expected type where it is known and is checked while compiling; computed values need an explicit constructor | 2026-10-12 |
| Variable types | Every `var` states its type; loop and `match` bindings take theirs from what they bind | 2026-10-12 |
| Setting options | A setting with a fixed set of options takes an enum value written `Type.value` (`version: UuidVersion.v7`, `kind: FailureKind.not_found`, `on_delete: OnDelete.cascade`); a bare word in a setting is always a declared name; option values read as values or moments, not verbs; quantities stay numbers (revised 2026-10-12, Richard) | 2026-10-12 |
| Object keyword | `value` is dropped; objects are `type Name = Object { fields }` | 2026-10-12 |
| `Object` | Only a base in a `type` declaration; no anonymous inline objects | 2026-10-12 |
| Field settings | A field's settings go in a block after its type, one per line (`optional: true`, `default: 25`) | 2026-10-12 |
| Lists | Square brackets are the only way to write a list: `pick: [Todo.id, Todo.title]`, `[create, read]`; braces never hold comma lists; signatures keep their commas (parameters, `fails A, B`) | 2026-10-12 |

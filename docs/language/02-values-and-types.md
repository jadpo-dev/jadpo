---
title: Values and types
area: 2
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - type.constructor_argument_count
  - type.constructor_input
  - type.duplicate_variant_field
  - type.field_on_non_record
  - type.invalid_literal
  - type.mismatch
  - type.missing_field
  - type.missing_variant_field
  - type.not_record
  - type.nullable_selection
  - type.primitive_signature
  - type.redundant_nullability
  - type.sibling_mismatch
  - type.unknown_enum_variant
  - type.unknown_field
  - type.unknown_variant_field
  - syntax.constraint_colon_required
  - syntax.enum_empty
  - syntax.expected_constraint
  - syntax.expected_constraint_value
  - syntax.expected_field_initialiser
  - syntax.expected_literal
  - syntax.type_parent_required
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

```jadpo project=shop
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

```jadpo project=refine
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

Today the compiler accepts constraints in some places where they have no
effect: `min` on text, `min_length` on a number, a `pattern` other than
`[a-z0-9_]+`, and any `format` other than `email` are all accepted but not
checked at runtime. The Review lists these.

## Creating values

Applying a type to a value, `Type(value)`, creates a value of that type by
checking every rule on it and on the types it builds on. It is validation,
never a cast.

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

When the value is a literal, the compiler checks it while compiling:
`Quantity(0)` is a compile error (see `type.invalid_literal` below). When the
value is computed, the check runs when the program runs.

A constructor accepts a value with the same representation as the type: `Int`
or any `Int`-based type for `Quantity`, a decimal literal such as `0.0` (not
`0`) for a `Decimal`-based type. This is also how you deliberately convert
between two named types: the value is re-checked against the target's rules.

```jadpo project=convert
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

How a failed runtime check is reported is not designed yet (Review, question 1).

## Signatures use named types

Functions, actions and queries cannot take or return a built-in representation
type directly. `Text`, `Int`, `Bool`, `Uuid`, the time types and their nullable
forms all have to be wrapped in a named type that says what they mean.

```jadpo project=signature
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

```jadpo fragment
type Money = Object {               // used inside the application
    amount: Pence
}

type CreateProduct = Input {        // arrives from outside, such as a request body
    name: ProductName
    price: Pence
}

type ProductView = Output {         // sent back, such as a response
    name: ProductName
    price: Money
}
```

Only an `Input` may leave fields out or give them defaults. A route accepts
only an `Input` and returns only an `Output`. There is no unnamed object: a
nested shape is declared once, with a name, and used by that name.

An object value is written with its type name and every field:

```jadpo fragment
function view(name: ProductName, price: Pence) -> ProductView {
    return ProductView {
        name: name
        price: price
    }
}
```

Every declared field must be given, and unknown fields are errors. Fields can
hold other objects and lists, nested as deeply as needed.

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

Field types are how identifiers stay apart: `Customer.id` and `Supplier.id`
are both UUIDs, and passing one where the other is expected is
`type.sibling_mismatch`. To move a value into a field type on purpose, use the
field as a constructor, as in `Customer.email(email)`.

A field can add rules of its own in a block after its type:

```jadpo fragment
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

> **Decided on 2026-10-12.** Objects and inputs use `type Name = Object { ... }` and `type Name = Input { ... }`; today's spelling is collapsed below. Question 20 proposes moving `optional` and `default` into a settings block.

There is no `null` or `undefined`. A type followed by `?` may hold `none`, and
reading it means handling both cases, usually with `match` (area 3).

```jadpo fragment
type Listing = Object {
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

```jadpo fragment
type ListProducts = Input {
    name: ProductName optional
    page_size: PageSize default 25
}

type RenameProduct = Input {
    name: ProductName
    subtitle: ProductName? default none
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

- `optional` means the request may leave the field out. Leaving it out is
  different from sending `none`, which lets an update tell "don't change this"
  from "clear this" (area 6).
- `default 25` fills in a value when the request leaves the field out. A
  default is checked against the field's rules while compiling.
- `default none` fills in `none` for a nullable field.

A value can widen through `?`: a `Customer.email?` can be used where an
`Email?` is expected.

## Enums

> **Decided on 2026-10-12.** Enums are `type Name = Enum { ... }`; today's `enum Name { ... }` is collapsed below.

An enum is a closed list of named alternatives. Variants are always written
with their enum's name.

```jadpo fragment
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

```jadpo fragment
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

```jadpo fragment
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

```jadpo fragment
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

> The examples in this section use today's syntax, because they show the compiler's real output; they are rewritten as the rework lands.

Every error this area can produce, with an example that triggers it and the
compiler's actual output.

### `type.mismatch`

```jadpo project=e-mismatch expect=type.mismatch
type Pence = Int {
    min: 0
}

function add(a: Pence, b: Pence) -> Pence {
    return a + b
}
```

### `type.sibling_mismatch`

```jadpo project=e-sibling expect=type.sibling_mismatch
entity Customer {
    id: Uuid
    identity: id
}

entity Order {
    id: Uuid
    identity: id
}

function customer_reference(id: Customer.id) -> Customer.id {
    return id
}

function misuse(order: Order) -> Customer.id {
    return customer_reference(order.id)
}
```

### `type.primitive_signature`

```jadpo project=e-primitive expect=type.primitive_signature
function normalise(value: Text) -> Text {
    return value
}
```

### `type.invalid_literal`

```jadpo project=e-literal expect=type.invalid_literal
type Quantity = Int {
    min: 1
    max: 1000
}

function starting_quantity() -> Quantity {
    return Quantity(0)
}
```

### `type.constructor_input`

```jadpo project=e-ctor-input expect=type.constructor_input
type OrderId = Uuid {}

type ProductName = Text {
    max_length: 120
}

function order_for(name: ProductName) -> OrderId {
    return OrderId(name)
}
```

### `type.constructor_argument_count`

```jadpo project=e-ctor-count expect=type.constructor_argument_count
type Quantity = Int {
    min: 1
}

function pair() -> Quantity {
    return Quantity(1, 2)
}
```

### `type.missing_field`

```jadpo project=e-missing-field expect=type.missing_field
output Receipt {
    email: Email
}

function incomplete_receipt() -> Receipt {
    return Receipt {}
}
```

### `type.unknown_field`

```jadpo project=e-unknown-field expect=type.unknown_field
type ProductName = Text {
    max_length: 120
}

value Product {
    name: ProductName
}

function product(name: ProductName) -> Product {
    return Product {
        name: name
        title: name
    }
}
```

### `type.not_record`

```jadpo project=e-not-record expect=type.not_record
type ProductName = Text {
    max_length: 120
}

function blank() -> ProductName {
    return ProductName {}
}
```

### `type.field_on_non_record`

```jadpo project=e-field-scalar expect=type.field_on_non_record
type ProductName = Text {
    max_length: 120
}

function raw(name: ProductName) -> ProductName {
    return name.value
}
```

### `type.nullable_selection`

```jadpo project=e-nullable expect=type.nullable_selection
type ProductName = Text {
    max_length: 120
}

value Product {
    name: ProductName
}

function product_name(product: Product?) -> ProductName {
    return product.name
}
```

### `type.redundant_nullability`

```jadpo project=e-redundant expect=type.redundant_nullability
type ProductName = Text {
    max_length: 120
}

value Listing {
    subtitle: ProductName?
}

value ListingPreview {
    subtitle: Listing.subtitle?
}
```

### `type.unknown_enum_variant`

```jadpo project=e-variant expect=type.unknown_enum_variant
enum TicketStatus {
    open
    closed
}

function reopen() -> TicketStatus {
    return TicketStatus.opne
}
```

### `type.missing_variant_field`

```jadpo project=e-variant-missing expect=type.missing_variant_field
type ReceiptId = Uuid {}

enum PaymentOutcome {
    pending
    paid {
        receipt_id: ReceiptId
    }
}

function paid() -> PaymentOutcome {
    return PaymentOutcome.paid {}
}
```

### `type.unknown_variant_field`

```jadpo project=e-variant-unknown expect=type.unknown_variant_field
type DeclineReason = Text {
    max_length: 200
}

enum PaymentOutcome {
    declined {
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

### `type.duplicate_variant_field`

```jadpo project=e-variant-duplicate expect=type.duplicate_variant_field
type DeclineReason = Text {
    max_length: 200
}

enum PaymentOutcome {
    declined {
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

### `syntax.type_parent_required`

```jadpo project=e-parent expect=syntax.type_parent_required
type Dimensions = {
    width: Int
}
```

### `syntax.expected_constraint`

```jadpo project=e-constraint expect=syntax.expected_constraint
type ProductName = Text {
    min_length: 1
    maxlength: 120
}
```

### `syntax.constraint_colon_required`

```jadpo project=e-constraint-colon expect=syntax.constraint_colon_required
type ContactEmail = Text {
    format email
}
```

### `syntax.expected_constraint_value`

```jadpo project=e-constraint-value expect=syntax.expected_constraint_value
type Price = Decimal {
    min: zero
}
```

### `syntax.expected_literal`

```jadpo project=e-literal-value expect=syntax.expected_literal
type PageSize = Int {
    min: 1
}

input ListProducts {
    page_size: PageSize default
}
```

### `syntax.expected_field_initialiser`

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

```jadpo fragment
type Price = Decimal {                 // rules on a number
    scale: 2
    min: 0.00
}

type Sku = Text {                      // rules on text
    pattern: "[A-Z]{3}-[0-9]{4}"
}

type OrderId = Uuid {}                 // a new identity type with no extra rules

type Money = Object {                  // fields
    amount: Price
    currency: Currency
}

type OrderStatus = Enum {              // variants
    pending
    paid
    shipped
}

type PaymentOutcome = Enum {           // variants that carry fields
    pending
    paid: {
        receipt_id: ReceiptId
    }
    declined: {
        reason: DeclineReason
    }
}

type CreateOrder = Input {             // an object arriving from outside
    lines: List<OrderLineRequest>
    note: OrderNote {
        optional: true
    }
}

type OrderView = Output {              // an object sent back
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

### Decided: derived types

Most object types in a real application are slices of an entity or of another
object: a create input without the generated fields, a patch input where every
field is optional, a public view without private fields, a list summary with
fewer fields than the detail view, a v2 response that adds a field. Today each
is written out field by field (`title: Todo.title`). The proposal adds
`from <source>` after the base, three operations, and new fields listed after
them.

**`pick`** keeps the listed fields, with their types and validation rules:

```jadpo fragment
type TodoView = Output from Todo {
    pick: [id, title, status, due_at]
}
```

**`omit`** keeps every field except the listed ones (never on an `Output`):

```jadpo fragment
type ImportTodo = Input from Todo {
    omit: [id, owner_id, created_at, deleted_at]
}
```

**`optional`** makes the listed fields, or `all` of them, optional (only on an
`Input`, because only incoming data can leave fields out):

```jadpo fragment
type PatchTodo = Input from Todo {
    pick: [title, due_at]
    optional: all
}
```

**New fields** are listed after the operations, written exactly like fields
anywhere else. A new field's type can be any type, including another
entity's field type. With `Object` as the base, the result is a plain object
used inside the application:

```jadpo fragment
type TodoReminder = Object from Todo {
    pick: [id, title, due_at]
    owner_email: User.email
    days_overdue: DayCount
}
```

**Objects inside objects, and lists.** Picking a field keeps its whole type,
so `pick: [lines]` on an order keeps `List<OrderLine>`. To change what a nested
field holds, leave it out of `pick` and list a field of the same name with the
new type. This is the only way to change a field's type, and it is visible in
the declaration:

```jadpo fragment
type OrderLineView = Output from OrderLine {
    pick: [product_name, quantity, line_total]
}

type OrderView = Output from Order {
    pick: [id, placed_at, total]
    lines: List<OrderLineView>
}
```

**Deriving from a derived type** works the same way, so versions and
summaries build on each other:

```jadpo fragment
type TodoSummary = Output from TodoView {
    pick: [id, title]
}

type TodoViewV2 = Output from TodoView {
    pick: [id, title, status, due_at]
    tags: List<TagName>
}
```

Rules:

- Three operations only: `pick`, `omit` and `optional`, followed by any new
  fields. `pick` and `omit` cannot both appear. No renaming. A new field
  cannot reuse the name of a field that was picked, but can reuse one that was
  left out, which is how a field's type is changed. `pick`, `omit` and
  `optional` are reserved and cannot be used as field names.
- Picked fields follow their source: renaming `Todo.title` renames it in
  every type that picked it, and its validation rules come with it.
- Every derived type has a name. There are no inline `Pick<...>` types in
  signatures, so OpenAPI schemas and error messages always have a name, and
  the editor and generated docs show the expanded fields.
- An `Output` may only use `pick`. With `omit`, adding a sensitive field to
  the entity later (a password hash, an internal note) would silently expose
  it in every response; an allowlist cannot leak by addition. Generated
  fields such as `id` and `created_at` can never appear in an `Input`, which
  closes the mirror-image mass-assignment bug.
- Policy checks still apply to every derived output.

### Decided changes in examples

Every decision on this page, shown as code with all the others applied.
They describe the language after the rework, so they are shown without being
compiled; exact syntax for logic is settled in area 3.

#### Q1: a runtime check that can fail

```jadpo fragment
failure QuantityOutOfRange {
    kind: invalid_value
    code: "quantity_out_of_range"
}

action double_quantity(quantity: Quantity) fails QuantityOutOfRange -> Quantity {
    var doubled: Quantity = attempt Quantity(quantity * 2) invalid: QuantityOutOfRange
    return doubled
}
```

A literal needs no `attempt`: `var quantity: Quantity = 0` is rejected while
compiling.

#### Q2: building an object runs its fields' rules

```jadpo fragment
type BadgeName = Text {
    max_length: 12
}

type Badge = Object {
    name: BadgeName
}

action make_badge(user: User) fails BadgeNameTooLong -> Badge {
    // user.display_name allows 80 characters; the field allows 12,
    // so this check can fail and must say how.
    var badge: Badge = attempt Badge {
        name: user.display_name
    } invalid: BadgeNameTooLong
    return badge
}
```

#### Q3: `optional` only on inputs, read as nullable

```jadpo fragment
type UpdateProfile = Input {
    nickname: Nickname {
        optional: true
    }
}

action update_profile(request: UpdateProfile) -> Unit {
    var nickname: Nickname? = request.nickname    // left out reads as none
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
boundary is a compile error, because JSON object keys are text.

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

var share: Price = decimal.divide(total, people, rounding: half_even)
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

```jadpo fragment
entity Order {
    id: Uuid                    // generated as version 7
}

type PartnerRef = Uuid {
    version: v4                 // accepts and generates only version 4
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

#### Q17: settings take a name from their list

```jadpo fragment
type ContactEmail = Text {
    format: email
}

type PartnerRef = Uuid {
    version: v4
}

failure TodoNotFound {
    kind: not_found             // was: kind: NotFound
    code: "todo_not_found"
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
| Failure `kind` values become lower_snake_case (`kind: not_found`), following the setting-options decision | 4 | Follows from a decision |
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
   (an exception), and closes TYPE-001.
2. **Should building an object check fields against their declared type or
   their field type?** Object construction uses the declared type and skips
   field rules; `create` demands the exact field type, which is why the golden
   app writes `Todo.status(TodoStatus.open)`. *Decided 2026-10-12:* one rule for
   both: accept the declared type or anything that widens to it, and run the
   field's own rules at that point (fallible under question 1 only when the
   field adds rules). Identifier safety is unaffected, because a reference
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
    `type PartnerRef = Uuid { version: v4 }`, which governs both what is
    accepted and what is generated (for example where a version 7 ID would
    reveal sensitive creation times, or another system dictates the version).

14. **Adopt derived types?** See "Derived types" above. *Decided 2026-10-12 (agreed in discussion, with new fields listed directly and lists in brackets):*
    yes, with exactly `pick`, `omit` and `optional` plus listed new fields,
    named results only, and outputs restricted to `pick`. This covers create,
    patch, view, summary and versioned shapes without TypeScript's anonymous
    type algebra, whose expanded types reviewers and error messages cannot
    name.

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

17. **Should every setting with a fixed set of options take a name from that
    set?** *Decided 2026-10-12:* yes, everywhere: `version: v7`, not
    `version: 7`; genuine quantities such as `min`, `max` and `scale` stay
    numbers. In settings the bare name is written because the setting fixes
    the list; in ordinary code enum values stay qualified
    (`OrderStatus.paid`). Every option name is lower_snake_case, which changes
    failures' `kind: InvalidValue` to `kind: invalid_value` (area 4).

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
| Runtime construction failure | A runtime `Type(value)` check needs `attempt ... invalid: SomeFailure` naming an `invalid_value` failure; literal construction is checked while compiling and never fails at runtime | 2026-10-12 |
| Object construction | Building an object or creating an entity accepts the declared type or anything that widens to it, and runs each field's own rules at that point | 2026-10-12 |
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
| UUID versions | Any version accepted by default, normalised to lowercase; new identities are version 7; a named type can pin one, `type PartnerRef = Uuid { version: v4 }`, governing what is accepted and generated | 2026-10-12 |
| Derived types | `type X = Base from Source { pick/omit/optional, then new fields }`; named results only; `Output` may only `pick`; generated fields never in an `Input` | 2026-10-12 |
| Literal typing | A literal takes the expected type where it is known and is checked while compiling; computed values need an explicit constructor | 2026-10-12 |
| Variable types | Every `var` states its type; loop and `match` bindings take theirs from what they bind | 2026-10-12 |
| Setting options | A setting with a fixed set of options takes a lower_snake_case name from that set (`version: v7`, `format: email`, `kind: not_found`); genuine quantities stay numbers | 2026-10-12 |
| Object keyword | `value` is dropped; objects are `type Name = Object { fields }` | 2026-10-12 |
| `Object` | Only a base in a `type` declaration; no anonymous inline objects | 2026-10-12 |
| Field settings | A field's settings go in a block after its type, one per line (`optional: true`, `default: 25`) | 2026-10-12 |
| Lists | Square brackets are the only way to write a list: `pick: [id, title]`, `[create, read]`; braces never hold comma lists; signatures keep their commas (parameters, `fails A, B`) | 2026-10-12 |

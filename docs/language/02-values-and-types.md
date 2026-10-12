---
title: Values and types
area: 2
status: in-review
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
| Containers | `List<T>`, `Set<T>`, `Map<K, V>`, `Object { ... }` | Described below. |
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

`Email`, `Url`, `IpAddress`, records, enums, field types and `Unit` are
allowed. Inside a body, locals can still hold built-in types.

## Records

A record is a fixed set of named fields. Four declaration forms create one:

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

`value` is for data used inside the application, `input` for request bodies
and other data arriving from outside, and `output` for responses. Today the
compiler treats all four alike except in a few query and update positions
(area 6); any of them can be used at either boundary.

A record value is written with its type name and every field:

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

Every declared field must be given, and unknown fields are errors. Fields can
hold other records, lists, or an anonymous `Object { ... }`, nested as deeply
as needed.

### Field types

Every field of a record, entity or enum variant is a type of its own, written
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

Reading through a nested record gives the inner record's field type:
`customer.billing_address.postal_code` has type `Address.postal_code`, whether
it came from the billing or the shipping address.

## Absent values

There is no `null` or `undefined`. A type followed by `?` may hold `none`, and
reading it means handling both cases, usually with `match` (area 3).

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

A nullable field is still required: a `Listing` must say `subtitle: none`
explicitly. Being allowed to leave a field out is a separate idea that belongs
to inputs:

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

- `optional` means the request may leave the field out. Leaving it out is
  different from sending `none`, which lets an update tell "don't change this"
  from "clear this" (area 6).
- `default 25` fills in a value when the request leaves the field out. A
  default is checked against the field's rules while compiling.
- `default none` fills in `none` for a nullable field.

A value can widen through `?`: a `Customer.email?` can be used where an
`Email?` is expected.

## Enums

An enum is a closed list of named alternatives. Variants are always written
with their enum's name.

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

A variant can carry its own fields, which makes an enum a tagged union: each
value is exactly one variant and has only that variant's fields.

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

The only way to read a variant's fields is to `match` on it. Payload fields
have field types too, written `PaymentOutcome.declined.reason`.

An enum cannot be created from text: `TicketStatus("open")` is an error. Text
from outside becomes a variant only by being decoded at a boundary. On the
wire a plain variant is its name as a string (`"open"`), and a variant with
fields is an object with a `tag` naming the variant plus its fields. Enums have
no methods or display labels.

## Collections

`List<T>`, `Set<T>` and `Map<K, V>` can be used as field types, parameter
types and boundary types, nested in each other or holding records.

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
- **Same shape is not same type.** Two records with identical fields are still
  different types.
- **Nothing unwraps.** A named type never becomes its built-in representation
  except through arithmetic and comparison operators.

## Values never change

Every value is immutable. Passing a value to a function or returning it never
lets anyone else change it. A local declared with `var mut` can be pointed at a
new value of the same type, but fields cannot be assigned: to change a record,
build a new one.

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

## Errors

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
| Records | Works in general. Two bugs: record construction checks each field against its declared type rather than its field type, so `Badge { label: name }` skips the field's own `max_length: 12` (no runtime check is generated), while entity `create` demands the exact field type; and a field given twice in a record (`Product { name: a name: b }`) is accepted, although the same mistake in an enum variant is an error. |
| `optional` and `default` | Partial and unsound outside inputs. `optional` is accepted on `value` and `output` fields; such a field can be left out at construction and then read as a non-nullable `T`, which is `undefined` at runtime. On an input, an `optional` field reads as `T` and cannot be matched with `some`/`none`. `default` on a `value` is accepted but construction still requires the field. `name: Name default none` on a non-nullable field reports "`none` can't be used where `Name?` is required", which contradicts itself. |
| Enums and tagged unions | Works in general for typing, matching and boundary decoding. Storage and migration of payload variants is designed in area 5. `type X = Enum { ... }` is a second accepted spelling. |
| Collections | Types only. `List<T>` works at boundaries and in records. `Set<T>` and `Map<K, V>` compile at HTTP boundaries, but the generated decoder requires a JavaScript `Set`/`Map`, which JSON never produces, so every request containing one is rejected with 422 (read from generated code, not run). No literals, iteration or `collection.*` functions exist; the `collection` namespace is reserved but empty. The formatter writes `List<Object { ... } >` with a stray space. |
| Compatibility rules | Works in general: widening, sibling rejection, invariance and nullable lifting all behave as documented. |
| Equality | Bug. `==` on records and lists compiles to JavaScript `===`, which compares object identity, so two equal records built separately are unequal. |
| Value semantics | Works in general: no field assignment, `var mut` rebinding only. |

### Consistency with the rest of the language

- **Two spellings for records and enums.** `value X { }` and
  `type X = Object { }` produce the same thing, as do `enum X { }` and
  `type X = Enum { }`. The golden app uses only the keyword forms.
- **`input` and `output` are keywords without a meaning.** The decision register
  says boundary roles "are not type categories", yet `input` and `output`
  declare records, and nothing stops an `output` being a route's input.
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
  `Customer.email(x)` for field types, `Product { ... }` for records,
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

### Proposal: derived records

Most record types in a real application are slices of an entity or of another
record: a create input without the generated fields, a patch input where every
field is optional, a public view without private fields, a list summary with
fewer fields than the detail view, a v2 response that adds a field. Today each
is written out field by field (`title: Todo.title`). The proposal adds `from`
to the three record kinds (`value` for data used inside the application,
`input` for data arriving from outside, `output` for responses) with four
operations and nothing else.

**`pick`** keeps the listed fields, with their types and validation rules:

```jadpo fragment
output TodoView from Todo {
    pick: id, title, status, due_at
}
```

**`omit`** keeps every field except the listed ones (inputs and values only):

```jadpo fragment
input ImportTodo from Todo {
    omit: id, owner_id, created_at, deleted_at
}
```

**`optional`** makes the listed fields, or `all` of them, optional (inputs
only, because only incoming data can leave fields out):

```jadpo fragment
input PatchTodo from Todo {
    pick: title, due_at
    optional: all
}
```

**`add`** declares new fields, written exactly like fields anywhere else. A
new field's type can be any type, including another entity's field type:

```jadpo fragment
value TodoReminder from Todo {
    pick: id, title, due_at
    add {
        owner_email: User.email
        days_overdue: DayCount
    }
}
```

`value TodoReminder` is the plain "just a new type" case: the same rules
apply whichever of `value`, `input` or `output` you derive.

**Records inside records and lists.** Picking a field keeps its whole type, so
`pick: lines` on an order keeps `List<OrderLine>`. To change what a nested
field holds, leave it out and add a field of the same name with the new type.
This is the only way to change a field's type, and it is visible in the
declaration:

```jadpo fragment
output OrderLineView from OrderLine {
    pick: product_name, quantity, line_total
}

output OrderView from Order {
    pick: id, placed_at, total
    add {
        lines: List<OrderLineView>
    }
}
```

**Deriving from a derived record** works the same way, so versions and
summaries build on each other:

```jadpo fragment
output TodoSummary from TodoView {
    pick: id, title
}

output TodoViewV2 from TodoView {
    pick: id, title, status, due_at
    add {
        tags: List<TagName>
    }
}
```

Rules:

- Four operations only: `pick`, `omit`, `optional` and `add`. `pick` and
  `omit` cannot both appear. No renaming. `add` cannot reuse the name of a
  field that was picked, but can reuse one that was left out, which is how a
  field's type is changed.
- Picked fields follow their source: renaming `Todo.title` renames it in
  every record that picked it, and its validation rules come with it.
- Every derived record has a name. There are no inline `Pick<...>` types in
  signatures, so OpenAPI schemas and error messages always have a name, and
  the editor and generated docs show the expanded fields.
- An `output` may only use `pick`. With `omit`, adding a sensitive field to
  the entity later (a password hash, an internal note) would silently expose
  it in every response; an allowlist cannot leak by addition. Generated
  fields such as `id` and `created_at` can never appear in an `input`, which
  closes the mirror-image mass-assignment bug.
- Policy checks still apply to every derived output.

### Open questions

1. **How does a runtime `Type(value)` failure reach the caller?** Today it
   escapes as an internal fault. The language already maps expected failures
   with clauses on fallible operations (`missing: TodoNotFound`,
   `conflict: TodoMutationConflict`). *Recommendation:* make runtime
   construction a fallible operation that needs `attempt` and an
   `invalid: SomeFailure` clause naming an `InvalidValue`-kind failure, as in
   `attempt Quantity(count) invalid: QuantityOutOfRange`. Literal construction
   stays infallible because it is checked while compiling. This mirrors Rust's
   `TryFrom` (failure is in the signature) rather than Kotlin's `require`
   (an exception), and closes TYPE-001.
2. **Should building a record check fields against their declared type or
   their field type?** Record construction uses the declared type and skips
   field rules; `create` demands the exact field type, which is why the golden
   app writes `Todo.status(TodoStatus.open)`. *Recommendation:* one rule for
   both: accept the declared type or anything that widens to it, and run the
   field's own rules at that point (fallible under question 1 only when the
   field adds rules). Identifier safety is unaffected, because a reference
   field's declared type is itself a field type (`customer_id: Customer.id`
   still rejects `supplier.id`), and the noise disappears from the golden app.
3. **Where may `optional` be used, and what type does reading it give?**
   *Recommendation:* only on `input` fields, and reading one gives `T?`
   (absent reads as `none`). Only the update `patch:` form keeps the
   omitted-versus-`none` distinction. `default` is likewise input-only. This
   removes the `undefined` leak; TypeScript's `field?: T` reading as
   `T | undefined` is the same idea.
4. **Which collection operations come first?** Collections are declared but
   cannot be built, counted or iterated. *Recommendation:* build list
   literals (`[a, b]`), `for item in items` (already the recommended loop form
   in the decision register) and a small namespaced set:
   `collection.count`, `collection.contains`, `collection.map`,
   `collection.filter`. Keep collections invariant (Kotlin and Rust do;
   TypeScript's covariant arrays are unsound). Closes TYPE-002 for lists.
5. **How are `Set` and `Map` written in JSON?** They compile at boundaries but
   are always rejected. *Recommendation:* a `Set<T>` is a JSON array that is
   rejected if it has duplicates; a `Map<K, V>` is a JSON object and is allowed
   only when `K` is text-based or `Uuid`-based. Other key types are a compile
   error at a boundary.
6. **Transparent aliases (TYPE-003)?** *Recommendation:* no, and close the
   issue. Every `type` stays a new type. Also reject the block-less
   `type OrderId = Uuid`: in TypeScript, Rust and Kotlin that shape is a
   transparent alias, so an agent or reviewer will misread it. Require
   `type OrderId = Uuid {}`.
7. **Is explicit construction the only way to reach a value's
   representation (TYPE-004)?** Today any named type converts to any other
   with the same representation through its constructor, re-checking rules,
   and operators work on the representation. *Recommendation:* yes, and close
   the issue: no `as` cast and no unwrap escape hatch. The golden app has not
   needed one.
8. **Which number types, and what are they at runtime?** `Int` silently
   loses precision past 2^53 and `Decimal` is a JavaScript float, which is
   wrong for money (JavaScript has one number type, a 64-bit float, plus
   `BigInt` since 2020, and no decimal). *Recommendation:* no sized types
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
9. **What do `pattern` and `format` mean exactly?** *Recommendation:* a
   pattern must match the whole text, uses a regular-expression subset with
   no backreferences or lookaround (RE2-style, so it runs in linear time and
   behaves the same on every target), is compiled and checked at compile time,
   and is applied to literals. `format` accepts only `email`; `Url` and
   `IpAddress` are types, not formats. Every constraint must fit its base type,
   and contradictory or repeated bounds are errors.
10. **One spelling per kind of record?** *Recommendation:* remove
    `type X = Object { }` and `type X = Enum { }`, so `type ... =` only ever
    refines a scalar; keep `value`, `input`, `output` and `enum`. Give `input`
    and `output` a meaning: only `input` fields may use `optional` and
    `default`, a route's `input:` must name an `input`, and its `output:` an
    `output`. This amends the decision-register line that boundary roles are
    not type categories.
11. **Which built-in types does the signature rule cover?** *Recommendation:*
    keep it for representation types and `Instant`, `CalendarDate`, `Time` and
    `Duration` (a bare `Instant` parameter has the same ambiguity as a bare
    `Int`), allow the closed prelude enums `Zone`, `Locale` and `Weekday`, and
    close the gaps: check collection element types and reject inline `Object`
    parameters.
12. **What does `==` mean on records, lists and payload variants?**
    *Recommendation:* structural equality, field by field, with sets and maps
    compared as sets, as in Rust's derived `PartialEq`, Kotlin data classes and
    Elixir. The alternative of rejecting `==` on records would force agents to
    write field-by-field comparisons by hand.

13. **Which UUIDs?** Today only canonical hyphenated text with versions 1 to
    5 is accepted, and new identities are random version 4. Version 7
    (RFC 9562, 2024) is time-ordered, which keeps database indexes compact and
    sorts IDs by creation, and an ID arriving from a system that already uses
    it is currently rejected. *Recommendation:* by default accept any RFC 9562
    version, normalise to lowercase, generate version 7 for new identities and
    store PostgreSQL `uuid`. A named type can pin a version with a constraint,
    `type ExternalRef = Uuid { version: 4 }`, which then governs both what is
    accepted and what is generated; use it where a version 7 ID would reveal
    sensitive creation times, or where another system dictates the version.
14. **Adopt derived records?** See the proposal above. *Recommendation:* yes,
    with exactly `pick`, `omit`, `optional` and `add`, named results only, and
    outputs restricted to `pick`. This covers create, patch, view, summary and
    versioned shapes without TypeScript's anonymous type algebra, whose
    expanded types reviewers and error messages cannot name.
15. **Should a literal take on the type it is going into?** Today
    `var price: Price = 0.0`, `return 0.0` and `Item { price: 0.0 }` are all
    errors; you must write `Price(0.0)`. *Recommendation:* yes, for literals
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
    where `total` is returned as `Pence`. *Recommendation:* yes: every `var`
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

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Runtime construction failure | | |
| Record construction narrowing | | |
| `optional` and `default` | | |
| Collection operations | | |
| `Set` and `Map` encoding | | |
| Transparent aliases | | |
| Representation access | | |
| Number types | | |
| `pattern` and `format` | | |
| Record spellings | | |
| Signature rule scope | | |
| Equality | | |
| UUID versions | | |
| Derived records | | |
| Literal typing | | |
| Variable types | | |

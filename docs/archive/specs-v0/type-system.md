# Type system

**Status:** semantic specification v0.1  
**Purpose:** define when values are compatible, how trusted values are created,
and which facts a type does and does not prove

This document closes the loop between reusable validation and strict semantic
typing. The governing principle is:

> Representation is not meaning.

Two values are not interchangeable merely because both happen to use text,
integers, UUIDs, or the same object fields at runtime. Compatibility comes from
an explicit semantic relationship known to the compiler.

The `Type(value)` validated-constructor spelling is accepted. The precise
failure payload and handling syntax for dynamic construction remain
provisional. The compatibility and trust rules are the important part and
should survive other syntax changes.

**Accepted time boundary:** the digest-pinned
[TIME-001/TEST-001 contract](time-testing-plan.md) replaces the earlier
`Date`/zone-less-`Time`/`DateTime` sketch with `Instant`, `CalendarDate`,
resolved zone-aware `Time`, fixed `Duration`, generated `Zone`, and bounded
`Locale`. It governs compatibility, boundary decoding, persistence, operations,
and formatting for those types.

## 1. Goals

The type system should:

- prevent primitive blindness, such as confusing an email address with a
  username because both use text;
- prevent identifier confusion, such as passing a `Customer.id` where an
  `Order.id` is required;
- let declarations reuse another declaration's field contract without copying
  its validation;
- make intentional interoperability possible through shared semantic types;
- validate untrusted data exactly once at a trust boundary;
- keep existence, provenance, authorisation, and other stateful facts separate
  from value validation;
- produce local diagnostics that tell an agent how to repair a mismatch;
- retain enough semantic information for schemas, database constraints,
  documentation, tests, and migrations.

The type system is deliberately stricter than TypeScript's default structural
typing. Boilerplate is less costly than silent semantic substitution in an
agent-authored system.

## 2. Kinds of type

### 2.1 Representation primitives

Representation primitives are the small substrate used to store and compute
values:

```text
Bool
Int
Decimal
Text
Bytes
```

The exact set is provisional. Primitives describe machine representation, not
application meaning. They are valid beneath semantic type declarations, in
local inference and operators, and in compiler or standard-library intrinsics.
They are not the preferred vocabulary of application interfaces.

### 2.2 Named semantic types

A named semantic type gives a representation domain meaning and may constrain
its valid values:

```text
type Email = Text {
    format: email
    max_length: 254
}

type Username = Text {
    min_length: 3
    max_length: 30
    pattern: "[a-z0-9_]+"
}

type Quantity = Int {
    min: 1
    max: 1000
}
```

These declarations are nominal. `Email`, `Username`, and `Text` are three
different types even though their runtime representation may be identical.
`type A = B` creates a new semantic type; it is not a transparent alias.
Whether the language also needs transparent aliases is open.

Type identity belongs to the declaration, not to its current spelling or
structure. Compiler metadata should preserve stable identity through a tracked
rename rather than treating a rename as an unrelated type.

A constrained semantic type still describes an open value space. `InviteCode`
may accept any text satisfying its declared constraints; individual accepted
strings are values, not named members of the type. Code that needs one exact
semantic value constructs and names that value explicitly:

```text
var reserved_invite_code = InviteCode("reserved")

if (input.invite_code == reserved_invite_code) {
    reject InviteCodeRejected
}
```

A finite set of domain alternatives is an enumeration instead. State such as
`active`, `blocked`, `used`, or `expired` belongs to an `InviteStatus` enum (or
to separate domain operations), not to magic `InviteCode` string values. Enum
variants are closed named alternatives; constrained strings, numbers, IDs, and
other semantic values remain open except for their intrinsic constraints.

Enum variants are referenced only through qualified dot syntax, such as
`InviteStatus.blocked`. `InviteStatus("blocked")` and
`InviteStatus(raw_text)` are invalid: the validated `Type(value)` constructor
belongs to open semantic/refinement types and must not become a stringly typed
back door into a closed enum. Boundary decoding or an explicit typed parser may
map an external representation to a variant, but ordinary source never names
that variant by constructing the enum from its wire string.

#### 2.2.1 Data-carrying enum variants

**Accepted and implemented:** a closed enum may allow different variants to
carry different typed payloads. This makes the enum a nominal tagged sum rather than
a record whose state-specific fields are all nullable:

```text
enum PaymentOutcome {
    pending {
        started_at: Instant
    }

    paid {
        receipt_id: PaymentReceipt.id
        paid_at: Instant
    }

    declined {
        reason: DeclineReason
    }
}
```

A value has exactly one variant. Only the active variant's fields exist, and an
exhaustive `match` is the operation that proves a variant before its fields can
be used. Variant payload construction is checked nominally and completely; the
compiler must reject missing fields, extra fields, and values of incompatible
semantic types. Adding a variant invalidates incomplete matches, while changing
a payload invalidates affected constructors and patterns.

Payload fields have nominal `Enum.variant.field` identities and widen through
their declared field type. Boundary encodings carry a compiler-known `tag` and
validate the corresponding closed payload before producing a trusted value. Persistent
representation, database constraints, schema evolution, and migrations require
separate design; no generic JSON, nullable-column bundle, or host-language
escape is implied by the type declaration.

Plain and data-carrying enums share the same exhaustive matching rules.

### 2.3 Declared field types

Every field on a named record-like declaration has its own semantic type,
referenced as `Type.field`:

```text
Customer.id
Customer.email
Order.subtotal
CreateOrder.items
OrderCreated.order_id
```

This applies to entities, values, inputs, outputs, events, and other named
declarations whose fields participate in the type system.

A field type is a nominal refinement of the type written in its declaration:

```text
type Email = Text {
    format: email
}

entity Customer {
    email: Email
}

entity Supplier {
    email: Email
}
```

The resulting relationship is:

```text
Customer.email  <:  Email
Supplier.email  <:  Email
Email           distinct from Text
```

Here `<:` means “may widen to.” `Customer.email` and `Supplier.email` share the
base semantic type `Email`, but remain distinct sibling field types.

Field identity is semantic, not provenance. A `Customer.email` value means
“valid for the declared customer email field.” It does not mean that the value
was read from a persisted customer or that any customer with that email exists.

### 2.4 Structured types

Named inputs, outputs, values, entities, events, and other records are nominal
and complete. Two named records with identical fields are not automatically
compatible.

Every declared field is present in a concrete value. A field whose value may be
absent uses `T?`; a field that may be omitted from an input shape uses
`optional`. Omission is not a hidden value and does not become `undefined`.

#### 2.4.1 Structured fields and nested selection

A field may declare any named structured type as its parent:

```text
value Address {
    postal_code: PostalCode
}

entity Customer {
    billing_address: Address
    shipping_address: Address
}
```

The containing fields remain nominal refinements:

```text
Customer.billing_address  <:  Address
Customer.shipping_address <:  Address
```

Selection through either field resolves the selected member against the
declared structured parent. It does not manufacture a new path-dependent field
type:

```text
customer.billing_address.postal_code : Address.postal_code
```

In other words, selection widens `Customer.billing_address` to its declared
`Address` shape and then selects `Address.postal_code`. Both customer address
fields therefore share the same nested field contract intentionally. If billing
and shipping postal codes need distinct nominal meanings, the author must
declare distinct structured types such as `BillingAddress` and
`ShippingAddress`.

This rule applies equally when the containing or nested record is an `entity`,
`value`, `input`, `output`, event, or other named record kind. It does not add
structural compatibility between those records. A nullable structured field
must have `none` handled before any member is selected.

### 2.5 Collections

Collections retain their element and key type identities:

```text
List<Customer.id>
Set<Order.id>
Map<Customer.id, Customer.email>
```

They must not erase their arguments to `List<Uuid>` or `Map<Uuid, Text>`.

**Provisional:** user-visible collection types are invariant initially. Even
though a `Customer.email` can widen to `Email`, a `List<Customer.email>` does
not implicitly become `List<Email>`. An explicit `map` makes the conversion and
its cost visible. Covariance may be introduced later only if collection
immutability makes it sound and the golden applications show a clear benefit.

### 2.6 Projections

A database projection is a complete compiler-derived type, not a partial
entity. Its selected fields retain their source field types:

```text
var customers = attempt query Customer {
    select id, email
}
```

Each result contains `Customer.id` and `Customer.email`; it is not a complete
`Customer`. A projection does not gain compatibility with an unrelated record
merely because their visible fields match.

## 3. Compatibility rules

### 3.1 Exact compatibility

A value may be passed where its exact type is expected:

```text
function send_customer_receipt(to: Customer.email) -> DeliveryReceipt

send_customer_receipt(customer.email) // valid
```

### 3.2 Widening

A field value may widen to the semantic type declared beneath it:

```text
function send_email(to: Email) -> DeliveryReceipt

send_email(customer.email) // valid: Customer.email -> Email
send_email(supplier.email) // valid: Supplier.email -> Email
```

Widening forgets field-specific information. It does not unwrap a semantic type
to its primitive representation.

Widening follows declared refinement edges only. Matching constraints or
representations do not create an edge.

### 3.3 No implicit narrowing

A base semantic value cannot implicitly become a particular field type:

```text
var email: Email = input.email
send_customer_receipt(email) // compile error
```

Narrowing requires explicit validation or construction of `Customer.email`.
This remains true when the compiler can see that the field currently adds no
extra constraints; otherwise later schema changes could silently invalidate an
old assumption.

### 3.4 No sibling substitution

Sibling field types are not interchangeable:

```text
send_customer_receipt(supplier.email) // compile error
```

Code that intentionally accepts both should request their shared `Email` type.

### 3.5 No primitive substitution

A representation primitive cannot stand in for a semantic or field type:

```text
var raw: Text = request.body.email

send_email(raw)              // compile error
send_customer_receipt(raw)   // compile error
```

Likewise, a semantic type does not implicitly unwrap to its representation.
Representation access belongs to constrained low-level facilities, not ordinary
domain calls.

### 3.6 Optional values

Compatibility lifts through `?` when the contained conversion is valid:

```text
Customer.email?  -> Email? // valid widening
Supplier.email?  -> Customer.email? // invalid sibling substitution
Text?            -> Email? // invalid primitive substitution
```

`T` may be placed into `T?` as a present value. Extracting `T` from `T?`
requires exhaustive handling of `none`.

If a declared field is already nullable, its field reference includes that
nullability. Applying `?` again should be a redundant-type diagnostic rather
than creating nested absence.

### 3.7 Compatibility table

For the customer/supplier example, ordinary implicit compatibility is:

| Value type | `Customer.email` target | `Email` target | `Text` target |
|---|---:|---:|---:|
| `Customer.email` | yes | yes, widening | no implicit unwrap |
| `Supplier.email` | no | yes, widening | no implicit unwrap |
| `Email` | no implicit narrowing | yes | no implicit unwrap |
| `Text` | no | no | yes |

## 4. Creating trusted values

Strong nominal types are useful only if the language clearly defines how
values acquire them. Construction is validation, not casting.

### 4.1 Boundary decoding

At an untrusted boundary, the declared boundary type drives decoding and
validation:

```text
input RegisterCustomer {
    email: Customer.email
}
```

An incoming JSON string is not treated as `Customer.email` merely because it is
a string. The boundary decoder:

1. checks the wire representation;
2. validates the constraints of `Text`, `Email`, and `Customer.email`;
3. rejects unknown or invalid input according to the boundary contract;
4. produces a trusted `RegisterCustomer.email` value, which can widen through
   its declared chain to `Customer.email` and `Email`.

The input field has its own field identity because it is itself declared. That
identity does not obstruct use: it is a refinement of the type written on the
field and therefore widens to `Customer.email`.

The same rule applies to HTTP parameters and bodies, configuration,
environment values, queues, events from outside the trust domain, provider
responses, and deserialised caches.

### 4.2 Database reads

A database read reconstructs exact entity field types and validates any
constraints that are not guaranteed by the database representation. Legacy or
corrupt data that violates the application type is an operational fault; the
runtime must not bless it as trusted.

Reading `customer.email` therefore produces `Customer.email`. Reading
`customer.id` produces `Customer.id`, but does not by itself prove that a
different referenced ID exists or is authorised for the current actor.

### 4.3 Validated constructors

**Accepted syntax:** applying a semantic or field type to a value explicitly
constructs that type and automatically runs its validation:

```text
var support_email = Email("support@example.com")
var system_customer_email = Customer.email("system@example.com")
var initial_quantity = Quantity(1)
```

This syntax is a checked constructor, never a cast. Successful construction is
the point at which the result becomes a trusted value of the target type.

When the argument is a constant literal, the compiler runs the validation
during compilation. An invalid literal is a compile error, not a runtime branch:

```text
var email = Email("not an email") // compile error
```

When the argument is dynamic, the same constructor validates at runtime:

```text
var email = Email(raw_text)
var customer_email = Customer.email(email)
```

The first operation validates the representation and all `Email` constraints.
The second performs an explicit narrowing and checks any additional
`Customer.email` constraints. The compiler may remove duplicate runtime work
that is already proven, but it must preserve the explicit semantic transition.

A field constructor also accepts a value of the exact type declared by that
field, or a refinement of that type. For example,
`Todo.status(TodoStatus.open)` makes the transition from a closed enum variant
to the nominal `Todo.status` field type explicit; sibling field types remain
incompatible.

Dynamic construction is fallible. It introduces a compiler-visible typed
validation failure for the target type. The containing callable must declare,
map, or handle that failure under the [failure model](failure-model.md). The
exact payload and handling grammar are open and must align with `fails`,
`reject`, and `attempt`. Construction cannot silently return `none`, throw an
ambient exception, or produce a target value on failure.

The same spelling deliberately covers compile-time literal validation and
runtime validation. Whether validation happens at compile time, runtime, or is
partly elided is determined by what the compiler can prove, not by choosing a
different user-facing constructor.

### 4.5 Assignment and entity creation

Assignment follows ordinary compatibility rules. Creating or updating an entity
does not provide a hidden cast:

```text
input RegisterCustomer {
    email: Customer.email
}

action register_customer(input: RegisterCustomer) -> Customer {
    return attempt create Customer {
        email: input.email
    }
}
```

`input.email` widens from `RegisterCustomer.email` to `Customer.email`, so the
assignment is valid. A plain `Email` or `Text` would require explicit validation
as `Customer.email` first.

### 4.6 Trusted transformations

A function returning a semantic type must prove that every return path produces
that type. It cannot perform arbitrary primitive operations and reattach the
type name afterward.

Type-aware standard-library operations may declare that they preserve a
semantic type. Other transformations must produce a different semantic type or
revalidate their result. This is particularly important for money, encoded
identifiers, normalised text, and constrained numbers.

### 4.7 Representation access

Ordinary domain code should rarely need the primitive beneath a semantic type.
Serialisers, database adapters, cryptographic functions, and reviewed external
contracts may need representation access through compiler-known operations.

An unrestricted cast such as `email as Text` is rejected as the normal model.
Whether application authors need a conspicuous low-level escape hatch is open
and must be tested against the golden applications.

## 5. What a type proves

Possessing a value of type `Customer.email` proves:

- its representation was accepted;
- all intrinsic `Email` constraints hold;
- all intrinsic `Customer.email` constraints hold;
- the value entered trusted code through a compiler-recognised construction or
  validation path.

It does not prove:

- that a `Customer` containing the value exists;
- that the value came from a database;
- that it is unique;
- that it is current or verified;
- that the current actor may see or use it;
- that an entity referenced by an ID exists;
- that a cross-field or stateful business invariant holds.

These distinctions prevent a nominal type from becoming a misleading security
claim.

## 6. Constraints and inheritance

### 6.1 Intrinsic value constraints

Field references inherit constraints that can be decided from the value itself:

- representation and encoding;
- minimum and maximum values;
- minimum and maximum lengths;
- closed patterns or formats;
- closed enumeration membership;
- nullability;
- element constraints for contained values.

For valid Unicode text, the current compiler and runtime count Unicode scalar
values for `min_length` and `max_length`. A supplementary character such as `😀`
counts as one; `e` followed by a combining accent counts as two. This does not
normalize text or count displayed grapheme clusters. List length counts elements
independently of any length constraints on each element.

### 6.2 Shape rules

Required versus `optional` belongs to the containing input or object shape. It
is not inherited when another declaration references the field's value type.

For example, if `PatchCustomer.email` is optional, referencing
`PatchCustomer.email` names the type of the value when present. It does not make
another field optional.

### 6.3 Stateful and storage rules

The following are not intrinsic field-type validation:

- database uniqueness and indexes;
- default and generated values;
- foreign-key existence;
- record ownership and tenant scope;
- permission to read or write a field;
- verification, revocation, or lifecycle state;
- cross-record and provider-backed checks.

They are enforced by entity operations, queries, policies, actions, or other
compiler-visible stateful constructs.

### 6.4 Cross-field invariants

An invariant involving several fields belongs to the enclosing type or action,
not to one projected field type. For example, `starts_at < ends_at` is a
`DateRange` invariant. Referencing `DateRange.starts_at` does not claim that a
corresponding end time exists or that the range is valid.

## 7. Callable signatures

**Provisional, strongly recommended:** ordinary application-defined functions,
actions, jobs, handlers, service operations, and other callables must not accept
or return bare representation primitives.

Reject:

```text
function find_customer(email: Text) -> Customer?
function issue_refund(order_id: Uuid, amount: Decimal) -> Bool
```

Prefer:

```text
function find_customer(email: Email) -> Customer?

action issue_refund(
    order_id: Order.id,
    amount: Refund.amount
) -> Refund
```

Structured named types and collections of semantic types are also valid. The
restriction is about raw representation types, not about requiring every
parameter to be an entity field.

This rule deliberately makes boolean flags, unlabelled numbers, raw IDs, and
unconstrained text uncomfortable at domain boundaries. When two states have
domain meaning, a closed enum or separate operation is usually clearer than a
bare `Bool`.

Compiler and standard-library intrinsics may operate on primitives. If golden
applications demonstrate a real need for application-authored low-level
utilities, the language may add an explicit reviewed escape hatch rather than
weakening every callable.

## 8. Diagnostics

Type errors should explain semantic relationships, not only print two names.

```text
TYPE MISMATCH

send_customer_receipt expects:
  Customer.email

received:
  Supplier.email

Both fields refine Email, but sibling field types are not interchangeable.

Choose one:
- change the parameter to Email if either source is valid;
- pass a Customer.email value;
- explicitly validate as Customer.email if that transition is intended.
```

Primitive mismatch diagnostics should point to the relevant semantic type or
field declaration and show which validation boundary is missing.

Machine-readable diagnostics must include stable IDs for the expected type,
received type, call site, and relevant refinement edges so an agent can make a
small targeted repair.

## 9. Compiler representation

The semantic graph should represent:

- a stable node for every named semantic type;
- a stable node for every declared field type;
- `refines` edges from field types to their declared types;
- intrinsic constraints attached to the node they define;
- shape-presence rules attached to the containing record;
- stateful/storage rules attached to entities, actions, and policies rather
  than field value types;
- explicit validation and construction sites;
- widening paths selected during type checking;
- provenance, existence, and authorisation facts separately from value types.

The generated target may erase some nominal distinctions at runtime, but
metadata, validation, diagnostics, and audits must preserve them. Generated
TypeScript's structural compatibility is not the language's compatibility
model.

## 10. Open questions

The golden applications must resolve:

- final typed validation-failure payload and handling grammar for dynamic
  constructors;
- whether transparent aliases are ever necessary;
- whether immutable collection types can safely support covariance;
- how generic constraints express semantic/refinement relationships;
- which standard-library operations preserve which semantic types;
- whether application-authored low-level primitive utilities need an escape
  hatch;
- how explicit representation access is declared and audited;
- how stable semantic identity survives moves, splits, and merges of fields;
- whether field refinements may add inline constraints and their final syntax.

These are syntax and capability questions. They do not reopen structural
substitution, implicit primitive conversion, or the separation between value
validation and stateful proof.

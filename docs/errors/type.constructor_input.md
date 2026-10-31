# `type.constructor_input`

A type's constructor is given a value it can't convert, because the value is built on a different kind of value or may be `none`.

## Why Jadpo refuses it

A constructor, `Type(value)`, builds a value of a type by checking every
rule on it. It converts only between types built on the same kind of
value, such as two types built on `Uuid`, so it can check the rules
again without guessing how to change the value. Turning text into a
UUID isn't a check, and Jadpo never casts.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.constructor_input

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A value that may be `none` builds nothing either. Here `maybe` is a
`Count?`, and a `Quantity` can't be `none`:

```jadpo check project=e-ctor-input-absent expect=type.constructor_input
type Count = Int {
    min: 0
}

type Quantity = Int {
    min: 1
    max: 1000
}

function quantity_of(maybe: Count?) -> Quantity {
    return Quantity(maybe)
}
```

```text output
error: `Quantity` can't be built from `Count?`
 --> main.jadpo:11:21
   |
11 |     return Quantity(maybe)
   |                     ^^^^^ this is `Count?`
  reason: `Count?` may be `none`, and a `Quantity` can't be.
  next: Use `match` to handle `none` before building a `Quantity` (agent)
  rule: type.constructor_input
  help: https://jadpo.dev/docs/errors/type.constructor_input

check failed: 1 error, 1 file
```

Handle the `none` case first with `match`. The value inside `some(...)`
is a `Count`, built on `Int` as `Quantity` is, so a `Quantity` can be
built from it.

## How to fix it

**If the function should start from another id**, take a value built on
`Uuid`, such as a `CartId`:

```jadpo fragment
type CartId = Uuid {}

function order_for(cart: CartId) -> OrderId {
    return OrderId(cart)
}
```

```jadpo check project=e-ctor-input-fix-1 hidden
type OrderId = Uuid {}

type CartId = Uuid {}

function order_for(cart: CartId) -> OrderId {
    return OrderId(cart)
}
```

`OrderId(cart)` now converts one UUID-based type to another on purpose,
and the two stay separate types everywhere else.

## Related

- [Creating values](../language/02-values-and-types.md#creating-values)
- [Named types](../language/02-values-and-types.md#named-types)
- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)
- [When one type can stand in for another](../language/02-values-and-types.md#when-one-type-can-stand-in-for-another)

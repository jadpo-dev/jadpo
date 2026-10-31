# `type.unit_required`

A plain number is given where a physical quantity, such as a length or a mass, needs its unit.

## Why Jadpo refuses it

A quantity type, such as `Length`, holds an amount in a unit. A number
alone could be any length: 90 could be centimetres, inches or metres.
Mixing those up is the mistake the type exists to prevent, so a quantity
is always made with its unit, and this error can never be declined.

A plain number still scales a quantity, by `*` and `/`.

## What it looks like

main.jadpo:

```jadpo check project=e-unit-required expect=type.unit_required
type ShelfLength = Length {
    max: 3metre
}

/// The shelf a kitchen gets unless the customer asks for another.
function standard_shelf() -> ShelfLength {
    return ShelfLength(90)
}
```

```text output
error: `ShelfLength` needs a unit, and `90` is a bare number
 --> main.jadpo:7:24
  |
7 |     return ShelfLength(90)
  |                        ^^ no unit
  reason: A `Length` is always made with its unit, so a number alone could be any length: a number without its unit is the mistake the type exists to prevent.
  next: Write its unit after the number, such as `90metre`, or make it with `units.of(90, LengthUnit.metre)` in any of `LengthUnit`'s units (agent)
  rule: type.unit_required
  help: https://jadpo.dev/docs/errors/type.unit_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Two quantities of different kinds, such as a mass and a length, can't be
added either. That is a different error,
[`type.arithmetic_operand`](../errors/type.arithmetic_operand.md), which
names the two kinds.

main.jadpo:

```jadpo check project=e-quantity-kinds expect=type.arithmetic_operand
type ShelfLength = Length {}

type ShelfLoad = Mass {}

/// What a shelf is rated for, with its length added by mistake.
function rated(shelf_span: ShelfLength, load: ShelfLoad) -> ShelfLoad {
    return ShelfLoad(load + shelf_span)
}
```

```text output
error: `ShelfLoad` and `ShelfLength` can't be added: a `Mass` and a `Length` are different kinds
 --> main.jadpo:7:22
  |
7 |     return ShelfLoad(load + shelf_span)
  |                      ^^^^^^^^^^^^^^^^^ different kinds
  reason: `+` works within one kind of quantity, in any of its units; a `Mass` and a `Length` are different kinds, so the sum means nothing.
  next: Use values of one kind, or a product or quotient the table defines (agent)
  rule: type.arithmetic_operand
  help: https://jadpo.dev/docs/errors/type.arithmetic_operand

check failed: 1 error, 1 file
```

## How to fix it

**If the number is written in the code**, write its unit after it:

```jadpo fragment
    return ShelfLength(90cm)
```

```jadpo check project=e-unit-required-fix-1 hidden
type ShelfLength = Length {
    max: 3metre
}

/// The shelf a kitchen gets unless the customer asks for another.
function standard_shelf() -> ShelfLength {
    return ShelfLength(90cm)
}
```

The standard shelf is now 90 centimetres, and anyone reading the code
sees the unit.

To make a quantity from a number and a unit your code holds, use
`units.of`, such as `units.of(90, LengthUnit.centimetre)`. It takes an
`Int` or a `Decimal` and any unit of the kind.

## Related

- [Making a quantity](../language/19-standard-library.md#making-a-quantity)
- [Suffixed literals](../language/19-standard-library.md#suffixed-literals)
- [Units](../language/19-standard-library.md#units)
- [Quantity arithmetic](../language/19-standard-library.md#quantity-arithmetic)

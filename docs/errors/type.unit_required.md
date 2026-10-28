# `type.unit_required`

A bare number where a physical quantity goes: a quantity is always made
with its unit, a suffixed literal (`90cm`), `units.of(90,
LengthUnit.centimetre)`, `text.parse` of text that names one, or another
quantity of its kind. A number alone could be any length, which is the
mistake the type exists to prevent, so this is never declinable. A plain
number still scales a quantity, by `*` and `/`.

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
  help: docs/language/19-standard-library.html#type-unit-required

check failed: 1 error, 1 file
```

Two kinds that the table doesn't combine don't compile either, by area
3's `type.arithmetic_operand`, which names the two kinds:

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
  help: docs/language/03-logic.html#type-arithmetic-operand

check failed: 1 error, 1 file
```

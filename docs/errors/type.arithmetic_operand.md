# `type.arithmetic_operand`

A sum, difference or division mixes two different named types, or uses a value that isn't a number.

## Why Jadpo refuses it

A named type such as `Stock` or `Quantity` is a unit. Multiplying a pack
size by a number of packs means something, so `*` may mix types. Adding
them doesn't: the sum of a stock level and a quantity is neither, and
usually means the code reads the wrong value. So `+` and `-` need the same
named type on both sides, or a literal, and `/` needs the same type too.

## What it looks like

main.jadpo:

```jadpo check project=e-arithmetic expect=type.arithmetic_operand
type Stock = Int {
    min: 0
}

type Quantity = Int {
    min: 1
}

function wrong_total(per_pack: Stock, packs: Quantity) -> Stock {
    return Stock(per_pack + packs)
}
```

```text output
error: `Stock` and `Quantity` can't be added
 --> main.jadpo:10:18
   |
10 |     return Stock(per_pack + packs)
   |                  ^^^^^^^^^^^^^^^^ different types
  reason: `+` and `-` need the same named type on both sides, or a literal, because `Stock` and `Quantity` are different units; `*` and `%` may mix types.
  next: Use two values of the same type (agent)
  rule: type.arithmetic_operand
  help: https://jadpo.dev/docs/errors/type.arithmetic_operand

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Dividing one named type by another is the same error. `/` divides two
values of one type, and dropping the remainder is only one way to round:

main.jadpo:

```jadpo check project=e-arithmetic-divide expect=type.arithmetic_operand
type Stock = Int {
    min: 0
}

type Boxes = Int {
    min: 1
}

function per_box(items: Stock, boxes: Boxes) -> Stock {
    return Stock(items / boxes)
}
```

```text output
error: `Stock` and `Boxes` can't be divided
 --> main.jadpo:10:18
   |
10 |     return Stock(items / boxes)
   |                  ^^^^^^^^^^^^^ different types
  reason: `/` divides two values of the same named type; dividing `Stock` by `Boxes`, or rounding another way, is `integer.divide` or `decimal.divide`.
  next: Divide with `integer.divide` or `decimal.divide` (agent)
  rule: type.arithmetic_operand
  help: https://jadpo.dev/docs/errors/type.arithmetic_operand

check failed: 1 error, 1 file
```

A value that may be `none` is the error too, since `none` has no number.
Its `next:` line says to `match` on the value and use it inside
`some(...)`, as in [`type.ordering_operand`](../errors/type.ordering_operand.md).

## How to fix it

The question is what the code means to work out.

**If the two values are the same unit**, build one side as the other's
type with its constructor, so both sides match:

```jadpo fragment
    return Stock(per_pack + Stock(packs))
```

```jadpo check project=e-arithmetic-fix-1 hidden
type Stock = Int {
    min: 0
}

type Quantity = Int {
    min: 1
}

function wrong_total(per_pack: Stock, packs: Quantity) -> Stock {
    return Stock(per_pack + Stock(packs))
}
```

`Stock(packs)` checks the quantity against `Stock`'s rules. A quantity is
at least 1, so it is always a valid `Stock`, and building it can't fail.

**If you meant the stock for that many packs**, multiply, which may mix
types:

```jadpo fragment
    return Stock(per_pack * packs)
```

```jadpo check project=e-arithmetic-fix-2 hidden
type Stock = Int {
    min: 0
}

type Quantity = Int {
    min: 1
}

function wrong_total(per_pack: Stock, packs: Quantity) -> Stock {
    return Stock(per_pack * packs)
}
```

The result is the pack size times the number of packs.

**If you are dividing two different types**, use `integer.divide`, or
`decimal.divide` for decimals, and name how to round:

```jadpo fragment
    return Stock(integer.divide(items, boxes, rounding: Rounding.down))
```

```jadpo check project=e-arithmetic-divide-fix hidden
type Stock = Int {
    min: 0
}

type Boxes = Int {
    min: 1
}

function per_box(items: Stock, boxes: Boxes) -> Stock {
    return Stock(integer.divide(items, boxes, rounding: Rounding.down))
}
```

`Rounding.down` drops the remainder, rounding toward zero.
`Rounding.half_even`, `half_up` and `up` round the other ways
([Numbers](../language/19-standard-library.md#numbers)).

## Related

- [Expressions and operators](../language/03-logic.md#expressions-and-operators)
- [Creating values](../language/02-values-and-types.md#creating-values)
- [Numbers](../language/19-standard-library.md#numbers)

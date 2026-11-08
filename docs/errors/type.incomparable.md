# `type.incomparable`

An `==` or `!=` compares two values of different types, neither of which stands in for the other, or compares a value that is never `none` with `none`.

## Why Jadpo refuses it

A named type says what a value means, so a `Stock` of 3 and a `Quantity`
of 3 are different things, and asking whether they are equal is usually
a mistake: one side is the wrong variable. Comparing with `none` a value
whose type says it is never `none` always gives the same answer. Jadpo
won't convert one side for you, because only you know which value you
meant.

## What it looks like

main.jadpo:

```jadpo check project=e-incomparable expect=type.incomparable
type Stock = Int {
    min: 0
}

type Quantity = Int {
    min: 1
}

type Flag = Bool {}

function same(per_pack: Stock, packs: Quantity) -> Flag {
    return Flag(per_pack == packs)
}
```

```text output
error: `Stock` and `Quantity` can't be compared
 --> main.jadpo:12:17
   |
12 |     return Flag(per_pack == packs)
   |                 ^^^^^^^^^^^^^^^^^ different types
  reason: `==` compares two values of the same type, after widening; `Stock` and `Quantity` are different types, and neither widens to the other.
  next: Compare two values of the same type (agent)
  rule: type.incomparable
  help: https://jadpo.dev/docs/errors/type.incomparable

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Comparing a value that always has one with `none` is the same error:

main.jadpo:

```jadpo check project=e-incomparable-none expect=type.incomparable
type Stock = Int {
    min: 0
}

type Flag = Bool {}

function missing(per_pack: Stock) -> Flag {
    return Flag(per_pack == none)
}
```

```text output
error: `per_pack` is never `none`
 --> main.jadpo:8:17
  |
8 |     return Flag(per_pack == none)
  |                 ^^^^^^^^^^^^^^^^ `Stock` compared with `none`
  reason: Only a nullable value, written `T?`, can be `none`; `per_pack` is `Stock`.
  next: Remove the comparison, since it is always the same (agent)
  rule: type.incomparable
  help: https://jadpo.dev/docs/errors/type.incomparable

check failed: 1 error, 1 file
```

Its fix is to remove the comparison, whose answer is always `false`.

## How to fix it

The question is which two values you meant to compare.

**If one side is the wrong variable**, compare two values of the same
type:

```jadpo fragment
function same(per_pack: Stock, other_pack: Stock) -> Flag {
    return Flag(per_pack == other_pack)
}
```

```jadpo check project=e-incomparable-fix-1 hidden
type Stock = Int {
    min: 0
}

type Flag = Bool {}

function same(per_pack: Stock, other_pack: Stock) -> Flag {
    return Flag(per_pack == other_pack)
}
```

`same` now compares two stock levels.

**If the two really are comparable numbers**, construct one side as the
other's type first:

```jadpo fragment
function same(per_pack: Stock, packs: Quantity) -> Flag {
    return Flag(per_pack == Stock(packs))
}
```

```jadpo check project=e-incomparable-fix-2 hidden
type Stock = Int {
    min: 0
}

type Quantity = Int {
    min: 1
}

type Flag = Bool {}

function same(per_pack: Stock, packs: Quantity) -> Flag {
    return Flag(per_pack == Stock(packs))
}
```

`packs` is read as a `Stock`, so the comparison says whether the two
counts are the same number.

## Related

- [Expressions and operators](../language/03-logic.md#expressions-and-operators)
- [When one type can stand in for another](../language/02-values-and-types.md#when-one-type-can-stand-in-for-another)
- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)

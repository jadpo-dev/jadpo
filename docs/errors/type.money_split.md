# `type.money_split`

Code divides money by a whole number with `/`, which would share it out and lose minor units to rounding.

## Why Jadpo refuses it

Dividing money by a count is sharing it out, and rounding each share
loses minor units: £10.00 / 3 is £3.33, and three of those make £9.99.
`money.split` and `money.allocate` hand every minor unit to some share,
so the shares always add up to the total. `/` by a `Decimal` stays,
because that is a rate rather than a share: `/ 1.20` takes 20% VAT off.

## What it looks like

main.jadpo:

```jadpo check project=e-money-split expect=type.money_split
type BillTotal = Money {
    currency: Currency.gbp
    min: 0
}

/// One friend's share of a bill split three ways.
function share(total: BillTotal) -> BillTotal {
    return BillTotal(total / 3)
}
```

```text output
error: `BillTotal` is divided by a count
 --> main.jadpo:8:22
  |
8 |     return BillTotal(total / 3)
  |                      ^^^^^^^^^ money shared out by `/`
  reason: Dividing money by a count is sharing it out, and rounding each share loses minor units: £10.00 / 3 is £3.33, three times £9.99. `money.split` hands every minor unit to some share, and `/` takes only a `Decimal` rate, as `/ 1.20` takes a 20% tax off; `3` is a whole number.
  next: Share it with `money.split(total, 3)`, which gives a list of shares adding up to the total, or `money.allocate` for unequal shares; halve an amount with `* 0.5` (agent)
  rule: type.money_split
  help: https://jadpo.dev/docs/errors/type.money_split

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the shares are equal.

**If they are equal**, share the total with `money.split`:

```jadpo fragment
type Share = Money {}

/// Each friend's share of a bill split three ways.
function shares(total: BillTotal) -> List<Share> {
    return list.map(money.split(total, 3), share -> Share(share))
}
```

```jadpo check project=e-money-split-fix-1 hidden
type BillTotal = Money {
    currency: Currency.gbp
    min: 0
}

type Share = Money {}

/// Each friend's share of a bill split three ways.
function shares(total: BillTotal) -> List<Share> {
    return list.map(money.split(total, 3), share -> Share(share))
}
```

`shares` gives three near-equal amounts that add up to the total
exactly.

**If they are in proportion**, share it with `money.allocate` and the
ratios:

```jadpo fragment
type Share = Money {}

/// A bill shared between two adults and a child, who pays half.
function shares(total: BillTotal) -> List<Share> {
    return list.map(money.allocate(total, [2, 2, 1]), share -> Share(share))
}
```

```jadpo check project=e-money-split-fix-2 hidden
type BillTotal = Money {
    currency: Currency.gbp
    min: 0
}

type Share = Money {}

/// A bill shared between two adults and a child, who pays half.
function shares(total: BillTotal) -> List<Share> {
    return list.map(money.allocate(total, [2, 2, 1]), share -> Share(share))
}
```

`shares` gives the three amounts in the ratio 2 : 2 : 1, adding up to the
total exactly.

## Related

- [Money](../language/19-standard-library.md#money)
- [Money operations](../language/19-standard-library.md#money-operations)
- [Sharing money out](../language/19-standard-library.md#sharing-money-out)
- [`type.arithmetic_operand`](type.arithmetic_operand.md), for two amounts in different currencies

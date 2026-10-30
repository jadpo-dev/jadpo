# `type.money_split`

Money divided by a count: `/` by an `Int`. Dividing money by a count is
sharing it out, and rounding each share loses a minor unit; `money.split`
hands every one to some share, and `money.allocate` shares in proportion.
`/` by a `Decimal` stays, since that is a rate (`/ 1.20` takes VAT off).

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

Two amounts in currencies both types fix differently don't compile, by
area 3's `type.arithmetic_operand` (and `type.ordering_operand` for `<`):

```jadpo check project=e-currency-mismatch expect=type.arithmetic_operand
type CakePrice = Money {
    currency: Currency.gbp
}

type EuroPrice = Money {
    currency: Currency.eur
}

/// A cake priced in pounds, delivered from a shop that charges in euros.
function delivered(price: CakePrice, delivery: EuroPrice) -> CakePrice {
    return CakePrice(price + delivery)
}
```

```text output
error: `CakePrice` and `EuroPrice` can't be added: they're in different currencies
 --> main.jadpo:11:22
   |
11 |     return CakePrice(price + delivery)
   |                      ^^^^^^^^^^^^^^^^ GBP and EUR
  reason: `CakePrice` is in GBP and `EuroPrice` in EUR, as their types or the calls that made them fix; amounts are added only in one currency.
  next: Use two amounts in the same currency (agent)
  rule: type.arithmetic_operand
  help: https://jadpo.dev/docs/errors/type.arithmetic_operand

check failed: 1 error, 1 file
```

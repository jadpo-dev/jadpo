# `syntax.percent_ambiguous`

A number touching `%` is a percentage, `20%`, and the remainder (area 3)
is written with spaces, `20 % 3`, as the formatter writes every binary
operator. With a number or a name touching the `%` on both sides, either
could be meant, so neither is guessed.

```jadpo check project=e-percent-ambiguous expect=syntax.percent_ambiguous
type BoxCount = Int {}

/// The cakes left over when twenty are packed in boxes of three.
function twenty_in_threes() -> BoxCount {
    return BoxCount(20%3)
}
```

```text output
error: `20%3` could be `20% 3` or `20 % 3`
 --> main.jadpo:5:21
  |
5 |     return BoxCount(20%3)
  |                     ^^^^ a percentage or a remainder
  reason: A number touching `%` is a percentage, `20%`, and the remainder is written with spaces, `20 % 3`, as the formatter writes every binary operator; with `3` touching the `%` too, either could be meant.
  next: Write `20 % 3` for the remainder, or put the percentage `20%` apart from what follows it (agent)
  rule: syntax.percent_ambiguous
  help: docs/language/19-standard-library.html#syntax-percent-ambiguous

check failed: 1 error, 1 file
```

Adding a percentage to money doesn't compile either, by area 3's
`type.arithmetic_operand`, since it could mean the price raised by a
fifth or a fifth of something else; the fix writes both:

```jadpo check project=e-money-plus-percentage expect=type.arithmetic_operand
type CakePrice = Money {
    currency: Currency.gbp
}

/// A cake's price with VAT added.
function with_vat(price: CakePrice) -> CakePrice {
    return CakePrice(price + 20%)
}
```

```text output
error: `CakePrice` and a percentage can't be added: it could mean two things
 --> main.jadpo:7:22
  |
7 |     return CakePrice(price + 20%)
  |                      ^^^^^^^^^^^ an amount and a percentage
  reason: `price + 20%` could mean the amount raised by 20% of itself, or 20% of some other amount; the standard operator table refuses it rather than guess. A percentage scales money by `*`.
  next: Write `price * 120%`, or `price + price * 20%`, whichever is meant (agent)
  rule: type.arithmetic_operand
  help: docs/language/03-logic.html#type-arithmetic-operand

check failed: 1 error, 1 file
```

# `syntax.percent_ambiguous`

A number, `%` and a value touch with no spaces, so the `%` could mean a percentage or a remainder.

## Why Jadpo refuses it

A number touching `%` is a percentage, `20%`, and the remainder is written
with spaces, `20 % 3`, as the formatter writes every binary operator. With
a number or a name touching the `%` on both sides, either could be meant, and
the two give very different answers, so Jadpo doesn't guess.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/syntax.percent_ambiguous

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A name after the `%` is the same error:

main.jadpo:

```jadpo check project=e-percent-ambiguous-name expect=syntax.percent_ambiguous
type CakePrice = Money {
    currency: Currency.gbp
}

/// A fifth of a cake's price.
function fifth_of(rate: CakePrice) -> CakePrice {
    return CakePrice(20%rate)
}
```

```text output
error: `20%rate` could be `20% rate` or `20 % rate`
 --> main.jadpo:7:22
  |
7 |     return CakePrice(20%rate)
  |                      ^^^^^^^ a percentage or a remainder
  reason: A number touching `%` is a percentage, `20%`, and the remainder is written with spaces, `20 % rate`, as the formatter writes every binary operator; with `rate` touching the `%` too, either could be meant.
  next: Write `20 % rate` for the remainder, or put the percentage `20%` apart from what follows it (agent)
  rule: syntax.percent_ambiguous
  help: https://jadpo.dev/docs/errors/syntax.percent_ambiguous

check failed: 1 error, 1 file
```

## How to fix it

**If you meant the remainder**, put spaces around the `%`:

```jadpo fragment
function twenty_in_threes() -> BoxCount {
    return BoxCount(20 % 3)
}
```

```jadpo check project=e-percent-ambiguous-fix-1 hidden
type BoxCount = Int {}

/// The cakes left over when twenty are packed in boxes of three.
function twenty_in_threes() -> BoxCount {
    return BoxCount(20 % 3)
}
```

`twenty_in_threes` now gives the remainder, `2`.

**If you meant a percentage**, write the percentage apart from what it
scales, with `*`:

```jadpo fragment
function fifth_of(rate: CakePrice) -> CakePrice {
    return CakePrice(rate * 20%)
}
```

```jadpo check project=e-percent-ambiguous-fix-2 hidden
type CakePrice = Money {
    currency: Currency.gbp
}

/// A fifth of a cake's price.
function fifth_of(rate: CakePrice) -> CakePrice {
    return CakePrice(rate * 20%)
}
```

`fifth_of` now gives 20% of the price.

## Related

- [Percentages](../language/19-standard-library.md#percentages)
- [Expressions and operators](../language/03-logic.md#expressions-and-operators)

# `type.decimal_scale_required`

A type built on `Decimal` doesn't say its `scale`, the number of decimal places it keeps.

## Why Jadpo refuses it

A `Decimal` is exact, with a fixed number of decimal places. That
number, its `scale`, decides how the value is stored and sent, and how
far a result is rounded. There is no default: a price kept to 2 places
and a rate kept to 4 are different types, and guessing would store
values other than the ones you meant.

## What it looks like

main.jadpo:

```jadpo check project=e-scale expect=type.decimal_scale_required
type ItemPrice = Decimal {
    min: 0.00
}
```

```text output
error: `ItemPrice` needs a `scale`
 --> main.jadpo:1:6
  |
1 | type ItemPrice = Decimal {
  |      ^^^^^^^^^ a `Decimal` type
  reason: A `Decimal` is exact with a fixed number of decimal places, its `scale`, which sets how it is stored and sent.
  next: Add `scale:` with the number of decimal places, such as `scale: 2` (agent)
  rule: type.decimal_scale_required
  help: https://jadpo.dev/docs/errors/type.decimal_scale_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the value is a plain decimal number**, add `scale:` with the
number of decimal places it keeps:

```jadpo check project=e-scale-fix-1
type ItemPrice = Decimal {
    scale: 2
    min: 0.00
}
```

An `ItemPrice` keeps exactly 2 decimal places and is never below zero.

**If the value is an amount of money**, build it on `Money` instead,
which keeps its currency with the number:

```jadpo check project=e-scale-fix-2
type ItemPrice = Money {
    currency: Currency.gbp
    min: 0
}
```

An `ItemPrice` is now an amount in pounds, kept as a whole number of
pence, and never below zero.

## Related

- [Representation types](../language/02-values-and-types.md#representation-types)
- [Constraints](../language/02-values-and-types.md#constraints)
- [Standard types](../language/02-values-and-types.md#standard-types)

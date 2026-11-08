# `type.rule_not_condition`

A `rule`'s condition is something other than a `Bool`, such as a number.

## Why Jadpo refuses it

A rule holds for a value or it doesn't, so its condition has to be a
`Bool`: a comparison, or several joined with `and`, `or` and `not`. A
number or a piece of text doesn't say whether the rule holds, and Jadpo
has no truthiness to turn one into a `Bool`.

## What it looks like

main.jadpo:

```jadpo check project=e-rule-not-condition expect=type.rule_not_condition
type Stock = Int {
    min: 0
}

type PriceBand = Object {
    low: Stock
    high: Stock
    rule spread: high - low
}
```

```text output
error: The rule `spread` isn't a condition
 --> main.jadpo:8:18
  |
8 |     rule spread: high - low
  |                  ^^^^^^^^^^ this is `Int`
  reason: A rule holds for a value or it doesn't, so its condition is a `Bool`: a comparison such as `high > low`, or several joined with `and`, `or` and `not`.
  next: Write `spread` as a comparison of `PriceBand`'s fields (agent)
  rule: type.rule_not_condition
  help: https://jadpo.dev/docs/errors/type.rule_not_condition

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the rule is about how the fields compare**, write that comparison:

```jadpo fragment
type PriceBand = Object {
    ...
    rule spread: high > low
}
```

```jadpo check project=e-rule-not-condition-fix hidden
type Stock = Int {
    min: 0
}

type PriceBand = Object {
    low: Stock
    high: Stock
    rule spread: high > low
}
```

A `PriceBand` whose `high` isn't above its `low` is now refused whenever
one is made.

## Related

- [Rules across fields](../language/02-values-and-types.md#rules-across-fields)

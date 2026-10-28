# `type.rule_not_condition`

A rule holds or it doesn't, so its condition is a `Bool`:

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
  help: docs/language/02-values-and-types.html#type-rule-not-condition

check failed: 1 error, 1 file
```

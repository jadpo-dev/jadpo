# `syntax.named_argument_equals`

A named argument is `name: value`.

```jadpo check project=e-habit-named-equals expect=syntax.named_argument_equals
type ItemPrice = Decimal {
    scale: 2
    min: 0.00
}

type People = Int {
    min: 1
}

function share(total: ItemPrice, people: People) -> ItemPrice {
    return ItemPrice(decimal.divide(total, people, rounding = Rounding.half_even))
}
```

```text output
error: Jadpo names an argument with `:`, not `=`
 --> main.jadpo:11:61
   |
11 |     return ItemPrice(decimal.divide(total, people, rounding = Rounding.half_even))
   |                                                             ^ write `:`
  reason: `name = value` names an argument in Python, Kotlin and C#; Jadpo writes `name: value`.
  next: Use `:` instead of `=` (agent)
  rule: syntax.named_argument_equals
  help: https://jadpo.dev/docs/errors/syntax.named_argument_equals

check failed: 1 error, 1 file
```

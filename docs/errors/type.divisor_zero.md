# `type.divisor_zero`

A divisor's type excludes zero, so division by zero is ruled out while compiling:

```jadpo check project=e-divisor expect=type.divisor_zero
type Stock = Int {
    min: 0
}

function split_evenly(total: Stock, portions: Stock) -> Stock {
    return Stock(total / portions)
}
```

```text output
error: `portions` may be zero
 --> main.jadpo:6:26
  |
6 |     return Stock(total / portions)
  |                          ^^^^^^^^ a `Stock` can be `0`
  reason: Division by zero is ruled out while compiling: a divisor's type excludes zero, as `Int { min: 1 }` does, and `Stock` allows `0`.
  next: Divide by a value whose type excludes zero, building it with its constructor where the zero case is handled (agent)
  rule: type.divisor_zero
  help: https://jadpo.dev/docs/errors/type.divisor_zero

check failed: 1 error, 1 file
```

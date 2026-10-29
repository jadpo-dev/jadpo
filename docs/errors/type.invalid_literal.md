# `type.invalid_literal`

A literal takes the type it goes into and is checked against its rules while compiling, whether it is returned, assigned, passed or written as `Quantity(0)`:

```jadpo check project=e-literal expect=type.invalid_literal
type Quantity = Int {
    min: 1
    max: 1000
}

function starting_quantity() -> Quantity {
    return 0
}
```

```text output
error: `0` isn't a valid `Quantity`
 --> main.jadpo:7:12
  |
7 |     return 0
  |            ^ breaks `min: 1`
  reason: `Quantity` declares `min: 1`, and the value is below 1. A literal is checked against every rule while compiling.
  next: Use a value that `Quantity` allows (agent)
  rule: type.invalid_literal
  help: docs/errors/type.invalid_literal.md

check failed: 1 error, 1 file
```

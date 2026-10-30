# `type.constructor_argument_count`

```jadpo check project=e-ctor-count expect=type.constructor_argument_count
type Quantity = Int {
    min: 1
}

function pair() -> Quantity {
    return Quantity(1, 2)
}
```

```text output
error: `Quantity(...)` takes one value, not 2
 --> main.jadpo:6:12
  |
6 |     return Quantity(1, 2)
  |            ^^^^^^^^^^^^^^ 2 given
  reason: Constructing a `Quantity` checks one value against its rules.
  next: Pass one value to `Quantity(...)` (agent)
  rule: type.constructor_argument_count
  help: https://jadpo.dev/docs/errors/type.constructor_argument_count

check failed: 1 error, 1 file
```

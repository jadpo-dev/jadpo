# `failure.invalid_covers_nothing`

```jadpo check project=e-invalid-covers-nothing expect=failure.invalid_covers_nothing
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

type QuantityRefused = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooLarge, QuantityRefused -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityTooLarge
        invalid: QuantityRefused
    }
    return result
}
```

```text output
error: `invalid:` covers nothing here
 --> main.jadpo:17:9
   |
17 |         invalid: QuantityRefused
   |         ^^^^^^^ no rule left to cover
  reason: Every rule `Quantity(...)` can break has its own arm (`max`), so `invalid:` could never run.
  next: Remove `invalid: ...` (agent)
  rule: failure.invalid_covers_nothing
  help: docs/errors/failure.invalid_covers_nothing.md

check failed: 1 error, 1 file
```

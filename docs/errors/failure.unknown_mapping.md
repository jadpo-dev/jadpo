# `failure.unknown_mapping`

A construction's arms are the rules of its type:

```jadpo check project=e-unknown-mapping expect=failure.unknown_mapping
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        maximum: QuantityTooLarge
    }
    return result
}
```

```text output
error: `Quantity(...)` has no `maximum:` condition to map
 --> main.jadpo:12:9
   |
12 |         maximum: QuantityTooLarge
   |         ^^^^^^^ not a condition here
  reason: A construction's arms are the rules of its type and the types it builds on; `Quantity(...)`'s are `min` and `max`, and `invalid:` covers any without an arm of its own.
  next: Remove `maximum: ...`, or map one of `min` and `max` instead (agent)
  rule: failure.unknown_mapping
  help: docs/errors/failure.unknown_mapping.md

check failed: 1 error, 1 file
```

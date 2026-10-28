# `failure.duplicate_mapping`

```jadpo check project=e-duplicate-mapping expect=failure.duplicate_mapping
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityTooLarge
        max: QuantityTooLarge
    }
    return result
}
```

```text output
error: `max:` is mapped twice
 --> main.jadpo:13:9
   |
12 |         max: QuantityTooLarge
   |         --- first mapped here
13 |         max: QuantityTooLarge
   |         ^^^ mapped again
  reason: Each condition is mapped once, so there is never a question of which failure the caller gets.
  next: Remove one `max:` (agent)
  rule: failure.duplicate_mapping
  help: docs/language/04-failures.html#failure-duplicate-mapping

check failed: 1 error, 1 file
```

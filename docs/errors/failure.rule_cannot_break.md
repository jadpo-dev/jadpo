# `failure.rule_cannot_break`

An arm for a rule the declared ranges prove holds could never run:

```jadpo check project=e-rule-cannot-break expect=failure.rule_cannot_break
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooSmall = Failure {
    kind: FailureKind.invalid_value
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooSmall, QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        min: QuantityTooSmall
        max: QuantityTooLarge
    }
    return result
}
```

```text output
error: `Quantity(...)` can't break `min`, so it takes no arm
 --> main.jadpo:16:9
   |
16 |         min: QuantityTooSmall
   |         ^^^ never runs
  reason: The declared ranges prove the value always follows `min: 1`, so this arm could never run.
  next: Remove `min: ...` (agent)
  rule: failure.rule_cannot_break
  help: docs/errors/failure.rule_cannot_break.md

check failed: 1 error, 1 file
```

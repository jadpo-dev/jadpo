# `failure.rule_unmapped`

Each rule a construction can break has an arm, or `invalid:` covers it:

```jadpo check project=e-rule-unmapped expect=failure.rule_unmapped
type Count = Int {}

type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function from_count(n: Count) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(n) {
        max: QuantityTooLarge
    }
    return result
}
```

```text output
error: `Quantity(...)` can break `min`, and nothing maps it
 --> main.jadpo:13:28
   |
13 |     var result: Quantity = attempt Quantity(n) {
   |                            ^^^^^^^^^^^^^^^^^^^^^ can fail at runtime
  reason: The value is only known when the program runs, and neither the declared ranges nor a condition before it prove it follows `min: 1`; it needs an arm naming the `invalid_value` failure the caller gets, or `invalid:` covers every rule without its own.
  next: Add `min: SomeFailure` to the block, naming an `invalid_value` failure, or `invalid: SomeFailure` to cover every rule without an arm (agent)
  rule: failure.rule_unmapped
  help: docs/language/04-failures.html#failure-rule-unmapped

check failed: 1 error, 1 file
```

# `failure.mapping_kind`

```jadpo check project=e-mapping-kind expect=failure.mapping_kind
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityRefused = Failure {
    kind: FailureKind.rejected
}

function doubled(quantity: Quantity) fails QuantityRefused -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityRefused
    }
    return result
}
```

```text output
error: `max:` needs an `invalid_value` failure, and `QuantityRefused` is `rejected`
 --> main.jadpo:12:14
   |
12 |         max: QuantityRefused
   |              ^^^^^^^^^^^^^^^ kind `rejected`
  reason: Each mapping's condition fixes the kind of failure it becomes: a value that breaks a type's rules is `invalid_value`, so the client gets the status that means.
  next: Name an `invalid_value` failure here, or make `QuantityRefused`'s kind `FailureKind.invalid_value` (agent)
  rule: failure.mapping_kind
  help: docs/language/04-failures.html#failure-mapping-kind

check failed: 1 error, 1 file
```

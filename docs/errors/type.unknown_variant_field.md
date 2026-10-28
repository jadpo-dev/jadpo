# `type.unknown_variant_field`

```jadpo check project=e-variant-unknown expect=type.unknown_variant_field
type DeclineReason = Text {
    max_length: 200
}

type PaymentOutcome = Enum {
    declined: {
        reason: DeclineReason
    }
}

function declined_payment(reason: DeclineReason) -> PaymentOutcome {
    return PaymentOutcome.declined {
        reason: reason
        note: reason
    }
}
```

```text output
error: `PaymentOutcome.declined` has no field `note`
 --> main.jadpo:14:9
   |
14 |         note: reason
   |         ^^^^ not declared
  reason: `PaymentOutcome.declined`'s fields are `reason`.
  next: Remove `note`, or declare it on `PaymentOutcome.declined` (agent)
  rule: type.unknown_variant_field
  help: docs/language/02-values-and-types.html#type-unknown-variant-field

check failed: 1 error, 1 file
```

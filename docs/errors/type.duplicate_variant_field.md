# `type.duplicate_variant_field`

```jadpo check project=e-variant-duplicate expect=type.duplicate_variant_field
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
        reason: reason
    }
}
```

```text output
error: `reason` is given twice
 --> main.jadpo:14:9
   |
13 |         reason: reason
   |         ------ first given here
14 |         reason: reason
   |         ^^^^^^ given again
  reason: Each field of `PaymentOutcome.declined` is given once, so there is never a question of which value counts.
  next: Remove one `reason` (agent)
  rule: type.duplicate_variant_field
  help: https://jadpo.dev/docs/errors/type.duplicate_variant_field

check failed: 1 error, 1 file
```

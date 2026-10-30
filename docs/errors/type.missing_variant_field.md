# `type.missing_variant_field`

```jadpo check project=e-variant-missing expect=type.missing_variant_field
type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

function paid() -> PaymentOutcome {
    return PaymentOutcome.paid {}
}
```

```text output
error: `PaymentOutcome.paid` is missing `receipt_id`
 --> main.jadpo:11:12
   |
11 |     return PaymentOutcome.paid {}
   |            ^^^^^^^^^^^^^^^^^^^ fields missing
  reason: Every field of `PaymentOutcome.paid` is given when a value is built; a field that may be absent is declared `T?` and given as `none`.
  next: Add `receipt_id: ...` to the block (agent)
  rule: type.missing_variant_field
  help: https://jadpo.dev/docs/errors/type.missing_variant_field

check failed: 1 error, 1 file
```

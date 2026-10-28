# `syntax.match_pattern`

A match arm names a case's data in brackets, not braces:

```jadpo check project=e-match-pattern expect=syntax.match_pattern
type ReceiptId = Uuid {}

type Label = Text {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

function label(outcome: PaymentOutcome) -> Label {
    return match outcome {
        PaymentOutcome.pending -> "pending"
        PaymentOutcome.paid { receipt_id } -> "paid"
    }
}
```

```text output
error: A match arm names a case's data in brackets
 --> main.jadpo:15:29
   |
15 |         PaymentOutcome.paid { receipt_id } -> "paid"
   |                             ^^^^^^^^^^^^^^ expected `(name)`
  reason: An arm binds a case's data to one name, `PaymentOutcome.paid(paid)`, and reads its fields from it, as in `paid.field`.
  next: Write `PaymentOutcome.paid(paid)` (agent)
  rule: syntax.match_pattern
  help: docs/language/03-logic.html#syntax-match-pattern

check failed: 1 error, 1 file
```

The `match` examples below share a hidden file declaring
`DeliveryState { pending, sent, failed }`,
`PaymentOutcome { pending, paid: { receipt_id } }` and `Label = Text`.

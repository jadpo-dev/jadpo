# `type.match_unknown_variant`

```jadpo check project=e-match-variant file=shapes.jadpo hidden
type DeliveryState = Enum {
    pending
    sent
    failed
}

type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Label = Text {}
```

```jadpo check project=e-match-variant expect=type.match_unknown_variant
function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.snet -> "Sent"
        DeliveryState.failed -> "Failed"
    }
}
```

```text output
error: `DeliveryState` has no variant `snet`
 --> main.jadpo:4:23
  |
4 |         DeliveryState.snet -> "Sent"
  |                       ^^^^ not a variant
  reason: `DeliveryState`'s variants are `pending`, `sent` and `failed`. Did you mean `sent`?
  next: Use `DeliveryState.sent` (agent)
  rule: type.match_unknown_variant
  help: docs/errors/type.match_unknown_variant.md

check failed: 1 error, 2 files
```

# `type.match_non_exhaustive`

```jadpo check project=e-match-exhaustive file=shapes.jadpo hidden
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

```jadpo check project=e-match-exhaustive expect=type.match_non_exhaustive
function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.sent -> "Sent"
    }
}
```

```text output
error: This `match` doesn't cover `DeliveryState.failed`
 --> main.jadpo:2:12
  |
2 |     return match state {
  |            ^^^^^^^^^^^ 1 case not matched
  reason: A `match` on an enum lists every variant, so a variant added later is caught at every `match` that has to consider it; `_` isn't allowed on `DeliveryState`.
  next: Add an arm for `DeliveryState.failed` (agent)
  rule: type.match_non_exhaustive
  help: docs/errors/type.match_non_exhaustive.md

check failed: 1 error, 2 files
```

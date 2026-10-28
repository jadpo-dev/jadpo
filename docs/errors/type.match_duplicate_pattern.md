# `type.match_duplicate_pattern`

```jadpo check project=e-match-duplicate file=shapes.jadpo hidden
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

```jadpo check project=e-match-duplicate expect=type.match_duplicate_pattern
function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.pending -> "Waiting"
        DeliveryState.sent -> "Sent"
        DeliveryState.failed -> "Failed"
    }
}
```

```text output
error: `DeliveryState.pending` is matched twice
 --> main.jadpo:4:9
  |
3 |         DeliveryState.pending -> "Pending"
  |         --------------------- first matched here
4 |         DeliveryState.pending -> "Waiting"
  |         ^^^^^^^^^^^^^^^^^^^^^ matched again
  reason: Each case is matched once; the first arm takes every value this one would, so this arm never runs.
  next: Remove this arm, or merge its body into the first (agent)
  rule: type.match_duplicate_pattern
  help: docs/errors/type.match_duplicate_pattern.md

check failed: 1 error, 2 files
```

# `type.match_pattern_type`

```jadpo check project=e-match-type file=shapes.jadpo hidden
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

```jadpo check project=e-match-type expect=type.match_pattern_type
function describe(state: DeliveryState) -> Label {
    return match state {
        true -> "Yes"
        DeliveryState.pending -> "Pending"
        DeliveryState.sent -> "Sent"
        DeliveryState.failed -> "Failed"
    }
}
```

```text output
error: A `Bool` can't match a `DeliveryState`
 --> main.jadpo:3:9
  |
3 |         true -> "Yes"
  |         ^^^^ wrong kind of pattern
  reason: A `DeliveryState` is matched by its variants, such as `DeliveryState.pending`.
  next: Write a pattern for a `DeliveryState` (agent)
  rule: type.match_pattern_type
  help: docs/language/03-logic.html#type-match-pattern-type

check failed: 1 error, 2 files
```

# `type.match_unreachable_pattern`

```jadpo check project=e-match-unreachable file=shapes.jadpo hidden
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

```jadpo check project=e-match-unreachable expect=type.match_unreachable_pattern
function shout(label: Label) -> Label {
    return match label {
        "" -> "unnamed"
        _ -> label
        "quiet" -> "QUIET"
    }
}
```

```text output
error: This arm never runs
 --> main.jadpo:5:9
  |
4 |         _ -> label
  |         - matches everything left
5 |         "quiet" -> "QUIET"
  |         ^^^^^^^ after `_`
  reason: `_` matches every value the arms above it don't, so no arm after it runs.
  next: Move this arm above `_` (agent)
  rule: type.match_unreachable_pattern
  help: docs/language/03-logic.html#type-match-unreachable-pattern

check failed: 1 error, 2 files
```

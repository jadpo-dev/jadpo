# `type.match_wildcard_required`

```jadpo check project=e-match-wildcard file=shapes.jadpo hidden
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

```jadpo check project=e-match-wildcard expect=type.match_wildcard_required
function shout(label: Label) -> Label {
    return match label {
        "" -> "unnamed"
    }
}
```

```text output
error: This `match` needs a `_` arm
 --> main.jadpo:2:12
  |
2 |     return match label {
  |            ^^^^^^^^^^^ not every value is matched
  reason: `Label` is built on `Text`, so its values can't all be listed; `_` matches every value the arms above it don't.
  next: Add `_ -> ...` as the last arm (agent)
  rule: type.match_wildcard_required
  help: docs/language/03-logic.html#type-match-wildcard-required

check failed: 1 error, 2 files
```

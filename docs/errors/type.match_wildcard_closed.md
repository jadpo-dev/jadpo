# `type.match_wildcard_closed`

An enum, a `Bool` or a nullable value lists every case, so a case added later is caught:

```jadpo check project=e-match-wildcard-closed file=shapes.jadpo hidden
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

```jadpo check project=e-match-wildcard-closed expect=type.match_wildcard_closed
function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        _ -> "Other"
    }
}
```

```text output
error: `_` can't stand for the rest of a `DeliveryState`
 --> main.jadpo:4:9
  |
4 |         _ -> "Other"
  |         ^ wildcard
  reason: On enums, `Bool` and nullable values every case is listed, so a case added later is caught at every `match` that has to consider it.
  next: Replace `_` with arms for `DeliveryState.sent` and `DeliveryState.failed` (agent)
  rule: type.match_wildcard_closed
  help: https://jadpo.dev/docs/errors/type.match_wildcard_closed

check failed: 1 error, 2 files
```

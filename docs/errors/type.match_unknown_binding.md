# `type.match_unknown_binding`

Brackets after a case name the data it carries, and a plain variant carries none:

```jadpo check project=e-match-binding file=shapes.jadpo hidden
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

```jadpo check project=e-match-binding expect=type.match_unknown_binding
function describe(outcome: PaymentOutcome) -> Label {
    return match outcome {
        PaymentOutcome.pending(waiting) -> "Pending"
        PaymentOutcome.paid -> "Paid"
    }
}
```

```text output
error: `PaymentOutcome.pending` carries no data to name
 --> main.jadpo:3:31
  |
3 |         PaymentOutcome.pending(waiting) -> "Pending"
  |                               ^^^^^^^^^ no data
  reason: `PaymentOutcome.pending` is a plain variant; brackets after a case name the data it carries, and this one carries none.
  next: Write `PaymentOutcome.pending` without brackets (agent)
  rule: type.match_unknown_binding
  help: https://jadpo.dev/docs/errors/type.match_unknown_binding

check failed: 1 error, 2 files
```

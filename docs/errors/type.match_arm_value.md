# `type.match_arm_value`

```jadpo check project=e-match-arm-value file=shapes.jadpo hidden
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

```jadpo check project=e-match-arm-value expect=type.match_arm_value
action notify(outcome: PaymentOutcome) -> Unit {}

action receipt(outcome: PaymentOutcome) -> ReceiptId? {
    return match outcome {
        PaymentOutcome.pending -> none
        PaymentOutcome.paid -> {
            notify(outcome)
        }
    }
}
```

```text output
error: This arm gives no value
 --> main.jadpo:6:9
  |
6 |         PaymentOutcome.paid -> {
  |         ^^^^^^^^^^^^^^^^^^^^^^^^ ends without a value
  reason: A `match` used as a value gives one from every arm: an expression, `reject` or `propagate`; this block can reach its end without leaving.
  next: Write the arm's value after `->`, or end the block with `return` or `reject` (agent)
  rule: type.match_arm_value
  help: docs/language/03-logic.html#type-match-arm-value

check failed: 1 error, 2 files
```

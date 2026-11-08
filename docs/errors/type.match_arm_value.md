# `type.match_arm_value`

An arm of a `match` that gives a value is a block that can reach its end without one.

## Why Jadpo refuses it

A `match` written where a value goes, after `return` or `=`, gives one
value from whichever arm runs. An arm that is a block of statements has
to end in a way that gives or leaves: `return`, `reject` or `propagate`.
One that can run off its end leaves the `match` with nothing to give.
Jadpo won't make up a value for it, since it can't know what the arm
meant to give.

## What it looks like

main.jadpo:

```jadpo check project=e-match-arm-value expect=type.match_arm_value
type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

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
 --> main.jadpo:15:9
   |
15 |         PaymentOutcome.paid -> {
   |         ^^^^^^^^^^^^^^^^^^^^^^^^ ends without a value
  reason: A `match` used as a value gives one from every arm: an expression, `reject` or `propagate`; this block can reach its end without leaving.
  next: Write the arm's value after `->`, or end the block with `return` or `reject` (agent)
  rule: type.match_arm_value
  help: https://jadpo.dev/docs/errors/type.match_arm_value

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the arm's statements belong in the `match`.

**If the arm only gives a value**, write the value after `->`:

```jadpo fragment
action receipt(outcome: PaymentOutcome) -> ReceiptId? {
    notify(outcome)
    return match outcome {
        PaymentOutcome.pending -> none
        PaymentOutcome.paid(paid) -> paid.receipt_id
    }
}
```

```jadpo check project=e-match-arm-value-fix-1 hidden
type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

action notify(outcome: PaymentOutcome) -> Unit {}

action receipt(outcome: PaymentOutcome) -> ReceiptId? {
    notify(outcome)
    return match outcome {
        PaymentOutcome.pending -> none
        PaymentOutcome.paid(paid) -> paid.receipt_id
    }
}
```

`receipt` notifies first, for every outcome, then gives a paid outcome's
receipt.

**If the statements run only in that arm**, end the block with `return`:

```jadpo fragment
action receipt(outcome: PaymentOutcome) -> ReceiptId? {
    match outcome {
        PaymentOutcome.pending -> {
            return none
        }
        PaymentOutcome.paid(paid) -> {
            notify(outcome)
            return paid.receipt_id
        }
    }
}
```

```jadpo check project=e-match-arm-value-fix-2 hidden
type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

action notify(outcome: PaymentOutcome) -> Unit {}

action receipt(outcome: PaymentOutcome) -> ReceiptId? {
    match outcome {
        PaymentOutcome.pending -> {
            return none
        }
        PaymentOutcome.paid(paid) -> {
            notify(outcome)
            return paid.receipt_id
        }
    }
}
```

The `match` is now a statement, and each arm returns from `receipt`, so
only a paid outcome sends the notice.

## Related

- [Match](../language/03-logic.md#match)
- [Statements](../language/03-logic.md#statements)

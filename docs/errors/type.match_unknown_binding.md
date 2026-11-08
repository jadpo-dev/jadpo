# `type.match_unknown_binding`

A `match` arm puts a name in brackets after a variant that carries no data for it to name.

## Why Jadpo refuses it

Brackets after a variant name what it carries, so the arm can read its
fields: `PaymentOutcome.paid(paid)` gives `paid.receipt_id`. A plain
variant carries nothing, so the name would hold nothing to read. It
usually means the arm was meant for another variant, or the variant was
meant to carry fields.

## What it looks like

main.jadpo:

```jadpo check project=e-match-binding expect=type.match_unknown_binding
type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Label = Text {}

function describe(outcome: PaymentOutcome) -> Label {
    return match outcome {
        PaymentOutcome.pending(waiting) -> "Pending"
        PaymentOutcome.paid -> "Paid"
    }
}
```

```text output
error: `PaymentOutcome.pending` carries no data to name
 --> main.jadpo:14:31
   |
14 |         PaymentOutcome.pending(waiting) -> "Pending"
   |                               ^^^^^^^^^ no data
  reason: `PaymentOutcome.pending` is a plain variant; brackets after a case name the data it carries, and this one carries none.
  next: Write `PaymentOutcome.pending` without brackets (agent)
  rule: type.match_unknown_binding
  help: https://jadpo.dev/docs/errors/type.match_unknown_binding

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the arm reads nothing from the variant**, write it without brackets:

```jadpo fragment
function describe(outcome: PaymentOutcome) -> Label {
    return match outcome {
        PaymentOutcome.pending -> "Pending"
        PaymentOutcome.paid -> "Paid"
    }
}
```

```jadpo check project=e-match-binding-fix hidden
type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Label = Text {}

function describe(outcome: PaymentOutcome) -> Label {
    return match outcome {
        PaymentOutcome.pending -> "Pending"
        PaymentOutcome.paid -> "Paid"
    }
}
```

The arm matches every pending outcome, as before, and names nothing.

## Related

- [Match](../language/03-logic.md#match)
- [Enums](../language/02-values-and-types.md#enums)

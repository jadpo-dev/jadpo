# `semantic.unused_variable`

A variable is given a value that nothing ever reads.

## Why Jadpo refuses it

A name in the code tells the reader that its value matters later. A value
nobody reads is either left over from an edit or a sign that the code
reads the wrong name, and a reviewer can't tell which. So every variable
is read, or it isn't written.

## What it looks like

main.jadpo:

```jadpo check project=e-unused-variable expect=semantic.unused_variable
type Stock = Int {
    min: 0
}

function with_delivery(per_pack: Stock, delivery: Stock) -> Stock {
    var needed: Stock = per_pack
    return Stock(per_pack + delivery)
}
```

```text output
error: `needed` is never used
 --> main.jadpo:6:9
  |
6 |     var needed: Stock = per_pack
  |         ^^^^^^ never read
  reason: A value nobody reads is never bound to a name, so a reader never wonders what it was for.
  next: Use `needed`, or remove its `var` line (agent)
  rule: semantic.unused_variable
  help: https://jadpo.dev/docs/errors/semantic.unused_variable

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `match` arm that names a variant's data and never reads it is the same
error. Here the arm only needs to know the payment was made:

main.jadpo:

```jadpo check project=e-unused-binding expect=semantic.unused_variable
type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Flag = Bool {}

function is_paid(outcome: PaymentOutcome) -> Flag {
    return match outcome {
        PaymentOutcome.pending -> false
        PaymentOutcome.paid(paid) -> true
    }
}
```

```text output
error: `paid` is never used
 --> main.jadpo:15:29
   |
15 |         PaymentOutcome.paid(paid) -> true
   |                             ^^^^ never read
  reason: A value nobody reads is never bound to a name; a pattern without brackets matches the case without naming its data.
  next: Remove `(paid)` from the pattern (agent)
  rule: semantic.unused_variable
  help: https://jadpo.dev/docs/errors/semantic.unused_variable

check failed: 1 error, 1 file
```

A `for` loop whose body never reads its item is the error too, and its
`next:` line suggests counting the list with `list.count` instead.

## How to fix it

**If the code was meant to read the variable**, read it where it was
meant to be used:

```jadpo fragment
    return Stock(needed + delivery)
```

```jadpo check project=e-unused-variable-fix-1 hidden
type Stock = Int {
    min: 0
}

function with_delivery(per_pack: Stock, delivery: Stock) -> Stock {
    var needed: Stock = per_pack
    return Stock(needed + delivery)
}
```

The result is worked out from `needed`, so the name says what the value
is for.

**If nothing needs it**, remove its `var` line. `main.jadpo` becomes:

```jadpo check project=e-unused-variable-fix-2
type Stock = Int {
    min: 0
}

function with_delivery(per_pack: Stock, delivery: Stock) -> Stock {
    return Stock(per_pack + delivery)
}
```

The function works out the same result, with one name fewer to follow.

**If a `match` arm doesn't read the data**, remove the brackets and the
name from its pattern:

```jadpo fragment
    return match outcome {
        PaymentOutcome.pending -> false
        PaymentOutcome.paid -> true
    }
```

```jadpo check project=e-unused-binding-fix hidden
type ReceiptId = Uuid {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_id: ReceiptId
    }
}

type Flag = Bool {}

function is_paid(outcome: PaymentOutcome) -> Flag {
    return match outcome {
        PaymentOutcome.pending -> false
        PaymentOutcome.paid -> true
    }
}
```

The arm still matches every paid outcome, without naming its receipt.

## Related

- [Variables](../language/03-logic.md#variables)
- [Match](../language/03-logic.md#match)

# `type.match_non_exhaustive`

A `match` on an enum, a `Bool` or a value that may be `none` leaves out one of its cases.

## Why Jadpo refuses it

A `match` picks what to do for each case of a value, and on an enum, a
`Bool` or a nullable value every case can be listed. Listing them all
means a variant added to an enum later stops every `match` that has to
handle it from compiling, until it does. A `match` that left a case out
would have nothing to do when that case came, so Jadpo refuses it, and
`_` isn't allowed to stand for the rest.

## What it looks like

`DeliveryState`, an enum of `pending`, `sent` and `failed`, and the `Label`
text type are declared in another file of the project, `shapes.jadpo`.

```jadpo check project=e-match-exhaustive file=shapes.jadpo hidden
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

main.jadpo:

```jadpo check project=e-match-exhaustive expect=type.match_non_exhaustive
function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.sent -> "Sent"
    }
}
```

```text output
error: This `match` doesn't cover `DeliveryState.failed`
 --> main.jadpo:2:12
  |
2 |     return match state {
  |            ^^^^^^^^^^^ 1 case not matched
  reason: A `match` on an enum lists every variant, so a variant added later is caught at every `match` that has to consider it; `_` isn't allowed on `DeliveryState`.
  next: Add an arm for `DeliveryState.failed` (agent)
  rule: type.match_non_exhaustive
  help: https://jadpo.dev/docs/errors/type.match_non_exhaustive

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `match` on a value that may be `none` lists `some(name)` and `none`.
Here the `none` arm is missing:

main.jadpo:

```jadpo check project=e-match-exhaustive-none expect=type.match_non_exhaustive
type Label = Text {}

function headline(title: Label, subtitle: Label?) -> Label {
    return match subtitle {
        some(shown) -> shown
    }
}
```

```text output
error: This `match` doesn't cover `none`
 --> main.jadpo:4:12
  |
4 |     return match subtitle {
  |            ^^^^^^^^^^^^^^ 1 case not matched
  reason: A `match` on a nullable value lists `some(name)` and `none`.
  next: Add an arm for `none` (agent)
  rule: type.match_non_exhaustive
  help: https://jadpo.dev/docs/errors/type.match_non_exhaustive

check failed: 1 error, 1 file
```

## How to fix it

**If a variant is missing**, add an arm for each case the error names:

```jadpo fragment
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.sent -> "Sent"
        DeliveryState.failed -> "Failed"
    }
```

```jadpo check project=e-match-exhaustive-fix-1 hidden
function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.sent -> "Sent"
        DeliveryState.failed -> "Failed"
    }
}
```

```jadpo check project=e-match-exhaustive-fix-1 file=shapes.jadpo hidden
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

A failed delivery is now described as "Failed". If a variant is added to
`DeliveryState` later, this `match` stops compiling until it has an arm
for it too.

**If the `none` arm is missing**, add it, with what to use when there is
no value:

```jadpo fragment
    return match subtitle {
        some(shown) -> shown
        none -> title
    }
```

```jadpo check project=e-match-exhaustive-none-fix hidden
type Label = Text {}

function headline(title: Label, subtitle: Label?) -> Label {
    return match subtitle {
        some(shown) -> shown
        none -> title
    }
}
```

With no subtitle, the headline is the title.

## Related

- [Match](../language/03-logic.md#match)
- [Enums](../language/02-values-and-types.md#enums)
- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)

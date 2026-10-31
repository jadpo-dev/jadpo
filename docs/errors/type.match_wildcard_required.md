# `type.match_wildcard_required`

A `match` on text or a number lists some values but has no `_` arm for all the others.

## Why Jadpo refuses it

A `match` must cover every value it can be given. An enum's cases can all
be listed, but text and numbers have more values than any list of arms.
Arms for `""` or `0` pick out a few of them, and `_`, as the last arm,
says what to do with the rest. Without it, the `match` has nothing to do
for most values.

## What it looks like

The `Label` text type is declared in another file of the project,
`shapes.jadpo`.

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

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.match_wildcard_required

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the other values all get the same result**, add `_` as the last
arm, with what to do for them:

```jadpo fragment
        _ -> label
```

```jadpo check project=e-match-wildcard-fix hidden
function shout(label: Label) -> Label {
    return match label {
        "" -> "unnamed"
        _ -> label
    }
}
```

```jadpo check project=e-match-wildcard-fix file=shapes.jadpo hidden
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

An empty label becomes "unnamed", and any other label is kept as it is.
`_` is only for values whose cases can't be listed: on an enum, a `Bool`
or a value that may be `none`, each case gets its own arm
([`type.match_wildcard_closed`](../errors/type.match_wildcard_closed.md)).

## Related

- [Match](../language/03-logic.md#match)

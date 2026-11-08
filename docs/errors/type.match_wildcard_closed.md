# `type.match_wildcard_closed`

A `match` on an enum, a `Bool` or a value that may be `none` has a `_` arm instead of an arm for each case.

## Why Jadpo refuses it

The cases of an enum, a `Bool` or a nullable value can all be listed, so
a `match` on one lists them. Then adding a variant to the enum makes
every `match` that has to consider it fail to compile until it does. A
`_` arm would take the new variant silently, giving it whatever the arm
gives the others, which may be wrong for it.

## What it looks like

main.jadpo:

```jadpo check project=e-match-wildcard-closed expect=type.match_wildcard_closed
type DeliveryState = Enum {
    pending
    sent
    failed
}

type Label = Text {}

function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        _ -> "Other"
    }
}
```

```text output
error: `_` can't stand for the rest of a `DeliveryState`
 --> main.jadpo:12:9
   |
12 |         _ -> "Other"
   |         ^ wildcard
  reason: On enums, `Bool` and nullable values every case is listed, so a case added later is caught at every `match` that has to consider it.
  next: Replace `_` with arms for `DeliveryState.sent` and `DeliveryState.failed` (agent)
  rule: type.match_wildcard_closed
  help: https://jadpo.dev/docs/errors/type.match_wildcard_closed

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the other cases share one value**, give each its own arm with that
value:

```jadpo fragment
function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.sent -> "Other"
        DeliveryState.failed -> "Other"
    }
}
```

```jadpo check project=e-match-wildcard-closed-fix hidden
type DeliveryState = Enum {
    pending
    sent
    failed
}

type Label = Text {}

function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.sent -> "Other"
        DeliveryState.failed -> "Other"
    }
}
```

`describe` gives the same labels as before, and a variant added to
`DeliveryState` later makes it fail to compile until it has an arm.

## Related

- [Match](../language/03-logic.md#match)
- [Enums](../language/02-values-and-types.md#enums)

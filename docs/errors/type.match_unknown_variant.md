# `type.match_unknown_variant`

A `match` arm names a variant that the enum doesn't declare.

## Why Jadpo refuses it

An enum lists every variant it has, so an arm can only name one of them.
A name it doesn't list is usually a typo, or a variant renamed or removed
since the arm was written. The error gives the nearest variant's name
when there is one.

## What it looks like

main.jadpo:

```jadpo check project=e-match-variant expect=type.match_unknown_variant
type DeliveryState = Enum {
    pending
    sent
    failed
}

type Label = Text {}

function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.snet -> "Sent"
        DeliveryState.failed -> "Failed"
    }
}
```

```text output
error: `DeliveryState` has no variant `snet`
 --> main.jadpo:12:23
   |
12 |         DeliveryState.snet -> "Sent"
   |                       ^^^^ not a variant
  reason: `DeliveryState`'s variants are `pending`, `sent` and `failed`. Did you mean `sent`?
  next: Use `DeliveryState.sent` (agent)
  rule: type.match_unknown_variant
  help: https://jadpo.dev/docs/errors/type.match_unknown_variant

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the enum is missing the variant, or the arm
misnames it.

**If the arm misnames it**, write the variant's name:

```jadpo fragment
function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.sent -> "Sent"
        DeliveryState.failed -> "Failed"
    }
}
```

```jadpo check project=e-match-variant-fix-1 hidden
type DeliveryState = Enum {
    pending
    sent
    failed
}

type Label = Text {}

function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.sent -> "Sent"
        DeliveryState.failed -> "Failed"
    }
}
```

A sent delivery is described as "Sent".

**If the enum is missing it**, add the variant, and an arm for it to
every `match` on the enum:

```jadpo fragment
type DeliveryState = Enum {
    pending
    sent
    failed
    returned
}

function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.sent -> "Sent"
        DeliveryState.failed -> "Failed"
        DeliveryState.returned -> "Returned"
    }
}
```

```jadpo check project=e-match-variant-fix-2 hidden
type DeliveryState = Enum {
    pending
    sent
    failed
    returned
}

type Label = Text {}

function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.sent -> "Sent"
        DeliveryState.failed -> "Failed"
        DeliveryState.returned -> "Returned"
    }
}
```

A `DeliveryState` can now be `returned`, and every other `match` on it
fails to compile until it handles that case too.

## Related

- [Match](../language/03-logic.md#match)
- [Enums](../language/02-values-and-types.md#enums)

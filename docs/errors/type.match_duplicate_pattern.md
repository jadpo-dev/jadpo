# `type.match_duplicate_pattern`

A `match` arm matches a case that an arm above it already matches.

## Why Jadpo refuses it

Arms are tried from the top, so the first arm for a case takes every
value of it, and a second arm for the same case never runs. Two arms
usually mean a slip: a second case misnamed, or an arm copied and not
changed. Jadpo won't drop either, because it can't tell which of the two
you meant to keep.

## What it looks like

main.jadpo:

```jadpo check project=e-match-duplicate expect=type.match_duplicate_pattern
type DeliveryState = Enum {
    pending
    sent
    failed
}

type Label = Text {}

function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.pending -> "Waiting"
        DeliveryState.sent -> "Sent"
        DeliveryState.failed -> "Failed"
    }
}
```

```text output
error: `DeliveryState.pending` is matched twice
 --> main.jadpo:12:9
   |
11 |         DeliveryState.pending -> "Pending"
   |         --------------------- first matched here
12 |         DeliveryState.pending -> "Waiting"
   |         ^^^^^^^^^^^^^^^^^^^^^ matched again
  reason: Each case is matched once; the first arm takes every value this one would, so this arm never runs.
  next: Remove this arm, or merge its body into the first (agent)
  rule: type.match_duplicate_pattern
  help: https://jadpo.dev/docs/errors/type.match_duplicate_pattern

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If one of the two arms is wanted**, remove the other:

```jadpo fragment
function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Waiting"
        DeliveryState.sent -> "Sent"
        DeliveryState.failed -> "Failed"
    }
}
```

```jadpo check project=e-match-duplicate-fix hidden
type DeliveryState = Enum {
    pending
    sent
    failed
}

type Label = Text {}

function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Waiting"
        DeliveryState.sent -> "Sent"
        DeliveryState.failed -> "Failed"
    }
}
```

A pending delivery is now described as "Waiting", the wording of the arm
that was kept.

## Related

- [Match](../language/03-logic.md#match)

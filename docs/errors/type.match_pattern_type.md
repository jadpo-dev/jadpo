# `type.match_pattern_type`

A `match` arm's pattern is for a different kind of value from the one matched, such as `true` in a `match` on an enum.

## Why Jadpo refuses it

Each kind of value has its own patterns: an enum's variants, `true` and
`false` for a `Bool`, `some(...)` and `none` for a value that may be
missing, and literals for text and numbers. A pattern of another kind
can never match, so the arm would never run. It usually means the wrong
value is matched, or an arm was copied from another `match`.

## What it looks like

main.jadpo:

```jadpo check project=e-match-type expect=type.match_pattern_type
type DeliveryState = Enum {
    pending
    sent
    failed
}

type Label = Text {}

function describe(state: DeliveryState) -> Label {
    return match state {
        true -> "Yes"
        DeliveryState.pending -> "Pending"
        DeliveryState.sent -> "Sent"
        DeliveryState.failed -> "Failed"
    }
}
```

```text output
error: A `Bool` can't match a `DeliveryState`
 --> main.jadpo:11:9
   |
11 |         true -> "Yes"
   |         ^^^^ wrong kind of pattern
  reason: A `DeliveryState` is matched by its variants, such as `DeliveryState.pending`.
  next: Write a pattern for a `DeliveryState` (agent)
  rule: type.match_pattern_type
  help: https://jadpo.dev/docs/errors/type.match_pattern_type

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the arm doesn't belong to this `match`**, remove it, so every arm is
one of the value's own cases:

```jadpo fragment
function describe(state: DeliveryState) -> Label {
    return match state {
        DeliveryState.pending -> "Pending"
        DeliveryState.sent -> "Sent"
        DeliveryState.failed -> "Failed"
    }
}
```

```jadpo check project=e-match-type-fix hidden
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

`describe` has one arm for each variant of `DeliveryState`.

## Related

- [Match](../language/03-logic.md#match)
- [Enums](../language/02-values-and-types.md#enums)

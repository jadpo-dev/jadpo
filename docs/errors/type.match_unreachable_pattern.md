# `type.match_unreachable_pattern`

A `match` arm comes after the `_` arm, which has already matched every value it could.

## Why Jadpo refuses it

Arms are tried from the top, and `_` matches every value the arms above
it don't. So an arm below `_` never runs, though it reads as if it
handles its case. It is usually an arm added at the end of the list
without noticing the `_` above it.

## What it looks like

main.jadpo:

```jadpo check project=e-match-unreachable expect=type.match_unreachable_pattern
type Label = Text {}

function shout(label: Label) -> Label {
    return match label {
        "" -> "unnamed"
        _ -> label
        "quiet" -> "QUIET"
    }
}
```

```text output
error: This arm never runs
 --> main.jadpo:7:9
  |
6 |         _ -> label
  |         - matches everything left
7 |         "quiet" -> "QUIET"
  |         ^^^^^^^ after `_`
  reason: `_` matches every value the arms above it don't, so no arm after it runs.
  next: Move this arm above `_` (agent)
  rule: type.match_unreachable_pattern
  help: https://jadpo.dev/docs/errors/type.match_unreachable_pattern

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the arm's case should be handled on its own.

**If it should**, move the arm above `_`:

```jadpo fragment
function shout(label: Label) -> Label {
    return match label {
        "" -> "unnamed"
        "quiet" -> "QUIET"
        _ -> label
    }
}
```

```jadpo check project=e-match-unreachable-fix-1 hidden
type Label = Text {}

function shout(label: Label) -> Label {
    return match label {
        "" -> "unnamed"
        "quiet" -> "QUIET"
        _ -> label
    }
}
```

`"quiet"` is now shouted as `"QUIET"`, and every other label is given
back as it is.

**If `_` already gives what the case needs**, remove the arm:

```jadpo fragment
function shout(label: Label) -> Label {
    return match label {
        "" -> "unnamed"
        _ -> label
    }
}
```

```jadpo check project=e-match-unreachable-fix-2 hidden
type Label = Text {}

function shout(label: Label) -> Label {
    return match label {
        "" -> "unnamed"
        _ -> label
    }
}
```

`shout` behaves as it did, without an arm that never ran.

## Related

- [Match](../language/03-logic.md#match)

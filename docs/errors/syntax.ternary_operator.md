# `syntax.ternary_operator`

A value is chosen with `? :` instead of a `match`.

## Why Jadpo refuses it

`condition ? a : b` chooses a value in JavaScript, Java, C#, Kotlin and
Swift. Jadpo chooses between two values with a `match` on the condition, the
form it uses for every choice of a value, so there is no second one to learn
or to nest.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-ternary expect=syntax.ternary_operator
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    return count > 10 ? "large" : "small"
}
```

```text output
error: Jadpo chooses a value with `match`, not `? :`
 --> main.jadpo:8:23
  |
8 |     return count > 10 ? "large" : "small"
  |                       ^ write a `match`
  reason: `condition ? a : b` chooses a value in JavaScript, Java, C#, Kotlin and Swift; Jadpo chooses between two values with a `match` on the condition.
  next: Write `match (count > 10)` with the arms `true -> "large"` and `false -> "small"` (agent)
  rule: syntax.ternary_operator
  help: https://jadpo.dev/docs/errors/syntax.ternary_operator

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you were choosing between two values**, write the `match` the error
names:

```jadpo fragment
function size(count: Count) -> Size {
    return match (count > 10) {
        true -> "large"
        false -> "small"
    }
}
```

```jadpo check project=e-habit-ternary-fix hidden
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    return match (count > 10) {
        true -> "large"
        false -> "small"
    }
}
```

`size` now gives `"large"` above 10 and `"small"` otherwise.

## Related

- [Match](../language/03-logic.md#match)
- [Habits from other languages](../language/03-logic.md#habits-from-other-languages)

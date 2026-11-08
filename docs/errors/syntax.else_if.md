# `syntax.else_if`

An `if` is continued with `elif`, `elsif` or `elseif` instead of `else if`.

## Why Jadpo refuses it

`elif` continues an `if` in Python, `elsif` in Ruby and `elseif` in PHP and
Lua. Jadpo writes the two words it already has, so a chain of conditions
reads the same in every file.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-elif expect=syntax.else_if
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    if (count > 100) {
        return "large"
    } elif (count > 10) {
        return "medium"
    }
    return "small"
}
```

```text output
error: Jadpo writes `else if`, not `elif`
 --> main.jadpo:10:7
   |
10 |     } elif (count > 10) {
   |       ^^^^ write `else if`
  reason: `elif` continues an `if` in Python, `elsif` in Ruby and `elseif` in PHP and Lua; Jadpo writes `else if`.
  next: Use `else if` instead of `elif` (agent)
  rule: syntax.else_if
  help: https://jadpo.dev/docs/errors/syntax.else_if

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant to continue the `if`**, write `else if`:

```jadpo fragment
function size(count: Count) -> Size {
    if (count > 100) {
        return "large"
    } else if (count > 10) {
        return "medium"
    }
    return "small"
}
```

```jadpo check project=e-habit-elif-fix hidden
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    if (count > 100) {
        return "large"
    } else if (count > 10) {
        return "medium"
    }
    return "small"
}
```

`size` now tests `count > 10` only when `count > 100` is false.

## Related

- [Conditions](../language/03-logic.md#conditions)
- [Habits from other languages](../language/03-logic.md#habits-from-other-languages)

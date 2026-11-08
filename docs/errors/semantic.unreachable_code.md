# `semantic.unreachable_code`

Code comes after a statement that always leaves, so it can never run.

## Why Jadpo refuses it

Code that can never run is usually a mistake about the order of things,
and a person reading it would expect it to do something. Jadpo reports it
instead of leaving it for someone to wonder about.

## What it looks like

main.jadpo:

```jadpo check project=e-unreachable expect=semantic.unreachable_code
type Count = Int {
    min: 0
}

type Size = Text {}

function size_of(count: Count) -> Size {
    return "small"
    if (count > 10) {
        return "large"
    }
}
```

```text output
error: This code never runs
 --> main.jadpo:9:5
  |
8 |     return "small"
  |     -------------- leaves on every path
9 |     if (count > 10) {
  |     ^^^^^^^^^^^^^^^^^ never reached
  reason: The statement before it ends in `return` or `reject` on every path, so nothing after it in this block can run.
  next: Remove it, or move it before the statement that leaves (agent)
  rule: semantic.unreachable_code
  help: https://jadpo.dev/docs/errors/semantic.unreachable_code

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the later code should run first**, move it above the `return`:

```jadpo fragment
function size_of(count: Count) -> Size {
    if (count > 10) {
        return "large"
    }
    return "small"
}
```

```jadpo check project=e-unreachable-fix hidden
type Count = Int {
    min: 0
}

type Size = Text {}

function size_of(count: Count) -> Size {
    if (count > 10) {
        return "large"
    }
    return "small"
}
```

The `if` now runs, and a count above ten gives `"large"`.

## Related

- [Statements](../language/03-logic.md#statements)

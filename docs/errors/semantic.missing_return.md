# `semantic.missing_return`

A function, action or query can end without giving its result, or a `return` gives no value.

## Why Jadpo refuses it

A callable that says it returns something has to, on every path through
it. If one path reached the end of the body, the caller would be left with
no value and nothing to say why.

## What it looks like

main.jadpo:

```jadpo check project=e-missing-return expect=semantic.missing_return
type Count = Int {
    min: 0
}

type Size = Text {}

function size_of(count: Count) -> Size {
    if (count > 10) {
        return "large"
    }
}
```

```text output
error: This function can end without returning a `Size`
 --> main.jadpo:11:1
   |
11 | }
   | ^ reached without `return`
  reason: Every path through a function with a result ends in `return` or `reject`; at least one path here reaches the end of the body.
  next: Return a `Size` at the end, or on every branch that reaches it (agent)
  rule: semantic.missing_return
  help: https://jadpo.dev/docs/errors/semantic.missing_return

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A bare `return` in a callable with a result is the same mistake:

```jadpo check project=e-return-no-value expect=semantic.missing_return
type Size = Text {}

function size_label() -> Size {
    return
}
```

```text output
error: This `return` gives no `Size`
 --> main.jadpo:4:5
  |
4 |     return
  |     ^^^^^^ no value
  reason: A bare `return` leaves a callable whose result is `Unit`; this one returns a `Size`.
  next: Write the `Size` to return after `return` (agent)
  rule: semantic.missing_return
  help: https://jadpo.dev/docs/errors/semantic.missing_return

check failed: 1 error, 1 file
```

## How to fix it

**If there's a value for the other paths**, return it:

```jadpo fragment
function size_of(count: Count) -> Size {
    if (count > 10) {
        return "large"
    }
    return "small"
}
```

```jadpo check project=e-missing-return-fix hidden
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

Every path through `size_of` now gives a `Size`.

## Related

- [Functions and actions](../language/03-logic.md#functions-and-actions)
- [Statements](../language/03-logic.md#statements)

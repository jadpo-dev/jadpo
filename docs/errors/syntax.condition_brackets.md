# `syntax.condition_brackets`

A condition isn't wholly inside brackets.

## Why Jadpo refuses it

Conditions are always in brackets, as in `if (total > limit) {`, so the `{`
that opens the block can't be mistaken for part of the condition. The rule
holds every time, so a reader never has to work out where a condition ends.

## What it looks like

main.jadpo:

```jadpo check project=e-condition expect=syntax.condition_brackets
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    if count > 10 {
        return "large"
    }
    return "small"
}
```

```text output
error: This condition needs brackets
 --> main.jadpo:8:8
  |
8 |     if count > 10 {
  |        ^^^^^^^^^^ expected `(` before the condition
  reason: Conditions are always in brackets, `if (total > limit) {`, so the block can't be mistaken for part of the condition.
  next: Write `if (count > 10) {` (agent)
  rule: syntax.condition_brackets
  help: https://jadpo.dev/docs/errors/syntax.condition_brackets

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Brackets round only part of a condition are this error too, and its `next:`
tells you to put them round the whole of it:

main.jadpo:

```jadpo check project=e-condition-2 expect=syntax.condition_brackets
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    if (count > 10) or count == 0 {
        return "large"
    }
    return "small"
}
```

```text output
error: The whole condition goes in brackets
 --> main.jadpo:8:8
  |
8 |     if (count > 10) or count == 0 {
  |        ^^^^^^^^^^^^^^^ only part of the condition is bracketed
  reason: Conditions are always in brackets, `if (total > limit) {`, so the block can't be mistaken for part of the condition.
  next: Put brackets around the whole condition (agent)
  rule: syntax.condition_brackets
  help: https://jadpo.dev/docs/errors/syntax.condition_brackets

check failed: 1 error, 1 file
```

## How to fix it

**If the condition has no brackets**, add them:

```jadpo fragment
function size(count: Count) -> Size {
    if (count > 10) {
        return "large"
    }
    return "small"
}
```

```jadpo check project=e-condition-fix-1 hidden
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    if (count > 10) {
        return "large"
    }
    return "small"
}
```

`size` now returns `"large"` for a count over 10.

**If only part of it is bracketed**, bracket all of it:

```jadpo fragment
function size(count: Count) -> Size {
    if (count > 10 or count == 0) {
        return "large"
    }
    return "small"
}
```

```jadpo check project=e-condition-fix-2 hidden
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    if (count > 10 or count == 0) {
        return "large"
    }
    return "small"
}
```

The whole condition is now inside the brackets.

## Related

- [Conditions](../language/03-logic.md#conditions)

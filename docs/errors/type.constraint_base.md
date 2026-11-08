# `type.constraint_base`

A type gives a constraint that doesn't apply to what it is built on, such as `min` on a `Text` type.

## Why Jadpo refuses it

Each constraint belongs to some representations and not others: `min`
and `max` bound a number, and `min_length` and `max_length` bound the
length of text or a collection. A `Text` type has no number to compare
with `min: 1`, so the line would check nothing. Jadpo won't read it as
the length bound it looks like, because a type's rules say exactly what
a valid value is.

## What it looks like

main.jadpo:

```jadpo check project=e-constraint-base expect=type.constraint_base
type Nickname = Text {
    min: 1
}
```

```text output
error: `min` doesn't apply to a `Text` type
 --> main.jadpo:2:5
  |
2 |     min: 1
  |     ^^^ doesn't fit the base
  reason: `min` constrains `Int`, `Decimal`, `Duration`, `Money`, `Percentage` and quantity types.
  next: Remove `min`, or use a constraint that fits (agent)
  rule: type.constraint_base
  help: https://jadpo.dev/docs/errors/type.constraint_base

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant the text's length**, use the length constraint:

```jadpo check project=e-constraint-base-fix-1
type Nickname = Text {
    min_length: 1
}
```

A `Nickname` now has at least one character, so it is never empty.

**If the type needs no rule there**, remove the line:

```jadpo check project=e-constraint-base-fix-2
type Nickname = Text {}
```

`Nickname` is still its own type, but any text, empty text included, is
a valid `Nickname`.

## Related

- [Constraints](../language/02-values-and-types.md#constraints)
- [Named types](../language/02-values-and-types.md#named-types)

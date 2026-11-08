# `type.constraint_contradiction`

A type's constraints leave no value that follows them all, such as a `max_length` below its `min_length`.

## Why Jadpo refuses it

A type's rules say what a valid value is. When two of them disagree, as
`min_length: 10` and `max_length: 3` do, no value could follow both, so
every construction of the type would fail. Jadpo won't guess which of
the two numbers is the mistake, because each gives a different type.

## What it looks like

main.jadpo:

```jadpo check project=e-contradiction expect=type.constraint_contradiction
type Nickname = Text {
    min_length: 10
    max_length: 3
}
```

```text output
error: `max_length: 3` is below `min_length: 10`
 --> main.jadpo:3:5
  |
2 |     min_length: 10
  |     ---------- the minimum
3 |     max_length: 3
  |     ^^^^^^^^^^ below the minimum
  reason: No value could follow both rules, so the type could never be built.
  next: Correct `min_length` or `max_length` (agent)
  rule: type.constraint_contradiction
  help: https://jadpo.dev/docs/errors/type.constraint_contradiction

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A type that builds on another can contradict what it inherits in the
same way, with a `max` below an inherited `min`, a different `scale` or
a different currency.

## How to fix it

The question is which bound is the one the type means.

**If the minimum is right**, raise the maximum to at least it:

```jadpo check project=e-contradiction-fix-1
type Nickname = Text {
    min_length: 3
    max_length: 10
}
```

A `Nickname` is now 3 to 10 characters long.

**If the maximum is right**, lower the minimum to at most it:

```jadpo check project=e-contradiction-fix-2
type Nickname = Text {
    min_length: 1
    max_length: 3
}
```

A `Nickname` is now 1 to 3 characters long.

## Related

- [Constraints](../language/02-values-and-types.md#constraints)
- [Named types](../language/02-values-and-types.md#named-types)

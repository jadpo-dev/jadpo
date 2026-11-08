# `type.constraint_repeated`

A type gives the same constraint more than once.

## Why Jadpo refuses it

Each constraint is written once, one per line. Two values for the same
constraint, such as `max_length: 20` and `max_length: 30`, leave a
reader to work out which one counts. Jadpo won't pick one, because the
two give different types.

## What it looks like

main.jadpo:

```jadpo check project=e-constraint-repeated expect=type.constraint_repeated
type Nickname = Text {
    max_length: 20
    max_length: 30
}
```

```text output
error: `max_length` is given twice
 --> main.jadpo:3:5
  |
2 |     max_length: 20
  |     ---------- first given here
3 |     max_length: 30
  |     ^^^^^^^^^^ given again
  reason: Each setting is given once, so there is never a question of which one counts.
  next: Remove one `max_length` (agent)
  rule: type.constraint_repeated
  help: https://jadpo.dev/docs/errors/type.constraint_repeated

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If one of the values is the one you mean**, keep it and remove the
other:

```jadpo check project=e-constraint-repeated-fix
type Nickname = Text {
    max_length: 20
}
```

A `Nickname` is now at most 20 characters long.

## Related

- [Constraints](../language/02-values-and-types.md#constraints)
- [Named types](../language/02-values-and-types.md#named-types)

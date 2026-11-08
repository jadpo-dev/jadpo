# `type.logical_operand`

An `and` or an `or` is given a value that isn't a `Bool`, or a `Bool` that may be `none`.

## Why Jadpo refuses it

Jadpo has no truthiness: a number, a text or a list is never read as true
or false. `score and score` in another language might mean "score isn't
zero", "score is set" or something else, and a reviewer can't tell which.
A `Bool` that may be `none` is refused for the same reason, since `none`
is neither answer. Writing the comparison out says which question is
asked.

## What it looks like

main.jadpo:

```jadpo check project=e-logical expect=type.logical_operand
type Score = Int {}

type Flag = Bool {}

function both(score: Score, bonus: Score) -> Flag {
    return Flag(score and bonus)
}
```

```text output
error: `and` needs `Bool` values, and `score` is a `Score`
 --> main.jadpo:6:17
  |
6 |     return Flag(score and bonus)
  |                 ^^^^^ this is `Score`
  reason: `and` and `or` work on `Bool` values and types built on `Bool`; Jadpo has no truthiness.
  next: Compare `score` to get a `Bool` (agent)
  rule: type.logical_operand
  help: https://jadpo.dev/docs/errors/type.logical_operand

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant a question about the values**, write the comparison that
asks it:

```jadpo fragment
function both(score: Score, bonus: Score) -> Flag {
    return Flag(score > 0 and bonus > 0)
}
```

```jadpo check project=e-logical-fix hidden
type Score = Int {}

type Flag = Bool {}

function both(score: Score, bonus: Score) -> Flag {
    return Flag(score > 0 and bonus > 0)
}
```

`both` now says whether both scores are above zero.

## Related

- [Expressions and operators](../language/03-logic.md#expressions-and-operators)
- [Conditions](../language/03-logic.md#conditions)

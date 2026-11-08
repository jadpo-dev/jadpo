# `type.unary_operand`

A `not` or a minus sign is given a value it can't work on, such as `not` on text.

## Why Jadpo refuses it

`not` works on a `Bool` or a type built on one, and a minus sign on a
number. Jadpo has no truthiness, so a value isn't true or false for being
empty or zero: the code has to say what it is asking. A value that may be
`none` is refused too, until a `match` has found it.

## What it looks like

main.jadpo:

```jadpo check project=e-unary expect=type.unary_operand
type Phrase = Text {}

type Flag = Bool {}

function negate(phrase: Phrase) -> Flag {
    return Flag(not phrase)
}
```

```text output
error: `not` needs a `Bool`, and `phrase` is a `Phrase`
 --> main.jadpo:6:21
  |
6 |     return Flag(not phrase)
  |                     ^^^^^^ this is `Phrase`
  reason: `not` works on `Bool` values and types built on `Bool`; Jadpo has no truthiness.
  next: Compare `phrase` to get a `Bool` (agent)
  rule: type.unary_operand
  help: https://jadpo.dev/docs/errors/type.unary_operand

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant to test the value**, compare it to get a `Bool`:

```jadpo fragment
    return Flag(phrase == "")
```

```jadpo check project=e-unary-fix hidden
type Phrase = Text {}

type Flag = Bool {}

function negate(phrase: Phrase) -> Flag {
    return Flag(phrase == "")
}
```

`negate` now says whether the phrase is empty.

## Related

- [Expressions and operators](../language/03-logic.md#expressions-and-operators)
- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)

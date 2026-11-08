# `syntax.equality_operator`

A comparison is written with another language's operator instead of `==` or `!=`.

## Why Jadpo refuses it

`===` and `!==` compare without converting types in JavaScript and
TypeScript, and Jadpo's `==` and `!=` never convert between types, so there
is no stricter form to ask for. SQL's `<>` and `=`, and the tests for a
missing value `IS NULL` and `is None`, are this error too: each has one
Jadpo spelling, written with `==` or `!=`.

## What it looks like

main.jadpo:

```jadpo check project=e-equality expect=syntax.equality_operator
type Count = Int {
    min: 0
}

type Done = Bool {}

function sold_out(count: Count) -> Done {
    return Done(count === 0)
}
```

```text output
error: Jadpo compares with `==`, not `===`
 --> main.jadpo:8:23
  |
8 |     return Done(count === 0)
  |                       ^^^ write `==`
  reason: `===` and `!==` compare without converting types in JavaScript and TypeScript; Jadpo's `==` and `!=` never convert between types, so there is no stricter form to ask for.
  next: Use `==` instead of `===` (agent)
  rule: syntax.equality_operator
  help: https://jadpo.dev/docs/errors/syntax.equality_operator

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A test for a missing value is this error too, and its `next:` writes the
comparison with `none`:

main.jadpo:

```jadpo check project=e-equality-2 expect=syntax.equality_operator
type Note = Text {}

type Blank = Bool {}

function blank(note: Note?) -> Blank {
    return Blank(note IS NULL)
}
```

```text output
error: Jadpo compares with `== none`, not `IS NULL`
 --> main.jadpo:6:23
  |
6 |     return Blank(note IS NULL)
  |                       ^^^^^^^ write `== none`
  reason: `IS NULL` and `IS NOT NULL` test for a missing value in SQL; Jadpo compares with `none` as with any other value.
  next: Use `== none` instead of `IS NULL` (agent)
  rule: syntax.equality_operator
  help: https://jadpo.dev/docs/errors/syntax.equality_operator

check failed: 1 error, 1 file
```

## How to fix it

**If you meant to compare two values**, write `==`:

```jadpo fragment
function sold_out(count: Count) -> Done {
    return Done(count == 0)
}
```

```jadpo check project=e-equality-fix-1 hidden
type Count = Int {
    min: 0
}

type Done = Bool {}

function sold_out(count: Count) -> Done {
    return Done(count == 0)
}
```

`sold_out` now says whether the count is exactly zero.

**If you meant to test for a missing value**, compare with `none`:

```jadpo fragment
function blank(note: Note?) -> Blank {
    return Blank(note == none)
}
```

```jadpo check project=e-equality-fix-2 hidden
type Note = Text {}

type Blank = Bool {}

function blank(note: Note?) -> Blank {
    return Blank(note == none)
}
```

`blank` now says whether there is no note.

## Related

- [Expressions and operators](../language/03-logic.md#expressions-and-operators)
- [Habits from other languages](../language/03-logic.md#habits-from-other-languages)

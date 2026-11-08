# `syntax.colon_required`

A member inside braces is written without the colon between its name and its value.

## Why Jadpo refuses it

Every member inside braces is `name: value`, with a colon even when the
value is a block. One form means a reader can always tell where a member's
name ends and its value begins, and Jadpo has no shorthand that repeats a
name.

## What it looks like

main.jadpo:

```jadpo check project=e-colon expect=syntax.colon_required
type ContactEmail = Text {
    format TextFormat.email
}
```

```text output
error: `format` needs a colon before its value
 --> main.jadpo:2:5
  |
2 |     format TextFormat.email
  |     ^^^^^^ expected `:` after `format`
  reason: Every member inside braces is `name: value`, with a colon even when the value is a block.
  next: Write `format: TextFormat.email` (agent)
  rule: syntax.colon_required
  help: https://jadpo.dev/docs/errors/syntax.colon_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A member written with `=`, as C# and Kotlin set a field, is this error too:

main.jadpo:

```jadpo check project=e-colon-2 expect=syntax.colon_required
type Count = Int {
    min = 0
}
```

```text output
error: Jadpo writes `min:`, not `min =`
 --> main.jadpo:2:9
  |
2 |     min = 0
  |         ^ write `:`
  reason: `name = value` sets a field in C# and Kotlin and a case's value in TypeScript; every member inside braces is `name: value`.
  next: Use `:` instead of `=` (agent)
  rule: syntax.colon_required
  help: https://jadpo.dev/docs/errors/syntax.colon_required

check failed: 1 error, 1 file
```

A field of a record written as its name alone is this error too, because
Jadpo has no shorthand that repeats the name:

main.jadpo:

```jadpo check project=e-colon-3 expect=syntax.colon_required
type Count = Int {
    min: 0
}

type Pair = Object {
    left: Count
}

function paired(left: Count) -> Pair {
    return Pair {
        left
    }
}
```

```text output
error: `left` needs a value
 --> main.jadpo:11:9
   |
11 |         left
   |         ^^^^ expected `name: value`
  reason: Every field in braces is written `name: value`; Jadpo has no shorthand that repeats the name.
  next: Write `left: left` (agent)
  rule: syntax.colon_required
  help: https://jadpo.dev/docs/errors/syntax.colon_required

check failed: 1 error, 1 file
```

## How to fix it

**If you left out the colon**, add it:

```jadpo check project=e-colon-fix-1
type ContactEmail = Text {
    format: TextFormat.email
}
```

`ContactEmail` now holds text in the email format.

**If you wrote `=`**, replace it with a colon:

```jadpo check project=e-colon-fix-2
type Count = Int {
    min: 0
}
```

`Count` now has a minimum of `0`.

**If a field repeats a name**, write the name twice:

```jadpo fragment
    return Pair {
        left: left
    }
```

```jadpo check project=e-colon-fix-3 hidden
type Count = Int {
    min: 0
}

type Pair = Object {
    left: Count
}

function paired(left: Count) -> Pair {
    return Pair {
        left: left
    }
}
```

The record's `left` now holds the parameter `left`.

## Related

- [Objects](../language/02-values-and-types.md#objects)
- [Statements](../language/03-logic.md#statements)

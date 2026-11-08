# `syntax.logical_operator`

A logical operator is written in another language's form instead of `and`, `or` or `not`.

## Why Jadpo refuses it

`&&`, `||` and `!` are the logical operators of JavaScript, Rust, Go, Java,
C#, Kotlin and Swift, and SQL writes `AND`, `OR` and `NOT` in capitals.
Jadpo writes them as the words `and`, `or` and `not`, so a condition reads
as the sentence it stands for.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-logic expect=syntax.logical_operator
type Open = Bool {}

type Paid = Bool {}

type Done = Bool {}

function done(open: Open, paid: Paid) -> Done {
    return Done(open && paid)
}
```

```text output
error: Jadpo writes `and`, not `&&`
 --> main.jadpo:8:22
  |
8 |     return Done(open && paid)
  |                      ^^ write `and`
  reason: `&&`, `||` and `!` are the logical operators of JavaScript, Rust, Go, Java, C#, Kotlin and Swift; Jadpo writes them as words: `and`, `or` and `not`.
  next: Use `and` instead of `&&` (agent)
  rule: syntax.logical_operator
  help: https://jadpo.dev/docs/errors/syntax.logical_operator

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant to combine the conditions**, use `and`:

```jadpo fragment
function done(open: Open, paid: Paid) -> Done {
    return Done(open and paid)
}
```

```jadpo check project=e-habit-logic-fix hidden
type Open = Bool {}

type Paid = Bool {}

type Done = Bool {}

function done(open: Open, paid: Paid) -> Done {
    return Done(open and paid)
}
```

`done` now says whether both `open` and `paid` are true.

## Related

- [Expressions and operators](../language/03-logic.md#expressions-and-operators)
- [Conditions](../language/03-logic.md#conditions)

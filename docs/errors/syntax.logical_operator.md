# `syntax.logical_operator`

Logical operators are the words `and`, `or` and `not`, for `&&`, `||` and `!`, and SQL's `AND`, `OR` and `NOT`.

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
  help: docs/language/03-logic.html#syntax-logical-operator

check failed: 1 error, 1 file
```

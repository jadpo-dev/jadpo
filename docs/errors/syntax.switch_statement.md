# `syntax.switch_statement`

A choice between cases is a `match`, with arms `pattern -> result`: `switch` and `when` are `match`, `case` goes, and `default` or `else` is `_`.

```jadpo check project=e-habit-switch expect=syntax.switch_statement
type TicketStatus = Enum {
    open
    closed
}

type Rank = Int {
    min: 1
}

function rank(status: TicketStatus) -> Rank {
    return switch (status) {
        TicketStatus.open -> 1
        TicketStatus.closed -> 2
    }
}
```

```text output
error: Jadpo writes `match`, not `switch`
 --> main.jadpo:11:12
   |
11 |     return switch (status) {
   |            ^^^^^^ write `match`
  reason: `switch` chooses between cases in JavaScript, Java, C#, Go and Swift, and `when` in Kotlin; Jadpo's `match` does, with one arm per line, `pattern -> result`, covering every case.
  next: Use `match` instead of `switch` (agent)
  rule: syntax.switch_statement
  help: docs/errors/syntax.switch_statement.md

check failed: 1 error, 1 file
```

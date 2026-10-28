# `syntax.ternary_operator`

A choice between two values is a `match` on the condition; the edit writes it.

```jadpo check project=e-habit-ternary expect=syntax.ternary_operator
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    return count > 10 ? "large" : "small"
}
```

```text output
error: Jadpo chooses a value with `match`, not `? :`
 --> main.jadpo:8:23
  |
8 |     return count > 10 ? "large" : "small"
  |                       ^ write a `match`
  reason: `condition ? a : b` chooses a value in JavaScript, Java, C#, Kotlin and Swift; Jadpo chooses between two values with a `match` on the condition.
  next: Write `match (count > 10)` with the arms `true -> "large"` and `false -> "small"` (agent)
  rule: syntax.ternary_operator
  help: docs/errors/syntax.ternary_operator.md

check failed: 1 error, 1 file
```

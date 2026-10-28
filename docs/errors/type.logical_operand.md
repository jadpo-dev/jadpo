# `type.logical_operand`

```jadpo check project=e-logical expect=type.logical_operand
type Score = Int {}

type Flag = Bool {}

function both(score: Score) -> Flag {
    return Flag(score and score)
}
```

```text output
error: `and` needs `Bool` values, and `score` is a `Score`
 --> main.jadpo:6:17
  |
6 |     return Flag(score and score)
  |                 ^^^^^ this is `Score`
  reason: `and` and `or` work on `Bool` values and types built on `Bool`; Jadpo has no truthiness.
  next: Compare `score` to get a `Bool` (agent)
  rule: type.logical_operand
  help: docs/language/03-logic.html#type-logical-operand

check failed: 1 error, 1 file
```

# `type.constraint_repeated`

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
  help: docs/language/02-values-and-types.html#type-constraint-repeated

check failed: 1 error, 1 file
```

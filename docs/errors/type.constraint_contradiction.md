# `type.constraint_contradiction`

```jadpo check project=e-contradiction expect=type.constraint_contradiction
type Nickname = Text {
    min_length: 10
    max_length: 3
}
```

```text output
error: `max_length: 3` is below `min_length: 10`
 --> main.jadpo:3:5
  |
2 |     min_length: 10
  |     ---------- the minimum
3 |     max_length: 3
  |     ^^^^^^^^^^ below the minimum
  reason: No value could follow both rules, so the type could never be built.
  next: Correct `min_length` or `max_length` (agent)
  rule: type.constraint_contradiction
  help: docs/language/02-values-and-types.html#type-constraint-contradiction

check failed: 1 error, 1 file
```

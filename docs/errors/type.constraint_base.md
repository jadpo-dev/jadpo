# `type.constraint_base`

```jadpo check project=e-constraint-base expect=type.constraint_base
type Nickname = Text {
    min: 1
}
```

```text output
error: `min` doesn't apply to a `Text` type
 --> main.jadpo:2:5
  |
2 |     min: 1
  |     ^^^ doesn't fit the base
  reason: `min` constrains `Int`, `Decimal`, `Duration`, `Money`, `Percentage` and quantity types.
  next: Remove `min`, or use a constraint that fits (agent)
  rule: type.constraint_base
  help: docs/errors/type.constraint_base.md

check failed: 1 error, 1 file
```

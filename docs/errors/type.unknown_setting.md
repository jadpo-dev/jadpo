# `type.unknown_setting`

A type's block holds only the constraints in the table above:

```jadpo check project=e-unknown-setting expect=type.unknown_setting
type ProductName = Text {
    min_length: 1
    maxlength: 120
}
```

```text output
error: `maxlength` isn't a setting Jadpo knows
 --> main.jadpo:3:5
  |
3 |     maxlength: 120
  |     ^^^^^^^^^ unknown setting
  reason: A type's block holds the constraints `min`, `max`, `min_length`, `max_length`, `pattern`, `format`, `scale`, `version`, `currency`, `rounding`, `from`, `to`, `dimensions` and `distance`. Did you mean `max_length`?
  next: Use `max_length` instead of `maxlength` (agent)
  rule: type.unknown_setting
  help: docs/errors/type.unknown_setting.md

check failed: 1 error, 1 file
```

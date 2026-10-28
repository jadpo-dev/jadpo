# `type.setting_value`

A setting with a fixed set of options takes an enum value, `Type.value`; quantities take numbers of the right kind:

```jadpo check project=e-setting-value expect=type.setting_value
type ContactEmail = Text {
    format: email
}
```

```text output
error: `format` takes a `TextFormat` value, such as `TextFormat.email`
 --> main.jadpo:2:13
  |
2 |     format: email
  |             ^^^^^ a name here
  reason: `format` is a constraint, and its value must be a `TextFormat` value, such as `TextFormat.email`.
  next: Write `TextFormat.email` (agent)
  rule: type.setting_value
  help: docs/language/02-values-and-types.html#type-setting-value

check failed: 1 error, 1 file
```

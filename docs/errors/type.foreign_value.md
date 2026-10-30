# `type.foreign_value`

There is no `null`, `nil`, `undefined` or `None`: a missing value is `none`, and Python's `True` and `False` are `true` and `false`.

```jadpo check project=e-habit-foreign-value expect=type.foreign_value
type ProductName = Text {
    min_length: 1
}

function no_subtitle() -> ProductName? {
    return null
}
```

```text output
error: Jadpo writes `none`, not `null`
 --> main.jadpo:6:12
  |
6 |     return null
  |            ^^^^ write `none`
  reason: `null` is a missing value in JavaScript, Java, C# and Kotlin; Jadpo writes it `none`.
  next: Use `none` instead of `null` (agent)
  rule: type.foreign_value
  help: https://jadpo.dev/docs/errors/type.foreign_value

check failed: 1 error, 1 file
```

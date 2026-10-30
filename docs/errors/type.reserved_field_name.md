# `type.reserved_field_name`

```jadpo check project=e-reserved expect=type.reserved_field_name
type Flag = Bool {}

type Settings = Object {
    optional: Flag
}
```

```text output
error: `optional` can't be a field's name
 --> main.jadpo:4:5
  |
4 |     optional: Flag
  |     ^^^^^^^^ reserved
  reason: `include`, `pick`, `omit` and `optional` are lines that build a type's fields, so no field may take their names.
  next: Rename the field (agent)
  rule: type.reserved_field_name
  help: https://jadpo.dev/docs/errors/type.reserved_field_name

check failed: 1 error, 1 file
```

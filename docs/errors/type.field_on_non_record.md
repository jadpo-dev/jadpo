# `type.field_on_non_record`

```jadpo check project=e-field-scalar expect=type.field_on_non_record
type ProductName = Text {
    max_length: 120
}

function raw(name: ProductName) -> ProductName {
    return name.value
}
```

```text output
error: `ProductName` has no fields
 --> main.jadpo:6:17
  |
6 |     return name.value
  |                 ^^^^^ no such field
  reason: `name` is a `ProductName`, built on `Text`; only objects, inputs, outputs, events, failures and entities have fields.
  next: Use `name` itself (agent)
  rule: type.field_on_non_record
  help: https://jadpo.dev/docs/errors/type.field_on_non_record

check failed: 1 error, 1 file
```

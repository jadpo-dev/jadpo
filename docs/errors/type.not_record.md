# `type.not_record`

```jadpo check project=e-not-record expect=type.not_record
type ProductName = Text {
    max_length: 120
}

function blank() -> ProductName {
    return ProductName {}
}
```

```text output
error: `ProductName` isn't built with `{ ... }`
 --> main.jadpo:6:12
  |
6 |     return ProductName {}
  |            ^^^^^^^^^^^^^^ has no fields
  reason: Braces build objects and variants with fields; `ProductName`, built on `Text`, takes one value in brackets.
  next: Write `ProductName(...)` with one value (agent)
  rule: type.not_record
  help: docs/language/02-values-and-types.html#type-not-record

check failed: 1 error, 1 file
```

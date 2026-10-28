# `type.missing_field`

```jadpo check project=e-missing-field expect=type.missing_field
type Receipt = Output {
    email: Email
}

function incomplete_receipt() -> Receipt {
    return Receipt {}
}
```

```text output
error: `Receipt` is missing `email`
 --> main.jadpo:6:12
  |
6 |     return Receipt {}
  |            ^^^^^^^ fields missing
  reason: Every field of `Receipt` is given when a value is built; a field that may be absent is declared `T?` and given as `none`.
  next: Add `email: ...` to the block (agent)
  rule: type.missing_field
  help: docs/errors/type.missing_field.md

check failed: 1 error, 1 file
```

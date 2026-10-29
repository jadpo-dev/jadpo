# `syntax.nullable_type`

A type that may hold `none` is written with `?`.

```jadpo check project=e-habit-nullable expect=syntax.nullable_type
type ProductName = Text {
    min_length: 1
}

type Listing = Object {
    name: ProductName
    subtitle: ProductName | null
}
```

```text output
error: Jadpo writes `ProductName?`, not `ProductName | null`
 --> main.jadpo:7:27
  |
7 |     subtitle: ProductName | null
  |                           ^^^^^^ write `ProductName?`
  reason: `T | null` and `T | undefined` are types that may be missing in TypeScript, and `T | None` and `Optional[T]` in Python; Jadpo writes `T?`, which holds a `T` or `none`.
  next: Write `ProductName?` (agent)
  rule: syntax.nullable_type
  help: docs/errors/syntax.nullable_type.md

check failed: 1 error, 1 file
```

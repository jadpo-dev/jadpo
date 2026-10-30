# `syntax.type_keyword`

Every type is `type Name = Base { ... }`: `struct`, `interface` and `enum` are other languages' words for `Object` and `Enum`.

```jadpo check project=e-habit-struct expect=syntax.type_keyword
type Count = Int {
    min: 0
}

struct GridCell {
    across: Count
    down: Count
}
```

```text output
error: Jadpo writes `type GridCell = Object`, not `struct GridCell`
 --> main.jadpo:5:1
  |
5 | struct GridCell {
  | ^^^^^^^^^^^^^^^ write `type GridCell = Object`
  reason: `struct` declares a record in Rust, Go, Swift and C#, `interface` an object's shape in TypeScript, and `enum` a list of cases in most languages; every Jadpo type is `type Name = Base { ... }`, with `Object` or `Enum` as the base.
  next: Write `type GridCell = Object` (agent)
  rule: syntax.type_keyword
  help: https://jadpo.dev/docs/errors/syntax.type_keyword

check failed: 1 error, 1 file
```

# `type.foreign_type`

A type named as another language names it, which the project doesn't declare: `string`, `int`, `bool`, `void`, `Vec`, `HashMap`, `Option<T>` and the rest of the table in `jadpo_syntax::habits`.

```jadpo check project=e-habit-foreign-type expect=type.foreign_type
type Listing = Object {
    name: string
}
```

```text output
error: Jadpo writes `Text`, not `string`
 --> main.jadpo:2:11
  |
2 |     name: string
  |           ^^^^^^ write `Text`
  reason: `string` is text in TypeScript, C# and Go; Jadpo calls it `Text`.
  next: Use `Text` instead of `string` (agent)
  rule: type.foreign_type
  help: https://jadpo.dev/docs/errors/type.foreign_type

check failed: 1 error, 1 file
```

# `type.primitive_signature`

```jadpo check project=e-primitive expect=type.primitive_signature
type Title = Text {
    max_length: 200
}

function normalise(value: Text) -> Title {
    return Title(value)
}
```

```text output
error: This parameter uses `Text` directly
 --> main.jadpo:5:27
  |
5 | function normalise(value: Text) -> Title {
  |                           ^^^^ a built-in type
  reason: Functions, actions and queries take and return named types, so each value says what it means and carries its rules; `Text` says neither.
  next: Declare a named type built on `Text` and use it here (agent)
  rule: type.primitive_signature
  help: docs/errors/type.primitive_signature.md

check failed: 1 error, 1 file
```

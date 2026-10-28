# `type.optional_outside_input`

```jadpo check project=e-optional expect=type.optional_outside_input
type Nickname = Text {
    max_length: 30
}

type Profile = Object {
    nickname: Nickname {
        optional: true
    }
}
```

```text output
error: `Profile` isn't an `Input`, so its fields are always given
 --> main.jadpo:7:9
  |
7 |         optional: true
  |         ^^^^^^^^ only for an `Input`
  reason: Only a request arriving from outside may leave fields out or have them filled by default; an object built inside the application states every field, using `none` for a field declared `T?`.
  next: Remove the setting, or make the field's type `T?` if it may be absent (agent)
  rule: type.optional_outside_input
  help: docs/errors/type.optional_outside_input.md

check failed: 1 error, 1 file
```

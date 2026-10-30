# `type.base_outside_declaration`

There is no unnamed object: `Object`, `Enum`, `Input` and `Output` appear only after `=` (question 19):

```jadpo check project=e-base expect=type.base_outside_declaration
type Basket = Object {
    details: Object
}
```

```text output
error: `Object` is used as a type
 --> main.jadpo:2:14
  |
2 |     details: Object
  |              ^^^^^^ only a base
  reason: `Object` is written only after `=` in a `type` declaration; every shape has a name, so a field or signature names that type.
  next: Declare `type Name = Object { ... }` and use its name here (agent)
  rule: type.base_outside_declaration
  help: https://jadpo.dev/docs/errors/type.base_outside_declaration

check failed: 1 error, 1 file
```

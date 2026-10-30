# `type.unknown_value`

```jadpo check project=e-unknown-value expect=type.unknown_value
type Name = Text {}

function echo(name: Name) -> Name {
    return nmae
}
```

```text output
error: `nmae` isn't defined in this scope
 --> main.jadpo:4:12
  |
4 |     return nmae
  |            ^^^^ not defined
  reason: Jadpo could not find this name. Did you mean `name`?
  next: Use `name` instead of `nmae` (agent)
  rule: type.unknown_value
  help: https://jadpo.dev/docs/errors/type.unknown_value

check failed: 1 error, 1 file
```

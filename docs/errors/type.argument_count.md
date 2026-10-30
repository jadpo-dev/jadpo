# `type.argument_count`

```jadpo check project=e-argument-count expect=type.argument_count
type Name = Text {}

function choose(first: Name, second: Name) -> Name {
    return first
}

function incomplete(name: Name) -> Name {
    return choose(name)
}
```

```text output
error: `choose` takes 2 values, not 1
 --> main.jadpo:8:18
  |
8 |     return choose(name)
  |                  ^^^^^^ 1 given
  reason: `choose(first: Name, second: Name)` has 2 parameters; each is given, in order, and none has a default.
  next: Pass `second` as well (agent)
  rule: type.argument_count
  help: https://jadpo.dev/docs/errors/type.argument_count

check failed: 1 error, 1 file
```

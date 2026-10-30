# `type.copy_cycle`

An `include` that leads back to its own type would copy its fields from
themselves:

```jadpo check project=e-copy-cycle expect=type.copy_cycle
type Title = Text {}

type TodoView = Output {
    include: [ListView]
    title: Title
}

type ListView = Output {
    include: [TodoView]
}
```

```text output
error: `ListView` includes its own fields
 --> main.jadpo:9:15
  |
9 |     include: [TodoView]
  |               ^^^^^^^^ leads back here
  reason: `TodoView` leads back to `ListView` through its own `include`, so its fields would be copied from themselves and the type could never be written out.
  next: Remove `TodoView` from `include`, or write the fields it needs (agent)
  rule: type.copy_cycle
  help: https://jadpo.dev/docs/errors/type.copy_cycle

check failed: 1 error, 1 file
```

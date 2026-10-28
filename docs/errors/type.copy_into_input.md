# `type.copy_into_input`

```jadpo check project=e-copy-input file=names.jadpo hidden
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
}
```

```jadpo check project=e-copy-input file=main.jadpo expect=type.copy_into_input
type CreateTodo = Input {
    include: [Todo]
}
```

```text output
error: `CreateTodo` is an `Input` and can't use `include`
 --> main.jadpo:2:5
  |
2 |     include: [Todo]
  |     ^^^^^^^ copies every field
  reason: `include` would also accept every field added to the source type later, so a request could set fields nobody chose to expose.
  next: Name the fields this input accepts with `pick: [...]` (human)
  rule: type.copy_into_input
  help: docs/errors/type.copy_into_input.md

check failed: 1 error, 2 files
```

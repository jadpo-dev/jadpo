# `type.absent_field`

```jadpo check project=e-absent file=names.jadpo hidden
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
}
```

```jadpo check project=e-absent file=main.jadpo expect=type.absent_field
type TodoView = Output {
    pick: [Todo.id, Todo.title]
    omit: [Todo.notes]
}
```

```text output
error: `TodoView` has no field `notes` at this point
 --> main.jadpo:3:12
  |
3 |     omit: [Todo.notes]
  |            ^^^^^^^^^^ not present
  reason: `omit` names a field the lines above bring in, so a typo or a field removed since is caught; none of them brings in `notes`.
  next: Remove `Todo.notes` from the list (agent)
  rule: type.absent_field
  help: https://jadpo.dev/docs/errors/type.absent_field

check failed: 1 error, 2 files
```

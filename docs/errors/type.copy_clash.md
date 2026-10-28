# `type.copy_clash`

```jadpo check project=e-copy-clash file=names.jadpo hidden
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
}

entity Note {
    id: Uuid
    title: TodoTitle
}
```

```jadpo check project=e-copy-clash file=main.jadpo expect=type.copy_clash
type TodoView = Output {
    pick: [Todo.id, Todo.title, Note.title]
}
```

```text output
error: `title` is already in `TodoView`, from `Todo`
 --> main.jadpo:2:33
  |
2 |     pick: [Todo.id, Todo.title, Note.title]
  |                                 ^^^^^^^^^^ copied again
  reason: A copied field never replaces another field, because a field added to a source type would then change this type unseen.
  next: Remove `title` from one of them with `omit: [...]`, or write `title` as a field below them (agent)
  rule: type.copy_clash
  help: docs/language/02-values-and-types.html#type-copy-clash

check failed: 1 error, 2 files
```

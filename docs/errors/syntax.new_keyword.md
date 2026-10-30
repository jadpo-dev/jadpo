# `syntax.new_keyword`

A value is made with `Type(value)`, or an object with `Type { field: value }`, without `new`.

```jadpo check project=e-habit-new expect=syntax.new_keyword
type CustomerNote = Text {
    max_length: 500
}

type NoteWasEdited = Bool {}

function edited(before: CustomerNote, after: CustomerNote) -> NoteWasEdited {
    return new NoteWasEdited(before != after)
}
```

```text output
error: Jadpo creates a value without `new`
 --> main.jadpo:8:12
  |
8 |     return new NoteWasEdited(before != after)
  |            ^^^ remove `new`
  reason: `new` creates an object in JavaScript, TypeScript, Java and C#; Jadpo creates a value with `Type(value)`, or an object with `Type { field: value }`.
  next: Remove `new` (agent)
  rule: syntax.new_keyword
  help: https://jadpo.dev/docs/errors/syntax.new_keyword

check failed: 1 error, 1 file
```

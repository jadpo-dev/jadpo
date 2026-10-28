# `data.set_generated_field`

```jadpo check project=e-set-generated-field expect=data.set_generated_field
type Title = Text {
    max_length: 80
}

entity Note {
    id: Uuid
    title: Title
    written_at: Instant {
        generated: Generated.on_create
    }
}

action jot_down(title: Note.title, at: Note.written_at) -> Note {
    return Note.create {
        set: {
            title: title
            written_at: at
        }
    }
}
```

```text output
error: `Note.create` sets `written_at`, which the store generates
 --> main.jadpo:17:13
   |
17 |             written_at: at
   |             ^^^^^^^^^^ generated
  reason: `Note.written_at` is generated from the operation's clock; code never assigns it.
  next: Remove `written_at` from `set:` (agent)
  rule: data.set_generated_field
  help: docs/language/06-reads-and-writes.html#data-set-generated-field

check failed: 1 error, 1 file
```

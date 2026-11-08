# `data.set_generated_field`

A write, a patch or a lifecycle sets `id` or a field the store generates.

## Why Jadpo refuses it

The store fills in a record's `id` when it is created, and each generated
field from the operation's clock. Code never assigns them, so a value from
anywhere else would say something the store then overwrites, or that a
caller could use to forge a timestamp.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/data.set_generated_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A patch whose `Input` has a generated field is refused the same way, at
the `patch:`:

```jadpo check project=e-set-generated-patch expect=data.set_generated_field
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

type NoteChanges = Input {
    pick: [Note.written_at]
    optional: all
}

action edit(note: Note, amendment: NoteChanges) -> Note {
    return Note.update(note) {
        patch: amendment
    }
}
```

```text output
error: `Note.update` sets `written_at`, which the store generates
 --> main.jadpo:20:16
   |
20 |         patch: amendment
   |                ^^^^^^^^^ generated
  reason: `Note.written_at` is generated from the operation's clock; code never assigns it.
  next: Remove `written_at` from `NoteChanges` (agent)
  rule: data.set_generated_field
  help: https://jadpo.dev/docs/errors/data.set_generated_field

check failed: 1 error, 1 file
```

So is a transition's `set:`, or a lifecycle's `initial:`:

```jadpo check project=e-set-generated-lifecycle expect=data.set_generated_field
entity Note {
    id: Uuid
    written_at: Instant {
        generated: Generated.on_create
    }

    lifecycle: {
        transitions: {
            touch: {
                set: {
                    written_at: clock.now
                }
            }
        }
    }
}
```

```text output
error: `set:` sets `written_at`, which the store generates
 --> main.jadpo:11:21
   |
11 |                     written_at: clock.now
   |                     ^^^^^^^^^^ generated
  reason: `Note.written_at` is generated from the operation's clock; nothing assigns it.
  next: Remove `written_at` from `set:` (agent)
  rule: data.set_generated_field
  help: https://jadpo.dev/docs/errors/data.set_generated_field

check failed: 1 error, 1 file
```

## How to fix it

**If the store should go on setting it**, remove it from what the write
sets:

```jadpo fragment
action jot_down(title: Note.title) -> Note {
    return Note.create {
        set: {
            title: title
        }
    }
}
```

```jadpo check project=e-set-generated-fix-1 hidden
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

action jot_down(title: Note.title) -> Note {
    return Note.create {
        set: {
            title: title
        }
    }
}
```

Each new note now gets the time it was created, from the store.

**If it came through a patch's `Input`**, take it out of the input:

```jadpo fragment
type NoteChanges = Input {
    pick: [Note.title]
    optional: all
}
```

```jadpo check project=e-set-generated-fix-3 hidden
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

type NoteChanges = Input {
    pick: [Note.title]
    optional: all
}

action edit(note: Note, amendment: NoteChanges) -> Note {
    return Note.update(note) {
        patch: amendment
    }
}
```

A request can no longer send a time for `written_at`.

**If your code should decide the value**, remove `generated:` from the
field:

```jadpo fragment
    written_at: Instant
```

```jadpo check project=e-set-generated-fix-2 hidden
type Title = Text {
    max_length: 80
}

entity Note {
    id: Uuid
    title: Title
    written_at: Instant
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

`written_at` is now set by whoever calls the action.

## Related

- [Fields and their settings](../language/05-data.md#fields-and-their-settings)
- [The operations every entity gets](../language/06-reads-and-writes.md#the-operations-every-entity-gets)

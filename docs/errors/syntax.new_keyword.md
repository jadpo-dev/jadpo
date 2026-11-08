# `syntax.new_keyword`

A value is created with `new`.

## Why Jadpo refuses it

`new` creates an object in JavaScript, TypeScript, Java and C#. Jadpo has
nothing for `new` to add: a value is made with `Type(value)`, or an object
with `Type { field: value }`, and the type's name already says what is being
made.

## What it looks like

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you were making a value**, remove `new`:

```jadpo fragment
function edited(before: CustomerNote, after: CustomerNote) -> NoteWasEdited {
    return NoteWasEdited(before != after)
}
```

```jadpo check project=e-habit-new-fix hidden
type CustomerNote = Text {
    max_length: 500
}

type NoteWasEdited = Bool {}

function edited(before: CustomerNote, after: CustomerNote) -> NoteWasEdited {
    return NoteWasEdited(before != after)
}
```

`edited` now builds its result with the type's name, as every value is
built.

## Related

- [Creating values](../language/02-values-and-types.md#creating-values)
- [Habits from other languages](../language/02-values-and-types.md#habits-from-other-languages)

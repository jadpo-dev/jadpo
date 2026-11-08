# `type.copy_clash`

A type copies a field with `include` or `pick` that another `include` or `pick` above it has already brought in.

## Why Jadpo refuses it

Copied fields aren't written in the declaration, so a clash between two
of them is invisible where you read the type. If a later copy replaced
an earlier one, a field added to a source type would change this type
without anyone seeing it. Jadpo won't decide which `title` wins, because
the two come from different records and may mean different things.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.copy_clash

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the type needs both titles.

**If it needs only the todo's title**, stop copying the note's:

```jadpo fragment
    pick: [Todo.id, Todo.title]
```

```jadpo check project=e-copy-clash-fix-1 file=main.jadpo hidden
type TodoView = Output {
    pick: [Todo.id, Todo.title]
}
```

```jadpo check project=e-copy-clash-fix-1 file=names.jadpo hidden
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

`TodoView` has `id` and `title`, both from `Todo`.

**If it needs both**, write the note's title as a field of its own name:

```jadpo fragment
type TodoView = Output {
    pick: [Todo.id, Todo.title]
    note_title: Note.title
}
```

```jadpo check project=e-copy-clash-fix-2 file=main.jadpo hidden
type TodoView = Output {
    pick: [Todo.id, Todo.title]
    note_title: Note.title
}
```

```jadpo check project=e-copy-clash-fix-2 file=names.jadpo hidden
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

`TodoView` has `id`, `title` and `note_title`, each visible in the
declaration, and `note_title` keeps `Note.title`'s rules.

## Related

- [Building a type from other types](../language/02-values-and-types.md#building-a-type-from-other-types)
- [Objects](../language/02-values-and-types.md#objects)

# `type.absent_field`

An `omit:` or `optional:` line names a field that the lines above it don't bring into the type.

## Why Jadpo refuses it

A type built from other types is read from top to bottom, and `omit:` and
`optional:` change the fields that are there at that point. A name that
isn't there is usually a typo, or a field removed from its source since
the line was written, so the line would quietly do nothing. Jadpo says so
instead.

## What it looks like

names.jadpo:

```jadpo check project=e-absent file=names.jadpo
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
}
```

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the field was removed, or never belonged in the type**, take it out
of the list, and remove the line once the list is empty:

```jadpo check project=e-absent-fix file=main.jadpo
type TodoView = Output {
    pick: [Todo.id, Todo.title]
}
```

```jadpo check project=e-absent-fix file=names.jadpo hidden
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
}
```

`TodoView` has the two fields its `pick:` line lists, `id` and `title`.

## Related

- [Building a type from other types](../language/02-values-and-types.md#building-a-type-from-other-types)

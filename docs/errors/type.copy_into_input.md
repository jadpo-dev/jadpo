# `type.copy_into_input`

An `Input` type copies every field of another type with `include`, so a request could set any of them.

## Why Jadpo refuses it

An `Input` is what arrives from outside, such as a request body.
`include: [Todo]` would accept every field `Todo` has, and every field
added to `Todo` later, so a caller could set a field nobody chose to
let them set. This is the mass-assignment bug.

Jadpo won't pick the fields for you. Which fields a request may set
decides what callers can change in your data.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.copy_into_input

check failed: 1 error, 2 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which of `Todo`'s fields a request may set.

## How to fix it

**If a request should set some of `Todo`'s fields**, name them with
`pick`:

```jadpo fragment
    pick: [Todo.title]
```

```jadpo check project=e-copy-input-fix-1 file=main.jadpo hidden
type CreateTodo = Input {
    pick: [Todo.title]
}
```

```jadpo check project=e-copy-input-fix-1 file=names.jadpo hidden
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
}
```

`CreateTodo` now accepts only `title`, with `Todo.title`'s rules. A
field added to `Todo` later isn't accepted until you add it here.
Generated fields such as `id` can never appear in an `Input`.

## Related

- [Building a type from other types](../language/02-values-and-types.md#building-a-type-from-other-types)
- [Objects](../language/02-values-and-types.md#objects)

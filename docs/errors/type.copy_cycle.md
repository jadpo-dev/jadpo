# `type.copy_cycle`

A type's `include` leads back to the type itself, directly or through another type.

## Why Jadpo refuses it

`include` copies every field of the types it lists. When `TodoView`
includes `ListView` and `ListView` includes `TodoView`, each type's
fields would be copied from its own, so neither has a list of fields
that could ever be written out. Jadpo won't break the loop for you,
because which type builds on which decides what each one holds.

## What it looks like

main.jadpo:

```jadpo check project=e-copy-cycle expect=type.copy_cycle
type Title = Text {}

type TodoView = Output {
    include: [ListView]
    title: Title
}

type ListView = Output {
    include: [TodoView]
}
```

```text output
error: `ListView` includes its own fields
 --> main.jadpo:9:15
  |
9 |     include: [TodoView]
  |               ^^^^^^^^ leads back here
  reason: `TodoView` leads back to `ListView` through its own `include`, so its fields would be copied from themselves and the type could never be written out.
  next: Remove `TodoView` from `include`, or write the fields it needs (agent)
  rule: type.copy_cycle
  help: https://jadpo.dev/docs/errors/type.copy_cycle

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is which of the two types builds on the other.

**If `ListView` builds on `TodoView`**, remove the `include` that leads
back, in `TodoView`:

```jadpo fragment
type TodoView = Output {
    title: Title
}
```

```jadpo check project=e-copy-cycle-fix-1 hidden
type Title = Text {}

type TodoView = Output {
    title: Title
}

type ListView = Output {
    include: [TodoView]
}
```

`ListView` copies `TodoView`'s fields, `title`, and a field added to
`TodoView` later reaches it too.

**If `TodoView` builds on `ListView`**, write the fields `ListView`
needs in it instead of including `TodoView`:

```jadpo fragment
type ListView = Output {
    title: Title
}
```

```jadpo check project=e-copy-cycle-fix-2 hidden
type Title = Text {}

type TodoView = Output {
    include: [ListView]
    title: Title
}

type ListView = Output {
    title: Title
}
```

`TodoView` copies `ListView`'s fields, and its own written `title`
replaces the copied one. A field added to `ListView` later reaches
`TodoView`, but not the other way round.

## Related

- [Building a type from other types](../language/02-values-and-types.md#building-a-type-from-other-types)

# `layout.route_prefix_mismatch`

A route group's prefix doesn't match its file name.

## Why Jadpo refuses it

A route group's file is named after the fixed parts of its prefix, joined
with `_`, so a URL leads straight to one file. If `/tasks` could live in
`todos.jadpo`, nobody could find a route's code from its address.

## What it looks like

The group is in `todos.jadpo`, and its prefix is `/tasks`.

```jadpo check project=e-prefix file=app.jadpo hidden
application {}
```

`domains/planning/routes/todos.jadpo`:

```jadpo check project=e-prefix file=domains/planning/routes/todos.jadpo expect=layout.route_prefix_mismatch
routes Todos {
    prefix: /tasks
}
```

```text output
error: Prefix `/tasks` doesn't match `todos.jadpo`
 --> domains/planning/routes/todos.jadpo:2:13
  |
2 |     prefix: /tasks
  |             ^^^^^^ gives `tasks.jadpo`
  reason: A route group's file is named after its prefix's fixed segments joined with `_`, so a URL leads to one file: `/tasks` lives in `tasks.jadpo`.
  next: Change the prefix to match `todos.jadpo`, or rename the file `tasks.jadpo` and the group `Tasks` (agent)
  rule: layout.route_prefix_mismatch
  help: https://jadpo.dev/docs/errors/layout.route_prefix_mismatch

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the file name is right**, change the prefix to match it. In
`domains/planning/routes/todos.jadpo`:

```jadpo check project=e-prefix-fix file=domains/planning/routes/todos.jadpo
routes Todos {
    prefix: /todos
}
```

```jadpo check project=e-prefix-fix file=app.jadpo hidden
application {}
```

Its routes are now served under `/todos`, where the file says.

**If the prefix is right**, rename the file and the group after it:
`domains/planning/routes/tasks.jadpo`.

```jadpo check project=e-prefix-rename file=domains/planning/routes/tasks.jadpo
routes Tasks {
    prefix: /tasks
}
```

```jadpo check project=e-prefix-rename file=app.jadpo hidden
application {}
```

The address stays `/tasks`, and its code is now in `tasks.jadpo`.

## Related

- [The layout in detail](../language/01-project-layout-and-modules.md#the-layout-in-detail)
- [Files and names](../language/01-project-layout-and-modules.md#files-and-names)

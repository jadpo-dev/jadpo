# `layout.route_prefix_mismatch`

```jadpo check project=e-prefix file=app.jadpo hidden
application {}
```

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
  help: docs/errors/layout.route_prefix_mismatch.md

check failed: 1 error, 2 files
```

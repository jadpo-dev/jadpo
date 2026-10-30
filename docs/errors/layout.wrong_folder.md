# `layout.wrong_folder`

```jadpo check project=e-folder file=app.jadpo hidden
application {}
```

```jadpo check project=e-folder file=domains/planning/types/complete_todo.jadpo expect=layout.wrong_folder
action complete_todo() -> Unit {}
```

```text output
error: Action `complete_todo` is in `types/`
 --> domains/planning/types/complete_todo.jadpo:1:8
  |
1 | action complete_todo() -> Unit {}
  |        ^^^^^^^^^^^^^ belongs in `actions/`
  reason: `types/` holds types shared by several files in the domain; every action has its own file in `actions/`.
  next: Move `complete_todo` to `domains/planning/actions/complete_todo.jadpo` (agent)
  rule: layout.wrong_folder
  help: https://jadpo.dev/docs/errors/layout.wrong_folder

check failed: 1 error, 2 files
```

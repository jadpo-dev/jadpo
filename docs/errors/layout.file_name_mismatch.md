# `layout.file_name_mismatch`

```jadpo check project=e-file-name file=app.jadpo hidden
application {}
```

```jadpo check project=e-file-name file=domains/planning/actions/snooze_todo.jadpo hidden
use planning/types { DueDate }

action snooze_todo(due: DueDate) -> Unit {}
```

```jadpo check project=e-file-name file=domains/planning/types/due.jadpo expect=layout.file_name_mismatch
type DueDate = CalendarDate {}
```

```text output
error: `DueDate` is in `due.jadpo`
 --> domains/planning/types/due.jadpo:1:6
  |
1 | type DueDate = CalendarDate {}
  |      ^^^^^^^ file name should be `due_date.jadpo`
  reason: A file is named after the declaration it holds, so a search for a name finds its file.
  next: Rename `due.jadpo` to `due_date.jadpo` (agent)
  rule: layout.file_name_mismatch
  help: docs/errors/layout.file_name_mismatch.md

check failed: 1 error, 3 files
```

# `layout.file_name_mismatch`

A file's name doesn't match the declaration it holds, or the file holds a second declaration that needs its own.

## Why Jadpo refuses it

A file is named after what it declares, so that searching for a name finds
its file, and an agent can tell from a path what's inside. A file called
`due.jadpo` that declares `DueDate` breaks that for anyone looking for
`DueDate`.

## What it looks like

`DueDate` is declared in `due.jadpo`.

```jadpo check project=e-file-name file=app.jadpo hidden
application {}
```

```jadpo check project=e-file-name file=domains/planning/actions/snooze_todo.jadpo hidden
use planning/types { DueDate }

action snooze_todo(due: DueDate) -> Unit {}
```

`domains/planning/types/due.jadpo`:

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
  help: https://jadpo.dev/docs/errors/layout.file_name_mismatch

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Each action has its own file, so a second one in a file is told to move:

```jadpo check project=e-file-name-second file=app.jadpo hidden
application {}
```

`domains/planning/actions/snooze_todo.jadpo`:

```jadpo check project=e-file-name-second file=domains/planning/actions/snooze_todo.jadpo expect=layout.file_name_mismatch
action unsnooze_todo() -> Unit {}

action snooze_todo() -> Unit {}
```

```text output
error: `unsnooze_todo` needs a file of its own
 --> domains/planning/actions/snooze_todo.jadpo:1:8
  |
1 | action unsnooze_todo() -> Unit {}
  |        ^^^^^^^^^^^^^ a second action in this file
  reason: Every action has its own file in `actions/`, named after it.
  next: Move `unsnooze_todo` to `domains/planning/actions/unsnooze_todo.jadpo` (agent)
  rule: layout.file_name_mismatch
  help: https://jadpo.dev/docs/errors/layout.file_name_mismatch

check failed: 1 error, 2 files
```

## How to fix it

**If the name of the declaration is right**, rename the file after it:
`domains/planning/types/due_date.jadpo`.

```jadpo check project=e-file-name-fix file=domains/planning/types/due_date.jadpo
type DueDate = CalendarDate {}
```

```jadpo check project=e-file-name-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-file-name-fix file=domains/planning/actions/snooze_todo.jadpo hidden
use planning/types { DueDate }

action snooze_todo(due: DueDate) -> Unit {}
```

The file is now where a search for `DueDate` looks, and the `use` line
doesn't change.

**If a file holds two actions**, move the second to its own file:
`domains/planning/actions/unsnooze_todo.jadpo`.

```jadpo check project=e-file-name-second-fix file=domains/planning/actions/unsnooze_todo.jadpo
action unsnooze_todo() -> Unit {}
```

```jadpo check project=e-file-name-second-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-file-name-second-fix file=domains/planning/actions/snooze_todo.jadpo hidden
action snooze_todo() -> Unit {}
```

Each action has its own file, named after it.

## Related

- [Files and names](../language/01-project-layout-and-modules.md#files-and-names)
- [One primary declaration per file](../language/01-project-layout-and-modules.md#one-primary-declaration-per-file)

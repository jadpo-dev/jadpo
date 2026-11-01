# `format.changes_required`

A file `jadpo fmt --check` read isn't laid out as `jadpo fmt` would write it.

## Why Jadpo refuses it

Jadpo has one layout, which `jadpo fmt` writes, so equivalent code always
looks the same and a diff shows only real changes. `jadpo fmt --check`
changes nothing: it is the check CI runs, and fails when a file differs
from what the formatter would write, pointing at the first place it
differs. The edit it would make is in `--json`, for an agent to apply.

## What it looks like

A type written with two spaces of indentation, where the formatter
writes four:

```jadpo check project=e-format-changes file=app.jadpo hidden
application {}
```

```jadpo check project=e-format-changes file=domains/tasks/types/task_title.jadpo hidden
/// What a task is called.
type TaskTitle = Text {
    max_length: 200
}
```

```jadpo check project=e-format-changes file=domains/tasks/actions/rename_task.jadpo hidden
use tasks/types { TaskTitle }

/// The title a task is given.
action rename_task(title: TaskTitle) -> TaskTitle {
    return title
}
```

```sh run project=e-format-changes expect=format.changes_required
cat > domains/tasks/types/task_title.jadpo <<'EOF2'
/// What a task is called.
type TaskTitle = Text {
  max_length: 200
}
EOF2
jadpo fmt --check
```

```text output
error: This file isn't formatted
 --> domains/tasks/types/task_title.jadpo:3:3
  |
3 |   max_length: 200
  |   ^ the formatter changes this line
  reason: `jadpo fmt` lays this file out differently from here on.
  next: Run `jadpo fmt` to format it (agent)
  rule: format.changes_required
  help: https://jadpo.dev/docs/errors/format.changes_required

fmt failed: 1 not formatted, 2 already formatted
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If the file should be in Jadpo's layout**, as every file is, format
it:

```sh
jadpo fmt
```

Every file is rewritten in the one layout, and `jadpo fmt --check` then
passes. An editor with the Jadpo extension formats each file as it
saves it.

## Related

- [Formatting](../language/14-tooling.md#formatting)
- [`jadpo fmt`](../language/14-tooling.md#jadpo-fmt)

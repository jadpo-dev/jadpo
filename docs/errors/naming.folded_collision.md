# `naming.folded_collision`

Two names in the project become the same file name.

## Why Jadpo refuses it

Each declaration lives in a file named after it, so `due_date` and `DueDate`
both become `due_date.jadpo`, and a search for one finds the other. A type
beside the action, query or function it belongs to, in the same file, is the
one exception, such as `CreateTodo` in `create_todo.jadpo`.

Jadpo can't tell which name you'd rather change, so it asks you to.

## What it looks like

```jadpo check project=e-folded file=due.jadpo
type DueDate = CalendarDate {}
```

```jadpo check project=e-folded file=main.jadpo expect=naming.folded_collision
function due_date(due: DueDate) -> DueDate {
    return due
}
```

```text output
error: `due_date` and `DueDate` share a file name
 --> main.jadpo:1:10
  |
1 | function due_date(due: DueDate) -> DueDate {
  |          ^^^^^^^^ also `due_date.jadpo`
  reason: Both names become `due_date.jadpo`, so a search for one finds the other; `DueDate` is a type in `due.jadpo`.
  next: Rename one of them so their file names differ (agent)
  rule: naming.folded_collision
  help: https://jadpo.dev/docs/errors/naming.folded_collision

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the two are different things**, rename one so their file names differ:

```jadpo fragment
function next_due(due: DueDate) -> DueDate {
    return due
}
```

```jadpo check project=e-folded-fix file=main.jadpo hidden
function next_due(due: DueDate) -> DueDate {
    return due
}
```

```jadpo check project=e-folded-fix file=due.jadpo hidden
type DueDate = CalendarDate {}
```

Each name now has a file of its own, so a search for either finds only that
one.

## Related

- [Naming](../language/01-project-layout-and-modules.md#naming)
- [When two names clash](../language/01-project-layout-and-modules.md#when-two-names-clash)

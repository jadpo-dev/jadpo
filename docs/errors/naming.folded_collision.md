# `naming.folded_collision`

Two names in the project become the same file name.

## Why Jadpo refuses it

Each declaration lives in a file named after it, so `due_date` and `DueDate`
both become `due_date.jadpo`, and a search for one finds the other. A type
beside the action, query or function it belongs to, in the same file, is the
one exception, such as `CreateTodo` in `create_todo.jadpo`.

Jadpo can't tell which name you'd rather change. Its next step names the
one it reached second, and either may be renamed.

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

In a project, `jadpo rename DueDate <new name>` does it: it renames the
declaration, moves it to the new name's file and changes every place that
uses it, so the error's `next:` names that command.

**If an input or output shares its action's name**, rename the type. An
input that only its action uses (its route and tests don't count) sits in
the action's file and may take its name: `type CompleteTodos = Input`
beside `action complete_todos` in `actions/complete_todos.jadpo`. Once
another action or query uses it, it has a file of its own in `types/`,
where it can't. The error then names the type and its new name: the
action's name with `Request` for an input, or `View` for an output (a
query's or function's name likewise), unless that name is taken:

```text
  reason: Both names become `complete_todos.jadpo`, so a search for one finds the other; `complete_todos` is an action in `domains/planning/actions/complete_todos.jadpo`. Only an input in its action's own file may take the action's name.
  next: Rename the input `CompleteTodos` to `CompleteTodosRequest` with `jadpo rename CompleteTodos CompleteTodosRequest`, which moves its file and every use with it (agent)
```

## Related

- [Naming](../language/01-project-layout-and-modules.md#naming)
- [When two names clash](../language/01-project-layout-and-modules.md#when-two-names-clash)

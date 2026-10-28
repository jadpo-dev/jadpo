# `naming.folded_collision`

Two names that become the same file name are an error, except a type beside
the action, query or function in its own file (`CreateTodo` in
`create_todo.jadpo`):

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
  help: docs/language/01-project-layout-and-modules.html#naming-folded-collision

check failed: 1 error, 2 files
```

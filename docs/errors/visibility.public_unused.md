# `visibility.public_unused`

A declaration is marked `public`, but nothing outside its domain uses it.

## Why Jadpo refuses it

`public` is a promise to keep offering a declaration to the rest of the
project, and one that nobody takes up is a promise for nothing. It also makes
the declaration harder to change, since a reader has to assume something
depends on it. A use counts when it comes from another domain's code or tests,
a root `tests/` scenario, or the signature of a public declaration one of those
uses. The domain's own tests don't count, and a package's public members are
its interface to other projects, so they are exempt.

## What it looks like

```jadpo check project=e-public-unused file=app.jadpo hidden
application {}
```

```jadpo check project=e-public-unused file=shared/types/todo_count.jadpo hidden
type TodoCount = Int {
    min: 0
}
```

```jadpo check project=e-public-unused file=domains/planning/actions/count_todos.test.jadpo hidden
test "two todos are open" {
    assert count_todos() == 2
}
```

domains/planning/actions/count_todos.jadpo:

```jadpo check project=e-public-unused file=domains/planning/actions/count_todos.jadpo expect=visibility.public_unused
use shared/types { TodoCount }

public action count_todos() -> TodoCount {
    return 2
}
```

```text output
error: `count_todos` is `public`, but nothing outside `planning` uses it
 --> domains/planning/actions/count_todos.jadpo:3:1
  |
3 | public action count_todos() -> TodoCount {
  | ^^^^^^ offered to no one
  reason: `public` offers a declaration to other domains, and no other domain's code or tests, no root `tests/` scenario and no public declaration they use names `count_todos`; `planning`'s own tests don't count.
  next: Remove `public` (agent)
  rule: visibility.public_unused
  help: https://jadpo.dev/docs/errors/visibility.public_unused

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If nothing outside the domain needs it**, remove `public`:

```jadpo fragment
action count_todos() -> TodoCount {
    ...
}
```

```jadpo check project=e-public-unused-fix-1 file=domains/planning/actions/count_todos.jadpo hidden
use shared/types { TodoCount }

action count_todos() -> TodoCount {
    return 2
}
```

```jadpo check project=e-public-unused-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=e-public-unused-fix-1 file=shared/types/todo_count.jadpo hidden
type TodoCount = Int {
    min: 0
}
```

```jadpo check project=e-public-unused-fix-1 file=domains/planning/actions/count_todos.test.jadpo hidden
test "two todos are open" {
    assert count_todos() == 2
}
```

The domain's own tests keep calling it by name.

**If another domain should use it**, call it from there through its domain:

```jadpo fragment
use planning

action show_todo_count() -> TodoCount {
    return planning.count_todos()
}
```

```jadpo check project=e-public-unused-fix-2 file=domains/reports/actions/show_todo_count.jadpo hidden
use shared/types { TodoCount }
use planning

action show_todo_count() -> TodoCount {
    return planning.count_todos()
}
```

```jadpo check project=e-public-unused-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=e-public-unused-fix-2 file=shared/types/todo_count.jadpo hidden
type TodoCount = Int {
    min: 0
}
```

```jadpo check project=e-public-unused-fix-2 file=domains/planning/actions/count_todos.jadpo hidden
use shared/types { TodoCount }

public action count_todos() -> TodoCount {
    return 2
}
```

`planning` now offers `count_todos`, and `reports` uses it.

## Related

- [What can be public](../language/01-project-layout-and-modules.md#what-can-be-public)
- [`visibility.public_not_allowed`](visibility.public_not_allowed.md)

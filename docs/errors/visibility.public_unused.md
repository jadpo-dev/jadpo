# `visibility.public_unused`

`public` offers a declaration to the rest of the project, so it marks only
what something outside the domain uses: another domain's code or tests, a
root `tests/` scenario, or the signature of a public declaration one of them
uses. The domain's own tests don't count, and a package's public members are
its interface to other projects, so they are exempt (area 11):

```jadpo check project=e-public-unused file=app.jadpo hidden
application {}
```

```jadpo check project=e-public-unused file=shared/types/todo_count.jadpo hidden
type TodoCount = Int {
    min: 0
}
```

```jadpo check project=e-public-unused file=domains/planning/actions/count_todos.test.jadpo
test "two todos are open" {
    assert count_todos() == 2
}
```

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
  help: docs/errors/visibility.public_unused.md

check failed: 1 error, 4 files
```

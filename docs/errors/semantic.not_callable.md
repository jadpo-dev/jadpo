# `semantic.not_callable`

```jadpo check project=e-not-callable expect=semantic.not_callable
type OutOfStock = Failure {
    kind: FailureKind.conflict
}

action reserve_item() fails OutOfStock -> Unit {
    reject OutOfStock()
}
```

```text output
error: `OutOfStock` is a failure and can't be called
 --> main.jadpo:6:12
  |
6 |     reject OutOfStock()
  |            ^^^^^^^^^^ not callable
  reason: A failure is raised with `reject OutOfStock`, with its fields in a block when it has any.
  next: Write `OutOfStock` without `()` (agent)
  rule: semantic.not_callable
  help: https://jadpo.dev/docs/errors/semantic.not_callable

check failed: 1 error, 1 file
```

A fixture builds test data, so only tests, other fixtures and seeds call it
(area 13); an action can't, even when the fixture is `public`:

```jadpo check project=e-fixture-from-action file=app.jadpo hidden
application {}
```

```jadpo check project=e-fixture-from-action file=domains/planning/types/todo_count.jadpo hidden
public type TodoCount = Int {
    min: 0
}
```

```jadpo check project=e-fixture-from-action file=domains/planning/fixtures/two_open_todos.jadpo hidden
use planning/types { TodoCount }

public fixture two_open_todos() -> TodoCount {
    return 2
}
```

```jadpo check project=e-fixture-from-action file=tests/open_todos.test.jadpo hidden
use planning

test "planning starts with two open todos" {
    assert planning.two_open_todos() == 2
}
```

```jadpo check project=e-fixture-from-action file=domains/planning/actions/count_todos.jadpo expect=semantic.not_callable
use planning/types { TodoCount }
use planning/fixtures { two_open_todos }

action count_todos() -> TodoCount {
    return two_open_todos()
}
```

```text output
error: An action can't call fixture `two_open_todos`
 --> domains/planning/actions/count_todos.jadpo:5:12
  |
5 |     return two_open_todos()
  |            ^^^^^^^^^^^^^^ test data
  reason: A fixture builds test data: only tests, other fixtures and seeds call it, and it is never compiled into a deployable.
  next: Call the actions `two_open_todos` calls, or move this call into a test, fixture or seed (agent)
  rule: semantic.not_callable
  help: https://jadpo.dev/docs/errors/semantic.not_callable

check failed: 1 error, 5 files
```

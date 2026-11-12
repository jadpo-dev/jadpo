# `semantic.not_callable`

Code calls something that can't be called, or can't be called from where it is.

## Why Jadpo refuses it

A call runs code, so what it names has to be something that runs, in a
place allowed to run it. A failure is raised, a fixture only builds test
data, and a test changes records through the actions that do. Refusing it
means the wrong thing can't be called by accident.

## What it looks like

A failure is raised, not called:

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A fixture builds test data, so only tests, other fixtures and seeds call
it, even when it is `public`:

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

domains/planning/actions/count_todos.jadpo:


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

`some` is only a `match` pattern. A value that may be missing takes the
value itself:

```jadpo check project=e-some-called expect=semantic.not_callable
type Title = Text {
    max_length: 40
}

function chosen(title: Title) -> Title? {
    return some(title)
}
```

```text output
error: `some` is a match pattern and can't be called
 --> main.jadpo:6:12
  |
6 |     return some(title)
  |            ^^^^ not callable
  reason: A value that may be missing takes the value itself, as in `return title`, or `none`; `some(name)` only matches a present value in a `match`.
  next: Write `title` instead of `some(title)` (agent)
  rule: semantic.not_callable
  help: https://jadpo.dev/docs/errors/semantic.not_callable

check failed: 1 error, 1 file
```

A value has no operations of its own, so a call can't be made on one:

```jadpo check project=e-value-member expect=semantic.not_callable
type Title = Text {
    max_length: 40
}

function shout(title: Title) -> Title {
    return title.upper()
}
```

```text output
error: `title.upper` is a value's member and can't be called
 --> main.jadpo:6:12
  |
6 |     return title.upper()
  |            ^^^^^^^^^^^ not callable
  reason: `title` is a variable, and a value has no operations of its own: Jadpo calls functions, actions, queries, library functions and the standard library's operations, never a value's methods.
  next: Pass `title` to a function or an operation instead (agent)
  rule: semantic.not_callable
  help: https://jadpo.dev/docs/errors/semantic.not_callable

check failed: 1 error, 1 file
```

A test changes records through the actions that do it, which is what it
proves:

```jadpo check project=e-test-write file=app.jadpo hidden
application {}
```

```jadpo check project=e-test-write file=domains/shop/types/count.jadpo hidden
type Count = Int {
    min: 0
}
```

```jadpo check project=e-test-write file=domains/shop/entities/shelf.jadpo hidden migrate=initial
use shop/types { Count }

entity Shelf {
    id: Uuid
    visits: Count
}
```

```jadpo check project=e-test-write file=domains/shop/entities/shelf.test.jadpo expect=semantic.not_callable
test "a shelf counts its visits" {
    var shelf: Shelf = attempt Shelf.update("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e") {
        set: {
            visits: 3
        }
    }
    assert shelf.visits == 3
}
```

```text output
error: A test can't call `Shelf.update`
 --> domains/shop/entities/shelf.test.jadpo:2:32
  |
2 |     var shelf: Shelf = attempt Shelf.update("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e") {
  |                                ^^^^^^^^^^^^ writes a record
  reason: Only actions write records (area 6), so a test changes data through the action that does it, which is what it proves; a fixture, a test-only action, or a data builder, `make.Shelf { ... }`, builds the data a test starts from. A test may still read, with `Shelf.get` and queries.
  next: Call the action that writes `Shelf`, or make the record it starts from with `make.Shelf { ... }` (agent)
  rule: semantic.not_callable
  help: https://jadpo.dev/docs/errors/semantic.not_callable

check failed: 1 error, 4 files
```

## How to fix it

**If it's a failure**, write it without `()`:

```jadpo fragment
action reserve_item() fails OutOfStock -> Unit {
    reject OutOfStock
}
```

```jadpo check project=e-not-callable-fix-1 hidden
type OutOfStock = Failure {
    kind: FailureKind.conflict
}

action reserve_item() fails OutOfStock -> Unit {
    reject OutOfStock
}
```

`reserve_item` now fails with `OutOfStock`.

**If the value may be missing**, return the value itself:

```jadpo fragment
function chosen(title: Title) -> Title? {
    return title
}
```

```jadpo check project=e-not-callable-fix-2 hidden
type Title = Text {
    max_length: 40
}

function chosen(title: Title) -> Title? {
    return title
}
```

`chosen` now returns the title, and its `Title?` result still tells a
caller it may be `none`.

## Related

- [Calling](../language/03-logic.md#calling)
- [Who can call a fixture](../language/13-testing.md#who-can-call-a-fixture)
- [Match](../language/03-logic.md#match)

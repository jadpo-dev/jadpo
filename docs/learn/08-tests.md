# Tests
<!-- topic: learn-tests -->

You've been checking the app by hand with `curl`. On this page you
write tests that do it for you: some call actions directly, and some
send real HTTP requests through the routes. Each test gets a fresh
database of its own, so tests never see each other's data.

## A test sits beside what it tests

A test file is named after the file it tests, with `.test` before
`.jadpo`, and sits next to it. It can use what that file declares, such
as `add_todo`, without a `use` line; everything else it imports as any
file does. Tests for `add_todo` go in `actions/add_todo.test.jadpo`:

`domains/todos/actions/add_todo.test.jadpo`:

```jadpo check project=learn-todo-08 file=domains/todos/actions/add_todo.test.jadpo
use todos/types { NewTodo, TodoStatus, TodoView }
use todos/failures { DueDateInPast }

test "a new todo is open" {
    var todo: TodoView = attempt add_todo(
        NewTodo {
            title: "Buy milk"
        }
    )
    assert todo.status == TodoStatus.open
    assert todo.due_at == none
}

test "a due date that has passed is refused" {
    clock: "2026-11-01T12:00:00Z"
    assert add_todo(
        NewTodo {
            title: "Buy milk"
            due_at: "2026-11-01T11:59:59Z"
        }
    ) fails DueDateInPast
}
```

- `test "..."` names the test with a sentence, which is what a failure
  report shows.
- `assert` checks that a condition holds. The test fails at the first
  that doesn't.
- `attempt add_todo(...)` calls the action, which may fail. In a test,
  an `attempt` that fails ends the test as a failure, naming it.
- `assert add_todo(...) fails DueDateInPast` checks that the call fails
  with exactly that failure.
- `clock:` sets the time the test runs at, so `clock.now` is
  `2026-11-01T12:00:00Z` and the test passes on any day. A test that
  doesn't set it runs at `2000-01-01T00:00:00Z`.

A test that calls an action directly has no caller and no HTTP request,
so it tests the action's own logic. Policy, which is about who is
calling, doesn't apply there.

## Data for tests: fixtures

Tests often need a todo that already exists. A fixture makes one. It's
like an action that only tests can call, and it lives in `fixtures/`.
Like an action test, it runs with no caller, so policy doesn't apply:

`domains/todos/fixtures/milk.jadpo`:

```jadpo check project=learn-todo-08 file=domains/todos/fixtures/milk.jadpo
use todos/entities { Todo }
use todos/types { TodoStatus }

/// An open todo to buy milk.
fixture milk() -> Todo {
    return Todo.create {
        set: {
            title: "Buy milk"
            status: TodoStatus.open
            due_at: none
        }
    }
}
```

## Tests through HTTP

A route test sends a request through the app's real HTTP handling: the
input is checked, policy applies and statuses are set, just as for
`curl`. Tests for the `Todos` routes go beside them, in
`routes/todos.test.jadpo`:

`domains/todos/routes/todos.test.jadpo`:

```jadpo check project=learn-todo-08 file=domains/todos/routes/todos.test.jadpo
use todos/entities { Todo }
use todos/types { NewTodo, TodoChanges, TodoFilter, TodoStatus }
use todos/fixtures { milk }

test "adding a todo answers 201 with the todo" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/todos"
        input: NewTodo {
            title: "Buy milk"
        }
    }
    assert response.status == 201
}

test "a finished todo leaves the open list" {
    var todo: Todo = milk()
    var changed: Response = request {
        method: HttpMethod.patch
        path: "/todos/{todo_id}"
        params: {
            todo_id: todo.id
        }
        input: TodoChanges {
            status: TodoStatus.done
        }
    }
    assert changed.status == 200
    var open: Response = request {
        method: HttpMethod.get
        path: "/todos"
        input: TodoFilter {
            status: TodoStatus.open
        }
    }
    assert open.status == 200
    assert open.output == []
}

test "a deleted todo is gone" {
    var todo: Todo = milk()
    var deleted: Response = request {
        method: HttpMethod.delete
        path: "/todos/{todo_id}"
        params: {
            todo_id: todo.id
        }
    }
    assert deleted.status == 204
    var fetched: Response = request {
        method: HttpMethod.get
        path: "/todos/{todo_id}"
        params: {
            todo_id: todo.id
        }
    }
    assert fetched.status == 404
    assert fetched.code == "todo_not_found"
}
```

- `request { }` sends one request and gives back its `Response`. A
  `Response` has the `status`, the `code` of a failure, and the
  `output` the route sent.
- `path:` is the route's path as you wrote it, with `{todo_id}` filled
  from `params:`.
- `input:` is the body for a `POST` or `PATCH`, and the query string for
  a `GET` or `DELETE`, built as a value of the route's input type.
- `open.output == []` checks the list came back empty.

## Run them

`jadpo test` runs every test. It starts a PostgreSQL server of its own,
builds a database from your migrations, and gives each test a copy:

```sh
jadpo test
```

```text
tested: 5 passed, 2 action tests, 3 route tests in 1.0 s
```

The tests run in a random order each time, to catch tests that depend on
each other. To run one file's tests, name it:

```sh
jadpo test domains/todos/actions/add_todo.test.jadpo
```

```text
tested: 2 passed, 2 of 5 tests in 0.9 s
```

## When a test fails

Break one: in `todos.test.jadpo`, change `assert response.status == 201`
to `== 200`, and run the tests:

```text
FAIL  adding a todo answers 201 with the todo
 --> domains/todos/routes/todos.test.jadpo:13

  assert response.status == 200
         |        |
         |        201
         Response { status: 201, code: none, output: TodoView { id: "00dc6acf-ac00-7c23-aed1-cba688488f78", title: "Buy milk", status: TodoStatus.open, due_at: none, created_at: "2000-01-01T00:00:00Z" } }

  expected: 200
  actual: 201

  rule: test.assertion_failed
  rerun: jadpo test domains/todos/routes/todos.test.jadpo:13 --seed 1

test failed: 1 failed, 4 passed, 2 action tests, 3 route tests in 1.0 s
```

It shows each part of the condition with its value, and a `rerun:` line
that runs just this test again in the same order: `--seed` fixes the
order the tests ran in. Put the `201` back.

`jadpo test --watch` keeps running, and reruns the tests a change
affects each time you save.

## What you have

Five tests that cover adding, refusing, listing, changing and deleting
todos, each on a fresh database. Later pages add tests as they go. Next, on page 9, the app reads a setting from its environment.

Read more in the reference: [Writing a test](../language/13-testing.md#writing-a-test),
[Kinds of test](../language/13-testing.md#kinds-of-test) and
[Shared test data: fixtures](../language/13-testing.md#shared-test-data-fixtures).

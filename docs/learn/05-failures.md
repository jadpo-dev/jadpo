# Failures
<!-- topic: learn-failures -->

Some requests are well formed but still wrong: a todo due yesterday has
a valid date, but no one can do it in time. On this page `add_todo`
refuses one, and the client gets a clear `422` with a code it can act
on.

## A failure is declared, not thrown

Jadpo has no exceptions. Something your code expects to go wrong is a
failure: a type you declare, with a message for people and a kind that
says what sort of problem it is. Failures have their own folder,
`failures/`:

`domains/todos/failures/due_date_in_past.jadpo`:

```jadpo check project=learn-todo-05 file=domains/todos/failures/due_date_in_past.jadpo
/// A new todo can't be due before it was added.
type DueDateInPast = Failure {
    kind: FailureKind.invalid_value
    message: "A todo's due date must be in the future."
}
```

The kind decides the HTTP status, so code never picks a status itself.
These are the kinds you can declare:

| Kind | Status | Use it when |
|---|---|---|
| `invalid_value` | 422 | a value is well formed but breaks a rule, like this date |
| `rejected` | 422 | every value is fine, but a business rule refuses the request, such as cancelling an order that has shipped |
| `not_found` | 404 | the record isn't there |
| `not_permitted` | 403 | the caller may not do this |
| `conflict` | 409 | the request clashes with what's stored, such as a name already taken |
| `rate_limited` | 429 | the caller is asking too often |
| `unavailable` | 503 | something the app needs is down |

The failure's code, sent to the client, is its name in lower case with
underscores: `due_date_in_past`.

## Refuse, and say so

`domains/todos/actions/add_todo.jadpo`, changed. The `fails` list, the `match` and the `use todos/failures` line are new.

```jadpo check project=learn-todo-05 file=domains/todos/actions/add_todo.jadpo
use todos/entities { Todo }
use todos/types { NewTodo, TodoStatus, TodoView }
use todos/failures { DueDateInPast }

/// Adds an open todo, refusing a due date that has passed.
action add_todo(input: NewTodo) fails DueDateInPast -> TodoView {
    match input.due_at {
        some(due) -> {
            if (due <= clock.now) {
                reject DueDateInPast
            }
        }
        none -> {}
    }
    var todo: Todo = Todo.create {
        set: {
            title: input.title
            status: TodoStatus.open
            due_at: input.due_at
        }
    }
    return TodoView {
        id: todo.id
        title: todo.title
        status: todo.status
        due_at: todo.due_at
        created_at: todo.created_at
    }
}
```

Four new things are in this action:

- **`match`** looks inside a value that may be `none`. `some(due)` is
  the case where there is a due date, and names it `due`; `none -> {}`
  is the case where there isn't, which does nothing. A `match` must
  cover every case, so you can't forget the `none` one.
- **`clock.now`** is the current time, as an `Instant`. Only actions
  read the clock; a function can't. In tests you set the clock, as page
  8 shows.
- **`if`** runs its block when the condition holds. The condition always
  sits in brackets, and `<=` compares two moments: "at or before".
- **`reject DueDateInPast`** stops the action with that failure. Nothing
  after it runs, and nothing the action wrote is kept.

`fails DueDateInPast`, before the `->`, says the action can end with
that failure. It's part of the action's type, like its result. The
compiler holds you to it: take `fails DueDateInPast` out, and check
says (the record's error is left out here):

```text
error: `DueDateInPast` can leave `add_todo`, but its `fails` list doesn't say so
 --> domains/todos/actions/add_todo.jadpo:10:17
   |
10 |                 reject DueDateInPast
   |                 ^^^^^^^^^^^^^^^^^^^^ rejected here
  reason: `fails` lists exactly the failures that can leave an action, so every caller knows what it must handle.
  next: Add `DueDateInPast` to `add_todo`'s `fails` list (agent)
  rule: failure.undeclared_propagation
  help: https://jadpo.dev/docs/errors/failure.undeclared_propagation
```

Put it back. You don't change the route: a route can fail with whatever
its action can. Update the record: the route's failures are part of
your API, so the record now lists this one, with its status and
message:

```text
POST /todos
    authentication none
    input NewTodo (body)
    output TodoView
    status 201
    fails due_date_in_past

...

failure due_date_in_past
    status 422
    message "A todo's due date must be in the future."
```

## What the client sees

Run `jadpo dev` and add a todo due in 2020:

```sh
curl -i -X POST http://localhost:3000/todos \
  -H 'content-type: application/json' \
  -d '{"title": "Buy milk", "due_at": "2020-01-01T09:00:00Z"}'
```

```text
HTTP/1.1 422 Unprocessable Entity
content-type: application/json
x-request-id: req_85e0aa0e96123fa4
content-length: 126
date: Thu, 29 Oct 2026 21:28:03 GMT

{"error":{"code":"due_date_in_past","message":"A todo's due date must be in the future.","request_id":"req_85e0aa0e96123fa4"}}
```

It's the same shape as the `invalid_input` errors on page 2: every
error a client gets has an `error` with a `code`, a `message` and the
`request_id`. A client checks the `code`, which never changes, and
shows the `message`.

Some failures you never declare, because Jadpo raises them itself:
`invalid_input` for a request that doesn't fit, `authentication_required`
(401) when someone needs to sign in, and `internal_fault` (500) for a
bug. They never go in a `fails` list.

## What you have

An action that refuses a due date that has passed, with a failure the
compiler checks and the client can act on. Next, on page 6, you read
the todos back.

Read more in the reference: [Declaring a failure](../language/04-failures.md#declaring-a-failure)
and [How failures reach the client](../language/07-http.md#how-failures-reach-the-client).

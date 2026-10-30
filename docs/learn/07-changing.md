# Changing and deleting todos
<!-- topic: learn-changing -->

This page adds the routes that change a todo: `PATCH /todos/{id}` edits
its title or marks it done, and `DELETE /todos/{id}` removes it.

## What a change may hold

A `PATCH` sends only the fields it changes. So every field of the input
is optional:

`domains/todos/types/todo_changes.jadpo`:

```jadpo check project=learn-todo-07 file=domains/todos/types/todo_changes.jadpo
use todos/types { TodoStatus, TodoTitle }

/// What a caller may change about a todo: any of these, or none.
type TodoChanges = Input {
    title: TodoTitle {
        optional: true
    }
    status: TodoStatus {
        optional: true
    }
}
```

A field the caller leaves out isn't changed. So `{"status": "done"}`
marks a todo done and leaves its title as it was.

## Load, then update

Changing a record is two steps in one action: load it, then update it.

`domains/todos/actions/change_todo.jadpo`:

```jadpo check project=learn-todo-07 file=domains/todos/actions/change_todo.jadpo
use todos/entities { Todo, TodoNotFound }
use todos/types { TodoChanges, TodoView }

/// Changes the fields of a todo that the caller sent.
action change_todo(todo_id: Todo.id, changes: TodoChanges) fails TodoNotFound -> TodoView {
    var todo: Todo = attempt Todo.get(todo_id)
    var changed: Todo = Todo.update(todo) {
        patch: changes
    }
    return TodoView {
        id: changed.id
        title: changed.title
        status: changed.status
        due_at: changed.due_at
        created_at: changed.created_at
    }
}
```

- `Todo.get(todo_id)` loads one todo, or fails with `TodoNotFound`, like
  the query on page 6. Every entity gets `get`, as it gets `create`.
- `attempt` in front of a call that can fail passes its failure on: if
  `Todo.get` fails, `change_todo` stops with the same failure. That's why
  `change_todo` says `fails TodoNotFound`. Without `attempt`, the
  compiler refuses the call, so a failure can't be ignored by accident.
- `Todo.update(todo) { patch: changes }` changes only the fields the
  caller sent, and gives back the todo as stored. To set fields from
  code instead, write `set: { status: TodoStatus.done }`.

`jadpo sql change_todo` shows how a patch reaches the database:

```text
write Todo.update
  UPDATE "todo"
  SET "title" = CASE WHEN $1 THEN $2 ELSE "title" END,
      "status" = CASE WHEN $3 THEN $4 ELSE "status" END
  WHERE "id" = $5
    AND ($6::text IS NULL OR "xmin"::text = $6)
  RETURNING "id", "title", "status", "due_at", "created_at"

  writes: updates the Todo loaded earlier
  file: domains/todos/actions/change_todo.jadpo:7
  sends: 1 statement
  $1: whether changes supplies title · boolean
  $2: changes.title · text
  $3: whether changes supplies status · boolean
  $4: changes.status · text
  $5: todo.id · uuid
  $6: NULL::text · text
```

The `CASE` lines change a column only when the caller sent that field.
The last condition compares PostgreSQL's row version, `xmin`. Jadpo
fills it in only when an action calls another service between loading a
record and changing it, to catch a change someone made meanwhile. Here
`$6` is `NULL`, so it does nothing.

Deleting is the same shape: load, then delete.

`domains/todos/actions/delete_todo.jadpo`:

```jadpo check project=learn-todo-07 file=domains/todos/actions/delete_todo.jadpo
use todos/entities { Todo, TodoNotFound }

/// Deletes a todo.
action delete_todo(todo_id: Todo.id) fails TodoNotFound -> Unit {
    var todo: Todo = attempt Todo.get(todo_id)
    Todo.delete(todo)
}
```

`Unit` is the result of an action that gives nothing back, like `void`.

## All or nothing

Each action runs in one database transaction. If it fails partway, with
a `reject` or a failure passed on by `attempt`, everything it wrote is
rolled back. You never open or commit a transaction yourself. The
reference also covers what happens when two requests change the same
todo at once.

## The routes

`domains/todos/routes/todos.jadpo`, changed. The `PATCH` and `DELETE` routes and the names they add to the `use` lines are new.

```jadpo check project=learn-todo-07 file=domains/todos/routes/todos.jadpo
use todos/entities { Todo }
use todos/types { NewTodo, TodoChanges, TodoFilter, TodoView }
use todos/actions { add_todo, change_todo, delete_todo }
use todos/queries { list_todos, todo_by_id }

/// The todo list.
routes Todos {
    prefix: /todos

    /// Adds a todo.
    POST / {
        authentication: RouteAuthentication.none
        input: NewTodo
        output: TodoView
        status: SuccessStatus.created
        run: add_todo(input)
    }

    /// Lists todos, newest first.
    GET / {
        authentication: RouteAuthentication.none
        input: TodoFilter
        output: List<TodoView>
        run: list_todos(input)
    }

    /// One todo.
    GET /{todo_id: Todo.id} {
        authentication: RouteAuthentication.none
        output: TodoView
        run: todo_by_id(todo_id)
    }

    /// Changes a todo's title or status.
    PATCH /{todo_id: Todo.id} {
        authentication: RouteAuthentication.none
        input: TodoChanges
        output: TodoView
        run: change_todo(todo_id, input)
    }

    /// Deletes a todo.
    DELETE /{todo_id: Todo.id} {
        authentication: RouteAuthentication.none
        status: SuccessStatus.no_content
        run: delete_todo(todo_id)
    }
}
```

`SuccessStatus.no_content` answers `204` with no body, as a delete
usually does.

The entity has to allow the changes too. Add `update:` and `delete:` to
its policy:

`domains/todos/entities/todo.jadpo`, changed. The `update:` and `delete:` lines are new.

```jadpo check project=learn-todo-07 file=domains/todos/entities/todo.jadpo
use todos/types { TodoStatus, TodoTitle }

/// Something to do, kept in the database.
entity Todo {
    id: Uuid
    title: TodoTitle
    status: TodoStatus
    due_at: Instant?
    created_at: Instant {
        generated: Generated.on_create
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }
}
```

Update the record, and `jadpo check` passes.

## Try it

Run `jadpo dev`, add a todo as on page 4, and use its id. Mark it done:

```sh
curl -X PATCH http://localhost:3000/todos/01a18991-3d34-78a5-b36d-e043061c61a0 \
  -H 'content-type: application/json' \
  -d '{"status": "done"}'
```

```text
{"id":"01a18991-3d34-78a5-b36d-e043061c61a0","title":"Buy milk","status":"done","due_at":null,"created_at":"2026-10-29T21:29:19.412Z"}
```

Change its title, leaving the status alone:

```sh
curl -X PATCH http://localhost:3000/todos/01a18991-3d34-78a5-b36d-e043061c61a0 \
  -H 'content-type: application/json' \
  -d '{"title": "Buy oat milk"}'
```

```text
{"id":"01a18991-3d34-78a5-b36d-e043061c61a0","title":"Buy oat milk","status":"done","due_at":null,"created_at":"2026-10-29T21:29:19.412Z"}
```

Delete it, then try again:

```sh
curl -i -X DELETE http://localhost:3000/todos/01a18991-3d34-78a5-b36d-e043061c61a0
curl -X DELETE http://localhost:3000/todos/01a18991-3d34-78a5-b36d-e043061c61a0
```

```text
HTTP/1.1 204 No Content
x-request-id: req_2c14f1f910abf38f
date: Thu, 29 Oct 2026 21:29:19 GMT

{"error":{"code":"todo_not_found","message":"Not found.","request_id":"req_be2371df39b011e2"}}
```

## What you have

A todo API you can add to, read, change and delete from. You've checked
each route by hand with `curl`. Next, on page 8, you write tests that do
it for you, each against a fresh database.

Read more in the reference: [The operations every entity gets](../language/06-reads-and-writes.md#the-operations-every-entity-gets)
and [All-or-nothing actions and concurrent changes](../language/06-reads-and-writes.md#all-or-nothing-actions-and-concurrent-changes).

# Reading todos
<!-- topic: learn-reading -->

The app stores todos but can't show them. On this page you add
`GET /todos`, which lists them newest first and can show only the open
or only the done ones, and `GET /todos/{id}`, which fetches one.

## A query is a named read

Jadpo has no hand-written SQL. A read is a query: a declaration that
says which entity to read, which records, in what order and how many.
The compiler turns it into SQL. Queries have their own folder,
`queries/`, one per file:

`domains/todos/queries/todo_by_id.jadpo`:

```jadpo check project=learn-todo-06 file=domains/todos/queries/todo_by_id.jadpo
use todos/entities { Todo, TodoNotFound }
use todos/types { TodoView }

/// One todo.
query todo_by_id(todo_id: Todo.id) fails TodoNotFound -> TodoView {
    from: Todo
    where: Todo.id == todo_id
    missing: TodoNotFound
}
```

- `todo_id: Todo.id` is the query's parameter. `Todo.id` is a type of its
  own: a UUID that is a todo's id. So a query that wants a `Todo.id`
  can't be handed some other record's id by mistake.
- `from: Todo` is the entity it reads.
- `where:` picks the records, written like a condition in code:
  `Todo.id == todo_id`.
- The result type says how many records come back. `TodoView` means
  exactly one, and when there's none the query fails with the failure
  `missing:` names. `TodoView?` would mean one or none, and a list
  type means many, as below.
- `TodoNotFound` is a failure every entity gets, without your declaring
  it: kind `not_found`, so a `404`. You import it from `entities/`,
  beside `Todo`.

A query only reads. It can't write, call an action, or read the clock.

## A list, in order

The list takes a filter, which the caller sends in the query string:

`domains/todos/types/todo_filter.jadpo`:

```jadpo check project=learn-todo-06 file=domains/todos/types/todo_filter.jadpo
use todos/types { TodoStatus }

/// What a caller may ask for when listing todos.
type TodoFilter = Input {
    status: TodoStatus {
        optional: true
    }
}
```

`optional: true` means the caller may leave `status` out. Unlike
`default: none` on page 3, it gives the field no value at all, which the
next query uses.

`domains/todos/queries/list_todos.jadpo`:

```jadpo check project=learn-todo-06 file=domains/todos/queries/list_todos.jadpo
use todos/entities { Todo }
use todos/types { TodoFilter, TodoView }

/// Todos, newest first, with only one status when the caller asks.
query list_todos(filter: TodoFilter) -> List<TodoView> {
    from: Todo
    where_present: Todo.status == filter.status
    order: [Todo.created_at desc]
    limit: 100
}
```

- A list result, `List<TodoView>`, needs `order:` and `limit:`, so a
  list can never grow without bound by accident. For a list too long
  for one answer, a query returns `Page<TodoView>` instead, a page at a
  time; the reference covers it.
- `where_present:` is a condition that applies only when the caller
  sent the field. With no `status`, every todo comes back.
- `Todo.created_at desc` is newest first.

## Routes that read

A `GET` route runs a query. A route that changes something, such as a
`POST`, runs an action:

`domains/todos/routes/todos.jadpo`, changed. The two `GET` routes and their `use` lines are new.

```jadpo check project=learn-todo-06 file=domains/todos/routes/todos.jadpo
use todos/entities { Todo }
use todos/types { NewTodo, TodoFilter, TodoView }
use todos/actions { add_todo }
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
}
```

`{todo_id: Todo.id}` is a placeholder in the path, with its type. The
segment of the URL is checked as a `Todo.id`, a UUID, and handed to the
query by name.

## Let people read todos

Check the project. Besides the record, two things need fixing. The first
is permission: the entity says who may create a todo, but not who may
read one, so the compiler refuses both new routes:

```text
error: `GET /todos` reads `Todo` records, which nobody may do
  reason: Policy is always on: every create, read, update and delete a route reaches needs a grant, and `Todo`'s policy grants nobody `read`, so this route could never succeed.
  next: Add `read: [...]` to `Todo`'s `policy:`, naming who may read one (human)
```

(The same error follows for `GET /todos/{todo_id}`, and each has its
code frame and help line, left out here.) Add `read:` to `Todo`'s
policy:

`domains/todos/entities/todo.jadpo`, changed. The `read:` line is new.

```jadpo check project=learn-todo-06 file=domains/todos/entities/todo.jadpo
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
    }
}
```

The second is new:

```text
error: The entities have changed since the latest migration
 --> domains/todos/entities/todo.jadpo:4:8
  |
4 | entity Todo {
  |        ^^^^ changed
  reason: `jadpo check` compares the database schema the entities ask for with the latest migration's snapshot (`database/migrations/20261029T1000_initial/snapshot.json`): new index for query `list_todos`.
  next: Run `jadpo migrate new index_list_todos` (agent)
  rule: migration.out_of_date
  help: docs/errors/migration.out_of_date.md
```

You didn't change the entity, but the compiler saw that `list_todos`
sorts by `created_at`, and that a table of todos needs an index for
that to stay fast. So it asks for a migration that makes the index:

```sh
jadpo migrate new index_list_todos
```

```text
  compared the entities with 20261029T1000_initial

  Todo
    + index  (created_at, id)  for list_todos

Wrote database/migrations/20261029T1100_index_list_todos/
    snapshot.json  the whole schema at this point
    postgres.sql   1 statement: create index

next: jadpo check  confirms the migration matches the entities
```

The index includes `id` because the query's order does too: two todos
made at the same moment still come back in the same order every time.
`jadpo sql list_todos` shows the SQL the query sends:

```text
query list_todos
  SELECT "id", "title", "status", "due_at", "created_at"
  FROM "todo"
  WHERE ($1 IS NULL OR "status" = $1)
  ORDER BY "created_at" DESC, "id" DESC
  LIMIT 100

  reads: Todo, a list of TodoView
  file: domains/todos/queries/list_todos.jadpo:5
  sends: 1 statement
  $1: filter.status · text
```

It reads only the columns `TodoView` sends. Run `jadpo api update`, and
`jadpo check` passes.

## Try it

Run `jadpo dev`, add two todos as on page 4, then list them:

```sh
curl http://localhost:3000/todos
```

```text
[{"id":"01a18990-d0bd-7593-a195-4202a0086aed","title":"Post the letter","status":"open","due_at":null,"created_at":"2026-10-29T21:28:51.645Z"},{"id":"01a18990-d0a8-77e8-8892-8a07e884a2d6","title":"Buy milk","status":"open","due_at":null,"created_at":"2026-10-29T21:28:51.624Z"}]
```

Only the done ones, of which there are none yet:

```sh
curl "http://localhost:3000/todos?status=done"
```

```text
[]
```

One todo, by its id. Use an id from your own list:

```sh
curl http://localhost:3000/todos/01a18990-d0bd-7593-a195-4202a0086aed
```

```text
{"id":"01a18990-d0bd-7593-a195-4202a0086aed","title":"Post the letter","status":"open","due_at":null,"created_at":"2026-10-29T21:28:51.645Z"}
```

An id no todo has, and one that isn't an id at all:

```sh
curl -i http://localhost:3000/todos/01a1898f-0000-7000-8000-000000000000
curl http://localhost:3000/todos/42
```

```text
HTTP/1.1 404 Not Found
content-type: application/json
x-request-id: req_34931ef0e7674e96
content-length: 94
date: Thu, 29 Oct 2026 21:28:51 GMT

{"error":{"code":"todo_not_found","message":"Not found.","request_id":"req_34931ef0e7674e96"}}
{"error":{"code":"invalid_input","message":"Request input is invalid.","request_id":"req_c85d30a895414635","details":{"problems":[{"field":"todo_id","rule":"format"}]}}}
```

## What you have

Four routes: add a todo, list them, filter them and fetch one. Next, on
page 7, you change and delete todos.

Read more in the reference: [Named queries](../language/06-reads-and-writes.md#named-queries),
[Filters](../language/06-reads-and-writes.md#filters) and
[Order and pages](../language/06-reads-and-writes.md#order-and-pages).

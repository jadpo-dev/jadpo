---
removes: policy.jadpo
---

# Storing todos
<!-- topic: learn-storing -->

So far `add_todo` sends a todo back and forgets it. On this page each
todo is kept in PostgreSQL, and the route answers `201 Created` with the
stored todo, its id and when it was made. You need PostgreSQL installed,
as page 1 said; you don't create a database.

## An entity is a table with types

An entity is a kind of record the app stores. Think of it as a table,
where each field is a column with a Jadpo type, so the rules you gave
the type hold for what's stored too. The entity also says who may
create, read, change and delete its records, and the compiler applies
that to every query and write.

Entities have their own folder, `entities/`:

`domains/todos/entities/todo.jadpo`:

```jadpo check project=learn-todo-04 file=domains/todos/entities/todo.jadpo
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
    }
}
```

- `id: Uuid` is the record's key. Every entity has one, and Jadpo makes
  a new one for each record, so you never set it.
- `title`, `status` and `due_at` use the types from page 3.
- `created_at` is filled in with the time the record is made:
  `generated: Generated.on_create`. Code never sets it either.
- `policy:` says who may do what with a todo. For now, anyone may
  create one: `Access.public`, as on page 2. Page 12 changes this to
  each todo's owner.

Because the todo now carries its own rule, `policy.jadpo` from page 2
isn't needed any more. Delete it. An action takes its permission from
the entities it uses, which is what the page 2 error meant by "reaches".
The root `policy.jadpo` is for actions that use no entity, as
`add_todo` did on page 2.

## Make the response from the entity

`TodoView` was a list of fields. Now that a todo is stored, the
response can name the entity's fields it sends instead:

`domains/todos/types/todo_view.jadpo`, changed. Replace the whole file.

```jadpo check project=learn-todo-04 file=domains/todos/types/todo_view.jadpo
use todos/entities { Todo }

/// A todo, as the app sends it back.
type TodoView = Output {
    pick: [Todo.id, Todo.title, Todo.status, Todo.due_at, Todo.created_at]
}
```

`pick:` takes fields from the entity, with their types. The response
sends exactly these, so a field you add to `Todo` later stays private
until you pick it.

## Write the record

`Todo.create` adds a record. Every entity gets it, without your writing
it:

`domains/todos/actions/add_todo.jadpo`, changed. Replace the whole file: `new_view` is gone, and `add_todo` stores the todo.

```jadpo check project=learn-todo-04 file=domains/todos/actions/add_todo.jadpo
use todos/entities { Todo }
use todos/types { NewTodo, TodoStatus, TodoView }

/// Adds an open todo.
action add_todo(input: NewTodo) -> TodoView {
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

- `set:` gives every field except `id` and the generated ones. Leave one
  out and the compiler says which.
- `Todo.create` gives back the record as stored, with its `id` and
  `created_at`.
- `var todo: Todo = ...` names a value, like `const` in TypeScript: it
  never changes once set. Every name says its type. For a name you do
  want to change, Jadpo has `var mut`, which this path doesn't need.

Jadpo writes the SQL. `jadpo sql add_todo` shows it:

```text
write Todo.create
  INSERT INTO "todo" ("id", "title", "status", "due_at", "created_at")
  VALUES ($1, $2, $3, $4, $5)
  RETURNING "id", "title", "status", "due_at", "created_at"

  writes: creates a Todo
  file: domains/todos/actions/add_todo.jadpo:6
  sends: 1 statement
  $1: a new identity · uuid
  $2: input.title · text
  $3: TodoStatus.open · text
  $4: input.due_at · timestamptz
  $5: clock.now · timestamptz
```

`clock.now` is the current time, which page 5 covers.

## Answer 201 Created

A route that makes something should say so. Add `status:` to the route.
Without it a route answers `200`; page 7 uses `no_content`, for `204`.

`domains/todos/routes/todos.jadpo`, changed. The `status:` line is new.

```jadpo check project=learn-todo-04 file=domains/todos/routes/todos.jadpo
use todos/types { NewTodo, TodoView }
use todos/actions { add_todo }

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
}
```

## Write the migration

Check the project:

```sh
jadpo check
```

It reports the API record, which `jadpo api update` fixes, and something
new. The "system tables" it lists are Jadpo's own, explained below:

```text
error: No migration creates these entities' tables yet
 --> domains/todos/entities/todo.jadpo:4:8
  |
4 | entity Todo {
  |        ^^^^ no table yet
  reason: `jadpo check` compares the database schema the entities ask for with the migrations in `database/migrations/`, and there are none: new entity `Todo`, new system table for circuit breakers, new system table for deliveries, new system table for handled deliveries, new system table for housekeeping, and 3 more.
  next: Run `jadpo migrate new initial` (agent)
  rule: migration.out_of_date
  help: docs/errors/migration.out_of_date.md
```

A migration is the SQL that changes the database to match your
entities, as with any ORM. You never write one: `jadpo migrate new`
compares the entities with the last migration and writes the next.
`initial` is the migration's name, saying what it is for.

```sh
jadpo migrate new initial
```

```text
  compared the entities with an empty database

  Todo
    + table  todo                      new entity

  System tables
    + table  jadpo_circuit_breakers    system table for circuit breakers
    + table  jadpo_deliveries          system table for deliveries
    + table  jadpo_handled_deliveries  system table for handled deliveries
    + table  jadpo_housekeeping        system table for housekeeping
    + table  jadpo_idempotency_keys    system table for idempotency keys
    + table  jadpo_operator_actions    system table for operator actions
    + table  jadpo_outbox              system table for outbox

Wrote database/migrations/20261029T1000_initial/
    snapshot.json  the whole schema at this point
    postgres.sql   14 statements: create table, add unique rule, add reference, create index

next: jadpo check  confirms the migration matches the entities
```

The folder's name starts with the date and time you ran it, so yours
differs. It holds two files, both written by `jadpo migrate new`: you
commit them, and never edit them. `postgres.sql` is the SQL, and this
is the part for your todos:

```text
CREATE TABLE "todo" (
    "id" uuid NOT NULL,
    "title" text NOT NULL,
    "status" text NOT NULL,
    "due_at" timestamptz(3),
    "created_at" timestamptz(3) NOT NULL,
    CONSTRAINT "todo_pkey" PRIMARY KEY ("id")
);
```

The tables starting `jadpo_` are Jadpo's own. It keeps its bookkeeping
in your database, such as work waiting to be retried, so that it commits
or rolls back with your data. You don't use them directly.

`snapshot.json` is the whole schema as it stands after this migration;
the next `jadpo migrate new` compares against it.

Run `jadpo api update` for the record too, and `jadpo check` passes:

```text
checked: 8 files
```

## Run it

```sh
jadpo dev
```

```text
jadpo dev, started 21:27:33
listening on http://localhost:3000
    routes    2 routes
    database  disposable PostgreSQL, gone when dev stops
serving: saves reload, press ctrl-c to quit
```

`jadpo dev` started a PostgreSQL database of its own and ran the
migrations on it. It is thrown away when `jadpo dev` stops, so each run
starts empty. Add two todos:

```sh
curl -i -X POST http://localhost:3000/todos \
  -H 'content-type: application/json' \
  -d '{"title": "Buy milk"}'
curl -X POST http://localhost:3000/todos \
  -H 'content-type: application/json' \
  -d '{"title": "Post the letter", "due_at": "2099-11-02T09:00:00Z"}'
```

```text
HTTP/1.1 201 Created
content-type: application/json
x-request-id: req_9d1edfda96680c0f
content-length: 134
date: Thu, 29 Oct 2026 21:54:17 GMT

{"id":"01a189a8-1a08-779a-9a4b-9d7b9a671b80","title":"Buy milk","status":"open","due_at":null,"created_at":"2026-10-29T21:54:17.736Z"}
{"id":"01a189a8-1a16-707d-bd55-ffa1a0560ec4","title":"Post the letter","status":"open","due_at":"2099-11-02T09:00:00Z","created_at":"2026-10-29T21:54:17.750Z"}
```

Your ids and times will differ. A `due_at` of `none` is sent as `null`.

## What you have

Todos that are stored, with an id and a time each, and a migration
history you commit. You can't read them back yet: on page 6 you add
`GET /todos`. First, page 5 makes the app refuse a due date that has
already passed.

Read more in the reference: [Entities and the id](../language/05-data.md#entities-and-the-id),
[Migrations](../language/05-data.md#migrations) and
[The operations every entity gets](../language/06-reads-and-writes.md#the-operations-every-entity-gets).

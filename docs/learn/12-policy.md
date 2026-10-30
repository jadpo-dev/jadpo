# Who may see what
<!-- topic: learn-policy -->

Since page 10, any signed-in person can read and change anyone's
todos. On this page each todo becomes visible to its owner only. It
takes one line per kind of access, and no change to any query, action
or route.

## Policy

Policy is the list, on each entity, of who may create, read, update and
delete its records. You've used it since page 4. The ways to name who
are:

| In a policy list | Means |
|---|---|
| `Access.public` | anyone, signed in or not |
| `Access.authenticated` | anyone signed in |
| `Access.system` | the app itself, with no person behind the request |
| a field holding a `User.id`, such as `owner_id` | the user that field names |
| `id`, on the user entity itself | that user |

Apps with teams or customers also give access by role, which the
reference covers.

Anything not listed is refused. Jadpo checks this for every query and
every write, so there is no handler where a check can be forgotten.

## Owners only

Change each list in `Todo`'s policy to `owner_id`:

`domains/todos/entities/todo.jadpo`, changed. Each policy list is now `[owner_id]`.

```jadpo check project=learn-todo-12 file=domains/todos/entities/todo.jadpo
use todos/entities { User }
use todos/types { TodoStatus, TodoTitle }

/// Something to do, kept in the database.
entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    title: TodoTitle
    status: TodoStatus
    due_at: Instant?
    created_at: Instant {
        generated: Generated.on_create
    }

    policy: {
        create: [owner_id]
        read: [owner_id]
        update: [owner_id]
        delete: [owner_id]
    }
}
```

That is the whole change. `create: [owner_id]` means a signed-in user
may create a todo whose `owner_id` is their own id, which `add_todo`
already sets. The others mean only the owner may read, change or delete
a todo.

## How it reaches the database

Jadpo adds the policy to the SQL of every query. `jadpo sql list_todos`
now shows a condition you didn't write:

```text
query list_todos
  SELECT "id", "title", "status", "due_at", "created_at"
  FROM "todo"
  WHERE ($1 IS NULL OR "status" = $1)
    AND "todo"."owner_id" = $2
  ORDER BY "created_at" DESC, "id" DESC
  LIMIT 100

  reads: Todo, a list of TodoView
  file: domains/todos/queries/list_todos.jadpo:5
  sends: 1 statement
  $1: filter.status · text
  $2: the signed-in user's id, or none · uuid
```

So `GET /todos` lists only the caller's todos, and the database does the
filtering. The same goes for `todo_by_id` and `Todo.get`.

## What someone else gets

When Bob asks for one of Ann's todos, the answer is `404`, the same as
for a todo that doesn't exist. A `403` would tell Bob the todo is there.
The rule is: a record you may not read doesn't exist, as far as you can
tell. A change you may not make to a record you can read is a `403`,
`not_permitted`. This app has no such case, since only the owner may
read a todo; one would be a todo everyone signed in may read but only
its owner may change.

A route test shows it:

`domains/todos/routes/todos.test.jadpo`, changed. The last test is new.

```jadpo check project=learn-todo-12 file=domains/todos/routes/todos.test.jadpo
use todos/entities { Todo, User }
use todos/types { NewTodo, TodoChanges, TodoFilter, TodoStatus }
use todos/fixtures { milk, someone }

test "adding a todo answers 201 with the todo" {
    var ann: User = someone("ann@example.com")
    var response: Response = request {
        method: HttpMethod.post
        path: "/todos"
        as: Principal.user(ann)
        input: NewTodo {
            title: "Buy milk"
        }
    }
    assert response.status == 201
}

test "a finished todo leaves the open list" {
    var ann: User = someone("ann@example.com")
    var todo: Todo = milk(ann)
    var changed: Response = request {
        method: HttpMethod.patch
        path: "/todos/{todo_id}"
        params: {
            todo_id: todo.id
        }
        as: Principal.user(ann)
        input: TodoChanges {
            status: TodoStatus.done
        }
    }
    assert changed.status == 200
    var open: Response = request {
        method: HttpMethod.get
        path: "/todos"
        as: Principal.user(ann)
        input: TodoFilter {
            status: TodoStatus.open
        }
    }
    assert open.status == 200
    assert open.output == []
}

test "a deleted todo is gone" {
    var ann: User = someone("ann@example.com")
    var todo: Todo = milk(ann)
    var deleted: Response = request {
        method: HttpMethod.delete
        path: "/todos/{todo_id}"
        params: {
            todo_id: todo.id
        }
        as: Principal.user(ann)
    }
    assert deleted.status == 204
    var fetched: Response = request {
        method: HttpMethod.get
        path: "/todos/{todo_id}"
        params: {
            todo_id: todo.id
        }
        as: Principal.user(ann)
    }
    assert fetched.status == 404
    assert fetched.code == "todo_not_found"
}

test "adding a todo needs someone signed in" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/todos"
        input: NewTodo {
            title: "Buy milk"
        }
    }
    assert response.status == 401
    assert response.code == "authentication_required"
}

test "nobody sees another person's todos" {
    var ann: User = someone("ann@example.com")
    var bob: User = someone("bob@example.com")
    var todo: Todo = milk(ann)
    var listed: Response = request {
        method: HttpMethod.get
        path: "/todos"
        as: Principal.user(bob)
        input: TodoFilter {}
    }
    assert listed.output == []
    var fetched: Response = request {
        method: HttpMethod.get
        path: "/todos/{todo_id}"
        params: {
            todo_id: todo.id
        }
        as: Principal.user(bob)
    }
    assert fetched.status == 404
    var changed: Response = request {
        method: HttpMethod.patch
        path: "/todos/{todo_id}"
        params: {
            todo_id: todo.id
        }
        as: Principal.user(bob)
        input: TodoChanges {
            status: TodoStatus.done
        }
    }
    assert changed.status == 404
}
```

`TodoFilter {}` is a filter with nothing set, so every todo Bob may see.

```sh
jadpo test
```

```text
tested: 9 passed, 3 action tests, 6 route tests in 1.1 s
```

## The permissions report

`jadpo check` writes a summary of every entity's policy to
`build/report/permissions.md`, for a reviewer to read in one place:

```text
# Permissions

Who may do what to each entity's records, from the `policy:` beside each entity and root `policy.jadpo`. Written by `jadpo check`.

A list means any of the callers it names, and nobody else. `Access.authenticated` is anyone signed in, `Access.public` anyone at all, and `Access.system` schedules and subscribers.

| `todos.Todo` | create | read | update | delete |
|---|---|---|---|---|
| the record | `owner_id` | `owner_id` | `owner_id` | `owner_id` |

Only an action listed under `actions:` changes `owner_id`.

| `todos.User` | create | read | update | delete |
|---|---|---|---|---|
| the record | `Access.system` | `id`, `Access.system` | nobody | nobody |

## Actions listed separately

None: every action takes its permission from the entities it reaches.
```

"Schedules and subscribers" are kinds of background work, which this
path doesn't cover. "Only an action listed under `actions:` changes
`owner_id`" means that changing who owns a todo takes an action you
list by name in `policy.jadpo`, as page 2 did for `add_todo`.

## What you have

Every todo private to its owner, enforced in the SQL, with a test that
proves it and a report a reviewer can read. Next, on page 13, you build
the app and run it on a server.

Read more in the reference: [Grants](../language/09-policy.md#grants),
[A user field on the record](../language/09-policy.md#a-user-field-on-the-record)
and [When policy says no](../language/09-policy.md#when-policy-says-no).

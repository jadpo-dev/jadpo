# Users and the principal
<!-- topic: learn-users -->

Until now anyone could call every route, and a todo belonged to no one.
On this page the app gets users, every todo gets an owner, and every
route needs someone signed in. How people actually sign in, by email,
is page 11. Here, tests sign a user in directly, so you can build and
test everything else first.

## Users

A user is a record like any other, so it's an entity:

`domains/todos/entities/user.jadpo`:

```jadpo check project=learn-todo-10 file=domains/todos/entities/user.jadpo
/// Someone who keeps a todo list.
entity User {
    id: Uuid
    email: Email {
        unique: true
    }

    policy: {
        read: [id]
    }
}
```

- `unique: true` means no two users have the same address. The
  migration makes it a rule in the database too.
- `read: [id]` means a user may read their own record. In a policy list,
  a field that holds a user's id names that user, and on the user
  entity itself, `id` is the user's own.

## Who is calling: the principal

The principal is Jadpo's word for whoever is calling. `app.jadpo` says
which entity a signed-in caller is, and which field finds them:

`app.jadpo`, changed. The `use` line and the `principal` block are new.

```jadpo check project=learn-todo-10 file=app.jadpo
use todos/entities { User }

application {
    jadpo: "0.2"
}

principal {
    user: {
        subject: User.email
    }
}
```

`user: { subject: User.email }` says a signed-in caller is a `User`. The
subject is the field that tells users apart when they sign in: here,
their email address.

Three names come from this, and the page uses all three:

| Name | What it is |
|---|---|
| `principal { }` | the block in `app.jadpo` above |
| `Principal.user` | the type of a signed-in user, as a parameter |
| `current_principal.user` | in a route: the user who made this request |

## Sessions

`authentication.jadpo`, next to `app.jadpo`, says how a signed-in person
is remembered between requests:

`authentication.jadpo`:

```jadpo check project=learn-todo-10 file=authentication.jadpo
authentication {
    browser_session: {
        cookie: "todo_session"
    }
}
```

`browser_session:` keeps a person signed in with a cookie, here called
`todo_session`. The session itself is kept in the database, in one of
Jadpo's own tables. Page 11 adds the way to start one, by email.

## Each todo has an owner

`domains/todos/entities/todo.jadpo`, changed. The `use todos/entities` line, the `owner_id` field and the policy lists are new.

```jadpo check project=learn-todo-10 file=domains/todos/entities/todo.jadpo
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
        create: [Access.authenticated]
        read: [Access.authenticated]
        update: [Access.authenticated]
        delete: [Access.authenticated]
    }
}
```

- `owner_id: User.id` is a reference: a foreign key to a user's `id`.
  `on_delete: OnDelete.cascade` deletes a user's todos when the user is
  deleted.
- The policy is now `Access.authenticated`: anyone signed in. That's
  still too loose, since anyone signed in can read everyone's todos.
  Page 12 fixes that.

`add_todo` takes the caller, and sets the owner from them:

`domains/todos/actions/add_todo.jadpo`, changed. The `caller` parameter and the `owner_id:` line are new; `jadpo fmt` put the parameters one to a line.

```jadpo check project=learn-todo-10 file=domains/todos/actions/add_todo.jadpo
use todos/entities { Todo }
use todos/types { NewTodo, TodoStatus, TodoView }
use todos/failures { DueDateInPast, DueDateTooFar }

/// Adds an open todo, refusing a due date that has passed or is too far
/// ahead.
action add_todo(
    input: NewTodo,
    caller: Principal.user
) fails DueDateInPast, DueDateTooFar -> TodoView {
    match input.due_at {
        some(due) -> {
            if (due <= clock.now) {
                reject DueDateInPast
            }
            if (due > clock.now + config.max_due_ahead) {
                reject DueDateTooFar
            }
        }
        none -> {}
    }
    var todo: Todo = Todo.create {
        set: {
            owner_id: caller.id
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

`caller.id` is the caller's `User.id`. The route passes
`current_principal.user`:

`domains/todos/routes/todos.jadpo`, changed. Every `authentication: RouteAuthentication.none` line is gone, and `add_todo` gets `current_principal.user`.

```jadpo check project=learn-todo-10 file=domains/todos/routes/todos.jadpo
use todos/entities { Todo }
use todos/types { NewTodo, TodoChanges, TodoFilter, TodoView }
use todos/actions { add_todo, change_todo, delete_todo }
use todos/queries { list_todos, todo_by_id }

/// The todo list.
routes Todos {
    prefix: /todos

    /// Adds a todo.
    POST / {
        input: NewTodo
        output: TodoView
        status: SuccessStatus.created
        run: add_todo(input, current_principal.user)
    }

    /// Lists todos, newest first.
    GET / {
        input: TodoFilter
        output: List<TodoView>
        run: list_todos(input)
    }

    /// One todo.
    GET /{todo_id: Todo.id} {
        output: TodoView
        run: todo_by_id(todo_id)
    }

    /// Changes a todo's title or status.
    PATCH /{todo_id: Todo.id} {
        input: TodoChanges
        output: TodoView
        run: change_todo(todo_id, input)
    }

    /// Deletes a todo.
    DELETE /{todo_id: Todo.id} {
        status: SuccessStatus.no_content
        run: delete_todo(todo_id)
    }
}
```

With the `authentication: RouteAuthentication.none` lines gone, every
route needs someone signed in, which is the default from page 2.

## The migration needs a decision

Run `jadpo migrate new add_users`. This time it stops to ask something
(the list of changes it prints first is left out here):

```text
Wrote database/migrations/20261029T1200_add_users/
    snapshot.json  the whole schema at this point
    postgres.sql   10 statements: create table, add column, decision to make, add unique rule, add reference, create index
    changes.jadpo  1 decision to make

decision: existing `Todo` records need a `owner_id`
 --> database/migrations/20261029T1200_add_users/changes.jadpo:4
  next: Set `backfill:` for `Todo.owner_id` (human)
```

Every todo needs an owner now, so the migration has to know what owner
to give the todos already in a database. A migration answers a question
like this in the `changes.jadpo` file it wrote, with `backfill:`, a
value for the existing rows. But there's no right answer here: no users
exist yet.

This app has never run anywhere except `jadpo dev`, whose database is
thrown away each time, so no stored todo needs an owner. **Only in a
case like this, where no real database has the app's data yet**, the
simplest answer is to start the migrations again: delete the
`database/migrations` folder and write one migration for the app as it
is now.

```sh
rm -r database/migrations
jadpo migrate new initial
```

```text
Wrote database/migrations/20261029T1200_initial/
    snapshot.json  the whole schema at this point
    postgres.sql   23 statements: create table, add unique rule, add reference, create index

next: jadpo check  confirms the migration matches the entities
```

Once the app has a real database anywhere, never do this: answer the
decision in `changes.jadpo` instead, as the reference describes. Update
the record too: it now says every route needs a session.

## Tests that sign in

The tests from page 8 now fail to check, because `add_todo` takes a
caller and a todo needs an owner. Tests need users, and a fixture makes
one:

`domains/todos/fixtures/someone.jadpo`:

```jadpo check project=learn-todo-10 file=domains/todos/fixtures/someone.jadpo
use todos/entities { User }
use todos/failures { AlreadyAUser }

/// A user with the address given.
fixture someone(email: User.email) -> User {
    return attempt User.create {
        set: {
            email: email
        }
        conflict: {
            User.email: AlreadyAUser
        }
    }
}
```

It needs a failure for an address that's taken:

`domains/todos/failures/already_a_user.jadpo`:

```jadpo check project=learn-todo-10 file=domains/todos/failures/already_a_user.jadpo
/// An address that already belongs to a user.
type AlreadyAUser = Failure {
    kind: FailureKind.conflict
    message: "That address already belongs to a user."
}
```

`conflict:` is part of a create. A create that could break a `unique:`
rule must say which failure that becomes, so the compiler can be sure
nothing is left unhandled. `attempt` passes the failure on, and a
fixture that fails stops the test that called it.

`milk` takes the owner:

`domains/todos/fixtures/milk.jadpo`, changed. It takes `owner`, and sets `owner_id`.

```jadpo check project=learn-todo-10 file=domains/todos/fixtures/milk.jadpo
use todos/entities { Todo, User }
use todos/types { TodoStatus }

/// An open todo to buy milk, belonging to the user given.
fixture milk(owner: User) -> Todo {
    return Todo.create {
        set: {
            owner_id: owner.id
            title: "Buy milk"
            status: TodoStatus.open
            due_at: none
        }
    }
}
```

An action test passes the caller itself, as `Principal.user(ann)`:

`domains/todos/actions/add_todo.test.jadpo`, changed. Each test makes `ann` and passes `Principal.user(ann)` to `add_todo`.

```jadpo check project=learn-todo-10 file=domains/todos/actions/add_todo.test.jadpo
use todos/entities { User }
use todos/types { NewTodo, TodoStatus, TodoView }
use todos/failures { DueDateInPast, DueDateTooFar }
use todos/fixtures { someone }

test "a new todo is open" {
    var ann: User = someone("ann@example.com")
    var todo: TodoView = attempt add_todo(
        NewTodo {
            title: "Buy milk"
        },
        Principal.user(ann)
    )
    assert todo.status == TodoStatus.open
    assert todo.due_at == none
}

test "a due date that has passed is refused" {
    clock: "2026-11-01T12:00:00Z"
    var ann: User = someone("ann@example.com")
    assert add_todo(
        NewTodo {
            title: "Buy milk"
            due_at: "2026-11-01T11:59:59Z"
        },
        Principal.user(ann)
    ) fails DueDateInPast
}

test "a due date further ahead than the setting allows is refused" {
    clock: "2026-11-01T12:00:00Z"
    var ann: User = someone("ann@example.com")
    assert add_todo(
        NewTodo {
            title: "Renew passport"
            due_at: "2026-12-15T12:00:00Z"
        },
        Principal.user(ann)
    ) fails DueDateTooFar
}
```

A route test signs in with `as:`, which makes the request as that user.
The last test checks that a request without anyone signed in is
refused:

`domains/todos/routes/todos.test.jadpo`, changed. Each request has `as: Principal.user(ann)`, `milk` takes `ann`, and the last test is new.

```jadpo check project=learn-todo-10 file=domains/todos/routes/todos.test.jadpo
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
```

```sh
jadpo test
```

```text
tested: 7 passed, 3 action tests, 4 route tests in 1.2 s
```

## Try it

In `jadpo dev`, every route now needs someone signed in, and there's no
way to sign in yet:

```sh
curl http://localhost:3000/todos
```

```text
{"error":{"code":"authentication_required","message":"Sign in first.","request_id":"req_6a1019db203240af"}}
```

## What you have

Users, todos that each have an owner, routes that need someone signed
in, and tests that sign in as a user. Next, on page 11, people sign in
by email.

Read more in the reference: [The principal](../language/08-identity.md#the-principal),
[Who is calling](../language/08-identity.md#who-is-calling-current_principal)
and [Migrations](../language/05-data.md#migrations).

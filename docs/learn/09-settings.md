# Settings
<!-- topic: learn-settings -->

Some values differ between your machine and the server the app runs
on: the site's address, a mail server's password, a limit you want to
tune without a release. On this page you add the app's first setting,
how far ahead a todo may be due, read from an environment variable.
Page 11 then adds the settings sign-in needs.

## Declare it once

Every setting is a field of the one `config { }` block, in
`config.jadpo` next to `app.jadpo`:

`config.jadpo`:

```jadpo check project=learn-todo-09 file=config.jadpo
config {
    /// How far ahead a todo may be due.
    max_due_ahead: Duration {
        default: 365d
        test: 30d
    }
}
```

- The field's name gives its environment variable, in capitals:
  `max_due_ahead` reads `MAX_DUE_AHEAD`.
- Its type checks the value before the app starts. `Duration` is a
  standard type for a length of time, written like `365d`, `12h` or
  `30m`.
- `default:` is the value when the variable isn't set. A setting with no
  default is required, and the app won't start without it.
- `test:` is the value tests use, whatever the environment says, so
  tests don't change from one machine to the next.

Code reads it as `config.max_due_ahead`, anywhere.

## Use it

A due date too far ahead is a new failure:

`domains/todos/failures/due_date_too_far.jadpo`:

```jadpo check project=learn-todo-09 file=domains/todos/failures/due_date_too_far.jadpo
/// A todo can't be due further ahead than the app allows.
type DueDateTooFar = Failure {
    kind: FailureKind.invalid_value
    message: "That due date is too far ahead."
}
```

and `add_todo` checks it beside the first one:

`domains/todos/actions/add_todo.jadpo`, changed. `DueDateTooFar` is new in the `use` line, the `fails` list and a second `if`.

```jadpo check project=learn-todo-09 file=domains/todos/actions/add_todo.jadpo
use todos/entities { Todo }
use todos/types { NewTodo, TodoStatus, TodoView }
use todos/failures { DueDateInPast, DueDateTooFar }

/// Adds an open todo, refusing a due date that has passed or is too far
/// ahead.
action add_todo(input: NewTodo) fails DueDateInPast, DueDateTooFar -> TodoView {
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

`clock.now + config.max_due_ahead` adds a length of time to a moment,
which gives a moment. Jadpo knows which sums make sense: adding two
moments is an error.

Add a test for it to `add_todo.test.jadpo`, the last one here. Its due
date is 44 days ahead, past the `test:` value of 30 days:

`domains/todos/actions/add_todo.test.jadpo`, changed. The last test is new, and so is `DueDateTooFar` in the `use` line.

```jadpo check project=learn-todo-09 file=domains/todos/actions/add_todo.test.jadpo
use todos/types { NewTodo, TodoStatus, TodoView }
use todos/failures { DueDateInPast, DueDateTooFar }

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

test "a due date further ahead than the setting allows is refused" {
    clock: "2026-11-01T12:00:00Z"
    assert add_todo(
        NewTodo {
            title: "Renew passport"
            due_at: "2026-12-15T12:00:00Z"
        }
    ) fails DueDateTooFar
}
```

Update the record for the new failure. Then:

```sh
jadpo check
jadpo test
```

```text
checked: 20 files in 0.1 s
tested: 6 passed, 3 action tests, 3 route tests in 0.9 s
```

## Set it

`jadpo config check` lists every setting, whether it is set, and the
variable it reads. It never prints a value:

```text
Values
  max_due_ahead  default  MAX_DUE_AHEAD

config ready: 1 setting
```

The columns are the setting, where its value comes from (`default`
here; `set` when the environment or `.env.local` gives one; `missing`
when a required one has none), and the variable it reads.

Run `jadpo dev`, and add a todo due in 2099. With the default limit of a
year, it's too far ahead:

```sh
curl -X POST http://localhost:3000/todos \
  -H 'content-type: application/json' \
  -d '{"title": "Plant a tree", "due_at": "2099-11-02T09:00:00Z"}'
```

```text
{"error":{"code":"due_date_too_far","message":"That due date is too far ahead.","request_id":"req_be315e671ffe4dd5"}}
```

Stop `jadpo dev`, and start it again with the limit set to a hundred
years in the variable:

```sh
MAX_DUE_AHEAD=36500d jadpo dev
```

```text
jadpo dev, started 21:54:29
listening on http://localhost:3000
    routes    6 routes
    settings  1 checked
    database  disposable PostgreSQL, gone when dev stops
serving: saves reload, press ctrl-c to quit
```

The same request now succeeds:

```text
{"id":"01a189a8-4c52-763f-b6d9-37adabfdd64c","title":"Plant a tree","status":"open","due_at":"2099-11-02T09:00:00Z","created_at":"2026-10-29T21:54:30.610Z"}
```

A value that doesn't fit the type stops the app before it serves
anything, naming the setting but never the value:

```sh
MAX_DUE_AHEAD=soon jadpo dev
```

```text
jadpo dev, started 21:31:18
error: `MAX_DUE_AHEAD` isn't a valid value for `max_due_ahead`
  reason: `max_due_ahead` of type `Duration` reads `MAX_DUE_AHEAD`, and the value there breaks `format`. The value isn't shown, so it can't leak into a log.
  next: Run `jadpo config set max_due_ahead` to store a valid one, or fix `MAX_DUE_AHEAD` in the environment (human)
  rule: configuration.value_invalid
  help: https://jadpo.dev/docs/errors/configuration.value_invalid

not running this version: 1 error, waiting for a save that fixes them
```

## Values on your machine

To keep a value on your machine without typing it each time, put it in
`.env.local` in the project's folder, one `NAME=value` a line:

`.env.local`:

```text
MAX_DUE_AHEAD=7d
```

`jadpo dev` and `jadpo config check` read it, and a variable set in the
environment wins over it. Keep `.env.local` out of git: add it to
`.gitignore` beside `build/`. On a server, you set the variables in its
environment instead, as page 13 does.

## Secrets

A password or a key is a setting marked `secret: true`. Your code can
never read a secret. It can only be handed to a connection to another
service, such as the mail server page 11 sets up, which uses it to sign
in there. So no response, log or error of your app can ever contain
one.

Never type a secret into a chat with an agent, or into a command where
your shell's history keeps it. When you need one on your machine,
`jadpo config set <setting>` asks for it at a prompt that hides what you
type, checks it against the setting's type, and writes it to
`.env.local`.

## What you have

A setting with a default, a test value and a variable to change it, and
a check that refuses a bad value before the app starts. Next, on page
10, todos get owners.

Read more in the reference: [Settings](../language/10-configuration-and-time.md#settings-configjadpo),
[Secrets](../language/10-configuration-and-time.md#secrets) and
[Values on your machine and in production](../language/10-configuration-and-time.md#values-on-your-machine-and-in-production).

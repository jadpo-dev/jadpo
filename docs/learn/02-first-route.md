# A project and your first route
<!-- topic: learn-first-route -->

On this page you start the todo app that the rest of this path builds,
and get it answering its first HTTP request. By the end you'll send it a
todo with `curl` and get one back. It won't store anything yet: that
comes on page 4.

## Look at what `jadpo new` wrote

A Jadpo project is a folder with a file called `app.jadpo` at its top.
That file describes the application as a whole, and `jadpo` finds your
project by looking for it. On the previous page, `jadpo new todo-app`
made this folder:

```text
todo-app/
  app.jadpo
  config.jadpo
  policy.jadpo
  api/main.jadpo_record
  domains/hello/
  database/
  tests/
  AGENTS.md
  .gitignore
  .gitattributes
```

Each code block on these pages starts with the path of the file it goes
in, relative to `todo-app/`. Create the file at that path, or replace
the one that is there, and copy the block into it.

`app.jadpo` (written by `jadpo new`):

```jadpo check project=learn-todo-02 file=app.jadpo
application {
    jadpo: "0.2"
}

suggestions {
    project_token: "e305757434158f902ea271ba8a8b9379"
}
```

`application { }` holds the settings for the whole app. For now it has
one: the release of Jadpo the app is written for. `"0.1"` means any
0.1 release, the one you built. A block like this holds one setting per
line, with no commas. `suggestions { }` holds a random token, different
in your project, that is all Jadpo's makers learn about your project if
you ever share [suggestions](../language/14-tooling.md#suggestions) with
them.

`config.jadpo` (written by `jadpo new`):

```jadpo check project=learn-todo-02 file=config.jadpo
// The settings the app reads from its environment, one field each.

config {}
```

It holds the app's settings, and has none yet; page 9 adds the first.
`.gitignore` keeps the `build/` folder, where `jadpo` writes its
reports, out of git, and `.gitattributes` keeps the files Jadpo
generates readable in a diff. `AGENTS.md` tells a coding agent what
Jadpo is and where its docs are.

## Clear out the starter

The rest is a starter. `domains/hello/` holds one route, the action it
runs and its test. `policy.jadpo` says who may run that action, and
`api/main.jadpo_record` records the route. This page has you write each
of those for the todo app, and seeing why the compiler asks for each
one is the point, so delete the starter:

```sh
rm -r domains/hello api policy.jadpo
jadpo check
```

```text
checked: 2 files
```

## Organise code into domains

Your code lives in domains. A domain is a folder for one part of your
app's business, inside `domains/`. A shop might have `orders` and
`billing`. This app has one domain, `todos`.

Inside a domain, each kind of thing has a folder with a fixed name:
types go in `types/`, actions in `actions/`, routes in `routes/`, and so
on. Each file holds one declaration, such as one type or one action.
The file is named after what it holds, in lower case with underscores:
the type `NewTodo` lives in `new_todo.jadpo`. That way anyone, or any
agent, can tell from a name which file to open.

You write these four files next:

```text
todo-app/
  domains/
    todos/
      types/new_todo.jadpo
      types/todo_view.jadpo
      actions/add_todo.jadpo
      routes/todos.jadpo
```

## Describe what comes in and what goes out

A request to add a todo sends some JSON, and the app answers with some
JSON. In Jadpo you describe each shape as a type: a name, and the fields
it has. An `Input` type is what a request may send, and an `Output` type
is what a response sends.

`domains/todos/types/new_todo.jadpo`:

```jadpo check project=learn-todo-02 file=domains/todos/types/new_todo.jadpo
/// What a caller sends to add a todo.
type NewTodo = Input {
    title: Text
}
```

`domains/todos/types/todo_view.jadpo`:

```jadpo check project=learn-todo-02 file=domains/todos/types/todo_view.jadpo
/// A todo, as the app sends it back.
type TodoView = Output {
    title: Text
    done: Bool
}
```

`Text` and `Bool` are built in. A comment starting `///` describes the
declaration below it, and your editor and the generated API description
show it. Page 3 covers types properly, including how to give a title
rules such as a maximum length.

## Write an action

An action is a piece of your app's work. Later actions read and write the
database. This one only builds the todo it was given and sends it back.

`domains/todos/actions/add_todo.jadpo`:

```jadpo check project=learn-todo-02 file=domains/todos/actions/add_todo.jadpo
use todos/types { NewTodo, TodoView }

/// Adds a todo. For now it only sends the todo back.
action add_todo(input: NewTodo) -> TodoView {
    return TodoView {
        title: input.title
        done: false
    }
}
```

It reads like a TypeScript function. `add_todo` takes one parameter,
`input`, of type `NewTodo`, and returns a `TodoView`. `TodoView { ... }`
builds a value of that type, with every field given by name.

The first line is a `use` line. It says where the names this file uses
come from: the domain, then the folder. `todos/types` is the `types/`
folder of the `todos` domain. Built-in names such as `Text` need no `use`
line.

## Add a route

A route is the HTTP door to an action. It gives a method and a path,
what the request sends, what the response sends, and the action it
runs. Routes come in groups that share the start of their path, one
group per file.

`domains/todos/routes/todos.jadpo`:

```jadpo check project=learn-todo-02 file=domains/todos/routes/todos.jadpo
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
        run: add_todo(input)
    }
}
```

This group answers paths starting `/todos`, so `POST /` here is
`POST /todos`. The request's JSON body is checked against `NewTodo`
before the action runs, and the action's result is sent as `TodoView`.

By default a route only answers callers who are signed in. This app has
no way to sign in yet, so `authentication: RouteAuthentication.none`
lets this route answer anyone. Page 10 adds signed-in users and takes this line
out.

## Fix what the compiler finds

Check the project again. This time, expect two errors: the code is
right, but the project is missing two things, and the errors say what.

```sh
jadpo check
```

```text
error: `api/main.jadpo_record` is missing
 --> api/main.jadpo_record
  reason: Each API with routes has a committed record of its routes, fields and failure codes, so every change to API `main` shows as a diff of one file.
  next: Run `jadpo api update`, and commit the record with the code (agent)
  rule: record.out_of_date
  help: https://jadpo.dev/docs/errors/record.out_of_date

error: `POST /todos` runs `add_todo`, which nobody may call
 --> domains/todos/routes/todos.jadpo:9:5
  |
9 |     POST / {
  |     ^^^^^^ reaches no entity
  reason: An action takes its permission from the entities it reaches, and `add_todo` reaches none, so it is refused to everyone unless root `policy { }` lists who may call it under `actions:`.
  next: List `todos.add_todo: [...]` under `actions:` in `policy.jadpo` (human)
  rule: policy.effect_ungranted
  help: https://jadpo.dev/docs/errors/policy.effect_ungranted

check failed: 2 errors, 6 files
```

Take the second error first.

### Say who may run the action

In Jadpo, nobody may run an action until the app says who may. Most
actions get that from the data they use. On page 4 you'll store todos,
and the stored todo will say who may read and change it. The error calls
stored data "entities". `add_todo` uses no stored data yet, so nothing
says who may run it, and nobody may.

The `next:` line ends `(human)`. Who may run an action is a decision
about your app, not a coding mistake, so an agent stops here and asks
you. For now, let anyone add a todo. Permission rules for the whole app
go in `policy.jadpo`, next to `app.jadpo`. That is the "root
`policy { }`" the error means.

`policy.jadpo`:

```jadpo check project=learn-todo-02 file=policy.jadpo
use todos

policy {
    actions: {
        todos.add_todo: [Access.public]
    }
}
```

`Access.public` means anyone. `use todos`, with no braces, brings in the
whole domain, so this file can name its action `todos.add_todo`.

So two settings let this request through, and they answer different
questions. The route's `authentication:` decides whether the caller must
be signed in. Policy decides whether this caller may run this action.
They're separate because an action doesn't only run from a route. An
app can also run actions on a schedule, with no request and no caller,
and policy still decides whether that may happen. Page 4 moves this
rule onto the stored todo, pages 10 and 11 cover signing in, and page 12 lets
each person see and change only their own todos.

### Write the API record

An app can serve more than one API, each under its own name. This app
declares none, so it has one, called `main`. For each API, Jadpo keeps a
plain-text record of its routes, their fields and their statuses, in
`api/<name>.jadpo_record`. You commit the record with your code. When a
change alters what your API's clients see, the record changes too, and a
reviewer sees it in the diff.

You never write the record yourself: `jadpo api update` writes it, and
`jadpo check` fails while it disagrees with the routes.

```sh
jadpo api update
```

```text
Wrote api/main.jadpo_record
    version  8f9dea6edf65

next: git diff api/  shows what changed for the API's clients
```

The `version` is a fingerprint of the record, which changes whenever
the API does. This is what it wrote, for you to read, not to copy:

`api/main.jadpo_record` (written by `jadpo api update`):

```jadpo check project=learn-todo-02 file=api/main.jadpo_record
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

GET /openapi.json
    generated openapi
    authentication none
    status 200

POST /todos
    authentication none
    input NewTodo (body)
    output TodoView
    status 200

input NewTodo
    title: Text

output TodoView
    done: Bool
    title: Text
```

There are two routes: yours, and `GET /openapi.json`, which Jadpo adds.
It serves an OpenAPI description of your routes, made from your types,
so you can generate a client or open it in any OpenAPI viewer.

Check once more, and format the files:

```sh
jadpo check
jadpo fmt
```

```text
checked: 7 files
formatted: 7 unchanged
```

`jadpo fmt` lays every file out the one way Jadpo formats code, and adds
or removes `use` lines to match what the file uses. Your files were
already formatted, so nothing changed. Run it whenever you've edited a
file.

## Run it

`jadpo dev` runs the app on your machine, and reloads it each time you
save a file:

```sh
jadpo dev
```

```text
jadpo dev, started 21:06:47
listening on http://localhost:3000
    routes  2 routes
serving: saves reload, press ctrl-c to quit
```

The two routes are `POST /todos` and `GET /openapi.json`. Leave it
running, and in another terminal add a todo:

```sh
curl -i -X POST http://localhost:3000/todos \
  -H 'content-type: application/json' \
  -d '{"title": "Buy milk"}'
```

```text
HTTP/1.1 200 OK
content-type: application/json
x-request-id: req_6089fae44fe4cb29
content-length: 33
date: Thu, 29 Oct 2026 21:07:14 GMT

{"title":"Buy milk","done":false}
```

Now send a body that doesn't fit `NewTodo`, with the field name spelt
wrong:

```sh
curl -i -X POST http://localhost:3000/todos \
  -H 'content-type: application/json' \
  -d '{"titel": "Buy milk"}'
```

```text
HTTP/1.1 422 Unprocessable Entity
content-type: application/json
x-request-id: req_6b1ec7d0df1524b8
content-length: 210
date: Thu, 29 Oct 2026 21:07:14 GMT

{"error":{"code":"invalid_input","message":"Request input is invalid.","request_id":"req_6b1ec7d0df1524b8","details":{"problems":[{"field":"title","rule":"required"},{"field":"titel","rule":"unknown_field"}]}}}
```

You wrote no validation code. The route said its input is a `NewTodo`,
so a body without `title`, or with a field `NewTodo` doesn't have, never
reaches the action. Every error a client gets has this shape: an
`error` holding a `code`, a `message`, the `request_id` and any
`details`.

Back in the first terminal, `jadpo dev` has printed a line for each
request. Each starts with the beginning of its request id, so you can
match a response to its line:

```text
req_6089  POST /todos                        200   11 ms   127.0.0.1
req_6b1e  POST /todos                        422 invalid_input   0 ms   127.0.0.1
```

Press ctrl-c to stop it.

## What you have

A project that checks cleanly, with one domain, two types, an action and
a route, a permission rule and a record of its API. Next, on page 3, you
give the todo's title real rules, so the app refuses an empty or very
long one.

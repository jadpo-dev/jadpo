# Types
<!-- topic: learn-types -->

On page 2 a todo's title was any `Text`, so the app took an empty title,
or one a megabyte long. On this page you give the title rules, add a
status that can only be "open" or "done", and an optional due date. The
compiler then refuses a bad value in your code before the app runs, and
the app refuses one in a request before your action sees it.

## A type with rules

In TypeScript you might write `type Title = string` and check its length
with a validation library at the edge. In Jadpo the rules are part of
the type, and every value of the type follows them, wherever it came
from.

`domains/todos/types/todo_title.jadpo`:

```jadpo check project=learn-todo-03 file=domains/todos/types/todo_title.jadpo
/// What a todo says: at least one character, at most 200.
type TodoTitle = Text {
    min_length: 1
    max_length: 200
}
```

`TodoTitle` is text with two rules: at least one character, and at most
200. You declare every type the same way: `type`, its name, `=`, the
type it builds on, and a block. Here the block holds rules. For the
other types on this page it holds fields or names instead. Text takes
rules on its length; numbers take `min` and `max`.

A type also says what a value means. A todo's title and a person's name
are both text, but they are different types, and Jadpo never turns one
into the other without being asked. So a function that wants a
`TodoTitle` can't be handed someone's name by mistake.

## A value from a fixed list

A todo is either open or done. That's an enum: a type whose values are
the names it lists.

`domains/todos/types/todo_status.jadpo`:

```jadpo check project=learn-todo-03 file=domains/todos/types/todo_status.jadpo
/// Whether a todo is still to do.
type TodoStatus = Enum {
    open
    done
}
```

You write a value as the type's name, a dot and the name:
`TodoStatus.open`. In JSON it is the name as a string, `"open"`. Unlike a
TypeScript string union, a `TodoStatus` is never mixed up with plain
text.

## A value that may be missing

Jadpo has no `null`. A type ending in `?` may hold a value, or `none`,
and the compiler makes you deal with both. A todo's due date is
optional:

`domains/todos/types/new_todo.jadpo`, changed. `title` is now a `TodoTitle`, and `due_at` is new.

```jadpo check project=learn-todo-03 file=domains/todos/types/new_todo.jadpo
use todos/types { TodoTitle }

/// What a caller sends to add a todo.
type NewTodo = Input {
    title: TodoTitle
    due_at: Instant? {
        default: none
    }
}
```

`Instant` is a built-in type for a moment in time. In JSON it is text in
the ISO 8601 form, such as `"2026-11-02T09:00:00Z"`. `Instant?` is "an
instant, or none". `default: none` means a request may leave `due_at`
out, and then it is `none`. Without a default, a request would have to
send JSON `null` for `due_at` every time.

The response type gets the same fields, and the status in place of
`done`:

`domains/todos/types/todo_view.jadpo`, changed. `done` is replaced by `status`, `title` is a `TodoTitle`, and `due_at` is new.

```jadpo check project=learn-todo-03 file=domains/todos/types/todo_view.jadpo
use todos/types { TodoStatus, TodoTitle }

/// A todo, as the app sends it back.
type TodoView = Output {
    title: TodoTitle
    status: TodoStatus
    due_at: Instant?
}
```

The other built-in types are `Bool`, `Int` for whole numbers, `Decimal`
for exact decimal numbers, `Text`, `Bytes` and `Uuid`. Beyond those,
Jadpo has about 250 standard types for common things, such as `Email`,
`Url`, `Money` and `Duration`, each with its rules already written. Use
one when it fits, rather than declaring your own.

## A function

The action now has more to build, so give it a helper. A function is
like an action, but it only works things out: it can't touch the
database, the current time (page 5) or anything outside the app. So the
same arguments always give the same result, and it's easy to test.

`domains/todos/actions/add_todo.jadpo`, changed. Replace the whole file: the body of `add_todo` moves into the new function `new_view`.

```jadpo check project=learn-todo-03 file=domains/todos/actions/add_todo.jadpo
use todos/types { NewTodo, TodoStatus, TodoView }

/// A new todo, as the app sends it back: always open.
function new_view(input: NewTodo) -> TodoView {
    return TodoView {
        title: input.title
        status: TodoStatus.open
        due_at: input.due_at
    }
}

/// Adds a todo. For now it only sends the todo back.
action add_todo(input: NewTodo) -> TodoView {
    return new_view(input)
}
```

A function that only one action uses can sit in that action's file, as
`new_view` does here. `add_todo` calls it by name.

## Refused while compiling

Try breaking a rule, then put it back. In `new_view`, change
`title: input.title` to `title: ""`, and check:

```text
error: `""` isn't a valid `TodoView.title`
 --> domains/todos/actions/add_todo.jadpo:6:16
  |
6 |         title: ""
  |                ^^ breaks `min_length: 1`
  reason: `TodoTitle` declares `min_length: 1`, and the value has 0 characters. A literal is checked against every rule while compiling.
  next: Use a value that `TodoView.title` allows (agent)
  rule: type.invalid_literal
  help: https://jadpo.dev/docs/errors/type.invalid_literal

check failed: 1 error, 9 files
```

A value written in the code is checked against the type's rules before
the app ever runs. A misspelt enum value is caught the same way. Change
`TodoStatus.open` to `TodoStatus.finished`:

```text
error: `TodoStatus` has no variant `finished`
 --> domains/todos/actions/add_todo.jadpo:7:28
  |
7 |         status: TodoStatus.finished
  |                            ^^^^^^^^ not a variant
  reason: `TodoStatus`'s variants are `open` and `done`.
  next: Use one of `TodoStatus`'s variants (agent)
  rule: type.unknown_enum_variant
  help: https://jadpo.dev/docs/errors/type.unknown_enum_variant

check failed: 1 error, 9 files
```

The error calls `open` and `done` variants: that is Jadpo's word for the
values an enum lists.

**Put both lines back as they were** before you go on, or the next
steps fail.

From here on, every time you change a route or a type a route uses,
`jadpo check` reports the API record as out of date, and the fix is
always `jadpo api update`. Later pages just say "update the record".

## Update the API record

Check the project. The types changed what clients send and get back, so
the API record is out of date:

```text
error: `api/main.jadpo_record` is out of date
 --> api/main.jadpo_record:15:1
   |
15 | input NewTodo
   | ^^^^^^^^^^^^^ the source says otherwise
  reason: The record is a promise to API `main`'s clients, and the routes now say otherwise: changed `input NewTodo`, added `enum TodoStatus` and changed `output TodoView`.
  next: Run `jadpo api update`, and commit the record with the code (agent)
  rule: record.out_of_date
  help: https://jadpo.dev/docs/errors/record.out_of_date
```

Run `jadpo api update`. If your project is in git, `git diff api/` shows
what changed, which is what a reviewer sees:

```text
 input NewTodo
-    title: Text
+    due_at: Instant? default none
+    title: Text { min_length: 1, max_length: 200 }
+
+enum TodoStatus
+    variant done
+    variant open

 output TodoView
-    done: Bool
-    title: Text
+    due_at: Instant?
+    status: TodoStatus
+    title: Text { min_length: 1, max_length: 200 }
```

`jadpo check` now says `checked: 9 files`.

## Refused at the door

Run `jadpo dev`, and add a todo with a due date:

```sh
curl -i -X POST http://localhost:3000/todos \
  -H 'content-type: application/json' \
  -d '{"title": "Buy milk", "due_at": "2099-11-02T09:00:00Z"}'
```

```text
HTTP/1.1 200 OK
content-type: application/json
x-request-id: req_f84f1aab5f40fb5a
content-length: 68
date: Thu, 29 Oct 2026 21:54:15 GMT

{"title":"Buy milk","status":"open","due_at":"2099-11-02T09:00:00Z"}
```

Now an empty title, and then a due date that isn't a date:

```sh
curl -X POST http://localhost:3000/todos \
  -H 'content-type: application/json' \
  -d '{"title": ""}'
curl -X POST http://localhost:3000/todos \
  -H 'content-type: application/json' \
  -d '{"title": "Buy milk", "due_at": "next tuesday"}'
```

```text
{"error":{"code":"invalid_input","message":"Request input is invalid.","request_id":"req_a7dafd2160674c97","details":{"problems":[{"field":"title","rule":"min_length"}]}}}
{"error":{"code":"invalid_input","message":"Request input is invalid.","request_id":"req_d6c5b05a9b3540ad","details":{"problems":[{"field":"due_at","rule":"format"}]}}}
```

Each problem names the field and the rule it broke. `format` means the
text isn't in the form the type needs: here, not an ISO 8601 date. You
wrote the rule
once, on `TodoTitle`, and it holds in the code, in requests and in the
API description.

## What you have

The same route, with a title that has rules, a status and an optional
due date. Next, on page 4, the todos are stored, so they're still there
after the request ends.

Read more in the reference: [Named types](../language/02-values-and-types.md#named-types),
[Enums](../language/02-values-and-types.md#enums) and
[Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing).

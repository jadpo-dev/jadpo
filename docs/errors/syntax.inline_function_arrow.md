# `syntax.inline_function_arrow`

An inline function is written in another language's form instead of `item -> expression`.

## Why Jadpo refuses it

`x => e` is an arrow function in JavaScript, TypeScript and C#, `lambda x:
e` in Python and `|x| e` in Rust. A Jadpo inline function is `item ->
expression`, with the same arrow a result type uses, so there is one arrow
to read.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-arrow expect=syntax.inline_function_arrow
type TodoTitle = Text {
    max_length: 200
}

type Todo = Object {
    title: TodoTitle
}

function titles(todos: List<Todo>) -> List<TodoTitle> {
    return list.map(todos, todo => todo.title)
}
```

```text output
error: Jadpo writes `todo -> todo.title`, not `todo => todo.title`
 --> main.jadpo:10:28
   |
10 |     return list.map(todos, todo => todo.title)
   |                            ^^^^^^^ write `todo -> todo.title`
  reason: `x => e` is an arrow function in JavaScript, TypeScript and C#, `lambda x: e` in Python and `|x| e` in Rust; a Jadpo inline function is `item -> expression`.
  next: Write `todo -> todo.title` (agent)
  rule: syntax.inline_function_arrow
  help: https://jadpo.dev/docs/errors/syntax.inline_function_arrow

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant an inline function**, write it with `->`:

```jadpo fragment
function titles(todos: List<Todo>) -> List<TodoTitle> {
    return list.map(todos, todo -> todo.title)
}
```

```jadpo check project=e-habit-arrow-fix hidden
type TodoTitle = Text {
    max_length: 200
}

type Todo = Object {
    title: TodoTitle
}

function titles(todos: List<Todo>) -> List<TodoTitle> {
    return list.map(todos, todo -> todo.title)
}
```

`titles` now returns each todo's title.

## Related

- [Passing functions](../language/03-logic.md#passing-functions)
- [Habits from other languages](../language/03-logic.md#habits-from-other-languages)

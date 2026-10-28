# `syntax.inline_function_arrow`

An inline function is `item -> expression`.

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
  help: docs/language/03-logic.html#syntax-inline-function-arrow

check failed: 1 error, 1 file
```

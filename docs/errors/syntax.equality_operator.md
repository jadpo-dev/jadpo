# `syntax.equality_operator`

```jadpo check project=e-equality expect=syntax.equality_operator
type Count = Int {
    min: 0
}

type Done = Bool {}

function sold_out(count: Count) -> Done {
    return Done(count === 0)
}
```

```text output
error: Jadpo compares with `==`, not `===`
 --> main.jadpo:8:23
  |
8 |     return Done(count === 0)
  |                       ^^^ write `==`
  reason: `===` and `!==` compare without converting types in JavaScript and TypeScript; Jadpo's `==` and `!=` never convert between types, so there is no stricter form to ask for.
  next: Use `==` instead of `===` (agent)
  rule: syntax.equality_operator
  help: docs/errors/syntax.equality_operator.md

check failed: 1 error, 1 file
```

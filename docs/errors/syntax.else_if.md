# `syntax.else_if`

An `if` goes on with `else if`.

```jadpo check project=e-habit-elif expect=syntax.else_if
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    if (count > 100) {
        return "large"
    } elif (count > 10) {
        return "medium"
    }
    return "small"
}
```

```text output
error: Jadpo writes `else if`, not `elif`
 --> main.jadpo:10:7
   |
10 |     } elif (count > 10) {
   |       ^^^^ write `else if`
  reason: `elif` continues an `if` in Python, `elsif` in Ruby and `elseif` in PHP and Lua; Jadpo writes `else if`.
  next: Use `else if` instead of `elif` (agent)
  rule: syntax.else_if
  help: https://jadpo.dev/docs/errors/syntax.else_if

check failed: 1 error, 1 file
```

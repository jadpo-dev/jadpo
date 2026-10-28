# `syntax.function_keyword`

A function starts with `function`, not `fn`, `func`, `def` or `fun`.

```jadpo check project=e-habit-fn expect=syntax.function_keyword
type Count = Int {
    min: 0
}

fn same(count: Count) -> Count {
    return count
}
```

```text output
error: Jadpo writes `function`, not `fn`
 --> main.jadpo:5:1
  |
5 | fn same(count: Count) -> Count {
  | ^^ write `function`
  reason: `fn` declares a function in Rust; Jadpo starts one with `function`, or `action` when it reads or writes stored data.
  next: Use `function` instead of `fn` (agent)
  rule: syntax.function_keyword
  help: docs/language/03-logic.html#syntax-function-keyword

check failed: 1 error, 1 file
```

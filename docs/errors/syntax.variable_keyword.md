# `syntax.variable_keyword`

A variable is `var name: Type = value`, or `var mut` when it can change.

```jadpo check project=e-habit-let expect=syntax.variable_keyword
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    let kept: Count = count
    return kept
}
```

```text output
error: Jadpo writes `var`, not `let`
 --> main.jadpo:6:5
  |
6 |     let kept: Count = count
  |     ^^^ write `var`
  reason: `let` declares a variable in JavaScript, TypeScript, Swift and Rust; a Jadpo variable is `var name: Type = value`, or `var mut` when it can change.
  next: Use `var` instead of `let` (agent)
  rule: syntax.variable_keyword
  help: docs/language/03-logic.html#syntax-variable-keyword

check failed: 1 error, 1 file
```

# `semantic.missing_return`

```jadpo check project=e-missing-return expect=semantic.missing_return
type Count = Int {
    min: 0
}

type Size = Text {}

function size_of(count: Count) -> Size {
    if (count > 10) {
        return "large"
    }
}
```

```text output
error: This function can end without returning a `Size`
 --> main.jadpo:11:1
   |
11 | }
   | ^ reached without `return`
  reason: Every path through a function with a result ends in `return` or `reject`; at least one path here reaches the end of the body.
  next: Return a `Size` at the end, or on every branch that reaches it (agent)
  rule: semantic.missing_return
  help: docs/language/03-logic.html#semantic-missing-return

check failed: 1 error, 1 file
```

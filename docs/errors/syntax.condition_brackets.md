# `syntax.condition_brackets`

```jadpo check project=e-condition expect=syntax.condition_brackets
type Count = Int {
    min: 0
}

type Size = Text {}

function size(count: Count) -> Size {
    if count > 10 {
        return "large"
    }
    return "small"
}
```

```text output
error: This condition needs brackets
 --> main.jadpo:8:8
  |
8 |     if count > 10 {
  |        ^^^^^^^^^^ expected `(` before the condition
  reason: Conditions are always in brackets, `if (total > limit) {`, so the block can't be mistaken for part of the condition.
  next: Write `if (count > 10) {` (agent)
  rule: syntax.condition_brackets
  help: docs/errors/syntax.condition_brackets.md

check failed: 1 error, 1 file
```

# `semantic.unreachable_code`

```jadpo check project=e-unreachable expect=semantic.unreachable_code
type Count = Int {
    min: 0
}

type Size = Text {}

function size_of(count: Count) -> Size {
    return "small"
    if (count > 10) {
        return "large"
    }
}
```

```text output
error: This code never runs
 --> main.jadpo:9:5
  |
8 |     return "small"
  |     -------------- leaves on every path
9 |     if (count > 10) {
  |     ^^^^^^^^^^^^^^^^^ never reached
  reason: The statement before it ends in `return` or `reject` on every path, so nothing after it in this block can run.
  next: Remove it, or move it before the statement that leaves (agent)
  rule: semantic.unreachable_code
  help: https://jadpo.dev/docs/errors/semantic.unreachable_code

check failed: 1 error, 1 file
```

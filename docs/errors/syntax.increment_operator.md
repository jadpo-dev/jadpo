# `syntax.increment_operator`

A `var mut` changes with `=`, and a named type converts the result back with its constructor.

```jadpo check project=e-habit-increment expect=syntax.increment_operator
type Count = Int {
    min: 0
}

function bumped(count: Count) -> Count {
    var mut next: Count = count
    next++
    return next
}
```

```text output
error: Jadpo writes `next = next + 1`, not `next++`
 --> main.jadpo:7:9
  |
7 |     next++
  |         ^^ write `next = next + 1`
  reason: `++`, `--`, `+=`, `-=` and `*=` change a variable in place in JavaScript, Java, C#, Go, Kotlin, Swift and Python; Jadpo assigns a `var mut` with `=`, and a named type converts the result back with its constructor, as in `count = Count(count + 1)`.
  next: Write `next = next + 1` (agent)
  rule: syntax.increment_operator
  help: https://jadpo.dev/docs/errors/syntax.increment_operator

check failed: 1 error, 1 file
```

# `syntax.try_catch`

A call that can fail is marked where it is made: `attempt` passes its failures on, and a `match` on the call handles them. `try {` and `catch` have no edit, since what to do with each failure is the author's to say.

```jadpo check project=e-habit-try expect=syntax.try_catch
type Count = Int {
    min: 0
}

type CountMissing = Failure {
    kind: FailureKind.not_found
}

function counted_or_missing(count: Count) fails CountMissing -> Count {
    if (count == 0) {
        reject CountMissing
    }
    return count
}

function first(count: Count) fails CountMissing -> Count {
    var result: Count = try counted_or_missing(count)
    return result
}
```

```text output
error: Jadpo writes `attempt`, not `try`
 --> main.jadpo:17:25
   |
17 |     var result: Count = try counted_or_missing(count)
   |                         ^^^ write `attempt`
  reason: `try` marks a call that can fail in Swift; Jadpo writes `attempt`, which passes the call's failures on to the caller.
  next: Use `attempt` instead of `try` (agent)
  rule: syntax.try_catch
  help: docs/errors/syntax.try_catch.md

check failed: 1 error, 1 file
```

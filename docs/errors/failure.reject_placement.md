# `failure.reject_placement`

`reject` stops the callable, so it starts a statement or ends a `match` arm,
never part of a value (question 11):

```jadpo check project=e-reject-placement expect=failure.reject_placement
type Count = Int {}

type CountRefused = Failure {
    kind: FailureKind.rejected
}

function capped(count: Count) fails CountRefused -> Count {
    var checked: Count = reject CountRefused
    return checked
}
```

```text output
error: `reject` can only start a statement
 --> main.jadpo:8:26
  |
8 |     var checked: Count = reject CountRefused
  |                          ^^^^^^ inside an expression
  reason: Every place a callable can stop early is at the start of a line: `reject` is a statement of its own, or the result of a `match` arm, never part of a value.
  next: Move `reject` to a line of its own, under an `if` or as a `match` arm (agent)
  rule: failure.reject_placement
  help: https://jadpo.dev/docs/errors/failure.reject_placement

check failed: 1 error, 1 file
```

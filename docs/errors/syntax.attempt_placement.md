# `syntax.attempt_placement`

`attempt` only starts a statement, so every early exit is at the start of a line:

```jadpo check project=e-attempt-placement expect=syntax.attempt_placement
type Count = Int {
    min: 0
    max: 1000
}

type TooBig = Failure {
    kind: FailureKind.invalid_value
}

function checked(count: Count) fails TooBig -> Count {
    if (count > 500) {
        reject TooBig
    }
    return count
}

function doubled(count: Count) fails TooBig -> Count {
    return Count(attempt checked(count) * 2)
}
```

```text output
error: `attempt` can only start a statement
 --> main.jadpo:18:18
   |
18 |     return Count(attempt checked(count) * 2)
   |                  ^^^^^^^ inside an expression
  reason: Every place a callable can stop early is at the start of a line: a `var`, an assignment, a `return`, or a statement of its own.
  next: Move the call into its own `var` statement with `attempt`, then use the variable (agent)
  rule: syntax.attempt_placement
  help: https://jadpo.dev/docs/errors/syntax.attempt_placement

check failed: 1 error, 1 file
```

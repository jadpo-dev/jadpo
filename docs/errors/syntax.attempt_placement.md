# `syntax.attempt_placement`

An `attempt` is written inside an expression instead of starting a statement.

## Why Jadpo refuses it

`attempt` passes a call's failures on to the caller, which ends the callable
early. Every place that can happen is at the start of a line: a `var`, an
assignment, a `return` or a statement of its own. Inside an expression it
would be an early exit in the middle of a calculation, where a reader
scanning the left edge of the code wouldn't see it.

## What it looks like

main.jadpo:

```jadpo check project=e-attempt-placement expect=syntax.attempt_placement
type Count = Int {}

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
 --> main.jadpo:15:18
   |
15 |     return Count(attempt checked(count) * 2)
   |                  ^^^^^^^ inside an expression
  reason: Every place a callable can stop early is at the start of a line: a `var`, an assignment, a `return`, or a statement of its own.
  next: Move the call into its own `var` statement with `attempt`, then use the variable (agent)
  rule: syntax.attempt_placement
  help: https://jadpo.dev/docs/errors/syntax.attempt_placement

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the expression needs the call's result**, move the call into its own
`var` with `attempt`, then use the variable:

```jadpo fragment
function doubled(count: Count) fails TooBig -> Count {
    var checked_count: Count = attempt checked(count)
    return Count(checked_count * 2)
}
```

```jadpo check project=e-attempt-placement-fix hidden
type Count = Int {}

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
    var checked_count: Count = attempt checked(count)
    return Count(checked_count * 2)
}
```

If `checked` fails, `doubled` now stops on its first line and passes
`TooBig` on; otherwise it doubles the value `checked` gave back.

## Related

- [Attempt](../language/03-logic.md#attempt)
- [Passing a failure on: `attempt`](../language/04-failures.md#passing-a-failure-on-attempt)

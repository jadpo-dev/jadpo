# `test.unexpected_failure`

A call under `attempt` failed, so the test ended with a failure it didn't
check for:

```jadpo check project=e-unexpected-failure file=priorities.jadpo hidden
type Priority = Int {
    min: 1
    max: 9
}

type PriorityTooHigh = Failure {
    kind: FailureKind.invalid_value
}

action raise_priority(priority: Priority) fails PriorityTooHigh -> Priority {
    var higher: Priority = attempt Priority(priority + 1) {
        max: PriorityTooHigh
    }
    return higher
}
```

```jadpo check project=e-unexpected-failure file=priorities.test.jadpo expect=test.unexpected_failure
test "the top priority can still be raised" {
    var bumped: Priority = attempt raise_priority(9)
    assert bumped == 9
}
```

```text output
FAIL  the top priority can still be raised
 --> priorities.test.jadpo:2

  failure: `PriorityTooHigh` from action raise_priority
  at: priorities.jadpo:11:36
   |
11 |     var higher: Priority = attempt Priority(priority + 1) {
   |                                    ^^^^^^^^^^^^^^^^^^^^^^ breaks `max: 9`
  reason: The value built here breaks `max: 9`, so `attempt` raises the failure its block maps that rule to.
  code: priority_too_high
  kind: FailureKind.invalid_value
  path: test "the top priority can still be raised" -> action raise_priority

  rule: test.unexpected_failure
  rerun: jadpo test priorities.test.jadpo:2 --seed 1

test failed: 1 failed, 1 action test
```

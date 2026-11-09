# `test.unexpected_failure`

A test ended in a failure it didn't check for.

## Why Jadpo refuses it

A test says which failures it expects with `assert ... fails`. A call under
`attempt` that fails with anything the test didn't expect ends the test
there, since every line after it assumed the call worked. The report shows
the failure and where it came from.

## What it looks like

`raise_priority` fails for the top priority, and the test doesn't expect
it:

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

`priorities.test.jadpo`:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the failure is what should happen**, say so:

```jadpo check project=e-unexpected-failure-fix file=priorities.test.jadpo
test "the top priority can't be raised" {
    assert raise_priority(9) fails PriorityTooHigh
}
```

```jadpo check project=e-unexpected-failure-fix file=priorities.jadpo hidden
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

The test now passes when the call fails with `PriorityTooHigh`, and fails
if it doesn't.

**If the call should have worked**, give it a value it accepts:

```jadpo check project=e-unexpected-failure-fix-2 file=priorities.test.jadpo
test "a priority can be raised" {
    var bumped: Priority = attempt raise_priority(8)
    assert bumped == 9
}
```

```jadpo check project=e-unexpected-failure-fix-2 file=priorities.jadpo hidden
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

The call succeeds, and the test checks what it gives back.

## Related

- [Checking that a call fails](../language/13-testing.md#checking-that-a-call-fails)
- [Calls that can fail](../language/13-testing.md#calls-that-can-fail)
- [Passing a failure on: `attempt`](../language/04-failures.md#passing-a-failure-on-attempt)
- [What a failed test reports](../language/13-testing.md#what-a-failed-test-reports)

# `test.fault`

A test stopped because the code it ran hit a fault.

## Why Jadpo refuses it

A fault is not an outcome code can handle, such as a whole number
overflowing: it is never declared, listed in `fails` or matched, and it
stops the run where it happened. Jadpo stops rather than carry on with a
wrong value, and the test reports where it stopped and the calls that led
there. A test that ran into a fault can't say anything about what it was
checking.

## What it looks like

A multiplication gives a number too large for `Int`:

```jadpo check project=e-test-fault file=plans.jadpo hidden
type Effort = Int {
    min: 0
}

function plan_effort(per_todo: Effort, todos: Effort) -> Effort {
    return Effort(per_todo * todos)
}
```

`plans.test.jadpo`:

```jadpo check project=e-test-fault file=plans.test.jadpo expect=test.fault
test "a long plan still adds up" {
    var per_todo: Effort = 4611686018427387904
    assert plan_effort(per_todo, 2) > 0
}
```

```text output
FAIL  a long plan still adds up
 --> plans.test.jadpo:3

  fault: int_overflow in function plan_effort
  at: plans.jadpo:6:19
  |
6 |     return Effort(per_todo * todos)
  |                   ^^^^^^^^^^^^^^^^ out of range
  reason: `per_todo * todos` gives a whole number outside `Int`'s range, -9223372036854775808 to 9223372036854775807; Jadpo stops rather than wrap around.
  next: Bound the values that feed it with `max`, so the result always fits (agent)
  stack: plans.jadpo:6       function plan_effort
         plans.test.jadpo:3  test "a long plan still adds up"

  rule: test.fault
  rerun: jadpo test plans.test.jadpo:3 --seed 1

test failed: 1 failed, 1 pure test
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An overflow is one fault of several; [Faults](../language/04-failures.md#faults)
lists them. The report gives each one's kind on its `fault:` line.

## How to fix it

**If the value could really be that large**, bound what feeds it with
`max`, so the result always fits:

```jadpo check project=e-test-fault-fix file=plans.jadpo
type Effort = Int {
    min: 0
    max: 1000000
}

type TotalEffort = Int {
    min: 0
}

function plan_effort(per_todo: Effort, todos: Effort) -> TotalEffort {
    return TotalEffort(per_todo * todos)
}
```

```jadpo check project=e-test-fault-fix file=plans.test.jadpo
test "a long plan still adds up" {
    var per_todo: Effort = 1000000
    assert plan_effort(per_todo, 2) > 0
}
```

A value too large for `Effort` is now refused where it comes in, and the
multiplication can't overflow.

**If the test's value could never happen**, use a real one:

```jadpo check project=e-test-fault-fix-2 file=plans.test.jadpo
test "a long plan still adds up" {
    var per_todo: Effort = 4
    assert plan_effort(per_todo, 2) > 0
}
```

```jadpo check project=e-test-fault-fix-2 file=plans.jadpo hidden
type Effort = Int {
    min: 0
}

function plan_effort(per_todo: Effort, todos: Effort) -> Effort {
    return Effort(per_todo * todos)
}
```

The test now checks the sum with values the code is given.

## Related

- [Faults](../language/04-failures.md#faults)
- [What a failed test reports](../language/13-testing.md#what-a-failed-test-reports)
- [`test.unexpected_failure`](../errors/test.unexpected_failure.md)

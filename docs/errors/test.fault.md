# `test.fault`

The runtime stopped the test with a fault (area 4), here an overflow:

```jadpo check project=e-test-fault file=plans.jadpo hidden
type Effort = Int {
    min: 0
}

function plan_effort(per_todo: Effort, todos: Effort) -> Effort {
    return Effort(per_todo * todos)
}
```

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

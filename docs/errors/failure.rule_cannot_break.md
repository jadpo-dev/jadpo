# `failure.rule_cannot_break`

An `attempt` block maps a rule to a failure, but the value being built can never break that rule.

## Why Jadpo refuses it

Building a named type from a computed value, such as `Quantity(quantity * 2)`,
maps each rule that can break to a failure, one arm per rule. Jadpo works
out which rules can break from the ranges the types declare and the
conditions before the construction. An arm for a rule that always holds
could never run, and a reader would go looking for the case where it does.
So every arm can run, and an arm that can't is refused.

## What it looks like

main.jadpo:

```jadpo check project=e-rule-cannot-break expect=failure.rule_cannot_break
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooSmall = Failure {
    kind: FailureKind.invalid_value
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooSmall, QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        min: QuantityTooSmall
        max: QuantityTooLarge
    }
    return result
}
```

```text output
error: `Quantity(...)` can't break `min`, so it takes no arm
 --> main.jadpo:16:9
   |
16 |         min: QuantityTooSmall
   |         ^^^ never runs
  reason: The declared ranges prove the value always follows `min: 1`, so this arm could never run.
  next: Remove `min: ...` (agent)
  rule: failure.rule_cannot_break
  help: https://jadpo.dev/docs/errors/failure.rule_cannot_break

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `quantity` is at least 1, so `quantity * 2` is at least 2 and can never
break `min: 1`. It can still pass 1000, so the `max:` arm is needed.

A condition before the construction proves a rule too. Here the `if`
leaves when `n` is over 12, so `CakeCount(n)` can't break `max: 12`, and
the error points at the check as well:

main.jadpo:

```jadpo check project=e-rule-cannot-break-condition expect=failure.rule_cannot_break
type Count = Int {}

type CakeCount = Int {
    min: 1
    max: 12
}

type OrderEmpty = Failure {
    kind: FailureKind.invalid_value
}

type OrderTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function from_count(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {
    if (n > 12) {
        reject OrderTooLarge
    }
    var count: CakeCount = attempt CakeCount(n) {
        min: OrderEmpty
        max: OrderTooLarge
    }
    return count
}
```

```text output
error: `CakeCount(...)` can't break `max`, so it takes no arm
 --> main.jadpo:22:9
   |
17 |     if (n > 12) {
   |         ------ checked here
22 |         max: OrderTooLarge
   |         ^^^ never runs
  reason: The declared ranges and the condition `n > 12` prove the value always follows `max: 12`, so this arm could never run.
  next: Remove `max: ...` (agent)
  rule: failure.rule_cannot_break
  help: https://jadpo.dev/docs/errors/failure.rule_cannot_break

check failed: 1 error, 1 file
```

## How to fix it

**If the declared ranges prove the rule**, remove its arm. When nothing
else can end the function with that arm's failure, remove it from `fails`
too:

```jadpo fragment
function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityTooLarge
    }
```

```jadpo check project=e-rule-cannot-break-fix-1 hidden
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityTooLarge
    }
    return result
}
```

`doubled` can now fail only with `QuantityTooLarge`, when doubling passes
1000. `QuantityTooSmall` is gone, since nothing could raise it.

**If a condition proves the rule**, remove its arm. The failure stays in
`fails` when the check before it still rejects it:

```jadpo fragment
    var count: CakeCount = attempt CakeCount(n) {
        min: OrderEmpty
    }
```

```jadpo check project=e-rule-cannot-break-fix-2 hidden
type Count = Int {}

type CakeCount = Int {
    min: 1
    max: 12
}

type OrderEmpty = Failure {
    kind: FailureKind.invalid_value
}

type OrderTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function from_count(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {
    if (n > 12) {
        reject OrderTooLarge
    }
    var count: CakeCount = attempt CakeCount(n) {
        min: OrderEmpty
    }
    return count
}
```

A count over 12 still ends `from_count` with `OrderTooLarge`, from the
`if`, and one below 1 with `OrderEmpty`, from the construction.

## Related

- [Construction failures by rule](../language/02-values-and-types.md#construction-failures-by-rule)
- [Conditions as proof](../language/02-values-and-types.md#conditions-as-proof)
- [When a value can't break its type](../language/04-failures.md#when-a-value-cant-break-its-type)

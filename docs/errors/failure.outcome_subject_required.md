# `failure.outcome_subject_required`

A `match` with `success(...)` arms is on a value or a construction, not a call.

## Why Jadpo refuses it

`success(...)` and failure arms match the outcome of a call that can fail.
A value that already exists has no outcome. Building a named type can fail,
but its failures are rules, not named failures, so the arms have nothing to
name until `attempt` maps them.

## What it looks like

main.jadpo:

```jadpo check project=e-outcome-subject expect=failure.outcome_subject_required
type LookupKey = Text {}

function not_a_call(key: LookupKey) -> LookupKey {
    return match key {
        success(value) -> value
    }
}
```

```text output
error: `key` isn't a call, so it has no outcome to match
 --> main.jadpo:4:12
  |
4 |     return match key {
  |            ^^^^^^^^^ not a call
  reason: `success(...)` and failure arms match the outcome of a call that can fail; a value is matched by its own cases, such as an enum's variants.
  next: Match `key`'s cases, or match on the call that can fail (agent)
  rule: failure.outcome_subject_required
  help: https://jadpo.dev/docs/errors/failure.outcome_subject_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Building a value can fail, but it has no named failures to match:

main.jadpo:

```jadpo check project=e-outcome-subject-construction expect=failure.outcome_subject_required
type Count = Int {}

type Quantity = Int {
    min: 1
    max: 1000
}

function checked(n: Count) -> Quantity {
    return match Quantity(n) {
        success(value) -> value
    }
}
```

```text output
error: `Quantity(...)` has no failure to match yet
 --> main.jadpo:9:12
  |
9 |     return match Quantity(n) {
  |            ^^^^^^^^^^^^^^^^^ a construction
  reason: A construction that can fail maps each rule it can break to a failure in its `attempt`'s block, and only a call's named failures can be matched.
  next: Write `attempt Quantity(...) { min: SomeFailure }` in a `var` first, or match on a call (agent)
  rule: failure.outcome_subject_required
  help: https://jadpo.dev/docs/errors/failure.outcome_subject_required

check failed: 1 error, 1 file
```

## How to fix it

**If you meant to match a call**, put the call there:

```jadpo fragment
function not_a_call(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

```jadpo check project=e-outcome-subject-fix-1 hidden
type LookupKey = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

type LookupRefused = Failure {
    kind: FailureKind.rejected
}

function lookup(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {
    if (key == "missing") {
        reject LookupMissing
    }
    if (key == "refused") {
        reject LookupRefused
    }
    return "found"
}

function not_a_call(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
        LookupRefused -> "refused"
    }
}
```

The arms now handle what the call gives.

**If you are building a value**, map its rules in an `attempt` block:

```jadpo fragment
function checked(n: Count) fails QuantityOutOfRange -> Quantity {
    return attempt Quantity(n) {
        invalid: QuantityOutOfRange
    }
}
```

```jadpo check project=e-outcome-subject-fix-2 hidden
type Count = Int {}

type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityOutOfRange = Failure {
    kind: FailureKind.invalid_value
}

function checked(n: Count) fails QuantityOutOfRange -> Quantity {
    return attempt Quantity(n) {
        invalid: QuantityOutOfRange
    }
}
```

A count outside the type's range now ends `checked` with
`QuantityOutOfRange`.

## Related

- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)
- [Match](../language/03-logic.md#match)
- [Construction failures by rule](../language/02-values-and-types.md#construction-failures-by-rule)

# `failure.invalid_covers_nothing`

An `attempt` block has `invalid:`, but every rule the value can break already has an arm.

## Why Jadpo refuses it

`invalid:` catches the rules of a construction that have no arm of their own.
Here each rule that can break has one, so `invalid:` could never run. Leaving
it would suggest a way to fail that doesn't exist, and list a failure callers
would handle for nothing.

## What it looks like

main.jadpo:

```jadpo check project=e-invalid-covers-nothing expect=failure.invalid_covers_nothing
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

type QuantityRefused = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooLarge, QuantityRefused -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityTooLarge
        invalid: QuantityRefused
    }
    return result
}
```

```text output
error: `invalid:` covers nothing here
 --> main.jadpo:17:9
   |
17 |         invalid: QuantityRefused
   |         ^^^^^^^ no rule left to cover
  reason: Every rule `Quantity(...)` can break has its own arm (`max`), so `invalid:` could never run.
  next: Remove `invalid: ...` (agent)
  rule: failure.invalid_covers_nothing
  help: https://jadpo.dev/docs/errors/failure.invalid_covers_nothing

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the arms are what you want**, remove `invalid:`, and the failure it
named if nothing else uses it:

```jadpo fragment
function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityTooLarge
    }
    return result
}
```

```jadpo check project=e-invalid-covers-nothing-fix-1 hidden
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

Callers now handle only the failure that can happen.

**If one failure will do for every rule**, keep `invalid:` and remove the
arm:

```jadpo fragment
    var result: Quantity = attempt Quantity(quantity * 2) {
        invalid: QuantityRefused
    }
```

```jadpo check project=e-invalid-covers-nothing-fix-2 hidden
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityRefused = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityRefused -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        invalid: QuantityRefused
    }
    return result
}
```

Any rule the doubled quantity breaks now gives `QuantityRefused`.

## Related

- [A value built at run time: rule keys and `invalid:`](../language/04-failures.md#a-value-built-at-run-time-rule-keys-and-invalid)
- [Construction failures by rule](../language/02-values-and-types.md#construction-failures-by-rule)

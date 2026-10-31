# `failure.rule_unmapped`

An `attempt` block builds a value that can break one of its type's rules, and no arm says which failure that gives.

## Why Jadpo refuses it

Building a named type from a value known only when the program runs can
break the type's rules, such as `Quantity`'s `min: 1`. When a rule
breaks, the caller gets a failure, and the `attempt` block says which:
one arm per rule, or `invalid:` for every rule without its own. A rule
with neither would leave the caller a failure nobody declared, so every
rule that can break is mapped.

## What it looks like

main.jadpo:

```jadpo check project=e-rule-unmapped expect=failure.rule_unmapped
type Count = Int {}

type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function from_count(n: Count) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(n) {
        max: QuantityTooLarge
    }
    return result
}
```

```text output
error: `Quantity(...)` can break `min`, and nothing maps it
 --> main.jadpo:13:28
   |
13 |     var result: Quantity = attempt Quantity(n) {
   |                            ^^^^^^^^^^^^^^^^^^^^^ can fail at runtime
  reason: The value is only known when the program runs, and neither the declared ranges nor a condition before it prove it follows `min: 1`; it needs an arm naming the `invalid_value` failure the caller gets, or `invalid:` covers every rule without its own.
  next: Add `min: SomeFailure` to the block, naming an `invalid_value` failure, or `invalid: SomeFailure` to cover every rule without an arm (agent)
  rule: failure.rule_unmapped
  help: https://jadpo.dev/docs/errors/failure.rule_unmapped

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`n` is a `Count`, which has no range, so it can be below 1 as well as
above 1000. The block maps only `max`.

## How to fix it

**If a count below 1 needs a failure of its own**, declare one, add an arm
for `min`, and list the failure after `fails`:

```jadpo fragment
function from_count(n: Count) fails QuantityTooSmall, QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(n) {
        min: QuantityTooSmall
        max: QuantityTooLarge
    }
```

```jadpo check project=e-rule-unmapped-fix-1 hidden
type Count = Int {}

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

function from_count(n: Count) fails QuantityTooSmall, QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(n) {
        min: QuantityTooSmall
        max: QuantityTooLarge
    }
    return result
}
```

A count below 1 ends `from_count` with `QuantityTooSmall`, and one above
1000 with `QuantityTooLarge`, so the caller can tell them apart.

**If one failure will do for every rule**, map them all with `invalid:`,
to a failure whose name says so, `QuantityOutOfRange` here:

```jadpo fragment
function from_count(n: Count) fails QuantityOutOfRange -> Quantity {
    var result: Quantity = attempt Quantity(n) {
        invalid: QuantityOutOfRange
    }
```

```jadpo check project=e-rule-unmapped-fix-2 hidden
type Count = Int {}

type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityOutOfRange = Failure {
    kind: FailureKind.invalid_value
}

function from_count(n: Count) fails QuantityOutOfRange -> Quantity {
    var result: Quantity = attempt Quantity(n) {
        invalid: QuantityOutOfRange
    }
    return result
}
```

Any count outside 1 to 1000 ends `from_count` with `QuantityOutOfRange`.
`invalid:` can also sit beside other arms: with `max: QuantityTooLarge`
kept, `max` has its own failure and `invalid:` covers `min`.

## Related

- [Construction failures by rule](../language/02-values-and-types.md#construction-failures-by-rule)
- [A value built at run time: rule keys and `invalid:`](../language/04-failures.md#a-value-built-at-run-time-rule-keys-and-invalid)
- [Conditions as proof](../language/02-values-and-types.md#conditions-as-proof)

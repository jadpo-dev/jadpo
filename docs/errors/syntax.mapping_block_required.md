# `syntax.mapping_block_required`

An `attempt`'s mappings are written after the call instead of in a block.

## Why Jadpo refuses it

An `attempt` says what each way a call can fail becomes, one per line, in a
block after the call. Writes and integration calls use the same block, so a
mapping looks the same wherever it is written and never reads as one more
argument to the call.

## What it looks like

main.jadpo:

```jadpo check project=e-mapping-block-required expect=syntax.mapping_block_required
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) invalid: QuantityTooLarge
    return result
}
```

```text output
error: An `attempt`'s mappings go in a block after the call
 --> main.jadpo:11:59
   |
11 |     var result: Quantity = attempt Quantity(quantity * 2) invalid: QuantityTooLarge
   |                                                           ^^^^^^^^^^^^^^^^^^^^^^^^^ after the call, not in a block
  reason: An `attempt` maps its failures in a block, one per line, the same form writes and integration calls use; a construction's keys are the rules that can break, with `invalid:` covering the rest.
  next: Move `invalid: QuantityTooLarge` into a block after the call: `{ invalid: QuantityTooLarge }` (agent)
  rule: syntax.mapping_block_required
  help: https://jadpo.dev/docs/errors/syntax.mapping_block_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the mapping belongs to this call**, move it into a block after the call:

```jadpo fragment
function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        invalid: QuantityTooLarge
    }
    return result
}
```

```jadpo check project=e-mapping-block-required-fix hidden
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        invalid: QuantityTooLarge
    }
    return result
}
```

If the doubled value breaks one of `Quantity`'s rules, `doubled` now fails
with `QuantityTooLarge`.

## Related

- [A value built at run time: rule keys and `invalid:`](../language/04-failures.md#a-value-built-at-run-time-rule-keys-and-invalid)
- [Turning one failure into another](../language/04-failures.md#turning-one-failure-into-another)

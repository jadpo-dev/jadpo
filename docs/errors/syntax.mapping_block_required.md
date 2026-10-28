# `syntax.mapping_block_required`

An `attempt`'s mappings go in a block after the call, never after it on the
same line:

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
  help: docs/errors/syntax.mapping_block_required.md

check failed: 1 error, 1 file
```

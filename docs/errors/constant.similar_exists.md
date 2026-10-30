# `constant.similar_exists`

A declinable error (area 1, [Declining a judgement call](../language/01-project-layout-and-modules.md#declining-a-judgement-call)):
read the constant it names instead, or decline with a reason when the two
share a value by coincidence.

```jadpo check project=e-constant-similar expect=constant.similar_exists
type HoldbackRate = Decimal {
    scale: 4
    min: 0.0000
    max: 1.0000
}

type Stock = Int {
    min: 0
}

library stocking {
    /// The standard share held back.
    constant holdback_rate: HoldbackRate = 0.2000
}

constant standard_holdback: HoldbackRate = 0.2000

function holdback_of(per_pack: Stock) -> Stock {
    if (stocking.holdback_rate == standard_holdback) {
        return Stock(per_pack / 5)
    }
    return per_pack
}
```

```text output
error: A constant with this value exists: `stocking.holdback_rate`
 --> main.jadpo:16:10
   |
16 | constant standard_holdback: HoldbackRate = 0.2000
   |          ^^^^^^^^^^^^^^^^^ same as `stocking.holdback_rate`
  reason: `standard_holdback` is a `HoldbackRate` holding `0.2000`, as `stocking.holdback_rate` in `main.jadpo` is.
  next: Read `stocking.holdback_rate` instead, and remove `standard_holdback`, or write `decline constant.similar_exists: "<why>"` above it if this is deliberate (agent)
  rule: constant.similar_exists
  help: https://jadpo.dev/docs/errors/constant.similar_exists

check failed: 1 error, 1 file
```

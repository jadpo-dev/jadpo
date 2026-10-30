# `type.arithmetic_operand`

`+` and `-` need the same named type on both sides; multiplying a price by a quantity is meaningful, adding them is not:

```jadpo check project=e-arithmetic expect=type.arithmetic_operand
type Stock = Int {
    min: 0
}

type Quantity = Int {
    min: 1
}

function wrong_total(per_pack: Stock, packs: Quantity) -> Stock {
    return Stock(per_pack + packs)
}
```

```text output
error: `Stock` and `Quantity` can't be added
 --> main.jadpo:10:18
   |
10 |     return Stock(per_pack + packs)
   |                  ^^^^^^^^^^^^^^^^ different types
  reason: `+` and `-` need the same named type on both sides, or a literal, because `Stock` and `Quantity` are different units; `*` and `%` may mix types.
  next: Use two values of the same type (agent)
  rule: type.arithmetic_operand
  help: https://jadpo.dev/docs/errors/type.arithmetic_operand

check failed: 1 error, 1 file
```

# `type.incomparable`

```jadpo check project=e-incomparable expect=type.incomparable
type Stock = Int {
    min: 0
}

type Quantity = Int {
    min: 1
}

type Flag = Bool {}

function same(per_pack: Stock, packs: Quantity) -> Flag {
    return Flag(per_pack == packs)
}
```

```text output
error: `Stock` and `Quantity` can't be compared
 --> main.jadpo:12:17
   |
12 |     return Flag(per_pack == packs)
   |                 ^^^^^^^^^^^^^^^^^ different types
  reason: `==` compares two values of the same type, after widening; `Stock` and `Quantity` are different types, and neither widens to the other.
  next: Compare two values of the same type (agent)
  rule: type.incomparable
  help: https://jadpo.dev/docs/errors/type.incomparable

check failed: 1 error, 1 file
```

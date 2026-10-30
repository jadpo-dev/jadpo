# `type.constructor_input`

```jadpo check project=e-ctor-input expect=type.constructor_input
type OrderId = Uuid {}

type ProductName = Text {
    max_length: 120
}

function order_for(name: ProductName) -> OrderId {
    return OrderId(name)
}
```

```text output
error: `OrderId` can't be built from `ProductName`
 --> main.jadpo:8:20
  |
8 |     return OrderId(name)
  |                    ^^^^ this is `ProductName`
  reason: `OrderId` is built on `Uuid` and `ProductName` is built on `Text`; a constructor converts only between types with the same representation, re-checking the rules.
  next: Pass a value built on `Uuid` (agent)
  rule: type.constructor_input
  help: https://jadpo.dev/docs/errors/type.constructor_input

check failed: 1 error, 1 file
```

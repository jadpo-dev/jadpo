# `type.nullable_selection`

```jadpo check project=e-nullable expect=type.nullable_selection
type ProductName = Text {
    max_length: 120
}

type Product = Object {
    name: ProductName
}

function listed_name(product: Product?) -> ProductName {
    return product.name
}
```

```text output
error: `product` may be `none`, so `name` can't be read yet
 --> main.jadpo:10:20
   |
10 |     return product.name
   |                    ^^^^ may be `none`
  reason: `product` is `Product?`; a field is read once `match` has found a value.
  next: Match on `product` and read `name` inside `some(...)` (agent)
  rule: type.nullable_selection
  help: docs/language/02-values-and-types.html#type-nullable-selection

check failed: 1 error, 1 file
```

# `type.unknown_field`

```jadpo check project=e-unknown-field expect=type.unknown_field
type ProductName = Text {
    max_length: 120
}

type Product = Object {
    name: ProductName
}

function build_product(name: ProductName) -> Product {
    return Product {
        name: name
        title: name
    }
}
```

```text output
error: `Product` has no field `title`
 --> main.jadpo:12:9
   |
12 |         title: name
   |         ^^^^^ not declared
  reason: `Product`'s fields are `name`.
  next: Remove `title`, or declare it on `Product` (agent)
  rule: type.unknown_field
  help: docs/language/02-values-and-types.html#type-unknown-field

check failed: 1 error, 1 file
```

# `type.duplicate_field`

A field is given once when a value is built, and declared once in a type:

```jadpo check project=e-duplicate-field expect=type.duplicate_field
type ProductName = Text {
    max_length: 120
}

type Product = Object {
    name: ProductName
}

function build_product(name: ProductName) -> Product {
    return Product {
        name: name
        name: name
    }
}
```

```text output
error: `name` is given twice
 --> main.jadpo:12:9
   |
11 |         name: name
   |         ---- first given here
12 |         name: name
   |         ^^^^ given again
  reason: Each field of `Product` is given once, so there is never a question of which value counts.
  next: Remove one `name` (agent)
  rule: type.duplicate_field
  help: https://jadpo.dev/docs/errors/type.duplicate_field

check failed: 1 error, 1 file
```

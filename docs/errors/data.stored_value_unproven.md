# `data.stored_value_unproven`

Nothing checks a value the database works out before it is stored, so the
declared ranges must prove it:

```jadpo check project=e-stored-value-unproven expect=data.stored_value_unproven
type Stock = Int {
    min: 0
    max: 1000
}

entity Product {
    id: Uuid
    stock: Stock
}

action restock(product_id: Product.id) fails ProductNotFound -> Product {
    return attempt Product.update(product_id) {
        set: {
            stock: Stock(Product.stock + 1)
        }
    }
}
```

```text output
error: `Product.update` by `id` could store a value that breaks `max: 1000`
 --> main.jadpo:14:20
   |
14 |             stock: Stock(Product.stock + 1)
   |                    ^^^^^^^^^^^^^^^^^^^^^^^^ worked out by the database
  reason: The database works out a value from `Product`'s stored fields when it writes, so nothing checks it before it is stored: the declared ranges must prove it follows its type's rules.
  next: Load the record and build the value with `attempt` before `Product.update`, or loosen the type's rules (human)
  rule: data.stored_value_unproven
  help: docs/language/06-reads-and-writes.html#data-stored-value-unproven

check failed: 1 error, 1 file
```

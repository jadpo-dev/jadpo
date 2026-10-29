# `type.sibling_mismatch`

```jadpo check project=e-sibling expect=type.sibling_mismatch
entity Customer {
    id: Uuid
}

entity Order {
    id: Uuid
}

function customer_reference(id: Customer.id) -> Customer.id {
    return id
}

function misuse(order: Order) -> Customer.id {
    return customer_reference(order.id)
}
```

```text output
error: `Order.id` can't be used where `Customer.id` is required
 --> main.jadpo:14:31
   |
14 |     return customer_reference(order.id)
   |                               ^^^^^^^^ this is `Order.id`
  reason: Both refine `Uuid`, but a field type widens only to its own declared type, so one type's values never stand in for another's.
  next: Pass a `Customer.id` here (agent)
  rule: type.sibling_mismatch
  help: docs/errors/type.sibling_mismatch.md

check failed: 1 error, 1 file
```

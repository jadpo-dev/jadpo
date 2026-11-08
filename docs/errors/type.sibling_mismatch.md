# `type.sibling_mismatch`

A value of one declaration's field type is used where another declaration's field type is required, such as one entity's `id` for another's.

## Why Jadpo refuses it

A field's type is a type of its own. `Customer.id` and `Order.id` are both
built on `Uuid`, but each stands in only for what it was built on, never
for the other. That is how Jadpo catches an order's id passed where a
customer's belongs, a mistake that would otherwise look up the wrong
record, or none.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.sibling_mismatch

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether this really is the value the call needs.

**If you have the wrong value**, pass the right one:

```jadpo fragment
function misuse(customer: Customer) -> Customer.id {
    return customer_reference(customer.id)
}
```

```jadpo check project=e-sibling-fix-1 hidden
entity Customer {
    id: Uuid
}

entity Order {
    id: Uuid
}

function customer_reference(id: Customer.id) -> Customer.id {
    return id
}

function misuse(customer: Customer) -> Customer.id {
    return customer_reference(customer.id)
}
```

The function now passes a customer's id, as `customer_reference` asks.

**If the value is the one you mean**, say so by using the field as a
constructor:

```jadpo fragment
function misuse(order: Order) -> Customer.id {
    return customer_reference(Customer.id(order.id))
}
```

```jadpo check project=e-sibling-fix-2 hidden
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
    return customer_reference(Customer.id(order.id))
}
```

The move from one type to the other is now written where a reviewer can
see it.

## Related

- [Field types](../language/02-values-and-types.md#field-types)
- [When one type can stand in for another](../language/02-values-and-types.md#when-one-type-can-stand-in-for-another)

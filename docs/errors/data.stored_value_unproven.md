# `data.stored_value_unproven`

An update by `id` works out a new value from a stored field, and the value could break its type's rules.

## Why Jadpo refuses it

An update by `id`, such as a counter, lets the database work out the new
value from the stored one as it writes. Nothing in Jadpo sees that value
before it is stored, so nothing could refuse it. It is safe only when
the declared ranges prove it always follows its type's rules.

Here `stock` is at most 1000, and the stored value plus 1 could be 1001.
Jadpo won't decide for you what happens at the limit: either the action
fails there, or the limit goes.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/data.stored_value_unproven

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what restocking a full product does.

## How to fix it

The question is whether a product's stock may pass 1000.

**If it may not**, load the product, build the new value with `attempt`,
and map `max:` to a failure of kind `invalid_value`:

```jadpo fragment
type StockFull = Failure {
    kind: FailureKind.invalid_value
}

action restock(product_id: Product.id) fails ProductNotFound, StockFull -> Product {
    var product: Product = attempt Product.get(product_id)
    var stock: Stock = attempt Stock(product.stock + 1) {
        max: StockFull
    }
    return Product.update(product) {
        set: {
            stock: stock
        }
    }
}
```

```jadpo check project=e-stored-value-unproven-fix-1 hidden
type Stock = Int {
    min: 0
    max: 1000
}

entity Product {
    id: Uuid
    stock: Stock
}

type StockFull = Failure {
    kind: FailureKind.invalid_value
}

action restock(product_id: Product.id) fails ProductNotFound, StockFull -> Product {
    var product: Product = attempt Product.get(product_id)
    var stock: Stock = attempt Stock(product.stock + 1) {
        max: StockFull
    }
    return Product.update(product) {
        set: {
            stock: stock
        }
    }
}
```

Restocking a product that holds 1000 now fails with `StockFull`, and the
stock stays as it was. Because the action loads the product, it runs at
repeatable read, so a request that changes the same product at the same
time makes it run again rather than lose a count.

**If it may**, remove `max:` from `Stock`. A stock of at least 0 plus 1
is always at least 0, so the ranges prove the value. `main.jadpo`
becomes:

```jadpo check project=e-stored-value-unproven-fix-2
type Stock = Int {
    min: 0
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

The update stays one statement worked out by the database, and stock has
no upper limit anywhere it is used.

## Related

- [Counters and other one-step updates](../language/06-reads-and-writes.md#counters-and-other-one-step-updates)
- [Which isolation level an action runs at](../language/06-reads-and-writes.md#which-isolation-level-an-action-runs-at)
- [Building a value at run time](../language/04-failures.md#building-a-value-at-run-time)
- [When a value can't break its type](../language/04-failures.md#when-a-value-cant-break-its-type)

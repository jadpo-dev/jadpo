# `data.stored_value_unproven`

An update by `id` works out a new value from a stored field, and the value could break its type's rules.

## Why Jadpo refuses it

An update by `id`, such as a counter, lets the database work out the new
value from the stored one as it writes. Nothing in Jadpo sees that value
before it is stored, so it is safe only when the declared ranges prove
it always follows its type's rules, or when a block after the
construction maps each rule it can break to a failure, which the
statement then checks. The database checks a number's, an amount's and
a duration's `min` and `max`, and text's lengths; a rule it can't check
as Jadpo does, such as a `pattern`, needs the record loaded first.

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
  reason: The database works out a value from `Product`'s stored fields when it writes, so nothing checks it before it is stored: the declared ranges must prove it follows its type's rules, or a block after the construction must map each rule it can break to a failure, which the statement then checks.
  next: Map each rule in a block after the construction, `Stock(...) { max: SomeFailure }`, naming a `conflict` failure, or loosen the type's rules (human)
  rule: data.stored_value_unproven
  help: https://jadpo.dev/docs/errors/data.stored_value_unproven

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what restocking a full product does.

## How to fix it

The question is whether a product's stock may pass 1000.

**If it may not**, map `max:` to a failure of kind `conflict` in a block
after the construction:

```jadpo fragment
type StockFull = Failure {
    kind: FailureKind.conflict
}

action restock(product_id: Product.id) fails ProductNotFound, StockFull -> Product {
    return attempt Product.update(product_id) {
        set: {
            stock: Stock(Product.stock + 1) {
                max: StockFull
            }
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
    kind: FailureKind.conflict
}

action restock(product_id: Product.id) fails ProductNotFound, StockFull -> Product {
    return attempt Product.update(product_id) {
        set: {
            stock: Stock(Product.stock + 1) {
                max: StockFull
            }
        }
    }
}
```

Restocking a product that holds 1000 now fails with `StockFull`, and the
stock stays as it was. The statement checks `max:` as it works the value
out, so the update stays one step: the action loads no record and runs
at read committed, and two requests restocking the same product at once
don't make each other run again
([A one-step update that refuses](../language/06-reads-and-writes.md#a-one-step-update-that-refuses)).
Loading the product and building the value with `attempt` before the
update works too, but then a request changing the same product at the
same time makes the action run again.

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
- [A one-step update that refuses](../language/06-reads-and-writes.md#a-one-step-update-that-refuses)
- [Which isolation level an action runs at](../language/06-reads-and-writes.md#which-isolation-level-an-action-runs-at)
- [Building a value at run time](../language/04-failures.md#building-a-value-at-run-time)
- [When a value can't break its type](../language/04-failures.md#when-a-value-cant-break-its-type)

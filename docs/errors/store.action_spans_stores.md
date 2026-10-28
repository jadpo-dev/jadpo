# `store.action_spans_stores`

An action writes one store, counting the actions it calls, so moving an
entity can break an action that never names it:

```jadpo check project=e-action-spans-stores file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        carts: {
            driver: Driver.postgres
        }
    }
}
```

```jadpo check project=e-action-spans-stores file=domains/ordering/entities/order.jadpo hidden
entity Order {
    id: Uuid
}
```

```jadpo check project=e-action-spans-stores file=domains/ordering/entities/cart.jadpo hidden migrate=initial
entity Cart {
    id: Uuid

    store: carts
}
```

```jadpo check project=e-action-spans-stores file=domains/ordering/actions/clear_cart.jadpo hidden
use ordering/entities { Cart }

action clear_cart(cart: Cart) -> Unit {
    Cart.delete(cart)
}
```

```jadpo check project=e-action-spans-stores file=domains/ordering/actions/checkout.jadpo expect=store.action_spans_stores
use ordering/entities { Cart, Order }
use ordering/actions { clear_cart }

action checkout(cart: Cart) -> Order {
    var order: Order = Order.create {
        set: {}
    }
    clear_cart(cart)
    return order
}
```

```text output
error: `checkout` writes two stores
 --> domains/ordering/actions/checkout.jadpo:8:5
  |
8 |     clear_cart(cart)
  |     ^^^^^^^^^^^^^^^^ store `carts`
  reason: An action's writes, and those of the actions it calls, commit as one, in one store's transaction; `checkout` writes `Order` (store `main`), and here it calls `clear_cart`, which writes `Cart` (store `carts`).
  next: Write `Cart` in a subscriber to an event `checkout` emits, or keep `Cart` in store `main` (human)
  rule: store.action_spans_stores
  help: docs/errors/store.action_spans_stores.md

check failed: 1 error, 5 files
```

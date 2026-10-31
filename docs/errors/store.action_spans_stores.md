# `store.action_spans_stores`

An action writes to two databases, itself or through an action it calls, but an action's writes all commit in one database.

## Why Jadpo refuses it

An action is all-or-nothing: its writes, and those of the actions it
calls, commit together in one database transaction. A transaction lives
in one database. Here `checkout` creates an `Order` in `main` and calls
`clear_cart`, which deletes a `Cart` in `carts`, so no single
transaction could hold both.

An action that never names the second entity can still break this way,
when an entity it reaches through a call moves to another store. The
error points at the write or the call that brings the second store in.

Jadpo won't choose the fix for you, because it decides whether clearing
the cart must happen together with placing the order, or may follow it.

## What it looks like

`app.jadpo` declares two databases, and `Cart` is kept in `carts`:

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

domains/ordering/actions/checkout.jadpo:

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
  help: https://jadpo.dev/docs/errors/store.action_spans_stores

check failed: 1 error, 5 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether a cart is cleared in the same step as its order is
placed, or just after.

## How to fix it

The question is whether placing the order and clearing the cart must
succeed or fail together.

**If they must**, keep `Cart` in `main` with `Order`.
`domains/ordering/entities/cart.jadpo` becomes:

```jadpo check project=e-action-spans-stores-fix-1 file=domains/ordering/entities/cart.jadpo
entity Cart {
    id: Uuid
}
```

```jadpo check project=e-action-spans-stores-fix-1 file=app.jadpo hidden
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

```jadpo check project=e-action-spans-stores-fix-1 file=domains/ordering/entities/order.jadpo hidden migrate=initial
entity Order {
    id: Uuid
}
```

```jadpo check project=e-action-spans-stores-fix-1 file=domains/ordering/actions/clear_cart.jadpo hidden
use ordering/entities { Cart }

action clear_cart(cart: Cart) -> Unit {
    Cart.delete(cart)
}
```

```jadpo check project=e-action-spans-stores-fix-1 file=domains/ordering/actions/checkout.jadpo hidden
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

`checkout` writes only `main`, so the order and the cleared cart commit
together, or neither does.

**If clearing the cart may follow**, have `checkout` emit an event, and
clear the cart in a subscriber to it. In
`domains/ordering/events/checked_out.jadpo`:

```jadpo fragment
/// A cart was turned into an order.
type CheckedOut = Event {
    cart_id: Cart.id
}
```

```jadpo check project=e-action-spans-stores-fix-2 file=domains/ordering/events/checked_out.jadpo hidden
use ordering/entities { Cart }

/// A cart was turned into an order.
type CheckedOut = Event {
    cart_id: Cart.id
}
```

`checkout` emits it in place of the call:

```jadpo fragment
    emit CheckedOut {
        cart_id: cart.id
    }
```

```jadpo check project=e-action-spans-stores-fix-2 file=domains/ordering/actions/checkout.jadpo hidden
use ordering/entities { Cart, Order }
use ordering/events { CheckedOut }

action checkout(cart: Cart) -> Order {
    var order: Order = Order.create {
        set: {}
    }
    emit CheckedOut {
        cart_id: cart.id
    }
    return order
}
```

A subscriber passes the action only fields of its event, so `clear_cart`
takes the cart's `id`, and the subscriber runs it. In
`domains/ordering/subscribers/clear_checked_out_cart.jadpo`:

```jadpo fragment
subscriber clear_checked_out_cart {
    on: CheckedOut
    run: clear_cart(event.cart_id)
}
```

```jadpo check project=e-action-spans-stores-fix-2 file=domains/ordering/subscribers/clear_checked_out_cart.jadpo hidden
use ordering/actions { clear_cart }
use ordering/events { CheckedOut }

subscriber clear_checked_out_cart {
    on: CheckedOut
    run: clear_cart(event.cart_id)
}
```

And `domains/ordering/actions/clear_cart.jadpo`:

```jadpo fragment
action clear_cart(cart_id: Cart.id) fails CartNotFound -> Unit {
    attempt Cart.delete(cart_id)
}
```

```jadpo check project=e-action-spans-stores-fix-2 file=domains/ordering/actions/clear_cart.jadpo hidden
use ordering/entities { Cart, CartNotFound }

action clear_cart(cart_id: Cart.id) fails CartNotFound -> Unit {
    attempt Cart.delete(cart_id)
}
```

```jadpo check project=e-action-spans-stores-fix-2 file=app.jadpo hidden
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

```jadpo check project=e-action-spans-stores-fix-2 file=domains/ordering/entities/order.jadpo hidden
entity Order {
    id: Uuid
}
```

```jadpo check project=e-action-spans-stores-fix-2 file=domains/ordering/entities/cart.jadpo hidden migrate=initial
entity Cart {
    id: Uuid

    store: carts
}
```

The order and the event commit together in `main`. Only once they have
committed does the subscriber run `clear_cart`, in a transaction of its
own in `carts`. If clearing the cart fails, the order stays placed.

## Related

- [One store per action](../language/06-reads-and-writes.md#one-store-per-action)
- [Stores](../language/05-data.md#stores)
- [Emitting an event](../language/12-jobs-and-events.md#emitting-an-event)
- [Subscribers](../language/12-jobs-and-events.md#subscribers)

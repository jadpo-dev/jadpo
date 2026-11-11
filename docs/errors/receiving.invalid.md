# `receiving.invalid`

A queue in `receiving:` isn't a connection a queue is read through, or doesn't say where each message's id and type are and which events it may be.

## Why Jadpo refuses it

`receiving:` turns each message on a queue into one of the app's events.
Each entry is named after the connection the queue is read through: one
to a package that provides `receiving`, such as Amazon SQS, or one with
`driver: Driver.cloudflare_queues`. A database's connection reads no
queue. The entry then says where each message's id is, `id_at:`, so a
message delivered twice is kept once, the events its messages may be,
`events:`, and, when it lists more than one, where each message's type
is, `type_at:`. Without any of these Jadpo can't tell which event a
message is or whether it has seen it before, and it never guesses,
since a message read wrongly would be dropped or kept twice.

## What it looks like

app.jadpo:

```jadpo check project=e-receiving-invalid file=app.jadpo expect=receiving.invalid
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
    }
    receiving: {
        main: {
            id_at: "message_id"
            events: []
        }
    }
}
```

```text output
error: `main` isn't a connection a queue is read through
 --> app.jadpo:8:9
  |
8 |         main: {
  |         ^^^^ not a queue's connection
  reason: A queue in `receiving:` is a connection to a package that provides `receiving`, or one with `driver: Driver.cloudflare_queues`; it gives `id_at:`, where each message's id is in its body, `events:`, the events its messages may be, and `type_at:`, where each message's type is, unless it lists one event.
  next: Add `main` to `connections:` with the `package:` its queue is read through, or `driver: Driver.cloudflare_queues`, or name one that is (agent)
  rule: receiving.invalid
  help: https://jadpo.dev/docs/errors/receiving.invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A queue that lists two events and doesn't say where each message's type
is is the same error:

```jadpo check project=e-receiving-invalid-type file=domains/stock/events/order_shipped.jadpo hidden
/// An order left the warehouse.
type OrderShipped = Event {
    wire_name: "order.shipped"
    order: Text {
        max_length: 12
    }
}
```

```jadpo check project=e-receiving-invalid-type file=domains/stock/events/stock_low.jadpo hidden
/// A product is running out.
type StockLow = Event {
    wire_name: "stock.low"
    sku: Text {
        max_length: 40
    }
}
```

```jadpo check project=e-receiving-invalid-type file=config.jadpo hidden
config {
    main_url: PostgresUrl
}
```

```jadpo check project=e-receiving-invalid-type file=app.jadpo expect=receiving.invalid
use stock/events { OrderShipped, StockLow }

application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
        warehouse_queue: {
            driver: Driver.cloudflare_queues
            queue: "warehouse-events"
            dead_letter_queue: "warehouse-events-dead"
        }
    }
    receiving: {
        warehouse_queue: {
            id_at: "message_id"
            events: [OrderShipped, StockLow]
        }
    }
}
```

```text output
error: `warehouse_queue` lists 2 events but doesn't say where each message's type is
 --> app.jadpo:16:9
   |
16 |         warehouse_queue: {
   |         ^^^^^^^^^^^^^^^ no `type_at:`
  reason: A queue in `receiving:` is a connection to a package that provides `receiving`, or one with `driver: Driver.cloudflare_queues`; it gives `id_at:`, where each message's id is in its body, `events:`, the events its messages may be, and `type_at:`, where each message's type is, unless it lists one event.
  next: Add `type_at:` with the member holding each message's type, such as `type_at: "type"` (agent)
  rule: receiving.invalid
  help: https://jadpo.dev/docs/errors/receiving.invalid

check failed: 1 error, 4 files
```

## How to fix it

**If the queue is on Cloudflare Queues**, read it through a connection
of its own with `driver: Driver.cloudflare_queues`, name that
connection in `receiving:`, and list the events its messages are. Each
message's event is kept in the outbox of `main`, the database that holds
the app's records, until its subscribers have it. In `app.jadpo`:

```jadpo fragment
    connections: {
        ...
        warehouse_queue: {
            driver: Driver.cloudflare_queues
            queue: "warehouse-events"
            dead_letter_queue: "warehouse-events-dead"
        }
    }
    receiving: {
        warehouse_queue: {
            id_at: "message_id"
            events: [OrderShipped]
        }
    }
```

```jadpo check project=e-receiving-invalid-fix-1 file=app.jadpo hidden
use stock/events { OrderShipped }

application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
        warehouse_queue: {
            driver: Driver.cloudflare_queues
            queue: "warehouse-events"
            dead_letter_queue: "warehouse-events-dead"
        }
    }
    receiving: {
        warehouse_queue: {
            id_at: "message_id"
            events: [OrderShipped]
        }
    }
}
```

```jadpo check project=e-receiving-invalid-fix-1 file=config.jadpo hidden
config {
    main_url: PostgresUrl
}
```

```jadpo check project=e-receiving-invalid-fix-1 file=domains/stock/events/order_shipped.jadpo hidden
/// An order left the warehouse.
type OrderShipped = Event {
    wire_name: "order.shipped"
    order: Text {
        max_length: 12
    }
}
```

```jadpo check project=e-receiving-invalid-fix-1 file=domains/stock/entities/shipment.jadpo hidden migrate=initial
/// An order that has left the warehouse.
entity Shipment {
    id: Uuid
    order: Text {
        max_length: 12
    }

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
```

Each message on `warehouse-events` is now kept as an `OrderShipped`,
once whatever its `message_id`, and acknowledged once kept. One that
keeps failing reaches `warehouse-events-dead` and is parked for an
operator.

**If the queue carries several kinds of message**, say which member of
each message names its type, with `type_at:`. In `app.jadpo`:

```jadpo fragment
        warehouse_queue: {
            type_at: "type"
            id_at: "message_id"
            events: [OrderShipped, StockLow]
        }
```

```jadpo check project=e-receiving-invalid-fix-2 file=app.jadpo hidden
use stock/events { OrderShipped, StockLow }

application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
        warehouse_queue: {
            driver: Driver.cloudflare_queues
            queue: "warehouse-events"
            dead_letter_queue: "warehouse-events-dead"
        }
    }
    receiving: {
        warehouse_queue: {
            type_at: "type"
            id_at: "message_id"
            events: [OrderShipped, StockLow]
        }
    }
}
```

```jadpo check project=e-receiving-invalid-fix-2 file=config.jadpo hidden
config {
    main_url: PostgresUrl
}
```

```jadpo check project=e-receiving-invalid-fix-2 file=domains/stock/events/order_shipped.jadpo hidden
/// An order left the warehouse.
type OrderShipped = Event {
    wire_name: "order.shipped"
    order: Text {
        max_length: 12
    }
}
```

```jadpo check project=e-receiving-invalid-fix-2 file=domains/stock/events/stock_low.jadpo hidden
/// A product is running out.
type StockLow = Event {
    wire_name: "stock.low"
    sku: Text {
        max_length: 40
    }
}
```

```jadpo check project=e-receiving-invalid-fix-2 file=domains/stock/entities/shipment.jadpo hidden migrate=initial
/// An order that has left the warehouse.
entity Shipment {
    id: Uuid
    order: Text {
        max_length: 12
    }

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
```

A message whose `type` is `order.shipped` is now kept as an
`OrderShipped`, and one whose `type` is `stock.low` as a `StockLow`. A
message of a type the queue doesn't list is parked for an operator.

## Related

- [Queues, through `receiving`](../language/12-jobs-and-events.md#queues-through-receiving)
- [Events from outside](../language/12-jobs-and-events.md#events-from-outside)
- [Queues](../language/11-integrations.md#queues)

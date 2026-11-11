# `batches.unavailable`

The app's batch runs couldn't be read or cancelled in their database, because it can't be reached or has no table for them yet.

## Why Jadpo refuses it

`jadpo batches` reads the app's batch runs from the tables Jadpo keeps
them in, in each store a batch walks, and `cancel` writes there too.
Those tables come with the project's migrations. When the database can't
be reached, or has no such table, there is nothing to list, show or
cancel, so the command stops rather than say no run is kept.

Jadpo won't apply the migrations for you here or try another address.
The database may be production's, and whether it is the right one only
you know.

## What it looks like

A project whose `take_stock` walks every item in batches:

```jadpo check project=e-batches-unavailable file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-batches-unavailable file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-batches-unavailable file=domains/stock/types/item_page.jadpo hidden
/// A page of items.
type ItemPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}
```

```jadpo check project=e-batches-unavailable file=domains/stock/types/shelf_code.jadpo hidden
/// Where an item sits in the stockroom.
type ShelfCode = Text {
    min_length: 1
    max_length: 40
}
```

```jadpo check project=e-batches-unavailable file=domains/stock/entities/item.jadpo hidden
use stock/types { ShelfCode }

/// Something on a shelf.
entity Item {
    id: Uuid
    code: ShelfCode
    seen: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}
```

```jadpo check project=e-batches-unavailable file=domains/stock/queries/items.jadpo hidden
use stock/entities { Item }
use stock/types { ItemPage }

/// Every item, a page at a time.
query items(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
```

```jadpo check project=e-batches-unavailable file=domains/stock/actions/mark_seen.jadpo hidden
use stock/entities { Item }

/// Marks an item as seen in a stocktake.
action mark_seen(item: Item) -> Unit {
    var seen: Item = Item.update(item) {
        set: {
            seen: true
        }
    }
    assert seen.seen
}
```

domains/stock/actions/take_stock.jadpo:

```jadpo check project=e-batches-unavailable file=domains/stock/actions/take_stock.jadpo migrate=initial
use stock/types { ItemPage }
use stock/actions { mark_seen }
use stock/queries { items }

/// Marks every item as seen, a batch at a time.
action take_stock(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_seen(item))
}
```

Here `DATABASE_URL` names a new database, to which the project's
migrations haven't been applied:

```sh run project=e-batches-unavailable database expect=batches.unavailable
jadpo batches
```

```text output
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database the command reads, and whether its schema
changes.

A database whose server isn't running gives the same error, saying it
can't be reached. Here `DATABASE_URL` names a database on a port where
no server is listening:

```sh run project=e-batches-unavailable expect=batches.unavailable
DATABASE_URL=postgresql://localhost:5499/stock jadpo batches
```

```text output
```

## How to fix it

The question is whether the address names this project's database.

**If it is this project's database, not yet migrated**, apply the
migrations, then run the command again:

```sh
jadpo migrate apply
jadpo batches
```

The tables are made, and the command lists the runs kept: none, on a
new database.

**If its server isn't running**, start the server, then run the command
again:

```sh
jadpo batches
```

The command reaches the database and lists the runs it keeps.

**If the address names another database than the one the runs are in**,
name that one with `--url`:

```sh
jadpo batches --url "$PRODUCTION_DATABASE_URL"
```

The command reads that database instead, and leaves the other alone.

## Related

- [`jadpo batches`](../language/14-tooling.md#jadpo-batches)
- [Batches](../language/12-jobs-and-events.md#batches-work-over-many-items)
- [`jadpo migrate`](../language/14-tooling.md#jadpo-migrate)

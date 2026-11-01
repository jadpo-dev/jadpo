# `runs.unavailable`

The app's unfinished runs couldn't be read from their database, because it can't be reached or has no table for them yet.

## Why Jadpo refuses it

`jadpo runs unfinished` is run against production before a deploy. It
reads the unfinished multi-step runs and batch runs from the tables
Jadpo keeps them in, and lists those the new build changes, so you know
what will stop. Those tables come with the project's migrations. When
the database can't be reached, or has no such table, the command can't
say what the deploy changes, so it stops rather than list nothing.

Jadpo won't apply the migrations for you here or try another address.
The database is likely production's, and whether it is the right one
only you know.

## What it looks like

A project whose `take_stock` walks every item in batches:

```jadpo check project=e-runs-unavailable file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-runs-unavailable file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-runs-unavailable file=domains/stock/types/item_page.jadpo hidden
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

```jadpo check project=e-runs-unavailable file=domains/stock/types/shelf_code.jadpo hidden
/// Where an item sits in the stockroom.
type ShelfCode = Text {
    min_length: 1
    max_length: 40
}
```

```jadpo check project=e-runs-unavailable file=domains/stock/entities/item.jadpo hidden
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

```jadpo check project=e-runs-unavailable file=domains/stock/queries/items.jadpo hidden
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

```jadpo check project=e-runs-unavailable file=domains/stock/actions/mark_seen.jadpo hidden
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

```jadpo check project=e-runs-unavailable file=domains/stock/actions/take_stock.jadpo migrate=initial
use stock/types { ItemPage }
use stock/actions { mark_seen }
use stock/queries { items }

/// Marks every item as seen, a batch at a time.
action take_stock(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_seen(item))
}
```

`--build` names the build being deployed by its build id, which
`jadpo check` writes in `build/manifest.json` for each deployable. Here
it is read from there, and `DATABASE_URL` names a new database, to which
the project's migrations haven't been applied:

```sh run project=e-runs-unavailable database expect=runs.unavailable
jadpo check
build=$(grep -o 'b_[0-9a-f]\{12\}' build/manifest.json)
jadpo runs unfinished --build "$build"
```

```text output
checked: 8 files
error: The database `main` has no table for unfinished runs yet
  reason: `jadpo_batch_runs` comes with the project's migrations, so either the database hasn't applied them, or it isn't this project's database.
  next: Run `jadpo migrate apply` if it is this project's database, then run the command again (human)
  rule: runs.unavailable
  help: https://jadpo.dev/docs/errors/runs.unavailable
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database the command reads, and whether its schema
changes.

A database whose server isn't running gives the same error, saying it
can't be reached. Here `DATABASE_URL` names a database on a port where
no server is listening:

```sh run project=e-runs-unavailable expect=runs.unavailable
jadpo check
build=$(grep -o 'b_[0-9a-f]\{12\}' build/manifest.json)
DATABASE_URL=postgresql://localhost:5499/stock jadpo runs unfinished --build "$build"
```

```text output
checked: 8 files
error: The database `main` can't be reached
  reason: Connecting to it failed, so no unfinished runs could be read.
  next: Check the database is running and its address is right, then run the command again (human)
  rule: runs.unavailable
  help: https://jadpo.dev/docs/errors/runs.unavailable
```

## How to fix it

The question is whether the address names this project's database.

**If it is this project's database, not yet migrated**, apply the
migrations, then run the command again:

```sh
jadpo migrate apply
jadpo runs unfinished --build "$build"
```

The tables are made, and the command lists what the build changes:
nothing, on a new database.

**If its server isn't running**, start the server, then run the command
again:

```sh
jadpo runs unfinished --build "$build"
```

The command reaches the database and lists what the build changes.

**If the address names another database than production's**, name
production's with `--url`:

```sh
jadpo runs unfinished --build "$build" --url "$PRODUCTION_DATABASE_URL"
```

The command reads that database instead, and leaves the other alone.

## Related

- [`jadpo runs unfinished`](../language/14-tooling.md#jadpo-runs-unfinished)
- [Deploys](../language/12-jobs-and-events.md#deploys)
- [`jadpo migrate`](../language/14-tooling.md#jadpo-migrate)

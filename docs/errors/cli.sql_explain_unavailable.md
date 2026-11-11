# `cli.sql_explain_unavailable`

A `jadpo sql --explain` command can't make the disposable database it plans a query's statements on.

## Why Jadpo refuses it

`jadpo sql <query> --explain` shows PostgreSQL's plan for each statement
the query sends. PostgreSQL makes a plan only for tables it holds, so the
command makes a disposable database at the latest migration, as `jadpo
test` makes one: on the server `JADPO_TEST_DATABASE_URL` names, or on one
it starts from PostgreSQL's own tools. When it can make none, there is
nothing to plan on, so the command stops.

Jadpo won't plan on your app's own database instead. That database may be
production's, and whether to touch it only you can say.

## What it looks like

A project whose query reads a list's items:

app.jadpo:

```jadpo check project=e-sql-explain-unavailable file=app.jadpo
application {}
```

```jadpo check project=e-sql-explain-unavailable file=domains/lists/types/item_title.jadpo hidden
/// What an item says.
type ItemTitle = Text {
    max_length: 200
}
```

```jadpo check project=e-sql-explain-unavailable file=domains/lists/types/list_ref.jadpo hidden
/// Which list an item is on.
type ListRef = Uuid {}
```

```jadpo check project=e-sql-explain-unavailable file=domains/lists/entities/item.jadpo hidden
use lists/types { ItemTitle, ListRef }

/// One item on a list.
entity Item {
    id: Uuid
    list_id: ListRef
    title: ItemTitle

    policy: {
        read: [Access.public]
    }
}
```

`domains/lists/queries/items_on_list.jadpo`:

```jadpo check project=e-sql-explain-unavailable file=domains/lists/queries/items_on_list.jadpo migrate=initial
use lists/entities { Item }
use lists/types { ListRef }

/// A list's items, by title.
query items_on_list(list_id: ListRef) -> List<Item> {
    from: Item
    where: Item.list_id == list_id
    order: [Item.title asc]
    limit: 50
}
```

`--explain` with `JADPO_TEST_DATABASE_URL` naming a server on this machine
that isn't running:

```sh run project=e-sql-explain-unavailable expect=cli.sql_explain_unavailable
JADPO_TEST_DATABASE_URL=postgresql://localhost:5499/postgres jadpo sql items_on_list --explain
```

```text output
error: `items_on_list` couldn't be planned
  reason: `--explain` plans each statement on a disposable PostgreSQL database at the latest migration, and its server couldn't be reached.
  next: Start the server `JADPO_TEST_DATABASE_URL` names, or unset it to use PostgreSQL's own tools, then run the command again (human)
  rule: cli.sql_explain_unavailable
  help: https://jadpo.dev/docs/errors/cli.sql_explain_unavailable
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you, because the choice is yours: which PostgreSQL to install or
run, and which server Jadpo may make databases on.

## How to fix it

**If PostgreSQL isn't installed**, install it, so the command can start a
disposable server from its tools, then run the command again:

```sh
jadpo sql items_on_list --explain
```

The plan shows under each statement.

**If `JADPO_TEST_DATABASE_URL` names a server of your own**, start it, or
set the variable to a server that is running, then run the command again:

```sh
JADPO_TEST_DATABASE_URL=postgresql://localhost:5432/postgres jadpo sql items_on_list --explain
```

The command makes its disposable database there, as `jadpo test` does.

## Related

- [`jadpo sql`](../language/14-tooling.md#jadpo-sql)
- [The test database](../language/13-testing.md#the-test-database)

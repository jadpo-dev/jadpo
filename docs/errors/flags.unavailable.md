# `flags.unavailable`

The flags' state couldn't be read or changed in their database, because it can't be reached, has no table for them yet, or changed while the command ran.

## Why Jadpo refuses it

Each flag's state is kept in `jadpo_flags`, in the database the flags'
store names, and each change is logged beside it in
`jadpo_flag_changes`. `jadpo flags` reads them there, and `set` and
`reset` change them. The tables come with the migration that adds the
first flag. When the database can't be reached, or has no such table,
there is nothing to show or change, so the command stops before it does
either.

A change is made only to the state it read. When another change reached
the flag in between, this one is refused rather than overwrite it.

Jadpo won't apply the migrations for you here or try another address.
The database may be production's, and whether it is the right one only
you know.

## What it looks like

A project with one flag:

```jadpo check project=e-flags-unavailable file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-flags-unavailable file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-flags-unavailable file=flags.jadpo migrate=initial
flags {
    /// The one-page checkout replacing the three-step form.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }
}
```

`jadpo flags` against a new database, to which the project's migrations
haven't been applied:

```sh run project=e-flags-unavailable database expect=flags.unavailable
jadpo flags
```

```text output
error: The database `main` has no table for flags yet
  reason: `jadpo_flags` comes with the project's migrations, so either the database hasn't applied them, or it isn't this project's database.
  next: Run `jadpo migrate apply` if it is this project's database, then run the command again (human)
  rule: flags.unavailable
  help: https://jadpo.dev/docs/errors/flags.unavailable
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database the command reads, and whether its schema changes.

A database whose server isn't running gives the same error, saying it
can't be reached. Here `DATABASE_URL`, the variable the connection's
setting reads, names a database on this machine with no server running:

```sh run project=e-flags-unavailable expect=flags.unavailable
DATABASE_URL=postgresql://localhost:5499/app jadpo flags
```

```text output
error: The database `main` can't be reached
  reason: Connecting to it failed, so no flags could be read.
  next: Check the database is running and its address is right, then run the command again (human)
  rule: flags.unavailable
  help: https://jadpo.dev/docs/errors/flags.unavailable
```

## How to fix it

The question is whether the address names this project's database.

**If it is this project's database, not yet migrated**, apply the
migrations, then run the command again:

```sh
jadpo migrate apply
jadpo flags
```

The tables are made, and the command reads them.

**If its server isn't running**, start the server, then run the command
again:

```sh
jadpo flags
```

The command reaches the database and reads it.

**If the address names another database than the one you meant**, such
as production's, name that one with `--url`:

```sh
jadpo flags --url "$PRODUCTION_DATABASE_URL"
```

The command reads that database instead, and leaves the other alone.

**If the flag changed while the command ran**, see what it is now, then
make your change again:

```sh
jadpo flags
```

The list shows the other change, so yours can be made on top of it.

## Related

- [`jadpo flags`](../language/14-tooling.md#jadpo-flags)
- [Changing a flag](../language/16-built-in-services.md#changing-a-flag)
- [`jadpo migrate`](../language/14-tooling.md#jadpo-migrate)

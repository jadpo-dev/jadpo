# `schedules.unavailable`

The app's schedules couldn't be read from their database, because it can't be reached or has no table for them yet.

## Why Jadpo refuses it

Each schedule keeps its next due time and its lease, the claim that
lets one run happen at a time, in a table of the database that keeps the
schedules' state, and each run's record in another. `jadpo schedules`
and `jadpo why` read them there, and `pause`, `resume` and `run` change
them. The tables come with the migration that adds the first schedule.
When the database can't be reached, or has no such table, there is
nothing to show or change, so the command stops before it does either.

Jadpo won't apply the migrations for you here or try another address.
The database may be production's, and whether it is the right one only
you know.

## What it looks like

A project with one schedule:

```jadpo check project=e-schedules-unavailable file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

```jadpo check project=e-schedules-unavailable file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-schedules-unavailable file=domains/planning/actions/tidy.jadpo hidden
/// Tidies up.
action tidy() -> Unit {}
```

domains/planning/schedules/tidy_up.jadpo:

```jadpo check project=e-schedules-unavailable file=domains/planning/schedules/tidy_up.jadpo migrate=initial
use planning/actions { tidy }

/// Tidies up every quarter of an hour.
schedule tidy_up {
    every: 15m
    run: tidy()
}
```

`jadpo schedules` against a new database, to which the project's migrations
haven't been applied:

```sh run project=e-schedules-unavailable database expect=schedules.unavailable
jadpo schedules
```

```text output
error: The database `main` has no table for schedules yet
  reason: `jadpo_schedules` comes with the project's migrations, so either the database hasn't applied them, or it isn't this project's database.
  next: Run `jadpo migrate apply` if it is this project's database, then run the command again (human)
  rule: schedules.unavailable
  help: https://jadpo.dev/docs/errors/schedules.unavailable
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database the command reads, and whether its schema changes.

A database whose server isn't running gives the same error, saying it
can't be reached. Here `DATABASE_URL`, the variable the connection's
setting reads, names a database on this machine with no server running:

```sh run project=e-schedules-unavailable expect=schedules.unavailable
DATABASE_URL=postgresql://localhost:5499/app jadpo schedules
```

```text output
error: The database `main` can't be reached
  reason: Connecting to it failed, so no schedules could be read.
  next: Check the database is running and its address is right, then run the command again (human)
  rule: schedules.unavailable
  help: https://jadpo.dev/docs/errors/schedules.unavailable
```

## How to fix it

The question is whether the address names this project's database.

**If it is this project's database, not yet migrated**, apply the
migrations, then run the command again:

```sh
jadpo migrate apply
jadpo schedules
```

The tables are made, and the command reads them.

**If its server isn't running**, start the server, then run the command
again:

```sh
jadpo schedules
```

The command reaches the database and reads it.

**If the address names another database than the one you meant**, such
as production's, name that one with `--url`:

```sh
jadpo schedules --url "$PRODUCTION_DATABASE_URL"
```

The command reads that database instead, and leaves the other alone.

## Related

- [`jadpo schedules`](../language/14-tooling.md#jadpo-schedules)
- [Watching schedules](../language/12-jobs-and-events.md#watching-schedules)
- [Where a schedule's state is kept](../language/12-jobs-and-events.md#where-a-schedules-state-is-kept)
- [`jadpo migrate`](../language/14-tooling.md#jadpo-migrate)

# `ai.usage_unavailable`

The app's AI usage couldn't be read from its database, because it can't be reached or has no table for it yet.

## Why Jadpo refuses it

`jadpo ai usage` shows the tokens the app's `ai` calls used today and
this month, from the counts `ai: { limits: }` keeps in a table of the
limits' database. The table comes with the project's migrations. When
the database can't be reached, or has no such table, there are no counts
to show, so the command stops.

Jadpo won't apply the migrations for you here or try another address.
The database may be production's, and whether it is the right one only
you know.

## What it looks like

A project whose `ai:` sets a daily limit, kept in its database `main`:

app.jadpo:

```jadpo check project=e-ai-usage-unavailable file=app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        local_model: {
            driver: Driver.openai_compatible
            url: config.model_url
        }
    }
    ai: {
        connection: local_model
        models: {
            default: config.drafting_model
        }
        limits: {
            per_day: {
                tokens: 2000000
            }
        }
    }
}
```

```jadpo check project=e-ai-usage-unavailable file=config.jadpo hidden migrate=initial
config {
    database_url: PostgresUrl
    drafting_model: ModelId {
        default: "local/llama-3.1-8b-instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

`jadpo ai usage` against a new database, to which the project's migrations
haven't been applied:

```sh run project=e-ai-usage-unavailable database expect=ai.usage_unavailable
jadpo ai usage
```

```text output
error: The database `main` has no table for AI usage yet
  reason: `jadpo_ai_usage` comes with the project's migrations, so either the database hasn't applied them, or it isn't this project's database.
  next: Run `jadpo migrate apply` if it is this project's database, then run the command again (human)
  rule: ai.usage_unavailable
  help: https://jadpo.dev/docs/errors/ai.usage_unavailable
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database the command reads, and whether its schema changes.

A database whose server isn't running gives the same error, saying it
can't be reached. Here `DATABASE_URL`, the variable the connection's
setting reads, names a database on this machine with no server running:

```sh run project=e-ai-usage-unavailable expect=ai.usage_unavailable
DATABASE_URL=postgresql://localhost:5499/app jadpo ai usage
```

```text output
error: The database `main` can't be reached
  reason: Connecting to it failed, so no AI usage could be read.
  next: Check the database is running and its address is right, then run the command again (human)
  rule: ai.usage_unavailable
  help: https://jadpo.dev/docs/errors/ai.usage_unavailable
```

## How to fix it

The question is whether the address names this project's database.

**If it is this project's database, not yet migrated**, apply the
migrations, then run the command again:

```sh
jadpo migrate apply
jadpo ai usage
```

The tables are made, and the command reads them.

**If its server isn't running**, start the server, then run the command
again:

```sh
jadpo ai usage
```

The command reaches the database and reads it.

**If the address names another database than the one you meant**, such
as production's, name that one with `--url`:

```sh
jadpo ai usage --url "$PRODUCTION_DATABASE_URL"
```

The command reads that database instead, and leaves the other alone.

## Related

- [`jadpo ai usage`](../language/14-tooling.md#jadpo-ai-usage)
- [How budgets are counted](../language/21-ai.md#how-budgets-are-counted)
- [`jadpo migrate`](../language/14-tooling.md#jadpo-migrate)

# `cli.migrate_arguments`

A `jadpo migrate` command names no action, one it doesn't have, or a purpose for an action that takes none.

## Why Jadpo refuses it

`jadpo migrate new` writes the next migration, `apply` runs a database's pending ones, and `status` lists them. Changing a database is never done for a word it doesn't know.

## What it looks like

The action is misspelt:

```jadpo check project=e-migrate-arguments file=app.jadpo hidden
application {}
```

```sh run project=e-migrate-arguments expect=cli.migrate_arguments
jadpo migrate aply
```

```text output
error: `aply` isn't something `jadpo migrate` does
  reason: `jadpo migrate new` writes the next migration, `apply` runs a database's pending ones, and `status` lists them.
  next: Use `jadpo migrate new`, `jadpo migrate apply` or `jadpo migrate status` (agent)
  rule: cli.migrate_arguments
  help: https://jadpo.dev/docs/errors/cli.migrate_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

A purpose after an action that takes none is refused rather than ignored:

```sh run project=e-migrate-arguments expect=cli.migrate_arguments
jadpo migrate status add_due_date
```

```text output
error: `jadpo migrate status` takes no purpose
  reason: Only `jadpo migrate new` takes a purpose; `add_due_date` would be ignored.
  next: Run `jadpo migrate status` (agent)
  rule: cli.migrate_arguments
  help: https://jadpo.dev/docs/errors/cli.migrate_arguments
```

## How to fix it

**If you meant to apply the migrations**, name `apply`:

```sh
jadpo migrate apply
```

It runs the pending migrations on the database `config.database_url` names.

**If you meant a new migration**, give the purpose to `new`:

```sh
jadpo migrate new add_due_date
```

It writes the next migration, named after its purpose.

## Related

- [`jadpo migrate`](../language/14-tooling.md#jadpo-migrate)
- [Migrations](../language/05-data.md#migrations)

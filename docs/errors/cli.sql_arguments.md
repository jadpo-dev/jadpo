# `cli.sql_arguments`

A `jadpo sql` command names no query or action, one the project doesn't declare, or an action that writes nothing.

## Why Jadpo refuses it

`jadpo sql` prints the statements a query sends, or an action's writes send. It shows them for one name, and a name that isn't one could be a typo for another.

## What it looks like

A new project, whose one action is `greet`, and the command names `grete`:

```sh run project=e-sql-arguments expect=cli.sql_arguments
jadpo new todo > /dev/null
cd todo
jadpo sql grete
```

```text output
error: `grete` isn't a query or an action
  reason: This project declares no query or action called `grete`. Did you mean `greet`?
  next: Run `jadpo sql greet` (agent)
  rule: cli.sql_arguments
  help: https://jadpo.dev/docs/errors/cli.sql_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

An action that writes nothing has no statements to show:

```sh run project=e-sql-arguments expect=cli.sql_arguments
jadpo new todo > /dev/null
cd todo
jadpo sql greet
```

```text output
error: `greet` writes nothing
  reason: `jadpo sql` shows the statements an action's writes send, and `greet` has no `create`, `update`, `delete` or transition; the queries it calls are shown by name.
  next: Name a query, or an action that writes (agent)
  rule: cli.sql_arguments
  help: https://jadpo.dev/docs/errors/cli.sql_arguments
```

## How to fix it

**If you meant a query or an action that writes**, name it as the project declares it:

```sh
jadpo sql <query or action>
```

It prints each statement with its parameters.

## Related

- [`jadpo sql`](../language/14-tooling.md#jadpo-sql)

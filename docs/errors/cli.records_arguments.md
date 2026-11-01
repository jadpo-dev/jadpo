# `cli.records_arguments`

A `jadpo records` command names an action other than `update`.

## Why Jadpo refuses it

`jadpo records` lists every record and whether it is up to date, and `jadpo records update` brings them all up to date. Rewriting records is something you ask for by name.

## What it looks like

The action is misspelt:

```jadpo check project=e-records-arguments file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```sh run project=e-records-arguments expect=cli.records_arguments
jadpo records refresh
```

```text output
error: `refresh` isn't something `jadpo records` does
  reason: `jadpo records` lists every record and whether it is up to date; `jadpo records update` brings them all up to date.
  next: Run `jadpo records`, or `jadpo records update` (agent)
  rule: cli.records_arguments
  help: https://jadpo.dev/docs/errors/cli.records_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you meant to bring them up to date**, name `update`:

```sh
jadpo records update
```

Each record then matches the code, for review with it.

## Related

- [`jadpo records`](../language/14-tooling.md#jadpo-records)
- [`jadpo records`](../language/18-records-and-manifest.md#jadpo-records)

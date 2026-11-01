# `cli.api_arguments`

A `jadpo api` command names no action, one it doesn't have, or more than `update`.

## Why Jadpo refuses it

`jadpo api` has one action, `update`, which rewrites each API's record in `api/` from the routes. Rewriting records is something you ask for by name, so it isn't done for a word it doesn't know.

## What it looks like

The action is misspelt:

```jadpo check project=e-api-arguments file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```sh run project=e-api-arguments expect=cli.api_arguments
jadpo api updte
```

```text output
error: `updte` isn't something `jadpo api` does
  reason: `jadpo api update` rewrites each API's record in `api/` from the routes.
  next: Run `jadpo api update` (agent)
  rule: cli.api_arguments
  help: https://jadpo.dev/docs/errors/cli.api_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you meant to bring the API records up to date**, name `update`:

```sh
jadpo api update
```

Each record in `api/` then matches the routes, for review with the code.

## Related

- [`jadpo api update`](../language/14-tooling.md#jadpo-api-update)
- [Record files](../language/18-records-and-manifest.md#record-files)

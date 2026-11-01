# `cli.deliveries_arguments`

A `jadpo deliveries` command names an action it doesn't have, no id for one that needs it, or a state or page it doesn't list.

## Why Jadpo refuses it

`jadpo deliveries` lists the deliveries and schedule runs that need an operator, and `retry`, `dismiss`, `cancel`, `resolve`, `show` and `abandon` act on one. Acting on the wrong one could send something twice or drop it, so each needs the id the list shows.

## What it looks like

An action is given no id:

```jadpo check project=e-deliveries-arguments file=app.jadpo hidden
application {}
```

```sh run project=e-deliveries-arguments expect=cli.deliveries_arguments
jadpo deliveries retry
```

```text output
error: `jadpo deliveries retry` needs an id
  reason: Each acts on one delivery or schedule run, by the id the list shows.
  next: Run `jadpo deliveries` to see them, with their ids (agent)
  rule: cli.deliveries_arguments
  help: https://jadpo.dev/docs/errors/cli.deliveries_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

A state it doesn't list is refused with the ones it does:

```sh run project=e-deliveries-arguments expect=cli.deliveries_arguments
jadpo deliveries --state stuck
```

```text output
error: `stuck` isn't a state
  reason: The states listed are `failed`, `outcome_unknown`, `diverged`, `unfinished`, `pending`, `running`, `held`, `dismissed`, `cancelled` and `abandoned`.
  next: Use `--state pending`, or leave it out (agent)
  rule: cli.deliveries_arguments
  help: https://jadpo.dev/docs/errors/cli.deliveries_arguments
```

## How to fix it

**If you meant to act on one**, list them first, then name it by its id:

```sh
jadpo deliveries
jadpo deliveries retry <id>
```

The list shows each delivery's id, state and what went wrong.

## Related

- [`jadpo deliveries`](../language/14-tooling.md#jadpo-deliveries)
- [Commands for background work](../language/14-tooling.md#commands-for-background-work)

# `cli.batches_arguments`

A `jadpo batches` command names an action it doesn't have, no id or an id no batch run has, or a state or page it doesn't list.

## Why Jadpo refuses it

`jadpo batches` lists the app's batch runs, and `show` and `cancel` act
on one by the id the list shows. It never acts on a run for an id that
names none, or several, and never guesses which action or state was
meant.

## What it looks like

```jadpo check project=e-batches-arguments file=app.jadpo hidden
application {}
```

An action misspelt is refused with the one nearest:

```sh run project=e-batches-arguments expect=cli.batches_arguments
jadpo batches shwo 4fa2c09e
```

```text output
error: `shwo` isn't something `jadpo batches` does
  reason: `jadpo batches` lists the app's batch runs, and `show` and `cancel` act on one. Did you mean `show`?
  next: Use `show` instead of `shwo` (agent)
  rule: cli.batches_arguments
  help: https://jadpo.dev/docs/errors/cli.batches_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A state the list doesn't have is refused with the states it does:

```sh run project=e-batches-arguments expect=cli.batches_arguments
jadpo batches --state stuck
```

```text output
error: `stuck` isn't a state
  reason: A batch run is `running`, `finished`, `failed` or `cancelled`.
  next: Use `--state running`, or leave it out (agent)
  rule: cli.batches_arguments
  help: https://jadpo.dev/docs/errors/cli.batches_arguments
```

## How to fix it

**If the action or the state is misspelt**, use the one the command
names:

```sh
jadpo batches show 4fa2c09e
jadpo batches --state running
```

The first shows one run, and the second lists those still going.

**If you don't know a run's id**, list the runs first:

```sh
jadpo batches
```

Each run's id is the first column, its last 8 hex digits.

## Related

- [`jadpo batches`](../language/14-tooling.md#jadpo-batches)
- [Batches](../language/12-jobs-and-events.md#batches)

# `cli.seed_arguments`

A `jadpo seed` command names no seed, one the project doesn't have, or more than one.

## Why Jadpo refuses it

A seed writes records into a database, so `jadpo seed` runs the one you name and never another.

## What it looks like

A project with no seeds, asked to run one:

```jadpo check project=e-seed-arguments file=app.jadpo hidden
application {}
```

```sh run project=e-seed-arguments expect=cli.seed_arguments
jadpo seed demo
```

```text output
error: `demo` isn't a seed
  reason: The project has no seeds: each is a file in `database/seeds/`.
  next: Write one in `database/seeds/<name>.jadpo`, as `seed { … }` (agent)
  rule: cli.seed_arguments
  help: https://jadpo.dev/docs/errors/cli.seed_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If the seed isn't written yet**, write it in `database/seeds/demo.jadpo`, as `seed { … }`, then run it:

```sh
jadpo seed demo
```

It runs against the database on this machine.

## Related

- [`jadpo seed`](../language/14-tooling.md#jadpo-seed)
- [Seeds](../language/05-data.md#seeds)

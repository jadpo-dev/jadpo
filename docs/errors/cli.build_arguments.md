# `cli.build_arguments`

A `jadpo build` command names a deployable the app doesn't have, or more than one.

## Why Jadpo refuses it

`jadpo build` builds one of the app's deployables, or every one when none is named. A name that isn't one could be a typo for another, and building the wrong one would ship the wrong thing, so it builds nothing.

## What it looks like

An app that declares no `deployables:` has one, `app`, and the command names `api`:

```jadpo check project=e-build-arguments file=app.jadpo hidden
application {}
```

```sh run project=e-build-arguments expect=cli.build_arguments
jadpo build api
```

```text output
error: `api` isn't one of the app's deployables
  reason: The app declares no `deployables:` in `app.jadpo`, so it has one deployable, `app`, which runs everything.
  next: Run `jadpo build`, or `jadpo build app` (agent)
  rule: cli.build_arguments
  help: https://jadpo.dev/docs/errors/cli.build_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you meant the app's one deployable**, leave the name out:

```sh
jadpo build
```

It builds `app`, which runs everything the app has.

**If the app should have a deployable called `api`**, declare it under `deployables:` in `app.jadpo`, then build it by name:

```sh
jadpo build api
```

Only that deployable is built, with the APIs and work it is given.

## Related

- [`jadpo build`](../language/14-tooling.md#jadpo-build)
- [Declared deployables](../language/14-tooling.md#declared-deployables)

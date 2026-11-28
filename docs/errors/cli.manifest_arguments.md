# `cli.manifest_arguments`

A `jadpo manifest` command names no action, one it doesn't have, a deployable the app doesn't declare, or no address to check.

## Why Jadpo refuses it

`jadpo manifest send` gives the monitoring product your telemetry goes to
the manifest of each deployable, or of the one it names. A name that isn't
one of the app's deployables could be a typo for another, and sending the
wrong build's manifest would leave the deployed one's telemetry
undecoded, so it sends nothing.

## What it looks like

An app that declares no `deployables:` has one, `app`, and the command
names `api`:

```jadpo check project=e-manifest-arguments file=app.jadpo hidden
application {}
```

```sh run project=e-manifest-arguments expect=cli.manifest_arguments
jadpo check > /dev/null
jadpo manifest send api
```

```text output
error: `api` isn't one of the app's deployables
  reason: The app declares no `deployables:` in `app.jadpo`, so it has one deployable, `app`.
  next: Run `jadpo manifest send app` (agent)
  rule: cli.manifest_arguments
  help: https://jadpo.dev/docs/errors/cli.manifest_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you meant the app's one deployable**, leave the name out:

```sh
jadpo manifest send
```

It sends the manifest of `app`, which runs everything the app has.

## Related

- [`jadpo manifest`](../language/14-tooling.md#jadpo-manifest)
- [The manifest protocol](../language/16-built-in-services.md#the-manifest-protocol)

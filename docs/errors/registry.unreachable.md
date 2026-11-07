# `registry.unreachable`

The registry couldn't be reached, or answered in a way that can't be read.

## Why Jadpo refuses it

`jadpo package add` and `update` from the registry, `search`, `audit` and
`publish` ask the registry at `https://jadpo.dev/registry/v1/`. Every answer
is held to the protocol's table, and one that doesn't fit is treated as no
answer rather than guessed at. Nothing is installed or changed.

`jadpo check` and `jadpo build` never go online, so an app already holding
its packages isn't affected.

`JADPO_REGISTRY` names a stand-in for the registry, over HTTPS, or plain
HTTP on this machine only: another machine over plain HTTP is never tried.

## What it looks like

There is no network. The output is copied from a real run rather than
checked, since it needs the network to be down:

```sh
jadpo package add @acme/postcodes
```

```text
error: The registry couldn't be read
  reason: `jadpo.dev` couldn't be found.
  next: Check your connection, then run the command again (human)
  rule: registry.unreachable
  help: https://jadpo.dev/docs/errors/registry.unreachable
```

`(human)` at the end of the `next:` line tells your coding agent to stop and ask you which fix you want, because the choice is yours to make: it decides where your app's packages come from.

`JADPO_REGISTRY` names a stand-in that answers 404, here a local web server that
isn't a registry. This output is copied from a real run:

```sh
jadpo package add @acme/postcodes
```

```text
error: `http://127.0.0.1:8765/registry/v1/` isn't serving a registry
  reason: It answered 404 without a failure of the registry protocol's, so something answers there, but not a registry.
  next: Check `JADPO_REGISTRY`, which names the registry this command asks (human)
  rule: registry.unreachable
  help: https://jadpo.dev/docs/errors/registry.unreachable
```

## How to fix it

**If the network is down**, fix that, then run the same command again:

```sh
jadpo package add @acme/postcodes
```

It then reads the registry as it would have, and nothing was changed in the
meantime.

**If `JADPO_REGISTRY` is set**, it has to name a running registry and end in
`/registry/v1/`. Check it, or unset it to ask jadpo.dev:

```sh
env | grep JADPO_REGISTRY
unset JADPO_REGISTRY
```

The command then asks the registry the variable names, or jadpo.dev's once it
is unset.

**If the registry itself can't be reached**, install the package from its
folder or from a git tag instead, when you have one:

```sh
jadpo package add ../postcodes
jadpo package add github.com/acme/postcodes@v2.1.0
```

The package is then copied into `packages/` and locked with where it came from
([Where packages come from](../language/11-integrations.md#where-packages-come-from)).

## Related

- [Where packages come from](../language/11-integrations.md#where-packages-come-from)
- [The registry protocol](../language/11-integrations.md#the-registry-protocol)
- [Use a package from the registry](../guides/22-use-a-package-from-the-registry.md)

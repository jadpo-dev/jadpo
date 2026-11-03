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

There is no network:

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

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you, because it can't change your network.

## How to fix it

**Check your connection** and run the command again. If `JADPO_REGISTRY` is
set, check it's an address ending in `/registry/v1/`:

```sh
env | grep JADPO_REGISTRY
```

## Related

- [The registry protocol](../language/11-integrations.md#the-registry-protocol)

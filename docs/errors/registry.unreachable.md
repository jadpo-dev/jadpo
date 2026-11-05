# `registry.unreachable`

The registry couldn't be reached, or answered in a way that can't be read.

## Why Jadpo refuses it

`jadpo package add` and `update` from the registry, `search`, `audit` and
`publish` ask the registry at `https://jadpo.dev/registry/v1/`. The public
registry isn't live yet, so today they stop here. Every answer is held to the
protocol's table, and one that doesn't fit is treated as no answer rather than
guessed at. Nothing is installed or changed.

`jadpo check` and `jadpo build` never go online, so an app already holding
its packages isn't affected.

`JADPO_REGISTRY` names a stand-in for the registry, over HTTPS, or plain
HTTP on this machine only: another machine over plain HTTP is never tried.

## What it looks like

This is what any name gets today, with a working connection. The output is
copied from a real run rather than checked, since it needs the network:

```sh
jadpo package add stripe
```

```text
error: The public registry isn't live yet
  reason: `https://jadpo.dev/registry/v1/` answered 404: your connection works, and nothing is served there yet.
  next: Use a folder or a git tag meanwhile: `jadpo package add <folder>` or `jadpo package add <repository>@v<version>` (human)
  rule: registry.unreachable
  help: https://jadpo.dev/docs/errors/registry.unreachable
```

`(human)` at the end of the `next:` line tells your coding agent to stop and ask you which fix you want, because the choice is yours to make: it decides whose code your app runs.

There is no network, which this output, also copied, shows:

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

An agent can't change your network, so this `(human)` means it stops and asks you.

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

**If it says the public registry isn't live**, there is nothing to fix on your
side, and running the command again won't help. Install the package from its
folder or from a git tag instead, or import its API or write the package yourself
([Where packages come from](../language/11-integrations.md#where-packages-come-from)):

```sh
jadpo package add ../postcodes
jadpo package add github.com/acme/postcodes@v2.1.0
```

The package is then copied into `packages/` and locked with where it came from.
Jadpo's own Resend, OpenAI, Anthropic and Amazon SQS packages come from a clone
of its repository ([Jadpo's own packages](../language/11-integrations.md#jadpos-own-packages)).

**If the network is down**, or the address is wrong, fix that and run the
command again. Check `JADPO_REGISTRY` if it is set, which has to name a running
registry and end in `/registry/v1/`:

```sh
env | grep JADPO_REGISTRY
```

The command then reads the registry as it would have, and nothing was changed
in the meantime.

## Related

- [Where packages come from](../language/11-integrations.md#where-packages-come-from)
- [The registry protocol](../language/11-integrations.md#the-registry-protocol)

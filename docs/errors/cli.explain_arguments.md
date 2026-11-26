# `cli.explain_arguments`

A `jadpo explain` command is given an argument, which it doesn't take.

## Why Jadpo refuses it

`jadpo explain` prints a digest of the whole app, so it takes no name, and refuses a word rather than ignore it.

## What it looks like

A declaration is named:

```jadpo check project=e-explain-arguments file=app.jadpo hidden
application {}
```

```sh run project=e-explain-arguments expect=cli.explain_arguments
jadpo explain Ticket
```

```text output
error: `jadpo explain` takes no arguments
  reason: It was given `Ticket`.
  next: Run `jadpo explain` (agent)
  rule: cli.explain_arguments
  help: https://jadpo.dev/docs/errors/cli.explain_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you want the digest**, leave the argument out:

```sh
jadpo explain
```

It prints the app's access, entities, routes, background work, acceptances and the docs topics it uses.

**If you want one declaration**, with its fields, signature and edges, name it to `jadpo inspect` instead:

```sh
jadpo inspect Ticket
```

It prints where the declaration is, its fields or signature, and what it uses and is used by.

## Related

- [`jadpo explain`](../language/14-tooling.md#jadpo-explain)
- [`jadpo inspect`](../language/14-tooling.md#jadpo-inspect)

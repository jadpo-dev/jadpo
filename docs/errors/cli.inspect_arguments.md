# `cli.inspect_arguments`

A `jadpo inspect` command names a declaration the project doesn't have, or a name two domains share.

## Why Jadpo refuses it

`jadpo inspect <name>` prints one node of the project map. It looks the name up as code writes it, and a name it doesn't know could be a typo for another, so it says which it found rather than guess.

## What it looks like

A new project, whose types include `Greeting`, and the command names `Greting`:

```sh run project=e-inspect-arguments expect=cli.inspect_arguments
jadpo new todo > /dev/null
cd todo
jadpo inspect Greting
```

```text output
error: `Greting` isn't a declaration
  reason: This project declares nothing called `Greting`. Did you mean `Greeting`?
  next: Run `jadpo inspect Greeting` (agent)
  rule: cli.inspect_arguments
  help: https://jadpo.dev/docs/errors/cli.inspect_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you meant a declaration the project has**, name it as code writes it, or leave the name out for the whole map:

```sh
jadpo inspect
jadpo inspect <name>
```

The first prints every domain's declarations; the second prints the one you name.

**If two domains each keep a private one of that name**, name it with its domain, as the error lists it:

```sh
jadpo inspect ordering.at_least_one
```

It prints that domain's declaration alone.

## Related

- [`jadpo inspect`](../language/14-tooling.md#jadpo-inspect)

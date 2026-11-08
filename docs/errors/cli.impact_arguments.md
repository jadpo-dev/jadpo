# `cli.impact_arguments`

A `jadpo impact` command names no declaration, one the project doesn't have, or a name two domains share.

## Why Jadpo refuses it

`jadpo impact` lists what a change to one declaration reaches: the domains that call it, the entry points and the tests. It answers for one name, and a name it doesn't know could be a typo for another, so it says which it found rather than guess.

## What it looks like

A new project, whose one action is `greet`, and the command names `grete`:

```sh run project=e-impact-arguments expect=cli.impact_arguments
jadpo new todo > /dev/null
cd todo
jadpo impact grete
```

```text output
error: `grete` isn't a declaration
  reason: This project declares nothing called `grete`. Did you mean `greet`?
  next: Run `jadpo impact greet` (agent)
  rule: cli.impact_arguments
  help: https://jadpo.dev/docs/errors/cli.impact_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you meant a declaration the project has**, name it as code writes it:

```sh
jadpo impact <name>
```

`jadpo inspect` lists every declaration. **If two domains each keep a private one of that name**, the error lists both; name one with its domain, `ordering.at_least_one`.

## Related

- [`jadpo impact`](../language/14-tooling.md#jadpo-impact)
- [Seeing a change's reach](../language/01-project-layout-and-modules.md#seeing-a-changes-reach)

# `cli.version_arguments`

A `jadpo version` command is given an argument, which it doesn't take.

## Why Jadpo refuses it

`jadpo version` prints the version of the `jadpo` that runs, so it takes nothing after it, and refuses a word rather than ignore it.

## What it looks like

An argument is given:

```sh run project=e-version-arguments expect=cli.version_arguments
jadpo version latest
```

```text output
error: `jadpo version` takes no arguments
  reason: It was given `latest`.
  next: Run `jadpo version` (agent)
  rule: cli.version_arguments
  help: https://jadpo.dev/docs/errors/cli.version_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you want the version**, leave the argument out:

```sh
jadpo version
```

It prints the version; `jadpo toolchain` lists the releases kept.

## Related

- [Commands for debugging, editors and help](../language/14-tooling.md#commands-for-debugging-editors-and-help)
- [Which jadpo runs](../language/14-tooling.md#which-jadpo-runs)

# `cli.help_arguments`

A `jadpo help` command names more than one command or rule.

## Why Jadpo refuses it

`jadpo help` explains one command, or one rule by the id on an error's `rule:` line. It prints one page, so with two names it can't tell which you meant.

## What it looks like

Two commands are named:

```sh run project=e-help-arguments expect=cli.help_arguments
jadpo help check test
```

```text output
error: `jadpo help` takes one argument
  reason: It was given `check test`.
  next: Run `jadpo help check` (agent)
  rule: cli.help_arguments
  help: https://jadpo.dev/docs/errors/cli.help_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you want both**, ask for each in turn:

```sh
jadpo help check
jadpo help test
```

Each prints the command's usage, flags and examples.

## Related

- [Commands for debugging, editors and help](../language/14-tooling.md#commands-for-debugging-editors-and-help)
- [The `jadpo` command](../language/14-tooling.md#the-jadpo-command)

# `cli.completions_arguments`

A `jadpo completions` command names no shell, or one Jadpo writes no completions for.

## Why Jadpo refuses it

Each shell reads its own kind of completion script, and Jadpo writes them for zsh, bash and fish. A script for the wrong shell would be read as broken code, so it prints none.

## What it looks like

The shell is one Jadpo doesn't complete:

```sh run project=e-completions-arguments expect=cli.completions_arguments
jadpo completions powershell
```

```text output
error: `powershell` isn't a shell jadpo completes
  reason: Jadpo writes completion scripts for zsh, bash and fish.
  next: Use `zsh`, `bash` or `fish` (agent)
  rule: cli.completions_arguments
  help: https://jadpo.dev/docs/errors/cli.completions_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

With no shell, it asks for one:

```sh run project=e-completions-arguments expect=cli.completions_arguments
jadpo completions
```

```text output
error: `jadpo completions` needs a shell
  reason: Each shell gets its own script: zsh, bash or fish.
  next: Add the shell, for example `jadpo completions zsh` (agent)
  rule: cli.completions_arguments
  help: https://jadpo.dev/docs/errors/cli.completions_arguments
```

## How to fix it

**If your shell is zsh, bash or fish**, name it:

```sh
jadpo completions zsh > ~/.zfunc/_jadpo
```

The script completes commands, flags and their values after Tab.

## Related

- [Commands for debugging, editors and help](../language/14-tooling.md#commands-for-debugging-editors-and-help)
- [The `jadpo` command](../language/14-tooling.md#the-jadpo-command)

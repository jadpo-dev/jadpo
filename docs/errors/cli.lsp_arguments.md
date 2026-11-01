# `cli.lsp_arguments`

A `jadpo lsp` command is given an argument, which it doesn't take.

## Why Jadpo refuses it

`jadpo lsp` is the language server your editor starts. It finds the project from the folder the editor opens, so it takes no path, and refuses a word rather than ignore it.

## What it looks like

A path is given:

```sh run project=e-lsp-arguments expect=cli.lsp_arguments
jadpo lsp .
```

```text output
error: `jadpo lsp` takes no arguments
  reason: It was given `.`.
  next: Run `jadpo lsp` (agent)
  rule: cli.lsp_arguments
  help: https://jadpo.dev/docs/errors/cli.lsp_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If your editor runs it**, leave the argument out:

```sh
jadpo lsp
```

It talks to the editor on its standard input and output.

## Related

- [Editor support](../language/14-tooling.md#editor-support)
- [Commands for debugging, editors and help](../language/14-tooling.md#commands-for-debugging-editors-and-help)

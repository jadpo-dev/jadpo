# `cli.presentation_arguments`

A `--color` flag holds something other than `auto`, `always` or `never`, or nothing.

## Why Jadpo refuses it

`--color` sets whether output is coloured, and every command takes it. A value it doesn't know could mean either, so it is refused rather than guessed.

## What it looks like

The value is one it doesn't take:

```jadpo check project=e-presentation-arguments file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```sh run project=e-presentation-arguments expect=cli.presentation_arguments
jadpo check --color=yes
```

```text output
error: `yes` isn't a colour setting
  reason: `--color` takes `auto`, `always` or `never`.
  next: Use `--color=auto`, `--color=always` or `--color=never` (agent)
  rule: cli.presentation_arguments
  help: https://jadpo.dev/docs/errors/cli.presentation_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If you want colour even when the output is piped**, use `always`:

```sh
jadpo check --color=always
```

With `auto`, the default, colour is used only in a terminal; `never` turns it off.

## Related

- [Error messages](../language/14-tooling.md#error-messages)
- [The `jadpo` command](../language/14-tooling.md#the-jadpo-command)

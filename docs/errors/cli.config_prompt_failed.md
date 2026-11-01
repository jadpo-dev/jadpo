# `cli.config_prompt_failed`

Nothing was typed at the prompt of `jadpo config set`, or the terminal couldn't hide what you type there.

## Why Jadpo refuses it

`jadpo config set` stores the one value you type at its prompt. An
empty answer would leave the setting unset, which `jadpo config check`
already reports, so it stores nothing rather than an empty value. For a
secret it also turns off the terminal's echo first, and if the terminal
won't hide what you type, it reads nothing at all rather than show the
secret on screen.

Jadpo won't choose a value for you: only you know it.

## What it looks like

A project's `config.jadpo` declares a secret `mail_api_key`, and Enter
is pressed at the prompt with nothing typed:

```jadpo check project=e-config-prompt file=app.jadpo hidden
application {}
```

```jadpo check project=e-config-prompt file=config.jadpo hidden
config {
    /// The key the mail provider gave this app.
    mail_api_key: Text {
        secret: true
    }
}
```

```sh run project=e-config-prompt expect=cli.config_prompt_failed tty typed
jadpo config set mail_api_key
```

```text output
? Enter MAIL_API_KEY hidden
error: Nothing was typed for `MAIL_API_KEY`
  reason: An empty answer would leave the setting unset, which `jadpo config check` already shows.
  next: Run `jadpo config set mail_api_key` and type the value (human)
  rule: cli.config_prompt_failed
  help: https://jadpo.dev/docs/errors/cli.config_prompt_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make:
you type the value, so only you see it.

A terminal that can't hide what you type is refused the same way, with
`The prompt for MAIL_API_KEY failed` and the reason.

## How to fix it

**If Enter was pressed too soon**, run the command again and type the
value before pressing Enter:

```sh run project=e-config-prompt tty typed=mk_test_51ab
jadpo config set mail_api_key
```

```text output
? Enter MAIL_API_KEY hidden
mail_api_key configured locally
    variable   MAIL_API_KEY
    file       .env.local
    gitignore  added .env.local, so it isn't committed
```

The value is in `.env.local`, which git ignores.

**If the terminal can't hide what you type**, run the command in one
that can, such as your system's own terminal rather than an editor's
output pane:

```sh
jadpo config set mail_api_key
```

The prompt hides the value as you type it, and the command stores it.

## Related

- [Setting a value: `jadpo config set`](../language/10-configuration-and-time.md#setting-a-value-jadpo-config-set)
- [`jadpo config set` and `jadpo config check`](../language/14-tooling.md#jadpo-config-set-and-jadpo-config-check)

# `cli.config_tty_required`

You ran `jadpo config set` where there is no terminal to type the value at, such as from a coding agent or a script.

## Why Jadpo refuses it

`jadpo config set` takes a value only at a prompt, never as an argument
or from a pipe, so a secret can't land in your shell's history, a log, a
chat or your coding agent's context. Run where no one can type, such as
by an agent or a script, it has nowhere to ask, so it refuses before
asking.

Jadpo won't take the value another way for you: the point of the prompt
is that only you see the secret.

## What it looks like

A project's `config.jadpo` declares a secret `mail_api_key`, and a
coding agent runs the command:

```jadpo check project=e-config-tty file=app.jadpo hidden
application {
    jadpo: "0.1"
}
```

```jadpo check project=e-config-tty file=config.jadpo hidden
config {
    /// The key the mail provider gave this app.
    mail_api_key: Text {
        secret: true
    }
}
```

```sh run project=e-config-tty expect=cli.config_tty_required
jadpo config set mail_api_key
```

```text output
error: `jadpo config set` needs a terminal
  reason: The value is typed at a prompt, never given as an argument or piped in, so it can't land in shell history, a log or a chat; `jadpo config check` is the command that works without a terminal.
  next: Run `jadpo config set mail_api_key` yourself in a terminal (human)
  rule: cli.config_tty_required
  help: https://jadpo.dev/docs/errors/cli.config_tty_required
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make:
you type the secret yourself, so your agent never sees it.

## How to fix it

**If you are at a terminal**, run the command yourself and type the
value at its prompt, where it isn't shown:

```sh run project=e-config-tty tty typed=mk_test_51ab
jadpo config set mail_api_key
```

```text output
? Enter MAIL_API_KEY hidden
mail_api_key configured locally
    variable   MAIL_API_KEY
    file       .env.local
    gitignore  added .env.local, so it isn't committed
```

The value is in `.env.local`, which git ignores, and your agent can
carry on: `jadpo config check` shows the setting as `set` without
showing the value.

**If a script or a server gives the value**, set the variable in its
environment, from its secret manager, and check it with the command
that needs no terminal:

```sh
jadpo config check
```

`jadpo config check` reports `MAIL_API_KEY` as `set` or `missing`, never
its value, and the app reads it from the environment.

## Related

- [Setting a value: `jadpo config set`](../language/10-configuration-and-time.md#setting-a-value-jadpo-config-set)
- [Checking values: `jadpo config check`](../language/10-configuration-and-time.md#checking-values-jadpo-config-check)
- [Secrets](../language/10-configuration-and-time.md#secrets)

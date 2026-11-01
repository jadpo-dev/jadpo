# `cli.config_arguments`

A `jadpo config` command names no action, an action it doesn't have, or a setting `config.jadpo` doesn't declare.

## Why Jadpo refuses it

`jadpo config` checks the settings (`check`), stores one setting's value
(`set`) or makes a constant a setting (`move`), and `set` and `move`
each act on one name. Guessing a name that isn't declared could store a
secret under the wrong variable, so it names the nearest one instead and
does nothing.

## What it looks like

A project's `config.jadpo` declares `mail_api_key`, and the command
names `mail_key`:

```jadpo check project=e-config-arguments file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```jadpo check project=e-config-arguments file=config.jadpo hidden
config {
    /// The key the mail provider gave this app.
    mail_api_key: Text {
        secret: true
    }
}
```

```sh run project=e-config-arguments expect=cli.config_arguments
jadpo config set mail_key
```

```text output
error: `mail_key` isn't a setting
  reason: The settings are `mail_api_key`.
  next: Use one of them (agent)
  rule: cli.config_arguments
  help: https://jadpo.dev/docs/errors/cli.config_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can
make the fix itself.

An action `jadpo config` doesn't have is refused the same way, naming
the nearest one:

```sh run project=e-config-arguments expect=cli.config_arguments
jadpo config chek
```

```text output
error: `chek` isn't something `jadpo config` does
  reason: `jadpo config` does `check`, `move` and `set`. Did you mean `check`?
  next: Use `check` instead of `chek` (agent)
  rule: cli.config_arguments
  help: https://jadpo.dev/docs/errors/cli.config_arguments
```

## How to fix it

**If you meant a setting it declares**, use the name it suggests:

```sh
jadpo config set mail_api_key
```

The command asks for the value of `MAIL_API_KEY` at its prompt.

**If the setting isn't declared yet**, declare it in `config.jadpo`
first, beside the others:

```jadpo check project=e-config-arguments-declared file=config.jadpo
config {
    /// The key the mail provider gave this app.
    mail_api_key: Text {
        secret: true
    }

    /// The key for the mail provider's sandbox.
    mail_key: Text {
        secret: true
    }
}
```

```jadpo check project=e-config-arguments-declared file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

`jadpo config set mail_key` then asks for `MAIL_KEY`, and `jadpo config
check` lists the new setting.

## Related

- [`jadpo config set` and `jadpo config check`](../language/14-tooling.md#jadpo-config-set-and-jadpo-config-check)
- [Settings: `config.jadpo`](../language/10-configuration-and-time.md#settings-configjadpo)

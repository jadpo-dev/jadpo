# `configuration.value_missing`

A setting `config.jadpo` declares without a `default:` has no value on this machine, in the environment or `.env.local`.

## Why Jadpo refuses it

A setting with no `default:` is one the app can't run without, such as
a key a provider gave you. The app checks every setting before it
serves anything, so it never answers a request half configured:
`jadpo config check` reports the missing value, and `jadpo dev` stops
before it starts.

Jadpo won't make a value up or ask your agent for one. Only you know
the value, and a secret typed into a chat could end up in a log.

## What it looks like

A project's `config.jadpo` declares a secret `mail_api_key`, and neither
the environment nor `.env.local` sets it:

```jadpo check project=e-value-missing file=app.jadpo hidden
application {}
```

```jadpo check project=e-value-missing file=config.jadpo hidden
config {
    /// The key the mail provider gave this app.
    mail_api_key: Text {
        secret: true
    }
}
```

```sh run project=e-value-missing expect=configuration.value_missing
jadpo config check
```

```text output
Values
  mail_api_key  missing  MAIL_API_KEY

error: `MAIL_API_KEY` isn't set
  reason: `mail_api_key` is required: it has no `default:`, so the app can't start without a value for it.
  next: Run `jadpo config set mail_api_key` to store it in `.env.local`, or set `MAIL_API_KEY` in the environment (human)
  rule: configuration.value_missing
  help: https://jadpo.dev/docs/errors/configuration.value_missing

config incomplete: 1 problem, 1 setting

next: jadpo config set mail_api_key  stores it in .env.local
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides where this machine's value comes from, and the value is one
only you know.

`jadpo dev` checks the same values as it starts, and with this one it
serves nothing: it shows the same error, then waits for a save that
fixes it.

## How to fix it

The question is where the value on this machine should come from.

**If it should be kept in `.env.local`**, run `jadpo config set` and
type the value at its prompt, which hides it:

```sh run project=e-value-missing tty typed=mk_test_51ab
jadpo config set mail_api_key
```

```text output
? Enter MAIL_API_KEY hidden
mail_api_key configured locally
    variable   MAIL_API_KEY
    file       .env.local
    gitignore  added .env.local, so it isn't committed
```

The value is saved in `.env.local`, which git ignores, and `jadpo dev`
starts with it.

**If it comes from the environment**, as on a server, set the variable
the setting reads:

```sh run project=e-value-missing
MAIL_API_KEY=mk_test_51ab jadpo config check
```

```text output
Values
  mail_api_key  set      MAIL_API_KEY

config ready: 1 setting
```

The environment wins over `.env.local`. On a server, the variable is
set in its environment or by its secret manager.

## Related

- [Defaults and test values](../language/10-configuration-and-time.md#defaults-and-test-values)
- [Setting a value: `jadpo config set`](../language/10-configuration-and-time.md#setting-a-value-jadpo-config-set)
- [Checking values: `jadpo config check`](../language/10-configuration-and-time.md#checking-values-jadpo-config-check)
- [Values on your machine and in production](../language/10-configuration-and-time.md#values-on-your-machine-and-in-production)
- [Settings](../learn/09-settings.md)

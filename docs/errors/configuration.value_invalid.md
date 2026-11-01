# `configuration.value_invalid`

A setting's value on this machine, from the environment or `.env.local`, doesn't fit the type `config.jadpo` gives it.

## Why Jadpo refuses it

Each setting in `config.jadpo` has a type, and its value is checked
against that type before the app serves anything. A value that doesn't
fit would fail later, in the middle of a request, so `jadpo dev` stops
before it starts and `jadpo config check` reports it.

The error names the setting and the variable it reads, but never the
value, so a secret can't leak into a log or a chat. That is also why
Jadpo won't fix it for you: only you know the value you meant, and a
secret value shouldn't pass through your agent.

## What it looks like

A Learn path app's `config.jadpo` declares `max_due_ahead` as a
`Duration`, with a default of `365d`. Set its variable to something that
isn't a duration, and check the settings:

```jadpo check project=e-value-invalid file=app.jadpo hidden
application {}
```

```jadpo check project=e-value-invalid file=config.jadpo hidden
config {
    /// How far ahead a todo may be due.
    max_due_ahead: Duration {
        default: 365d
        test: 30d
    }
}
```

```sh run project=e-value-invalid expect=configuration.value_invalid
MAX_DUE_AHEAD=soon jadpo config check
```

```text output
Values
  max_due_ahead  invalid  MAX_DUE_AHEAD

error: `MAX_DUE_AHEAD` isn't a valid value for `max_due_ahead`
  reason: `max_due_ahead` of type `Duration` reads `MAX_DUE_AHEAD`, and the value there breaks `format`. The value isn't shown, so it can't leak into a log.
  next: Run `jadpo config set max_due_ahead` to store a valid one, or fix `MAX_DUE_AHEAD` in the environment (human)
  rule: configuration.value_invalid
  help: https://jadpo.dev/docs/errors/configuration.value_invalid

config incomplete: 1 problem, 1 setting

next: jadpo config set max_due_ahead  stores it in .env.local
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides the value the setting holds on this machine, which only you
know.

`jadpo dev` checks the same values as it starts, and with this one it
serves nothing: it shows the same error, then waits for a save that
fixes it.

## How to fix it

The question is where the value on this machine should come from.

**If it should be kept in `.env.local`**, run `jadpo config set` and
type the value at its prompt:

```sh
jadpo config set max_due_ahead
```

The command checks the value against the setting's type before writing
anything, and refuses one that breaks it by naming the rule, never the
value. A valid one goes in `.env.local`, which it adds to `.gitignore`.
Clear the variable in the environment too, since the environment wins
over `.env.local`.

**If it comes from the environment**, give the variable a value that
fits the type, such as a hundred years:

```sh run project=e-value-invalid
MAX_DUE_AHEAD=36500d jadpo config check
```

```text output
Values
  max_due_ahead  set      MAX_DUE_AHEAD

config ready: 1 setting
```

The setting now reads `36500d`, and `jadpo dev` starts with it. On a
server, the variable is set in its environment or by its secret manager.

## Related

- [Setting a value: `jadpo config set`](../language/10-configuration-and-time.md#setting-a-value-jadpo-config-set)
- [Checking values: `jadpo config check`](../language/10-configuration-and-time.md#checking-values-jadpo-config-check)
- [Values on your machine and in production](../language/10-configuration-and-time.md#values-on-your-machine-and-in-production)
- [`jadpo config set` and `jadpo config check`](../language/14-tooling.md#jadpo-config-set-and-jadpo-config-check)
- [Settings](../learn/09-settings.md)

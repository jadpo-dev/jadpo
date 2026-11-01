# `configuration.local_syntax`

A line of `.env.local` isn't blank, a `#` comment or `NAME=value`, or it sets a variable a line above it already sets.

## Why Jadpo refuses it

`.env.local` holds this machine's values for the settings in
`config.jadpo`, and Jadpo reads it one way only: each line is blank, a
`#` comment, or `NAME=value`, with a value that has spaces or other
unusual characters in double quotation marks. A line it can't read
might be a value meant for a setting, and guessing what it means could
start the app with the wrong secret, so every command that reads the
file stops instead.

The error names the line by its number, never by what it holds, since
the line may be a secret. That is also why Jadpo won't fix it for you:
the file holds values only you know, and a secret shouldn't pass through
your agent.

## What it looks like

A project's `config.jadpo` declares a secret `mail_api_key`, and
`.env.local` sets it with a colon, as a YAML file would:

```jadpo check project=e-local-syntax file=app.jadpo hidden
application {}
```

```jadpo check project=e-local-syntax file=config.jadpo hidden
config {
    /// The key the mail provider gave this app.
    mail_api_key: Text {
        secret: true
    }
}
```

```sh run project=e-local-syntax expect=configuration.local_syntax
printf 'MAIL_API_KEY: mk_test_51ab\n' > .env.local
jadpo config check
```

```text output
error: `.env.local` line 1 isn't `NAME=value`
  reason: Each line is blank, a `#` comment, or `NAME=value`; a value with spaces or other unusual characters goes in double quotation marks.
  next: Fix line 1 of `.env.local` (human)
  rule: configuration.local_syntax
  help: https://jadpo.dev/docs/errors/configuration.local_syntax
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what `.env.local` holds, which may be secrets only you should
see.

A line that sets a variable a line above it already sets is refused the
same way, since only one of the two values can be the one you meant:

```sh run project=e-local-syntax expect=configuration.local_syntax
printf 'MAIL_API_KEY=mk_test_51ab\nMAIL_API_KEY=mk_test_77cd\n' > .env.local
jadpo config check
```

```text output
error: `.env.local` line 2 sets `MAIL_API_KEY`, which line 1 already sets
  reason: Each line is blank, a `#` comment, or `NAME=value`; a value with spaces or other unusual characters goes in double quotation marks.
  next: Fix line 2 of `.env.local` (human)
  rule: configuration.local_syntax
  help: https://jadpo.dev/docs/errors/configuration.local_syntax
```

## How to fix it

**If the line holds a value**, write it as `NAME=value`, with the value
in double quotation marks if it has spaces:

```sh run project=e-local-syntax
printf 'MAIL_API_KEY=mk_test_51ab\n' > .env.local
jadpo config check
```

```text output
Values
  mail_api_key  set      MAIL_API_KEY

config ready: 1 setting
```

The setting now reads its value from `.env.local`, and `jadpo dev`
starts with it.

**If two lines set the same variable**, keep the line with the value
you mean and delete the other:

```sh run project=e-local-syntax
printf 'MAIL_API_KEY=mk_test_77cd\n' > .env.local
jadpo config check
```

```text output
Values
  mail_api_key  set      MAIL_API_KEY

config ready: 1 setting
```

The app reads the value you kept, on this machine only.

## Related

- [Values on your machine and in production](../language/10-configuration-and-time.md#values-on-your-machine-and-in-production)
- [Checking values: `jadpo config check`](../language/10-configuration-and-time.md#checking-values-jadpo-config-check)
- [Secrets](../language/10-configuration-and-time.md#secrets)

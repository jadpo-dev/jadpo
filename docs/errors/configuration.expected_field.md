# `configuration.expected_field`

A `config { }` holds something that isn't a setting.

## Why Jadpo refuses it

`config { }` is the one list of everything a deployment must provide. Each
line says a setting's name and what its value must be, so the application
can refuse to start with a value that isn't one. A line that doesn't say
both tells Jadpo nothing it can check.

## What it looks like

```jadpo check project=e-expected-field expect=configuration.expected_field
config {
    mail_sender
}
```

```text output
error: `mail_sender` isn't a setting
 --> main.jadpo:2:5
  |
2 |     mail_sender
  |     ^^^^^^^^^^^ not `name: Type`
  reason: `config { }` holds settings, each written `name: Type`, with a block after the type when it has `secret:`, `default:`, `binding:` or `test:`.
  next: Write the setting as `name: Type`, or remove this line (agent)
  rule: configuration.expected_field
  help: https://jadpo.dev/docs/errors/configuration.expected_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the line is a setting**, give it a type:

```jadpo check project=e-expected-field-setting
config {
    mail_sender: Email
}
```

The app now reads `MAIL_SENDER` as an email address, and won't start
without a valid one.

**If it was left behind**, remove the line, and keep the settings around
it:

```jadpo check project=e-expected-field-removed
config {
    page_size: Int {
        default: 25
    }
}
```

`config { }` now lists only what a deployment must provide.

## Related

- [Settings: `config.jadpo`](../language/10-configuration-and-time.md#settings-configjadpo)
- [What a setting can hold](../language/10-configuration-and-time.md#what-a-setting-can-hold)

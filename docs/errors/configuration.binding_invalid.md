# `configuration.binding_invalid`

A setting's `binding:` isn't a variable name of capital letters, digits and `_`, in quotes.

## Why Jadpo refuses it

`binding:` names the environment variable a setting is read from. A name
of capital letters, digits and `_`, starting with a capital letter, is one
every platform accepts, so a deployment can always set it. Any other name
might work on your machine and be refused by the platform you deploy to.

## What it looks like

```jadpo check project=e-binding-invalid expect=configuration.binding_invalid
config {
    mail_sender: Email {
        binding: "MAIL-SENDER"
    }
}
```

```text output
error: `MAIL-SENDER` can't be a variable name
 --> main.jadpo:3:18
  |
3 |         binding: "MAIL-SENDER"
  |                  ^^^^^^^^^^^^^ not a variable name
  reason: A variable name is capital letters, digits and `_`, starting with a capital letter, so every platform accepts it.
  next: Use `MAIL_SENDER` (agent)
  rule: configuration.binding_invalid
  help: https://jadpo.dev/docs/errors/configuration.binding_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `binding:` that isn't in quotes is refused the same way, because the
variable's name is text:

```jadpo check project=e-binding-unquoted expect=configuration.binding_invalid
config {
    mail_sender: Email {
        binding: MAIL_SENDER
    }
}
```

```text output
error: `binding:` isn't a variable name in quotes
 --> main.jadpo:3:18
  |
3 |         binding: MAIL_SENDER
  |                  ^^^^^^^^^^^ not text
  reason: `binding:` names the environment variable a setting is read from, as text in quotes: a platform's own name for a value, such as `"HEROKU_POSTGRESQL_URL"`.
  next: Write the variable's name in quotes, or leave `binding:` out to read the setting's name in capitals (agent)
  rule: configuration.binding_invalid
  help: https://jadpo.dev/docs/errors/configuration.binding_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If the name holds other characters or isn't in quotes**, write it in
capitals, digits and `_`, in quotes:

```jadpo check project=e-binding-invalid-named
config {
    mail_sender: Email {
        binding: "MAIL_SENDER"
    }
}
```

`mail_sender` is now read from `MAIL_SENDER`, a name every platform takes.

**If the setting's own name in capitals will do**, leave `binding:` out:

```jadpo check project=e-binding-invalid-derived
config {
    mail_sender: Email
}
```

The variable's name now comes from the setting's, so renaming one renames
the other.

## Related

- [Environment variable names](../language/10-configuration-and-time.md#environment-variable-names)
- [Settings: `config.jadpo`](../language/10-configuration-and-time.md#settings-configjadpo)

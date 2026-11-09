# `configuration.binding_empty`

A setting's `binding:` names no environment variable.

## Why Jadpo refuses it

`binding:` says which environment variable a setting is read from, when
that isn't the setting's name in capitals. An empty name reads nothing, so
the setting could never get a value. Jadpo can't tell which variable you
meant, so it asks you to name one.

## What it looks like

```jadpo check project=e-binding-empty expect=configuration.binding_empty
config {
    mail_sender: Email {
        binding: ""
    }
}
```

```text output
error: `binding:` names no variable
 --> main.jadpo:3:18
  |
3 |         binding: ""
  |                  ^^ empty
  reason: `binding:` names the environment variable a setting is read from; an empty name reads nothing.
  next: Name the variable, or leave `binding:` out to read the setting's name in capitals (agent)
  rule: configuration.binding_empty
  help: https://jadpo.dev/docs/errors/configuration.binding_empty

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If a platform already sets the value under its own name**, write that
name:

```jadpo check project=e-binding-empty-named
config {
    mail_sender: Email {
        binding: "MAIL_FROM"
    }
}
```

`mail_sender` is now read from `MAIL_FROM` in every deployment.

**If the setting's own name in capitals will do**, leave `binding:` out:

```jadpo check project=e-binding-empty-derived
config {
    mail_sender: Email
}
```

`mail_sender` is now read from `MAIL_SENDER`.

## Related

- [Environment variable names](../language/10-configuration-and-time.md#environment-variable-names)
- [Settings: `config.jadpo`](../language/10-configuration-and-time.md#settings-configjadpo)

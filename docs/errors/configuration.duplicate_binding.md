# `configuration.duplicate_binding`

Two settings read the same environment variable.

## Why Jadpo refuses it

Each setting has its own environment variable. Two that share one would
always hold the same value, and renaming either would change both. Jadpo
won't choose which setting gives up the variable, because the deployment's
environment names it.

## What it looks like

```jadpo check project=e-duplicate-binding expect=configuration.duplicate_binding
config {
    mail_sender: Email {
        binding: "MAIL"
    }

    reply_to: Email {
        binding: "MAIL"
    }
}
```

```text output
error: `reply_to` and `mail_sender` both read `MAIL`
 --> main.jadpo:7:18
  |
3 |         binding: "MAIL"
  |                  ------ first read here
7 |         binding: "MAIL"
  |                  ^^^^^^ the same variable
  reason: Each setting has its own environment variable; two settings sharing one would always hold the same value, and renaming either would change both.
  next: Give `reply_to` a variable of its own with `binding:`, or merge the two settings (agent)
  rule: configuration.duplicate_binding
  help: https://jadpo.dev/docs/errors/configuration.duplicate_binding

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `SigningKey` or an `EncryptionKey` that is secret also reads its variable
with `_PREVIOUS` after it, for rotating the key, so no other setting can
bind that name:

```jadpo check project=e-duplicate-binding-previous expect=configuration.duplicate_binding
config {
    old_session_key: SigningKey {
        secret: true
        binding: "SESSION_KEY_PREVIOUS"
    }
    session_key: SigningKey {
        secret: true
    }
}
```

```text output
error: `session_key` and `old_session_key` both read `SESSION_KEY_PREVIOUS`
 --> main.jadpo:6:5
  |
4 |         binding: "SESSION_KEY_PREVIOUS"
  |                  ---------------------- first read here
6 |     session_key: SigningKey {
  |     ^^^^^^^^^^^ the same variable
  reason: Each setting has its own environment variable; two settings sharing one would always hold the same value, and renaming either would change both.
  next: Give `session_key` a variable of its own with `binding:`, or merge the two settings (agent)
  rule: configuration.duplicate_binding
  help: https://jadpo.dev/docs/errors/configuration.duplicate_binding

check failed: 1 error, 1 file
```

## How to fix it

**If the settings hold different values**, give each a variable of its own:

```jadpo check project=e-duplicate-binding-own
config {
    mail_sender: Email {
        binding: "MAIL"
    }

    reply_to: Email {
        binding: "MAIL_REPLY_TO"
    }
}
```

Each deployment now sets the two values separately.

**If they are one value**, keep one setting and read it in both places:

```jadpo check project=e-duplicate-binding-merged
config {
    mail_sender: Email {
        binding: "MAIL"
    }
}
```

There is one setting to change, and one variable to set.

## Related

- [Environment variable names](../language/10-configuration-and-time.md#environment-variable-names)
- [Signing keys](../language/10-configuration-and-time.md#signing-keys)

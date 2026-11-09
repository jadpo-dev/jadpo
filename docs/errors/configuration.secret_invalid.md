# `configuration.secret_invalid`

A setting's `secret:` isn't `true` or `false`.

## Why Jadpo refuses it

`secret: true` marks a value that must never leave the server. A setting is
either secret or it isn't, so anything else leaves Jadpo not knowing whether
to keep the value out of logs, answers and source.

## What it looks like

```jadpo check project=e-secret-invalid expect=configuration.secret_invalid
config {
    api_key: Text {
        secret: yes
    }
}
```

```text output
error: `secret:` on `api_key` isn't `true` or `false`
 --> main.jadpo:3:17
  |
3 |         secret: yes
  |                 ^^^ not a flag
  reason: `secret: true` marks a value that must never leave the server; the setting is either secret or it is not.
  next: Write `secret: true`, or leave `secret:` out (agent)
  rule: configuration.secret_invalid
  help: https://jadpo.dev/docs/errors/configuration.secret_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the value must never leave the server**, write `true`:

```jadpo check project=e-secret-invalid-true
config {
    api_key: Text {
        secret: true
    }
}
```

`config.api_key` can now reach only the places where Jadpo keeps it on the
server.

**If it isn't secret**, leave `secret:` out:

```jadpo check project=e-secret-invalid-open
config {
    api_key: Text
}
```

The setting can now be read anywhere, and returned in an answer like any
other value.

## Related

- [Secrets](../language/10-configuration-and-time.md#secrets)

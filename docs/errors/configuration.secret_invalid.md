# `configuration.secret_invalid`

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
  help: docs/errors/configuration.secret_invalid.md

check failed: 1 error, 1 file
```

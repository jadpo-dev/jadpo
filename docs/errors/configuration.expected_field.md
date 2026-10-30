# `configuration.expected_field`

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

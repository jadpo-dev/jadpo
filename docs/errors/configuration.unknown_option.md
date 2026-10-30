# `configuration.unknown_option`

```jadpo check project=e-unknown-option expect=configuration.unknown_option
config {
    mail_sender: Email {
        binding: "MAIL_SENDER"
        required: true
    }
}
```

```text output
error: `required` isn't an option a setting takes
 --> main.jadpo:4:9
  |
4 |         required: true
  |         ^^^^^^^^ unknown option
  reason: `mail_sender` can say `secret:`, `default:`, `binding:` and `test:`. A setting is required unless it has a `default:`; there are no optional settings, so give a default or leave the setting required.
  next: Remove `required` (agent)
  rule: configuration.unknown_option
  help: https://jadpo.dev/docs/errors/configuration.unknown_option

check failed: 1 error, 1 file
```

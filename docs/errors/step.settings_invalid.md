# `step.settings_invalid`

`steps:` holds one setting, `encryption_key:`, which reads a `SigningKey`
setting:

```jadpo check project=e-step-settings-invalid file=config.jadpo hidden
config {
    step_encryption_key: Text {
        secret: true
    }
}
```

```jadpo check project=e-step-settings-invalid file=app.jadpo expect=step.settings_invalid
application {
    steps: {
        encryption_key: config.step_encryption_key
    }
}
```

```text output
error: `encryption_key:` reads `step_encryption_key`, which isn't a `SigningKey`
 --> app.jadpo:3:25
  |
3 |         encryption_key: config.step_encryption_key
  |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^ `step_encryption_key` is `Text`
  reason: A record's answers are encrypted with a `SigningKey` setting: secret, at least 32 characters, and replaced by moving the old value to the variable with `_PREVIOUS` after its name (area 10), as grants' `encryption_key:` is.
  next: Declare `step_encryption_key: SigningKey` in `config.jadpo` (agent)
  rule: step.settings_invalid
  help: https://jadpo.dev/docs/errors/step.settings_invalid

check failed: 1 error, 2 files
```

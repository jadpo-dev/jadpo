# `step.settings_invalid`

The application's `steps:` holds something other than an `encryption_key:` that reads a `SigningKey` setting.

## Why Jadpo refuses it

An action that makes several changing calls keeps each call's answer,
so that a retry replays it rather than sending the call again. Those
answers can hold anything a provider sent back, so they are kept
encrypted, with the key `application { steps: { encryption_key: } }`
reads. That key must be a `SigningKey` setting: secret, at least 32
characters, and replaced without losing what was kept by moving the old
value to the variable with `_PREVIOUS` after its name. A key written in
the source would sit in the repository and every review, and a setting
of another type has none of those guarantees, so Jadpo refuses both, as
it does any other setting under `steps:`.

## What it looks like

config.jadpo:

```jadpo check project=e-step-settings-invalid file=config.jadpo
config {
    step_encryption_key: Text {
        secret: true
    }
}
```

app.jadpo:

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
  reason: A record's answers are encrypted with a `SigningKey` setting: secret, at least 32 characters, and replaced by moving the old value to the variable with `_PREVIOUS` after its name, as grants' `encryption_key:` is.
  next: Declare `step_encryption_key: SigningKey` in `config.jadpo` (agent)
  rule: step.settings_invalid
  help: https://jadpo.dev/docs/errors/step.settings_invalid

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the setting is the key**, declare it as a `SigningKey`. In
`config.jadpo`:

```jadpo fragment
config {
    step_encryption_key: SigningKey
}
```

```jadpo check project=e-step-settings-invalid-fix-1 file=config.jadpo hidden
config {
    step_encryption_key: SigningKey
}
```

```jadpo check project=e-step-settings-invalid-fix-1 file=app.jadpo hidden
application {
    steps: {
        encryption_key: config.step_encryption_key
    }
}
```

The app now refuses to start without a value of at least 32 characters
for `step_encryption_key`, and the answers its multi-step actions keep
are sealed with it.

## Related

- [How the record is kept](../language/12-jobs-and-events.md#how-the-record-is-kept)
- [Several changing calls in one action](../language/12-jobs-and-events.md#several-changing-calls-in-one-action)
- [Signing keys](../language/10-configuration-and-time.md#signing-keys)

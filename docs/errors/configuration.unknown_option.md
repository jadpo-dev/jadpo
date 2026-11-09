# `configuration.unknown_option`

A setting has an option that settings don't take.

## Why Jadpo refuses it

A setting can say `secret:`, `default:`, `binding:` and `test:`, and
nothing else. An option Jadpo doesn't know does nothing, so leaving it in
would suggest a behaviour the setting doesn't have. In particular a setting
is required unless it has a `default:`, and there are no optional settings.

## What it looks like

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A misspelt option names the one it is nearest to:

```jadpo check project=e-unknown-option-near expect=configuration.unknown_option
config {
    page_size: Int {
        defualt: 25
    }
}
```

```text output
error: `defualt` isn't an option a setting takes
 --> main.jadpo:3:9
  |
3 |         defualt: 25
  |         ^^^^^^^ unknown option
  reason: `page_size` can say `secret:`, `default:`, `binding:` and `test:`. Did you mean `default`?
  next: Use `default` instead of `defualt` (agent)
  rule: configuration.unknown_option
  help: https://jadpo.dev/docs/errors/configuration.unknown_option

check failed: 1 error, 1 file
```

## How to fix it

**If it says the setting is required**, remove it, because a setting
without a `default:` is:

```jadpo check project=e-unknown-option-required
config {
    mail_sender: Email {
        binding: "MAIL_SENDER"
    }
}
```

The app still won't start until `MAIL_SENDER` is set.

**If an option is misspelt**, write it as the error says:

```jadpo check project=e-unknown-option-spelt
config {
    page_size: Int {
        default: 25
    }
}
```

A deployment that doesn't set `PAGE_SIZE` now starts with 25.

## Related

- [Settings: `config.jadpo`](../language/10-configuration-and-time.md#settings-configjadpo)
- [Defaults and test values](../language/10-configuration-and-time.md#defaults-and-test-values)

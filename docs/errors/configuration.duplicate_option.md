# `configuration.duplicate_option`

A setting gives one of its options twice, or a test's `config:` gives one setting twice.

## Why Jadpo refuses it

A setting says each of `secret:`, `default:`, `binding:` and `test:` once,
and a test says each setting's value once, so there is one answer to read.
Jadpo can't tell which of two you meant, so it asks you to keep one.

## What it looks like

```jadpo check project=e-duplicate-option expect=configuration.duplicate_option
config {
    mail_sender: Email {
        binding: "MAIL_SENDER"
        binding: "SENDER"
    }
}
```

```text output
error: `binding:` is given twice on `mail_sender`
 --> main.jadpo:4:9
  |
3 |         binding: "MAIL_SENDER"
  |         ------- first given here
4 |         binding: "SENDER"
  |         ^^^^^^^ given again
  reason: A setting says each of `secret:`, `default:`, `binding:` and `test:` once, so there is one answer to read.
  next: Remove one `binding:` (agent)
  rule: configuration.duplicate_option
  help: https://jadpo.dev/docs/errors/configuration.duplicate_option

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A test's `config:` is refused the same way:

```jadpo check project=e-duplicate-option-test expect=configuration.duplicate_option
type Label = Text {
    min_length: 3
}

config {
    label: Label {
        test: "valid"
    }
}

function label_in_use() -> Label {
    return config.label
}

test "a test's label is its own" {
    config: {
        label: "other"
        label: "again"
    }
    assert label_in_use() == "other"
}
```

```text output
error: `label` is given twice in `config:`
 --> main.jadpo:18:9
   |
17 |         label: "other"
   |         ----- first given here
18 |         label: "again"
   |         ^^^^^ given again
  reason: A test says each setting's value once, so there is one answer to read.
  next: Remove one `label:` (agent)
  rule: configuration.duplicate_option
  help: https://jadpo.dev/docs/errors/configuration.duplicate_option

check failed: 1 error, 1 file
```

## How to fix it

**If one of them is what you meant**, remove the other:

```jadpo check project=e-duplicate-option-fix
config {
    mail_sender: Email {
        binding: "MAIL_SENDER"
    }
}
```

`mail_sender` is now read from `MAIL_SENDER` alone.

## Related

- [Settings: `config.jadpo`](../language/10-configuration-and-time.md#settings-configjadpo)
- [Defaults and test values](../language/10-configuration-and-time.md#defaults-and-test-values)
- [Configuration in tests](../language/13-testing.md#configuration-in-tests)

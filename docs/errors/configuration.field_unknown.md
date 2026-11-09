# `configuration.field_unknown`

Code or a test names a setting `config { }` doesn't declare.

## Why Jadpo refuses it

Every setting is declared in the one `config { }`, so a reader can see
everything a deployment must provide. A name that isn't there would have no
variable to read and no type to check it against, so it can't be compiled.
Jadpo won't declare it for you, because a new setting is something every
deployment then has to set.

## What it looks like

```jadpo check project=e-field-unknown expect=configuration.field_unknown
type FromAddress = Email {}

config {
    mail_sender: FromAddress
}

function from_address() -> FromAddress {
    return config.mail_sendr
}
```

```text output
error: `config.mail_sendr` isn't a setting
 --> main.jadpo:8:12
  |
8 |     return config.mail_sendr
  |            ^^^^^^^^^^^^^^^^^ no such setting
  reason: `config { }` declares no setting called `mail_sendr`. Did you mean `mail_sender`?
  next: Use `config.mail_sender` instead of `config.mail_sendr` (agent)
  rule: configuration.field_unknown
  help: https://jadpo.dev/docs/errors/configuration.field_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A test's `config:` naming a setting `config { }` doesn't declare is refused
the same way, because it would have nothing to change:

```jadpo check project=e-field-unknown-test expect=configuration.field_unknown
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
        lable: "other"
    }
    assert label_in_use() == "other"
}
```

```text output
error: `lable` isn't a setting
 --> main.jadpo:17:9
   |
17 |         lable: "other"
   |         ^^^^^ no such setting
  reason: `config { }` declares no setting called `lable`, so a test has nothing to change. Did you mean `label`?
  next: Use `label` instead of `lable` (agent)
  rule: configuration.field_unknown
  help: https://jadpo.dev/docs/errors/configuration.field_unknown

check failed: 1 error, 1 file
```

A project with no `config { }` at all has no settings to read:

```jadpo check project=e-field-unknown-none expect=configuration.field_unknown
type ListLimit = Int {
    min: 1
}

function page_size() -> ListLimit {
    return config.page_size
}
```

```text output
error: `config.page_size` isn't a setting
 --> main.jadpo:6:12
  |
6 |     return config.page_size
  |            ^^^^^^^^^^^^^^^^ no such setting
  reason: The project declares no `config { }`, so it has no settings; they are declared in `config.jadpo`.
  next: Declare `page_size: <Type>` in `config.jadpo` (agent)
  rule: configuration.field_unknown
  help: https://jadpo.dev/docs/errors/configuration.field_unknown

check failed: 1 error, 1 file
```

## How to fix it

**If the name is misspelt**, write it as `config { }` does:

```jadpo fragment
function from_address() -> FromAddress {
    return config.mail_sender
}
```

```jadpo check project=e-field-unknown-spelt hidden
type FromAddress = Email {}

config {
    mail_sender: FromAddress {
        test: "reminders@example.test"
    }
}

function from_address() -> FromAddress {
    return config.mail_sender
}
```

The function now returns the setting the deployment gives.

**If the setting is new**, declare it:

```jadpo fragment
config {
    page_size: ListLimit {
        default: 25
    }
}
```

```jadpo check project=e-field-unknown-declared hidden
type ListLimit = Int {
    min: 1
}

config {
    page_size: ListLimit {
        default: 25
    }
}

function page_size() -> ListLimit {
    return config.page_size
}
```

`config.page_size` now reads `PAGE_SIZE`, or 25 when a deployment doesn't
set it.

## Related

- [Settings: `config.jadpo`](../language/10-configuration-and-time.md#settings-configjadpo)
- [Configuration in tests](../language/13-testing.md#configuration-in-tests)

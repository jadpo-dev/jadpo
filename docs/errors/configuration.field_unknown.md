# `configuration.field_unknown`

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
  help: docs/language/10-configuration-and-time.html#configuration-field-unknown

check failed: 1 error, 1 file
```

A test's `config:` names a setting `config { }` doesn't declare (the rule
is area 10's):

```jadpo check project=e-field-unknown expect=configuration.field_unknown
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
  help: docs/errors/configuration.field_unknown.md

check failed: 1 error, 1 file
```

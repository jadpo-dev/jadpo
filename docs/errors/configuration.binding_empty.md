# `configuration.binding_empty`

```jadpo check project=e-binding-empty expect=configuration.binding_empty
config {
    mail_sender: Email {
        binding: ""
    }
}
```

```text output
error: `binding:` names no variable
 --> main.jadpo:3:18
  |
3 |         binding: ""
  |                  ^^ empty
  reason: `binding:` names the environment variable a setting is read from; an empty name reads nothing.
  next: Name the variable, or leave `binding:` out to read the setting's name in capitals (agent)
  rule: configuration.binding_empty
  help: docs/language/10-configuration-and-time.html#configuration-binding-empty

check failed: 1 error, 1 file
```

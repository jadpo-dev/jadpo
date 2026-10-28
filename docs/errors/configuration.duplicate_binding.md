# `configuration.duplicate_binding`

```jadpo check project=e-duplicate-binding expect=configuration.duplicate_binding
config {
    mail_sender: Email {
        binding: "MAIL"
    }

    reply_to: Email {
        binding: "MAIL"
    }
}
```

```text output
error: `reply_to` and `mail_sender` both read `MAIL`
 --> main.jadpo:7:18
  |
3 |         binding: "MAIL"
  |                  ------ first read here
7 |         binding: "MAIL"
  |                  ^^^^^^ the same variable
  reason: Each setting has its own environment variable; two settings sharing one would always hold the same value, and renaming either would change both.
  next: Give `reply_to` a variable of its own with `binding:`, or merge the two settings (agent)
  rule: configuration.duplicate_binding
  help: docs/language/10-configuration-and-time.html#configuration-duplicate-binding

check failed: 1 error, 1 file
```

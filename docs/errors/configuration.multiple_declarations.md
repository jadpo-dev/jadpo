# `configuration.multiple_declarations`

```jadpo check project=e-multiple expect=configuration.multiple_declarations
config {
    mail_sender: Email {
        binding: "MAIL_SENDER"
    }
}

config {
    browser_origin: Url {
        binding: "BROWSER_ORIGIN"
    }
}
```

```text output
error: `config { }` is declared twice
 --> main.jadpo:7:1
  |
1 | config {
  | ------ first declared here
7 | config {
  | ^^^^^^ declared again
  reason: Every setting the application needs is a field of the one `config { }`, in `config.jadpo`, so one place lists everything a deployment must provide.
  next: Move the settings this `config` holds into the first, and remove it (agent)
  rule: configuration.multiple_declarations
  help: docs/errors/configuration.multiple_declarations.md

check failed: 1 error, 1 file
```

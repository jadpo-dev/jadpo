# `semantic.multiple_applications`

```jadpo check project=e-multiple-applications expect=semantic.multiple_applications
application {}

application {
    revocation_delay: 5m
}
```

```text output
error: `application { }` is declared twice
 --> main.jadpo:3:1
  |
1 | application {}
  | ----------- first declared here
3 | application {
  | ^^^^^^^^^^^ declared again
  reason: A project is one application, declared once in `app.jadpo`; its settings all go in that one block.
  next: Move what this `application` holds into the first, and remove it (agent)
  rule: semantic.multiple_applications
  help: docs/errors/semantic.multiple_applications.md

check failed: 1 error, 1 file
```

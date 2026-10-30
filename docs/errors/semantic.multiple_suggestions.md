# `semantic.multiple_suggestions`

A project has one `suggestions { }`.

```jadpo check project=e-suggestions-multiple expect=semantic.multiple_suggestions
suggestions {
    share: false
}

suggestions {
    share: true
}
```

```text output
error: `suggestions { }` is declared twice
 --> main.jadpo:5:1
  |
1 | suggestions {
  | ----------- first declared here
5 | suggestions {
  | ^^^^^^^^^^^ declared again
  reason: A project has one token and one answer to whether it shares, in one `suggestions { }` in `app.jadpo`.
  next: Move what this `suggestions` holds into the first, and remove it (agent)
  rule: semantic.multiple_suggestions
  help: https://jadpo.dev/docs/errors/semantic.multiple_suggestions

check failed: 1 error, 1 file
```

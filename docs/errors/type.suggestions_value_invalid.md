# `type.suggestions_value_invalid`

A token is the 32 random hex digits `jadpo suggest share` writes, so a name can't stand in for one; `share:` is `true` or `false`.

```jadpo check project=e-suggestions-value-invalid expect=type.suggestions_value_invalid
suggestions {
    project_token: "acme-shop"
}
```

```text output
error: `project_token: "acme-shop"` isn't a value `project_token` takes
 --> main.jadpo:2:20
  |
2 |     project_token: "acme-shop"
  |                    ^^^^^^^^^^^ not a token
  reason: A project token is 32 random hex digits, tied to nothing, so Jadpo HQ can tell one project from another without knowing which it is.
  next: Remove `project_token:`; `jadpo suggest share` writes a new one (agent)
  rule: type.suggestions_value_invalid
  help: https://jadpo.dev/docs/errors/type.suggestions_value_invalid

check failed: 1 error, 1 file
```

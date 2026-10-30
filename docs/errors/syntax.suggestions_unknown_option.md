# `syntax.suggestions_unknown_option`

`suggestions { }` takes `project_token:` and `share:`, nothing else.

```jadpo check project=e-suggestions-unknown-option expect=syntax.suggestions_unknown_option
suggestions {
    shared: false
}
```

```text output
error: `suggestions` has no option `shared:`
 --> main.jadpo:2:5
  |
2 |     shared: false
  |     ^^^^^^ no such option
  reason: `suggestions` takes `project_token:` and `share:`. Did you mean `share`?
  next: Use `share:` instead of `shared:` (agent)
  rule: syntax.suggestions_unknown_option
  help: https://jadpo.dev/docs/errors/syntax.suggestions_unknown_option

check failed: 1 error, 1 file
```

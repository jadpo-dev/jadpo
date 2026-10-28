# `syntax.suggestions_duplicate_option`

Each option is given once.

```jadpo check project=e-suggestions-duplicate-option expect=syntax.suggestions_duplicate_option
suggestions {
    share: false
    share: true
}
```

```text output
error: `share:` is given twice
 --> main.jadpo:3:5
  |
2 |     share: false
  |     ----- first given here
3 |     share: true
  |     ^^^^^ given again
  reason: `suggestions` says each of `project_token:` and `share:` once.
  next: Keep one `share:` and remove the other (agent)
  rule: syntax.suggestions_duplicate_option
  help: docs/language/14-tooling.html#syntax-suggestions-duplicate-option

check failed: 1 error, 1 file
```

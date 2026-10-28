# `syntax.unterminated_string`

```jadpo check project=e-unterminated expect=syntax.unterminated_string
type Label = Text {
    pattern: "abc
}
```

```text output
error: This string isn't closed
 --> main.jadpo:2:14
  |
2 |     pattern: "abc
  |              ^^^^ no closing `"` on this line
  reason: A string starts and ends with `"` on the same line.
  next: Add `"` where the string ends (agent)
  rule: syntax.unterminated_string
  help: docs/language/01-project-layout-and-modules.html#syntax-unterminated-string

check failed: 1 error, 1 file
```

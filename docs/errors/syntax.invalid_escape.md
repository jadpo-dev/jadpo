# `syntax.invalid_escape`

```jadpo check project=e-escape expect=syntax.invalid_escape
type Label = Text {
    pattern: "a\qb"
}
```

```text output
error: `\q` isn't an escape Jadpo knows
 --> main.jadpo:2:16
  |
2 |     pattern: "a\qb"
  |                ^^ unknown escape
  reason: Inside a string, `\` starts one of `\"`, `\\`, `\n`, `\r` or `\t`.
  next: Write `\\q` for a backslash followed by `q` (agent)
  rule: syntax.invalid_escape
  help: https://jadpo.dev/docs/errors/syntax.invalid_escape

check failed: 1 error, 1 file
```

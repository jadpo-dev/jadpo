# `syntax.colon_required`

Every member inside braces is `name: value`:

```jadpo check project=e-colon expect=syntax.colon_required
type ContactEmail = Text {
    format TextFormat.email
}
```

```text output
error: `format` needs a colon before its value
 --> main.jadpo:2:5
  |
2 |     format TextFormat.email
  |     ^^^^^^ expected `:` after `format`
  reason: Every member inside braces is `name: value`, with a colon even when the value is a block.
  next: Write `format: TextFormat.email` (agent)
  rule: syntax.colon_required
  help: docs/errors/syntax.colon_required.md

check failed: 1 error, 1 file
```

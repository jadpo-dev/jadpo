# `syntax.unexpected_character`

```jadpo check project=e-unexpected-character expect=syntax.unexpected_character
type Count = Int {
    min: 0 $
}
```

```text output
error: `$` isn't part of Jadpo
 --> main.jadpo:2:12
  |
2 |     min: 0 $
  |            ^ not Jadpo
  reason: Jadpo source is names, literals, brackets and operators such as `+` and `==`; this character has no meaning.
  next: Remove `$` (agent)
  rule: syntax.unexpected_character
  help: docs/language/01-project-layout-and-modules.html#syntax-unexpected-character

check failed: 1 error, 1 file
```

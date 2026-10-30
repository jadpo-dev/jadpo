# `type.invalid_pattern`

A pattern must match the whole text and uses a subset of regular expressions without backreferences or lookaround (question 9):

```jadpo check project=e-pattern expect=type.invalid_pattern
type Doubled = Text {
    pattern: "(a+)+\\1"
}
```

```text output
error: This pattern can't be used
 --> main.jadpo:2:14
  |
2 |     pattern: "(a+)+\\1"
  |              ^^^^^^^^^^ backreferences are not allowed
  reason: A pattern is a regular expression that must match the whole text, without backreferences or lookaround, so it runs in linear time and the same way on every target.
  next: Rewrite the pattern without the unsupported part (agent)
  rule: type.invalid_pattern
  help: https://jadpo.dev/docs/errors/type.invalid_pattern

check failed: 1 error, 1 file
```

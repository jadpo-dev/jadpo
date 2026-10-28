# `syntax.string_quotes`

Strings are in double quotes.

```jadpo check project=e-habit-quotes expect=syntax.string_quotes
type Code = Text {
    pattern: '^[a-z]+$'
}
```

```text output
error: Jadpo writes strings in double quotes
 --> main.jadpo:2:14
  |
2 |     pattern: '^[a-z]+$'
  |              ^^^^^^^^^^ write `"`
  reason: Single quotes enclose text in Python, JavaScript and SQL, and a character in Rust, Java, C# and Kotlin, and backticks a template in JavaScript; Jadpo has one kind of string, in double quotes.
  next: Write `"^[a-z]+$"` (agent)
  rule: syntax.string_quotes
  help: docs/language/02-values-and-types.html#syntax-string-quotes

check failed: 1 error, 1 file
```

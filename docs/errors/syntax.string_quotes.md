# `syntax.string_quotes`

A string is written in single quotes or backticks instead of double quotes.

## Why Jadpo refuses it

Single quotes enclose text in Python, JavaScript and SQL, and a character in
Rust, Java, C# and Kotlin, and backticks a template in JavaScript. Jadpo has
one kind of string, in double quotes, so a quote mark never changes what the
text inside it means.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/syntax.string_quotes

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Text in backticks is the same error, with the same fix.

## How to fix it

**If you wrote a string in other quotes**, write double quotes:

```jadpo check project=e-habit-quotes-fix
type Code = Text {
    pattern: "^[a-z]+$"
}
```

`Code` now holds text matching the pattern.

## Related

- [Constraints](../language/02-values-and-types.md#constraints)
- [Habits from other languages](../language/02-values-and-types.md#habits-from-other-languages)

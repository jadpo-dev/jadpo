# `syntax.unterminated_string`

A string has no closing `"` on its line.

## Why Jadpo refuses it

A string starts and ends with `"` on the same line. Without that rule a
missing quote would swallow the code after it, and the error would turn up
somewhere else, so Jadpo stops at the end of the line where the string began.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/syntax.unterminated_string

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the closing quote is missing**, add it where the string ends:

```jadpo check project=e-unterminated-fix
type Label = Text {
    pattern: "abc"
}
```

The string now ends after `abc`, and `Label` holds text matching it.

## Related

- [Constraints](../language/02-values-and-types.md#constraints)
- [Source files](../language/01-project-layout-and-modules.md#source-files)

# `syntax.invalid_escape`

A string holds a backslash followed by something that isn't an escape Jadpo knows.

## Why Jadpo refuses it

Inside a string, `\` starts one of `\"`, `\\`, `\n`, `\r` or `\t`. Reading
`\q` as a plain `q`, or as a backslash and a `q`, would give two different
strings from what was written, and Jadpo won't pick one for you.

## What it looks like

main.jadpo:

```jadpo check project=e-escape expect=syntax.invalid_escape
type SharePath = Text {}

constant share_root: SharePath = "C:\qdata"
```

```text output
error: `\q` isn't an escape Jadpo knows
 --> main.jadpo:3:37
  |
3 | constant share_root: SharePath = "C:\qdata"
  |                                     ^^ unknown escape
  reason: Inside a string, `\` starts one of `\"`, `\\`, `\n`, `\r` or `\t`.
  next: Write `\\q` for a backslash followed by `q` (agent)
  rule: syntax.invalid_escape
  help: https://jadpo.dev/docs/errors/syntax.invalid_escape

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant a backslash**, write it as `\\`:

```jadpo fragment
constant share_root: SharePath = "C:\\qdata"
```

```jadpo check project=e-escape-fix hidden
type SharePath = Text {}

constant share_root: SharePath = "C:\\qdata"
```

The constant now holds a real backslash followed by `qdata`.

## Related

- [Other built-in types](../language/02-values-and-types.md#other-built-in-types)

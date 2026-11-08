# `type.invalid_pattern`

A type's `pattern:` is a regular expression Jadpo can't use, because it has a backreference or lookaround or isn't a valid regular expression.

## Why Jadpo refuses it

A pattern checks text that a request or a file sends, so it has to run in
time that grows only with the text's length, and give the same answer on
every target. Backreferences and lookaround can take exponential time on
text made to be slow, and engines disagree about them. A pattern always
matches the whole text, so it needs no `^` or `$`.

## What it looks like

main.jadpo:

```jadpo check project=e-pattern expect=type.invalid_pattern
type VoucherCode = Text {
    min_length: 8
    pattern: "(?=.*[0-9])[a-z0-9]+"
}
```

```text output
error: This pattern can't be used
 --> main.jadpo:3:14
  |
3 |     pattern: "(?=.*[0-9])[a-z0-9]+"
  |              ^^^^^^^^^^^^^^^^^^^^^^ lookahead and lookbehind are not allowed
  reason: A pattern is a regular expression that must match the whole text, without backreferences or lookaround, so it runs in linear time and the same way on every target.
  next: Rewrite the pattern without the unsupported part (agent)
  rule: type.invalid_pattern
  help: https://jadpo.dev/docs/errors/type.invalid_pattern

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the pattern can say the same without the unsupported part**, rewrite
it. Here a lookahead asks for at least one digit, which a pattern can say
by putting the digit in its middle:

```jadpo check project=e-pattern-fix
type VoucherCode = Text {
    min_length: 8
    pattern: "[a-z]*[0-9][a-z0-9]*"
}
```

A `VoucherCode` is still eight or more lower-case letters and digits, at
least one of them a digit, and the pattern checks it in one pass over the
text.

## Related

- [Constraints](../language/02-values-and-types.md#constraints)

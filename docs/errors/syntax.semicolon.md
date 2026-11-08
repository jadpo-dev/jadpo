# `syntax.semicolon`

A statement or member ends with `;`.

## Why Jadpo refuses it

`;` ends a statement in JavaScript, Rust, Go, Java, C#, Kotlin, Swift and
SQL. A Jadpo statement or member ends at the end of its line, so a `;` could
only be noise, and Jadpo has one way to write each line.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-semicolon expect=syntax.semicolon
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    return count;
}
```

```text output
error: Jadpo has no `;`
 --> main.jadpo:6:17
  |
6 |     return count;
  |                 ^ remove `;`
  reason: `;` ends a statement in JavaScript, Rust, Go, Java, C#, Kotlin, Swift and SQL; a Jadpo statement or member ends at the end of its line.
  next: Remove `;` (agent)
  rule: syntax.semicolon
  help: https://jadpo.dev/docs/errors/syntax.semicolon

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you ended a line with `;`**, remove it:

```jadpo fragment
function same(count: Count) -> Count {
    return count
}
```

```jadpo check project=e-habit-semicolon-fix hidden
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    return count
}
```

The statement now ends where its line does.

## Related

- [Statements](../language/03-logic.md#statements)
- [Habits from other languages](../language/03-logic.md#habits-from-other-languages)

# `syntax.semicolon`

A statement or member ends at the end of its line.

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

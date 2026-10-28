# `syntax.expected_statement`

There is no `while` loop; `for` over a list is the only one:

```jadpo check project=e-statement expect=syntax.expected_statement
type Count = Int {
    min: 0
}

function first(count: Count) -> Count {
    while (count > 0) {
        return count
    }
    return count
}
```

```text output
error: Jadpo has no `while` loop
 --> main.jadpo:6:5
  |
6 |     while (count > 0) {
  |     ^^^^^ not a Jadpo statement
  reason: The only loop is `for item in items`, which always ends after its list; stop early with `return`.
  next: Loop over a list with `for`, or narrow it first with `list.filter` (agent)
  rule: syntax.expected_statement
  help: docs/errors/syntax.expected_statement.md

check failed: 1 error, 1 file
```

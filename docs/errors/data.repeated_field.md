# `data.repeated_field`

```jadpo check project=e-repeated-field expect=data.repeated_field
entity Todo {
    id: Uuid
    due_at: Instant

    indexes: {
        by_due_at: [due_at, due_at]
    }
}
```

```text output
error: The index `by_due_at` lists `due_at` twice
 --> main.jadpo:6:29
  |
6 |         by_due_at: [due_at, due_at]
  |                             ^^^^^^ listed again
  reason: Each field counts once in an index.
  next: Remove the second `due_at` (agent)
  rule: data.repeated_field
  help: docs/errors/data.repeated_field.md

check failed: 1 error, 1 file
```

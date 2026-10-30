# `type.duplicate_variant`

```jadpo check project=e-duplicate-variant expect=type.duplicate_variant
type TicketStatus = Enum {
    open
    closed
    open
}
```

```text output
error: `TicketStatus.open` is declared twice
 --> main.jadpo:4:5
  |
2 |     open
  |     ---- first declared here
4 |     open
  |     ^^^^ declared again
  reason: Each variant of an enum has its own name, so a value and a `match` arm name exactly one.
  next: Remove one `open`, or rename it (agent)
  rule: type.duplicate_variant
  help: https://jadpo.dev/docs/errors/type.duplicate_variant

check failed: 1 error, 1 file
```

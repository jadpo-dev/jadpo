# `failure.stale_declaration`

```jadpo check project=e-stale expect=failure.stale_declaration
type Note = Text {}

type Refused = Failure {
    kind: FailureKind.rejected
}

action complete(note: Note) fails Refused -> Note {
    return note
}
```

```text output
error: `Refused` can never leave `complete`
 --> main.jadpo:7:35
  |
7 | action complete(note: Note) fails Refused -> Note {
  |                                   ^^^^^^^ never raised or passed on
  reason: Nothing in `complete` rejects `Refused` or passes it on from a call, and `fails` lists exactly the failures that can leave, so callers aren't made to handle one that never happens.
  next: Remove `Refused` from `fails` (agent)
  rule: failure.stale_declaration
  help: docs/language/04-failures.html#failure-stale-declaration

check failed: 1 error, 1 file
```

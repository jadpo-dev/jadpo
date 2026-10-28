# `failure.attempt_required`

```jadpo check project=e-attempt expect=failure.attempt_required
type Note = Text {}

type Refused = Failure {
    kind: FailureKind.rejected
}

action child(note: Note) fails Refused -> Note {
    reject Refused
}

action parent(note: Note) fails Refused -> Note {
    return child(note)
}
```

```text output
error: `child` can fail, and nothing handles it
 --> main.jadpo:12:12
   |
12 |     return child(note)
   |            ^^^^^^^^^^^ can fail with `Refused`
  reason: A call that can fail says so where it is made: `attempt` passes its failures on to the caller's `fails` list, and `match` handles each one.
  next: Start the statement with `attempt`, or `match` on `child(...)` and handle `Refused` (agent)
  rule: failure.attempt_required
  help: docs/errors/failure.attempt_required.md

check failed: 1 error, 1 file
```

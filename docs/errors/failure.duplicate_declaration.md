# `failure.duplicate_declaration`

```jadpo check project=e-duplicate-fails expect=failure.duplicate_declaration
type Note = Text {}

type Refused = Failure {
    kind: FailureKind.rejected
}

action execute(note: Note) fails Refused, Refused -> Note {
    reject Refused
}
```

```text output
error: `Refused` is listed twice after `fails`
 --> main.jadpo:7:43
  |
7 | action execute(note: Note) fails Refused, Refused -> Note {
  |                                           ^^^^^^^ listed again
  |                                  ------- first listed here
  reason: `fails` lists each failure that can leave once.
  next: Remove the second `Refused` (agent)
  rule: failure.duplicate_declaration
  help: docs/language/04-failures.html#failure-duplicate-declaration

check failed: 1 error, 1 file
```

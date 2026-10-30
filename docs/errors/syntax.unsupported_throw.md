# `syntax.unsupported_throw`

```jadpo check project=e-throw expect=syntax.unsupported_throw
type Note = Text {}

type NoteRejected = Failure {
    kind: FailureKind.rejected
}

function checked_note(note: Note) fails NoteRejected -> Note {
    if (note == "") {
        throw NoteRejected
    }
    return note
}
```

```text output
error: Jadpo writes `reject`, not `throw`
 --> main.jadpo:9:9
  |
9 |         throw NoteRejected
  |         ^^^^^ write `reject`
  reason: `throw` raises an error in JavaScript, Java, C#, Kotlin and Swift, and `raise` in Python and Ruby; a Jadpo callable raises one of its declared failures with `reject`, and lists it in `fails`.
  next: Use `reject` instead of `throw` (agent)
  rule: syntax.unsupported_throw
  help: https://jadpo.dev/docs/errors/syntax.unsupported_throw

check failed: 1 error, 1 file
```

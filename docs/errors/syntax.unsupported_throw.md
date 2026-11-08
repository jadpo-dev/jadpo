# `syntax.unsupported_throw`

A failure is raised with `throw` or `raise` instead of `reject`.

## Why Jadpo refuses it

`throw` raises an error in JavaScript, Java, C#, Kotlin and Swift, and
`raise` in Python and Ruby. A Jadpo callable raises one of its declared
failures with `reject` and lists it in `fails`, so what can go wrong is in
the signature, never something a call throws without saying.

## What it looks like

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`raise` is the same error, with the same fix.

## How to fix it

**If you were raising a declared failure**, write `reject`:

```jadpo fragment
function checked_note(note: Note) fails NoteRejected -> Note {
    if (note == "") {
        reject NoteRejected
    }
    return note
}
```

```jadpo check project=e-throw-fix hidden
type Note = Text {}

type NoteRejected = Failure {
    kind: FailureKind.rejected
}

function checked_note(note: Note) fails NoteRejected -> Note {
    if (note == "") {
        reject NoteRejected
    }
    return note
}
```

An empty note now makes `checked_note` fail with `NoteRejected`, which its
signature already lists.

## Related

- [Raising a failure: `reject`](../language/04-failures.md#raising-a-failure-reject)
- [Saying what can fail: `fails`](../language/04-failures.md#saying-what-can-fail-fails)

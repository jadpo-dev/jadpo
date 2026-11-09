# `failure.duplicate_declaration`

A failure is listed twice after `fails`.

## Why Jadpo refuses it

`fails` lists each failure that can leave a callable, and callers handle each
one once. A second mention adds nothing, and leaves a reader wondering which
of two similar-looking names was meant.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/failure.duplicate_declaration

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it is a repeat**, remove it:

```jadpo fragment
action execute(note: Note) fails Refused -> Note {
    ...
}
```

```jadpo check project=e-duplicate-fails-fix-1 hidden
type Note = Text {}

type Refused = Failure {
    kind: FailureKind.rejected
}

action execute(note: Note) fails Refused -> Note {
    reject Refused
}
```

Callers now see `Refused` once.

## Related

- [Saying what can fail: `fails`](../language/04-failures.md#saying-what-can-fail-fails)
- [`failure.stale_declaration`](failure.stale_declaration.md)

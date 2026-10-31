# `failure.undeclared_propagation`

A function or action can end with a failure that its `fails` list doesn't name.

## Why Jadpo refuses it

`fails` lists exactly the failures that can leave a callable: everything
a `reject` or an `attempt` in its body can produce, less what a `match`
handles. Callers read the list to know what they must handle, and Jadpo
checks that they handle each one. A failure missing from it would reach
callers that never planned for it.

## What it looks like

main.jadpo:

```jadpo check project=e-undeclared expect=failure.undeclared_propagation
type Note = Text {}

type RegistrationClosed = Failure {
    kind: FailureKind.conflict
}

action register(note: Note) -> Note {
    reject RegistrationClosed
}
```

```text output
error: `RegistrationClosed` can leave `register`, but its `fails` list doesn't say so
 --> main.jadpo:8:5
  |
8 |     reject RegistrationClosed
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^ rejected here
  reason: `fails` lists exactly the failures that can leave an action, so every caller knows what it must handle.
  next: Add `RegistrationClosed` to `register`'s `fails` list (agent)
  rule: failure.undeclared_propagation
  help: https://jadpo.dev/docs/errors/failure.undeclared_propagation

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A failure passed on from a call with `attempt` is the same error, at the
`attempt`:

main.jadpo:

```jadpo check project=e-undeclared-attempt expect=failure.undeclared_propagation
type Note = Text {}

type Refused = Failure {
    kind: FailureKind.rejected
}

action child(note: Note) fails Refused -> Note {
    reject Refused
}

action parent(note: Note) -> Note {
    return attempt child(note)
}
```

```text output
error: `Refused` can leave `parent`, but its `fails` list doesn't say so
 --> main.jadpo:12:12
   |
12 |     return attempt child(note)
   |            ^^^^^^^^^^^^^^^^^^^ passed on here
  reason: `fails` lists exactly the failures that can leave an action, so every caller knows what it must handle.
  next: Add `Refused` to `parent`'s `fails` list (agent)
  rule: failure.undeclared_propagation
  help: https://jadpo.dev/docs/errors/failure.undeclared_propagation

check failed: 1 error, 1 file
```

## How to fix it

**If callers should get the failure**, add it to `fails`, before the
result type's `->`:

```jadpo fragment
action register(note: Note) fails RegistrationClosed -> Note {
    ...
}
```

```jadpo check project=e-undeclared-fix-1 hidden
type Note = Text {}

type RegistrationClosed = Failure {
    kind: FailureKind.conflict
}

action register(note: Note) fails RegistrationClosed -> Note {
    reject RegistrationClosed
}
```

Every caller of `register` now passes `RegistrationClosed` on with
`attempt` or handles it in a `match`. A route that runs it answers with
the failure's code and the status its kind gives, 409 for `conflict`.

**If the failure comes from a call and the caller can carry on**, `match`
on the call and handle it there, so it never leaves:

```jadpo fragment
    return match child(note) {
        success(kept) -> kept
        Refused -> note
    }
```

```jadpo check project=e-undeclared-fix-2 hidden
type Note = Text {}

type Refused = Failure {
    kind: FailureKind.rejected
}

action child(note: Note) fails Refused -> Note {
    reject Refused
}

action parent(note: Note) -> Note {
    return match child(note) {
        success(kept) -> kept
        Refused -> note
    }
}
```

When `child` refuses, `parent` returns the note it was given, and its
`fails` list stays empty.

## Related

- [Saying what can fail: `fails`](../language/04-failures.md#saying-what-can-fail-fails)
- [Passing a failure on: `attempt`](../language/04-failures.md#passing-a-failure-on-attempt)
- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)
- [`failure.stale_declaration`](../errors/failure.stale_declaration.md)

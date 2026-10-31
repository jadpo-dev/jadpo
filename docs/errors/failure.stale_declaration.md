# `failure.stale_declaration`

A function or action lists a failure after `fails` that nothing in it can ever raise or pass on.

## Why Jadpo refuses it

`fails` lists exactly the failures that can leave a callable: everything
a `reject` or an `attempt` in its body can produce, less what a `match`
handles. Every caller has to handle each failure listed there, so a
failure that never happens makes callers write code that never runs. It
is usually left over from an edit that took its `reject` away.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/failure.stale_declaration

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If `complete` can't fail**, remove the failure from `fails`. With nothing
left to list, `fails` goes too:

```jadpo fragment
action complete(note: Note) -> Note {
```

```jadpo check project=e-stale-fix-1 hidden
type Note = Text {}

type Refused = Failure {
    kind: FailureKind.rejected
}

action complete(note: Note) -> Note {
    return note
}
```

Callers of `complete` no longer handle `Refused`. A caller that matched
on the call to handle it is then an error of its own, such as
[`failure.outcome_infallible`](../errors/failure.outcome_infallible.md)
when the call can no longer fail at all, which says what to remove.

**If `complete` was meant to refuse some notes**, write the `reject` the
list promises, under the condition that refuses:

```jadpo fragment
    if (note == "") {
        reject Refused
    }
```

```jadpo check project=e-stale-fix-2 hidden
type Note = Text {}

type Refused = Failure {
    kind: FailureKind.rejected
}

action complete(note: Note) fails Refused -> Note {
    if (note == "") {
        reject Refused
    }
    return note
}
```

An empty note now ends `complete` with `Refused`, and any other note is
returned as before.

## Related

- [Saying what can fail: `fails`](../language/04-failures.md#saying-what-can-fail-fails)
- [Raising a failure: `reject`](../language/04-failures.md#raising-a-failure-reject)
- [`failure.undeclared_propagation`](../errors/failure.undeclared_propagation.md)

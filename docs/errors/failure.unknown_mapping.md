# `failure.unknown_mapping`

An `attempt` block maps a condition that what it attempts doesn't have.

## Why Jadpo refuses it

An `attempt` block's arms are the conditions of what it attempts: the rules of
a construction's type, or a provider operation's failure codes. An arm for
anything else could never run, so its failure would never happen. A call of
your own has no conditions to map, since `attempt` passes its failures on and
`match` turns one into another. A provider's operation maps `timed_out:` only
when it is safe to send again.

## What it looks like

main.jadpo:

```jadpo check project=e-unknown-mapping expect=failure.unknown_mapping
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        maximum: QuantityTooLarge
    }
    return result
}
```

```text output
error: `Quantity(...)` has no `maximum:` condition to map
 --> main.jadpo:12:9
   |
12 |         maximum: QuantityTooLarge
   |         ^^^^^^^ not a condition here
  reason: A construction's arms are the rules of its type and the types it builds on; `Quantity(...)`'s are `min` and `max`, and `invalid:` covers any without an arm of its own.
  next: Remove `maximum: ...`, or map one of `min` and `max` instead (agent)
  rule: failure.unknown_mapping
  help: https://jadpo.dev/docs/errors/failure.unknown_mapping

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A call of your own has no conditions to map:

main.jadpo:

```jadpo check project=e-unknown-mapping-call expect=failure.unknown_mapping
type Note = Text {}

type NoteRefused = Failure {
    kind: FailureKind.rejected
}

action check_note(note: Note) fails NoteRefused -> Note {
    if (note == "") {
        reject NoteRefused
    }
    return note
}

type NoteInvalid = Failure {
    kind: FailureKind.invalid_value
}

action save_note(note: Note) fails NoteRefused, NoteInvalid -> Note {
    return attempt check_note(note) {
        rejected: NoteInvalid
    }
}
```

```text output
error: `check_note(...)` has no `rejected:` condition to map
 --> main.jadpo:20:9
   |
20 |         rejected: NoteInvalid
   |         ^^^^^^^^ not a condition here
  reason: `check_note(...)` fails with the failures its `fails` list names; `attempt` passes them on as they are, and `match` turns one into another.
  next: Remove `rejected: ...`, and `match` on the call to map its failures (agent)
  rule: failure.unknown_mapping
  help: https://jadpo.dev/docs/errors/failure.unknown_mapping

check failed: 1 error, 1 file
```

## How to fix it

**If you meant one of the type's rules**, use its name:

```jadpo fragment
        max: QuantityTooLarge
```

```jadpo check project=e-unknown-mapping-fix-1 hidden
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityTooLarge = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityTooLarge
    }
    return result
}
```

A doubled quantity over 1000 now ends with `QuantityTooLarge`.

**If the call is one of your own**, `match` on it:

```jadpo fragment
action save_note(note: Note) fails NoteInvalid -> Note {
    return match check_note(note) {
        success(kept) -> kept
        NoteRefused -> reject NoteInvalid
    }
}
```

```jadpo check project=e-unknown-mapping-fix-2 hidden
type Note = Text {}

type NoteRefused = Failure {
    kind: FailureKind.rejected
}

action check_note(note: Note) fails NoteRefused -> Note {
    if (note == "") {
        reject NoteRefused
    }
    return note
}

type NoteInvalid = Failure {
    kind: FailureKind.invalid_value
}

action save_note(note: Note) fails NoteInvalid -> Note {
    return match check_note(note) {
        success(kept) -> kept
        NoteRefused -> reject NoteInvalid
    }
}
```

A note `check_note` refuses now ends `save_note` with `NoteInvalid`.

## Related

- [A value built at run time: rule keys and `invalid:`](../language/04-failures.md#a-value-built-at-run-time-rule-keys-and-invalid)
- [A failure from a call: `match`](../language/04-failures.md#a-failure-from-a-call-match)
- [Construction failures by rule](../language/02-values-and-types.md#construction-failures-by-rule)

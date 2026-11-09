# `failure.mapping_kind`

A condition is mapped to a failure of the wrong kind.

## Why Jadpo refuses it

Each condition fixes the kind of failure it becomes, so a client's status
always means the same thing: a value that breaks a type's rules is
`invalid_value` (422), a read that finds nothing is `not_found` (404), and a
write that clashes is `conflict` (409). A provider's code takes the kind of
the package failure it stands for. Mapping one to another kind would answer
the client with a status that says something that didn't happen.

## What it looks like

main.jadpo:

```jadpo check project=e-mapping-kind expect=failure.mapping_kind
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityRefused = Failure {
    kind: FailureKind.rejected
}

function doubled(quantity: Quantity) fails QuantityRefused -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityRefused
    }
    return result
}
```

```text output
error: `max:` needs an `invalid_value` failure, and `QuantityRefused` is `rejected`
 --> main.jadpo:12:14
   |
12 |         max: QuantityRefused
   |              ^^^^^^^^^^^^^^^ kind `rejected`
  reason: Each mapping's condition fixes the kind of failure it becomes: a value that breaks a type's rules is `invalid_value`, so the client gets the status that means.
  next: Name an `invalid_value` failure here, or make `QuantityRefused`'s kind `FailureKind.invalid_value` (agent)
  rule: failure.mapping_kind
  help: https://jadpo.dev/docs/errors/failure.mapping_kind

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A read that finds nothing needs a `not_found` failure:

main.jadpo:

```jadpo check project=e-mapping-kind-missing expect=failure.mapping_kind
type NoteGone = Failure {
    kind: FailureKind.rejected
}

entity Note {
    id: Uuid

    policy: {
        read: [Access.public]
    }
}

type NoteView = Output {
    id: Note.id
}

query note_view(note_id: Note.id) fails NoteGone -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteGone
}
```

```text output
error: `missing:` needs a `not_found` failure, and `NoteGone` is `rejected`
 --> main.jadpo:20:14
   |
20 |     missing: NoteGone
   |              ^^^^^^^^ kind `rejected`
  reason: Each mapping's condition fixes the kind of failure it becomes: a read that finds nothing is `not_found`, so the client gets the status that means.
  next: Name a `not_found` failure here, or make `NoteGone`'s kind `FailureKind.not_found` (agent)
  rule: failure.mapping_kind
  help: https://jadpo.dev/docs/errors/failure.mapping_kind

check failed: 1 error, 1 file
```

A write that clashes on a unique rule needs a `conflict` failure:

main.jadpo:

```jadpo check project=e-mapping-kind-conflict expect=failure.mapping_kind
type Handle = Text {
    max_length: 40
}

type HandleTaken = Failure {
    kind: FailureKind.rejected
}

entity Account {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
    }
}

action claim(handle: Handle) fails HandleTaken -> Account {
    var account: Account = attempt Account.create {
        set: {
            handle: handle
        }
        conflict: {
            Account.handle: HandleTaken
        }
    }
    return account
}
```

```text output
error: `Account.handle:` needs a `conflict` failure, and `HandleTaken` is `rejected`
 --> main.jadpo:26:29
   |
26 |             Account.handle: HandleTaken
   |                             ^^^^^^^^^^^ kind `rejected`
  reason: Each mapping's condition fixes the kind of failure it becomes: a write the store refuses because of the records already there is `conflict`, so the client gets the status that means.
  next: Name a `conflict` failure here, or make `HandleTaken`'s kind `FailureKind.conflict` (agent)
  rule: failure.mapping_kind
  help: https://jadpo.dev/docs/errors/failure.mapping_kind

check failed: 1 error, 1 file
```

## How to fix it

**If the failure is the right one**, give it the kind the condition needs:

```jadpo fragment
    kind: FailureKind.invalid_value
```

```jadpo check project=e-mapping-kind-fix-1 hidden
type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityRefused = Failure {
    kind: FailureKind.invalid_value
}

function doubled(quantity: Quantity) fails QuantityRefused -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityRefused
    }
    return result
}
```

A client now gets a 422 for a quantity that is too large.

**If that failure means something else**, map the condition to one of the
right kind:

```jadpo fragment
function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {
    var result: Quantity = attempt Quantity(quantity * 2) {
        max: QuantityTooLarge
    }
    return result
}
```

```jadpo check project=e-mapping-kind-fix-2 hidden
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

`QuantityRefused` keeps its meaning for the cases it was written for.

## Related

- [Turning one failure into another](../language/04-failures.md#turning-one-failure-into-another)
- [Failure kinds](../language/04-failures.md#failure-kinds)

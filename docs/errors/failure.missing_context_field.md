# `failure.missing_context_field`

A failure is raised without a field it declares.

## Why Jadpo refuses it

A failure's `client:` and `internal:` fields are a promise: the client and the
log always get them. Raising one without a field would break that promise
where nobody can see it, and leave a client or an on-call engineer reading an
empty value.

## What it looks like

main.jadpo:

```jadpo check project=e-context-missing expect=failure.missing_context_field
type Lookup = Text {}

type Missing = Failure {
    kind: FailureKind.not_found
    internal: {
        lookup: Lookup
    }
}

action find(lookup: Lookup) fails Missing -> Lookup {
    reject Missing
}
```

```text output
error: `Missing` is missing `lookup`
 --> main.jadpo:11:12
   |
11 |     reject Missing
   |            ^^^^^^^ fields missing
  reason: A failure is raised with every field its `client` and `internal` blocks declare, so the client and the log always get them.
  next: Give its fields in a block: `Missing { lookup: ... }` (agent)
  rule: failure.missing_context_field
  help: https://jadpo.dev/docs/errors/failure.missing_context_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A query's `missing:` raises its failure too, so it gives the same fields:

main.jadpo:

```jadpo check project=e-context-missing-query expect=failure.missing_context_field
type Missing = Failure {
    kind: FailureKind.not_found
    internal: {
        note_id: Uuid
    }
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

query note_view(note_id: Note.id) fails Missing -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: Missing
}
```

```text output
error: `Missing` is missing `note_id`
 --> main.jadpo:23:14
   |
23 |     missing: Missing
   |              ^^^^^^^ fields missing
  reason: A failure is raised with every field its `client` and `internal` blocks declare, so the client and the log always get them.
  next: Give its fields in a block: `missing: Missing { note_id: ... }` (agent)
  rule: failure.missing_context_field
  help: https://jadpo.dev/docs/errors/failure.missing_context_field

check failed: 1 error, 1 file
```

Only `json.read`'s `mismatch` can fill a `DecodeProblem` field, so a failure
that declares one can't be raised any other way:

main.jadpo:

```jadpo check project=e-context-missing-problem expect=failure.missing_context_field
type Unreadable = Failure {
    kind: FailureKind.invalid_value
    client: {
        problem: DecodeProblem
    }
}

action give_up() fails Unreadable -> Unit {
    reject Unreadable
}
```

```text output
error: `Unreadable` is missing `problem`
 --> main.jadpo:9:12
  |
9 |     reject Unreadable
  |            ^^^^^^^^^^ fields missing
  reason: `Unreadable` declares `problem: DecodeProblem`, which only a failure mapped from `json.read`'s `mismatch` receives, and nothing here fills it.
  next: Map `json.read`'s `mismatch` to `Unreadable` on its own, or raise a failure that doesn't declare `problem` here (agent)
  rule: failure.missing_context_field
  help: https://jadpo.dev/docs/errors/failure.missing_context_field

check failed: 1 error, 1 file
```

## How to fix it

**If the failure should carry the field**, give it where it is raised:

```jadpo fragment
    reject Missing {
        lookup: lookup
    }
```

```jadpo check project=e-context-missing-fix-1 hidden
type Lookup = Text {}

type Missing = Failure {
    kind: FailureKind.not_found
    internal: {
        lookup: Lookup
    }
}

action find(lookup: Lookup) fails Missing -> Lookup {
    reject Missing {
        lookup: lookup
    }
}
```

The log now says which lookup found nothing.

**If nothing needs the field**, remove it from the failure's declaration, and
raise the failure on its own:

```jadpo fragment
    reject Missing
```

```jadpo check project=e-context-missing-fix-2 hidden
type Lookup = Text {}

type Missing = Failure {
    kind: FailureKind.not_found
}

action find(lookup: Lookup) fails Missing -> Lookup {
    reject Missing
}
```

No raise of `Missing` has to give it any more.

## Related

- [`client:` and `internal:` fields](../language/04-failures.md#client-and-internal-fields)
- [Raising a failure: `reject`](../language/04-failures.md#raising-a-failure-reject)
- [A read that finds nothing: `missing:`](../language/04-failures.md#a-read-that-finds-nothing-missing)
- [Decoding JSON](../language/04-failures.md#decoding-json)

# `failure.unknown_context_field`

A failure is raised with a field it doesn't declare.

## Why Jadpo refuses it

A failure carries only the fields its `client:` and `internal:` blocks declare,
which is how each is known to go to the client or stay in the log. A field
declared nowhere would be neither, so Jadpo can't say where it may go.

## What it looks like

main.jadpo:

```jadpo check project=e-context-unknown expect=failure.unknown_context_field
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
        search: lookup
    }
}
```

```text output
error: `Missing` has no field `search`
 --> main.jadpo:13:9
   |
13 |         search: lookup
   |         ^^^^^^ not declared
  reason: `Missing`'s fields are `lookup`. A failure carries only the fields its `client` and `internal` blocks declare.
  next: Remove `search`, or declare it in `Missing`'s `client` or `internal` block (agent)
  rule: failure.unknown_context_field
  help: https://jadpo.dev/docs/errors/failure.unknown_context_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A query's `missing:` is held to the same fields:

main.jadpo:

```jadpo check project=e-context-unknown-query expect=failure.unknown_context_field
type Missing = Failure {
    kind: FailureKind.not_found
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
    missing: Missing {
        searched: note_id
    }
}
```

```text output
error: `Missing` has no field `searched`
 --> main.jadpo:21:9
   |
21 |         searched: note_id
   |         ^^^^^^^^ not declared
  reason: `Missing` has no fields. A failure carries only the fields its `client` and `internal` blocks declare.
  next: Remove `searched` (agent)
  rule: failure.unknown_context_field
  help: https://jadpo.dev/docs/errors/failure.unknown_context_field

check failed: 1 error, 1 file
```

## How to fix it

**If it isn't needed**, remove it:

```jadpo fragment
    reject Missing {
        lookup: lookup
    }
```

```jadpo check project=e-context-unknown-fix-1 hidden
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

The failure now holds only what it declares.

**If it should be kept**, declare it. The block you put it in decides who
sees it:

```jadpo fragment
    internal: {
        lookup: Lookup
        search: Lookup
    }
```

```jadpo check project=e-context-unknown-fix-2 hidden
type Lookup = Text {}

type Missing = Failure {
    kind: FailureKind.not_found
    internal: {
        lookup: Lookup
        search: Lookup
    }
}

action find(lookup: Lookup) fails Missing -> Lookup {
    reject Missing {
        lookup: lookup
        search: lookup
    }
}
```

`search` is now written to the log and never sent to a client.

## Related

- [`client:` and `internal:` fields](../language/04-failures.md#client-and-internal-fields)
- [Raising a failure: `reject`](../language/04-failures.md#raising-a-failure-reject)
- [A read that finds nothing: `missing:`](../language/04-failures.md#a-read-that-finds-nothing-missing)

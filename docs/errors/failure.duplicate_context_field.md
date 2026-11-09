# `failure.duplicate_context_field`

A failure is raised with a field given twice, or with a field `json.read` fills in itself.

## Why Jadpo refuses it

Each field of a failure holds one value, which goes to the client or the log.
Given twice, there would be two candidates and no way to say which counts, so
the log and the client could tell different stories.

## What it looks like

main.jadpo:

```jadpo check project=e-context-duplicate expect=failure.duplicate_context_field
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
        lookup: lookup
    }
}
```

```text output
error: `lookup` is given twice
 --> main.jadpo:13:9
   |
12 |         lookup: lookup
   |         ------ first given here
13 |         lookup: lookup
   |         ^^^^^^ given again
  reason: Each field of `Missing` is given once, so the log and the client never have to guess which value counts.
  next: Remove one `lookup` (agent)
  rule: failure.duplicate_context_field
  help: https://jadpo.dev/docs/errors/failure.duplicate_context_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A failure mapped from `json.read`'s `mismatch` gets its `DecodeProblem` field
from the document, so giving that field as well is refused:

main.jadpo:

```jadpo check project=e-context-duplicate-filled expect=failure.duplicate_context_field
type Name = Text {
    max_length: 40
}

type Person = Input {
    name: Name
}

type Document = Text {}

type Malformed = Failure {
    kind: FailureKind.invalid_value
}

type Unreadable = Failure {
    kind: FailureKind.invalid_value
    client: {
        problem: DecodeProblem
    }
}

action read_person(body: Document, earlier: DecodeProblem) fails Malformed, Unreadable -> Person {
    return attempt json.read(body, as: Person) {
        malformed: Malformed
        mismatch: Unreadable {
            problem: earlier
        }
    }
}
```

```text output
error: `problem` is filled in by `mismatch`
 --> main.jadpo:26:13
   |
25 |         mismatch: Unreadable {
   |         -------- fills it
26 |             problem: earlier
   |             ^^^^^^^ given as well
  reason: A failure mapped from `json.read`'s `mismatch` receives the first problem in its `problem: DecodeProblem` field, so the log and the client always get the problem the document had, never a value written here.
  next: Remove `problem: ...` from the block (agent)
  rule: failure.duplicate_context_field
  help: https://jadpo.dev/docs/errors/failure.duplicate_context_field

check failed: 1 error, 1 file
```

## How to fix it

**If it is the same value twice**, remove one:

```jadpo fragment
    reject Missing {
        lookup: lookup
    }
```

```jadpo check project=e-context-duplicate-fix-1 hidden
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

The failure is raised with `lookup` once.

**If `json.read` fills the field**, leave it out of the block:

```jadpo fragment
        mismatch: Unreadable
```

```jadpo check project=e-context-duplicate-fix-2 hidden
type Name = Text {
    max_length: 40
}

type Person = Input {
    name: Name
}

type Document = Text {}

type Malformed = Failure {
    kind: FailureKind.invalid_value
}

type Unreadable = Failure {
    kind: FailureKind.invalid_value
    client: {
        problem: DecodeProblem
    }
}

action read_person(body: Document) fails Malformed, Unreadable -> Person {
    return attempt json.read(body, as: Person) {
        malformed: Malformed
        mismatch: Unreadable
    }
}
```

The failure now always holds the first problem the document had.

## Related

- [`client:` and `internal:` fields](../language/04-failures.md#client-and-internal-fields)
- [Raising a failure: `reject`](../language/04-failures.md#raising-a-failure-reject)
- [Decoding JSON](../language/04-failures.md#decoding-json)

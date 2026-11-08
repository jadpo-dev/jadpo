# `syntax.match_pattern`

A `match` arm doesn't start with a pattern, or names a case's data other than with one name in brackets.

## Why Jadpo refuses it

An arm starts with the case it handles, and a case's data has one name, in
brackets, whose fields are read from that name. Other languages unpack the
fields in the pattern itself, in braces or as a list of names. Jadpo doesn't,
so an arm reads the same however many fields its case has, and a field added
later doesn't change any arm that doesn't use it.

## What it looks like

main.jadpo:

```jadpo check project=e-match-pattern expect=syntax.match_pattern
type Label = Text {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_note: Label
    }
}

function label(outcome: PaymentOutcome) -> Label {
    return match outcome {
        PaymentOutcome.pending -> "pending"
        PaymentOutcome.paid { receipt_note } -> receipt_note
    }
}
```

```text output
error: A match arm names a case's data in brackets
 --> main.jadpo:13:29
   |
13 |         PaymentOutcome.paid { receipt_note } -> receipt_note
   |                             ^^^^^^^^^^^^^^^^ expected `(name)`
  reason: An arm binds a case's data to one name, `PaymentOutcome.paid(paid)`, and reads its fields from it, as in `paid.field`.
  next: Write `PaymentOutcome.paid(paid)` (agent)
  rule: syntax.match_pattern
  help: https://jadpo.dev/docs/errors/syntax.match_pattern

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An arm that gives a case's data more than one name is the same error,
reported at the first thing after the name:

main.jadpo:

```jadpo check project=e-match-pattern-names expect=syntax.match_pattern
type Label = Text {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_note: Label
    }
}

function label(outcome: PaymentOutcome) -> Label {
    return match outcome {
        PaymentOutcome.pending -> "pending"
        PaymentOutcome.paid(receipt_note, amount) -> "paid"
    }
}
```

```text output
error: Expected `)` after the name
 --> main.jadpo:13:41
   |
13 |         PaymentOutcome.paid(receipt_note, amount) -> "paid"
   |                                         ^ expected `)`
  reason: A case names its data with one name in brackets, as in `some(x)`; found `,`.
  next: Close the brackets after the name (agent)
  rule: syntax.match_pattern
  help: https://jadpo.dev/docs/errors/syntax.match_pattern

check failed: 1 error, 1 file
```

So is an arm that handles a failure and starts with the word `failure`, as a
failure's name is a type of its own:

main.jadpo:

```jadpo check project=e-match-pattern-failure expect=syntax.match_pattern
type LookupCode = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

function lookup(key: LookupCode) fails LookupMissing -> LookupResult {
    if (key == "missing") {
        reject LookupMissing
    }
    return "found"
}

function lookup_or_default(key: LookupCode) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        failure LookupMissing -> "not found"
    }
}
```

```text output
error: Failure arms have no `failure` prefix
 --> main.jadpo:19:9
   |
19 |         failure LookupMissing -> "not found"
   |         ^^^^^^^ not part of a pattern
  reason: Failure names are types of their own, so an arm names the failure alone.
  next: Write `LookupMissing -> ...` (agent)
  rule: syntax.match_pattern
  help: https://jadpo.dev/docs/errors/syntax.match_pattern

check failed: 1 error, 1 file
```

And so is an arm with nothing before its arrow:

main.jadpo:

```jadpo check project=e-match-pattern-empty expect=syntax.match_pattern
type TicketStatus = Enum {
    open
    closed
}

type Rank = Int {
    min: 1
}

function rank(status: TicketStatus) -> Rank {
    return match status {
        TicketStatus.open -> 1
        -> 2
    }
}
```

```text output
error: Expected a pattern
 --> main.jadpo:13:9
   |
13 |         -> 2
   |         ^^ expected a pattern
  reason: An arm starts with a case such as `Status.open`, `some(x)`, a literal or `_`; found `->` on the next line.
  next: Start the arm with the case it handles (agent)
  rule: syntax.match_pattern
  help: https://jadpo.dev/docs/errors/syntax.match_pattern

check failed: 1 error, 1 file
```

## How to fix it

**If the arm reads the case's data**, name it once in brackets and read its
fields from that name:

```jadpo fragment
function label(outcome: PaymentOutcome) -> Label {
    return match outcome {
        PaymentOutcome.pending -> "pending"
        PaymentOutcome.paid(paid) -> paid.receipt_note
    }
}
```

```jadpo check project=e-match-pattern-fix-1 hidden
type Label = Text {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_note: Label
    }
}

function label(outcome: PaymentOutcome) -> Label {
    return match outcome {
        PaymentOutcome.pending -> "pending"
        PaymentOutcome.paid(paid) -> paid.receipt_note
    }
}
```

`paid` now holds the case's data, and `paid.receipt_note` reads its field.

**If the arm doesn't need the data**, leave the brackets out:

```jadpo fragment
function label(outcome: PaymentOutcome) -> Label {
    return match outcome {
        PaymentOutcome.pending -> "pending"
        PaymentOutcome.paid -> "paid"
    }
}
```

```jadpo check project=e-match-pattern-fix-2 hidden
type Label = Text {}

type PaymentOutcome = Enum {
    pending
    paid: {
        receipt_note: Label
    }
}

function label(outcome: PaymentOutcome) -> Label {
    return match outcome {
        PaymentOutcome.pending -> "pending"
        PaymentOutcome.paid -> "paid"
    }
}
```

The arm now handles the case without naming its data.

**If the arm starts with `failure`**, remove the word and name the failure
alone:

```jadpo fragment
function lookup_or_default(key: LookupCode) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "not found"
    }
}
```

```jadpo check project=e-match-pattern-fix-3 hidden
type LookupCode = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

function lookup(key: LookupCode) fails LookupMissing -> LookupResult {
    if (key == "missing") {
        reject LookupMissing
    }
    return "found"
}

function lookup_or_default(key: LookupCode) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "not found"
    }
}
```

The arm now handles `LookupMissing`, and `lookup_or_default` gives
`"not found"` for it.

**If the arm has nothing before its arrow**, start it with the case it
handles:

```jadpo fragment
    return match status {
        TicketStatus.open -> 1
        TicketStatus.closed -> 2
    }
```

```jadpo check project=e-match-pattern-fix-4 hidden
type TicketStatus = Enum {
    open
    closed
}

type Rank = Int {
    min: 1
}

function rank(status: TicketStatus) -> Rank {
    return match status {
        TicketStatus.open -> 1
        TicketStatus.closed -> 2
    }
}
```

Every case now has its own arm, and the `match` covers them all.

## Related

- [Match](../language/03-logic.md#match)
- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)

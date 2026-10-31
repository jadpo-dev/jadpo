# `failure.attempt_required`

A call that can fail is made as if it can't, with nothing passing its failure on or handling it.

## Why Jadpo refuses it

Every place where a callable can stop early is marked where you read it.
A call that can fail starts its statement with `attempt`, which passes its
failures on to the caller, or sits in a `match`, which handles each one.
A bare call would hide the point where the caller can stop, and the
failure would have nowhere to go.

## What it looks like

main.jadpo:

```jadpo check project=e-attempt expect=failure.attempt_required
type Note = Text {}

type Refused = Failure {
    kind: FailureKind.rejected
}

action child(note: Note) fails Refused -> Note {
    reject Refused
}

action parent(note: Note) fails Refused -> Note {
    return child(note)
}
```

```text output
error: `child` can fail, and nothing handles it
 --> main.jadpo:12:12
   |
12 |     return child(note)
   |            ^^^^^^^^^^^ can fail with `Refused`
  reason: A call that can fail says so where it is made: `attempt` passes its failures on to the caller's `fails` list, and `match` handles each one.
  next: Start the statement with `attempt`, or `match` on `child(...)` and handle `Refused` (agent)
  rule: failure.attempt_required
  help: https://jadpo.dev/docs/errors/failure.attempt_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Building a named type from a value known only when the program runs can
fail too, when nothing proves the value follows the type's rules. Here
`n` can be any whole number, and `Quantity` allows only 1 to 1000:

main.jadpo:

```jadpo check project=e-attempt-construction expect=failure.attempt_required
type Count = Int {}

type Quantity = Int {
    min: 1
    max: 1000
}

function from_count(n: Count) -> Quantity {
    return Quantity(n)
}
```

```text output
error: `Quantity(...)` can fail at runtime, and nothing handles it
 --> main.jadpo:9:12
  |
9 |     return Quantity(n)
  |            ^^^^^^^^^^^ checked when it runs
  reason: The value is only known when the program runs, and neither the declared ranges nor a condition before it prove it follows `min: 1` and `max: 1000`, so building it can fail.
  next: Write `attempt Quantity(...) { min: SomeFailure }`, mapping each rule that can break to an `invalid_value` failure, or `invalid:` for all of them (agent)
  rule: failure.attempt_required
  help: https://jadpo.dev/docs/errors/failure.attempt_required

check failed: 1 error, 1 file
```

## How to fix it

**If the caller should fail the same way**, start the statement with
`attempt`. The caller's `fails` list must name the failure, as `parent`'s
already does:

```jadpo fragment
    return attempt child(note)
```

```jadpo check project=e-attempt-fix-1 hidden
type Note = Text {}

type Refused = Failure {
    kind: FailureKind.rejected
}

action child(note: Note) fails Refused -> Note {
    reject Refused
}

action parent(note: Note) fails Refused -> Note {
    return attempt child(note)
}
```

When `child` rejects `Refused`, `parent` stops there and ends with
`Refused` too.

**If the caller can carry on**, `match` on the call and give each failure
an arm. Here `parent` keeps the note it was given:

```jadpo fragment
action parent(note: Note) -> Note {
    return match child(note) {
        success(kept) -> kept
        Refused -> note
    }
}
```

```jadpo check project=e-attempt-fix-2 hidden
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

`parent` no longer fails, so `Refused` leaves its `fails` list.

**If a value you build can break its type**, write `attempt` and map each
rule that can break to an `invalid_value` failure, in a block after the
call:

```jadpo fragment
function from_count(n: Count) fails QuantityOutOfRange -> Quantity {
    return attempt Quantity(n) {
        invalid: QuantityOutOfRange
    }
}
```

```jadpo check project=e-attempt-construction-fix hidden
type Count = Int {}

type Quantity = Int {
    min: 1
    max: 1000
}

type QuantityOutOfRange = Failure {
    kind: FailureKind.invalid_value
}

function from_count(n: Count) fails QuantityOutOfRange -> Quantity {
    return attempt Quantity(n) {
        invalid: QuantityOutOfRange
    }
}
```

A count outside 1 to 1000 now ends `from_count` with `QuantityOutOfRange`.
`invalid:` covers every rule without an arm of its own, and `min:` and
`max:` arms give each rule a failure of its own instead. A check before
the construction that leaves when `n` is out of range also proves it,
and then it takes no `attempt`
([Conditions as proof](../language/02-values-and-types.md#conditions-as-proof)).

## Related

- [Attempt](../language/03-logic.md#attempt)
- [Passing a failure on: `attempt`](../language/04-failures.md#passing-a-failure-on-attempt)
- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)
- [Construction failures by rule](../language/02-values-and-types.md#construction-failures-by-rule)

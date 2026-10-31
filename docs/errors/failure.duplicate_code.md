# `failure.duplicate_code`

Two failures have the same code, or a failure has a code the runtime already answers with itself.

## Why Jadpo refuses it

A failure's code is what a client reads to tell failures apart. It comes
from the failure's name in lower_snake_case, `CustomerNotFound` is
`customer_not_found`, unless `code:` gives another. A client relies on a
code always meaning the same failure, with the same status, message and
fields. Two failures with one code would make it mean either, so every
code belongs to one failure.

Jadpo won't choose the fix for you. Clients already rely on the codes
you send, and only you know whether the two failures are one, and which
of them keeps the code.

## What it looks like

main.jadpo:

```jadpo check project=e-duplicate-code expect=failure.duplicate_code
type CustomerNotFound = Failure {
    kind: FailureKind.not_found
    message: "Customer not found."
}

type CustomerMissing = Failure {
    kind: FailureKind.not_found
    code: "customer_not_found"
    message: "Customer not found."
}
```

```text output
error: The code `customer_not_found` is already used by `CustomerNotFound`
 --> main.jadpo:8:11
  |
8 |     code: "customer_not_found"
  |           ^^^^^^^^^^^^^^^^^^^^ the same code
  reason: `CustomerNotFound` has the code `customer_not_found` too. A client relies on a code always meaning the same failure, with the same status, message and fields, so every code belongs to one failure.
  next: Give one of them a code of its own, or use one failure for both (human)
  rule: failure.duplicate_code
  help: https://jadpo.dev/docs/errors/failure.duplicate_code

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which failure each code your clients see stands for.

The runtime answers some requests with codes of its own, such as
`invalid_input` for request input that breaks a rule, so no failure may
take them: `invalid_input`, `invalid_request`, `route_not_found`,
`deadline_exceeded`, `transaction_unavailable`, `read_unavailable`,
`outcome_unknown` and `internal_fault`. A failure whose name gives one of
these codes is the same error:

main.jadpo:

```jadpo check project=e-duplicate-code-runtime expect=failure.duplicate_code
type InvalidInput = Failure {
    kind: FailureKind.invalid_value
}
```

```text output
error: The code `invalid_input` belongs to the runtime
 --> main.jadpo:1:6
  |
1 | type InvalidInput = Failure {
  |      ^^^^^^^^^^^^ a runtime code
  reason: The runtime answers with `invalid_input` itself, so a client seeing it could not tell which failure it got; every code belongs to one failure. `InvalidInput`'s code comes from its name.
  next: Rename the failure, or give it a code of its own with `code:` (human)
  rule: failure.duplicate_code
  help: https://jadpo.dev/docs/errors/failure.duplicate_code

check failed: 1 error, 1 file
```

## How to fix it

The question is whether the two failures mean the same thing to a client.

**If they are one failure**, keep one and use it everywhere the other was
rejected. `main.jadpo` becomes:

```jadpo check project=e-duplicate-code-fix-1
type CustomerNotFound = Failure {
    kind: FailureKind.not_found
    message: "Customer not found."
}
```

Every missing customer now reaches the client as `customer_not_found`,
whichever action rejects it.

**If they are different failures**, remove the `code:` line, so
`CustomerMissing` takes the code its name gives. `main.jadpo` becomes:

```jadpo check project=e-duplicate-code-fix-2
type CustomerNotFound = Failure {
    kind: FailureKind.not_found
    message: "Customer not found."
}

type CustomerMissing = Failure {
    kind: FailureKind.not_found
    message: "Customer not found."
}
```

Clients get `customer_not_found` for one and `customer_missing` for the
other. A client that read `customer_not_found` for both now sees a new
code for `CustomerMissing`, and changing a public code is a breaking
change ([What the client sees](../language/04-failures.md#what-the-client-sees)).

**If a failure's name gives a runtime code**, rename the failure:

```jadpo fragment
type EntryInvalid = Failure {
    ...
}
```

```jadpo check project=e-duplicate-code-runtime-fix hidden
type EntryInvalid = Failure {
    kind: FailureKind.invalid_value
}
```

Its code is now `entry_invalid`, which a client can tell apart from the
runtime's `invalid_input`. Use `code:` instead only to keep a code clients
already rely on after a rename
([The code clients see](../language/04-failures.md#the-code-clients-see)).

## Related

- [The code clients see](../language/04-failures.md#the-code-clients-see)
- [Declaring a failure](../language/04-failures.md#declaring-a-failure)
- [What the client sees](../language/04-failures.md#what-the-client-sees)

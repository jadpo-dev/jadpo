# `failure.kind_required`

A failure is declared without a `kind:` saying what sort of failure it is.

## Why Jadpo refuses it

A failure's kind, such as `not_found` or `conflict`, decides how it
reaches a client, its HTTP status included, and how background work such
as a job treats it. A failure never states a status itself, so without a
kind Jadpo has no way to answer a client with it.

## What it looks like

main.jadpo:

```jadpo check project=e-kind expect=failure.kind_required
type Missing = Failure {
    message: "Not found."
}
```

```text output
error: `Missing` has no `kind`
 --> main.jadpo:1:6
  |
1 | type Missing = Failure {
  |      ^^^^^^^ a failure without a kind
  reason: A failure's kind decides how it reaches a client, its HTTP status included, and how background work treats it, so every failure names one.
  next: Add `kind:` with the `FailureKind` that fits, such as `kind: FailureKind.not_found` (agent)
  rule: failure.kind_required
  help: https://jadpo.dev/docs/errors/failure.kind_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the failure is for something that doesn't exist**, add
`kind: FailureKind.not_found`:

```jadpo fragment
    kind: FailureKind.not_found
```

```jadpo check project=e-kind-fix hidden
type Missing = Failure {
    kind: FailureKind.not_found
    message: "Not found."
}
```

A client that gets `Missing` gets a 404, with the code `missing` and the
message "Not found.".

For another outcome, use the kind that fits it. You can declare seven:
`invalid_value` (422) for a value that breaks a rule, `not_permitted`
(403), `not_found` (404), `conflict` (409), `rejected` (422) for a
business rule that refuses the operation, `rate_limited` (429) and
`unavailable` (503). The runtime raises the others itself
([Kinds](../language/04-failures.md#kinds)).

## Related

- [Kinds](../language/04-failures.md#kinds)
- [`kind:`, which sets the status](../language/04-failures.md#kind-which-sets-the-status)
- [Declaring a failure](../language/04-failures.md#declaring-a-failure)

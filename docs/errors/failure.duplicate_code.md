# `failure.duplicate_code`

Two failures cannot share a code, whether it is written or comes from the
name. Nor can a failure take a code the runtime answers with itself:
`invalid_input`, `invalid_request`, `route_not_found`, `deadline_exceeded`,
`transaction_unavailable`, `read_unavailable`, `outcome_unknown` and
`internal_fault`.

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
  help: docs/errors/failure.duplicate_code.md

check failed: 1 error, 1 file
```

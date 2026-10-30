# `failure.kind_required`

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

# `failure.context_field_overlap`

```jadpo check project=e-context-overlap expect=failure.context_field_overlap
type Reference = Text {
    max_length: 64
}

type PaymentRefused = Failure {
    kind: FailureKind.rejected
    client: {
        reference: Reference
    }
    internal: {
        reference: Reference
    }
}
```

```text output
error: `reference` is in both `client` and `internal`
 --> main.jadpo:11:9
   |
 8 |         reference: Reference
   |         --------- first declared here
11 |         reference: Reference
   |         ^^^^^^^^^ declared again
  reason: `client` fields are sent to the caller and `internal` fields never leave the server, so a field is one or the other.
  next: Keep `reference` in one of the two blocks (agent)
  rule: failure.context_field_overlap
  help: https://jadpo.dev/docs/errors/failure.context_field_overlap

check failed: 1 error, 1 file
```

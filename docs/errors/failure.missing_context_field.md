# `failure.missing_context_field`

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

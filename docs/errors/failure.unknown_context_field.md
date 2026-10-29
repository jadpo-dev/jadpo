# `failure.unknown_context_field`

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
  help: docs/errors/failure.unknown_context_field.md

check failed: 1 error, 1 file
```

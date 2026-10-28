# `failure.duplicate_context_field`

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
  help: docs/language/04-failures.html#failure-duplicate-context-field

check failed: 1 error, 1 file
```

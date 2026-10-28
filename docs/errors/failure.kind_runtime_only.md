# `failure.kind_runtime_only`

```jadpo check project=e-kind-runtime expect=failure.kind_runtime_only
type Broken = Failure {
    kind: FailureKind.internal_fault
}
```

```text output
error: `Broken` can't have the kind `internal_fault`
 --> main.jadpo:2:11
  |
2 |     kind: FailureKind.internal_fault
  |           ^^^^^^^^^^^^^^^^^^^^^^^^^^ raised only by the runtime
  reason: The runtime raises `internal_fault` itself; an application declares `invalid_value`, `not_found`, `not_permitted`, `conflict`, `rejected`, `rate_limited` and `unavailable`, so a client's status always means the same thing.
  next: Use the kind an application declares that fits this outcome (agent)
  rule: failure.kind_runtime_only
  help: docs/language/04-failures.html#failure-kind-runtime-only

check failed: 1 error, 1 file
```

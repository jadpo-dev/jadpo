# `failure.kind_runtime_only`

A failure is declared with a kind only the runtime raises.

## Why Jadpo refuses it

A kind sets the status a client gets, and that status has to mean the same
thing everywhere. Some kinds, such as `internal_fault`, say the app or the
platform broke, so only the runtime raises them. If an app could declare one,
a client couldn't tell a bug from a refusal.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/failure.kind_runtime_only

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the failure is an outcome the app expects**, use the kind that fits:

```jadpo check project=e-kind-runtime-fix-1
type Broken = Failure {
    kind: FailureKind.rejected
}
```

A client now gets the status for a refused operation, 422.

**If it stands for something out of your hands**, such as a service that is
down, use `unavailable`:

```jadpo check project=e-kind-runtime-fix-2
type Broken = Failure {
    kind: FailureKind.unavailable
}
```

A client now gets a 503.

## Related

- [Failure kinds](../language/04-failures.md#failure-kinds)
- [Declaring a failure](../language/04-failures.md#declaring-a-failure)

# `failure.context_field_overlap`

A failure declares a field in both `client:` and `internal:`.

## Why Jadpo refuses it

`client:` fields are sent to the caller, and `internal:` fields never leave the
server. A field in both would be sent and kept private at once, so Jadpo
won't guess which you meant. The choice decides whether the caller can read
the value.

## What it looks like

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the caller needs it**, keep it in `client:` only:

```jadpo fragment
    client: {
        reference: Reference
    }
```

```jadpo check project=e-context-overlap-fix-1 hidden
type Reference = Text {
    max_length: 64
}

type PaymentRefused = Failure {
    kind: FailureKind.rejected
    client: {
        reference: Reference
    }
}
```

The caller gets `reference` in the failure's answer.

**If it is for your logs**, keep it in `internal:` only:

```jadpo fragment
    internal: {
        reference: Reference
    }
```

```jadpo check project=e-context-overlap-fix-2 hidden
type Reference = Text {
    max_length: 64
}

type PaymentRefused = Failure {
    kind: FailureKind.rejected
    internal: {
        reference: Reference
    }
}
```

`reference` is written to the log and never sent to the caller.

## Related

- [`client:` and `internal:` fields](../language/04-failures.md#client-and-internal-fields)
- [Declaring a failure](../language/04-failures.md#declaring-a-failure)

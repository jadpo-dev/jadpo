# `api.idempotency_invalid`

An API's `idempotency:` holds something other than `Idempotency.required` or `Idempotency.optional`.

## Why Jadpo refuses it

`idempotency:` decides what happens to a write sent without an
`Idempotency-Key` header. `Idempotency.required` refuses it, and
`Idempotency.optional`, the default, lets a client leave the key out. A
value like `true` could mean either, and the difference is whether a
client's retry can apply a write twice, so Jadpo doesn't guess.

## What it looks like

app.jadpo:

```jadpo check project=e-api-idempotency file=app.jadpo expect=api.idempotency_invalid
application {
    apis: {
        main: {
            prefix: /v1
            idempotency: true
        }
    }
}
```

```text output
error: `true` isn't an `Idempotency`
 --> app.jadpo:5:26
  |
5 |             idempotency: true
  |                          ^^^^ not an `Idempotency`
  reason: `idempotency:` takes `Idempotency.required`, so a write without an `Idempotency-Key` header is refused, or `Idempotency.optional`, the default, where a client may send one.
  next: Write `Idempotency.required`, or leave `idempotency:` out (agent)
  rule: api.idempotency_invalid
  help: https://jadpo.dev/docs/errors/api.idempotency_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If every write must carry a key**, write `Idempotency.required`:

```jadpo check project=e-api-idempotency-required file=app.jadpo
application {
    apis: {
        main: {
            prefix: /v1
            idempotency: Idempotency.required
        }
    }
}
```

A write to this API without an `Idempotency-Key` header is now refused.

**If a client may leave the key out**, delete the line, since
`Idempotency.optional` is the default:

```jadpo check project=e-api-idempotency-optional file=app.jadpo
application {
    apis: {
        main: {
            prefix: /v1
        }
    }
}
```

A client may send a key, and a repeat with the same key gets the first
try's answer.

## Related

- [Idempotent writes](../language/07-http.md#idempotent-writes)
- [APIs and versions](../language/07-http.md#apis-and-versions)

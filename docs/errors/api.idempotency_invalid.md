# `api.idempotency_invalid`

An API that refuses writes without an `Idempotency-Key` says so with an `Idempotency`:

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

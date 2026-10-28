# `rate_limits.store_invalid`

`rate_limits: { store: }` names the connection rate limits are counted
in: a Redis connection, or on Workers Cloudflare's rate-limiting binding.
A database can't count them:

```jadpo check project=e-rate-limits-store-invalid expect=rate_limits.store_invalid file=app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
    }
    rate_limits: {
        store: main
    }
}
```

```text output
error: `store: main` can't count rate limits
 --> app.jadpo:8:16
  |
8 |         store: main
  |                ^^^^ not Redis or a rate-limiting binding
  reason: Rate limits are counted in one store the servers or a Worker's instances share: a Redis connection (`Driver.redis`), or on Workers Cloudflare's rate-limiting binding (`Driver.cloudflare_rate_limits`). `rate_limits: { store: <connection> }` names it. `main` is a PostgreSQL database.
  next: Name a connection with `driver: Driver.redis` or `driver: Driver.cloudflare_rate_limits` (agent)
  rule: rate_limits.store_invalid
  help: docs/language/16-built-in-services.html#rate-limits-store-invalid

check failed: 1 error, 1 file
```

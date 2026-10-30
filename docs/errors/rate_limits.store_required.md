# `rate_limits.store_required`

A Worker's instances share no memory, so a deployable built for Workers
that serves an API counts its rate limits in a store. An edge with none:

```jadpo check project=e-rate-limits-store-required expect=rate_limits.store_required file=app.jadpo
application {
    deployables: {
        edge: {
            serves: [main]
            target: Target.wasm
        }
    }
}
```

```text output
error: `edge` has nowhere to count its rate limits on Workers
 --> app.jadpo:5:21
  |
5 |             target: Target.wasm
  |                     ^^^^^^^^^^^ built for Workers
  reason: Every route is rate-limited, and a Worker's instances share no memory, so on Workers the counts are kept in a store: Cloudflare's rate-limiting binding, or Redis. `app.jadpo` names none in `rate_limits: { store: }`.
  next: Declare a connection with `driver: Driver.cloudflare_rate_limits` and a `namespace:` in `connections:`, and add `rate_limits: { store: <its name> }` to `application { }` (agent)
  rule: rate_limits.store_required
  help: https://jadpo.dev/docs/errors/rate_limits.store_required

check failed: 1 error, 1 file
```

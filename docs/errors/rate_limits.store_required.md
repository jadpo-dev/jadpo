# `rate_limits.store_required`

A deployable built for Workers has nowhere to count its rate limits.

## Why Jadpo refuses it

Every route is rate-limited, and a Worker's instances share no memory, so
counts kept in memory would be per instance and the limits wouldn't hold.
A deployable on Workers that serves an API therefore has to name a store.
Jadpo won't pick one, because the store is a connection of your own with your
own Cloudflare namespace.

## What it looks like

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it runs on Workers**, declare Cloudflare's rate-limiting binding and name
it as the store:

```jadpo fragment
    connections: {
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
        }
    }
    rate_limits: {
        store: limits
    }
```

```jadpo check project=e-rate-limits-store-required-fix file=app.jadpo hidden
application {
    connections: {
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
        }
    }
    rate_limits: {
        store: limits
    }
    deployables: {
        edge: {
            serves: [main]
            target: Target.wasm
        }
    }
}
```

The deployable now counts its limits in the binding. Pick a `namespace:` no
other rate limit in your Cloudflare account uses.

## Related

- [Rate limits on Workers](../language/16-built-in-services.md#rate-limits-on-workers)
- [Cloudflare's binding](../language/16-built-in-services.md#cloudflares-binding-drivercloudflare_rate_limits)

# `rate_limits.store_invalid`

The `rate_limits:` block names a store that can't count rate limits, or holds something it doesn't take.

## Why Jadpo refuses it

Rate limits are counted in one store every server or Worker instance shares:
a Redis connection, or on Workers Cloudflare's rate-limiting binding. A database
or another connection can't do it, so a store that isn't one of those two
would look like shared counting and never count anything.

## What it looks like

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
  help: https://jadpo.dev/docs/errors/rate_limits.store_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`store:` must name a connection declared in `connections:`:

```jadpo check project=e-rate-limits-store-unknown file=app.jadpo expect=rate_limits.store_invalid
application {
    connections: {
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
        }
    }
    rate_limits: {
        store: limit
    }
}
```

```text output
error: `store: limit` isn't a connection
 --> app.jadpo:9:16
  |
9 |         store: limit
  |                ^^^^^ no such connection
  reason: Rate limits are counted in one store the servers or a Worker's instances share: a Redis connection (`Driver.redis`), or on Workers Cloudflare's rate-limiting binding (`Driver.cloudflare_rate_limits`). `rate_limits: { store: <connection> }` names it.
  next: Use `limits` instead of `limit` (agent)
  rule: rate_limits.store_invalid
  help: https://jadpo.dev/docs/errors/rate_limits.store_invalid

check failed: 1 error, 1 file
```

`rate_limits:` holds `store:` and nothing else, is declared once, and needs a
block with a `store:`; each of these is this error too.

## How to fix it

**If the store is Cloudflare's binding**, declare that connection and name it:

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

```jadpo check project=e-rate-limits-store-invalid-fix-1 file=app.jadpo hidden
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
}
```

Workers now count the limits in the binding.

**If the store is Redis**, declare a Redis connection and name it:

```jadpo fragment
    connections: {
        cache: {
            driver: Driver.redis
        }
    }
    rate_limits: {
        store: cache
    }
```

```jadpo check project=e-rate-limits-store-invalid-fix-2 file=app.jadpo hidden
application {
    connections: {
        cache: {
            driver: Driver.redis
        }
    }
    rate_limits: {
        store: cache
    }
}
```

Servers now name Redis as the place the counts are kept.

## Related

- [Where the counts are kept](../language/16-built-in-services.md#where-the-counts-are-kept-rate_limits--store-)
- [Rate limits on Workers](../language/16-built-in-services.md#rate-limits-on-workers)

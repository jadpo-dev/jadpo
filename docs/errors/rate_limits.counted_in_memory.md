# `rate_limits.counted_in_memory`

A deployable built for Workers would count its rate limits in each instance's memory.

## Why Jadpo refuses it

A Worker runs as many instances as its traffic needs, and none of them shares
memory. Counts kept in memory are per instance, so a client whose requests
reach several instances gets more than each limit.

Redis would hold one shared count, but its driver comes with caches, and until
then a Redis store counts in memory too. Whether that is acceptable is a
judgement call, so Jadpo reports it and lets you decline it with a reason.

## What it looks like

An edge on Workers whose rate limits are counted in Redis:

```jadpo check project=e-rate-limits-counted-in-memory expect=rate_limits.counted_in_memory file=app.jadpo
application {
    connections: {
        cache: {
            driver: Driver.redis
        }
    }
    rate_limits: {
        store: cache
    }
    deployables: {
        edge: {
            serves: [main]
            target: Target.wasm
        }
    }
}
```

```text output
error: `edge` would count its rate limits in each instance's memory on Workers
 --> app.jadpo:11:9
   |
 8 |         store: cache
   |                ----- Redis, whose driver isn't built yet
11 |         edge: {
   |         ^^^^ counts in memory
13 |             target: Target.wasm
   |                     ----------- built for Workers
  reason: `cache` is a Redis connection, and Redis's driver comes with caches; until then each of a Worker's instances counts the rate limits in its own memory, which its other instances don't share, so a client whose requests reach several of them gets more than each limit.
  next: Count them in Cloudflare's rate-limiting binding: declare a connection with `driver: Driver.cloudflare_rate_limits` and a `namespace:`, and name it in `rate_limits: { store: }`, or write `decline rate_limits.counted_in_memory: "<why>"` above it if this is deliberate (agent)
  rule: rate_limits.counted_in_memory
  help: https://jadpo.dev/docs/errors/rate_limits.counted_in_memory

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If limits should hold across instances**, count them in Cloudflare's
rate-limiting binding:

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

```jadpo check project=e-rate-limits-counted-in-memory-fix-1 file=app.jadpo hidden
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

Every instance now counts in the binding, which is approximate but shared.

**If counting in each instance is acceptable for now**, decline the rule above
the deployable, with your reason:

```jadpo fragment
        decline rate_limits.counted_in_memory: "One instance is enough at this traffic"
        edge: {
            ...
        }
```

```jadpo check project=e-rate-limits-counted-in-memory-fix-2 file=app.jadpo hidden
application {
    connections: {
        cache: {
            driver: Driver.redis
        }
    }
    rate_limits: {
        store: cache
    }
    deployables: {
        decline rate_limits.counted_in_memory: "One instance is enough at this traffic"
        edge: {
            serves: [main]
            target: Target.wasm
        }
    }
}
```

The deployable keeps Redis, counting in each instance's memory, and the reason
stays beside it for review.

## Related

- [Rate limits on Workers](../language/16-built-in-services.md#rate-limits-on-workers)
- [Where the counts are kept](../language/16-built-in-services.md#where-the-counts-are-kept-rate_limits--store-)
- [Keeping code as it is, with a reason](../language/01-project-layout-and-modules.md#keeping-code-as-it-is-with-a-reason)

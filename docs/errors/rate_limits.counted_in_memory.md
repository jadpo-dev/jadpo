# `rate_limits.counted_in_memory`

Until Redis's driver comes with caches, a deployable whose rate limits are
counted in Redis counts them in its own memory, and a Worker runs as many
instances as its traffic needs, none sharing it, so each counts on its
own. A judgement call: Cloudflare's rate-limiting binding
counts them across instances, or a `decline` line above the deployable
keeps Redis, counting in memory until its driver comes. An edge on
Workers counting in Redis:

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
  help: docs/errors/rate_limits.counted_in_memory.md

check failed: 1 error, 1 file
```

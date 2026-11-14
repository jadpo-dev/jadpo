# `connection.memory_cache`

A cache connection keeps its cache in each server's own memory.

## Why Jadpo refuses it

`driver: Driver.memory` keeps a cache in each server's own memory
([Cache drivers and what a cache may hold](../language/16-built-in-services.md#cache-drivers-and-what-a-cache-may-hold)).
Each server then has a cache of its own, emptied on every restart, and a
change one server clears from its cache is still served by the others
until their entries reach their `keep:`. On one server that is fine; on
several it serves stale pages. Whether the app runs on one server is a
judgement call, so Jadpo reports it and lets a person decline it with a
reason.

## What it looks like

```jadpo check project=e-connection-memory-cache expect=connection.memory_cache file=app.jadpo
application {
    connections: {
        cache: {
            driver: Driver.memory
        }
    }
}
```

```text output
error: `cache` keeps a cache in each server's own memory
 --> app.jadpo:3:9
  |
3 |         cache: {
  |         ^^^^^ each server's own cache
  reason: `cache` has `driver: Driver.memory`, so each server keeps a cache of its own, emptied on every restart, and a change one server clears from its cache is still served by the others until their entries reach their `keep:`. One server, or a Redis connection, keeps every copy current.
  next: Give `cache` `driver: Driver.redis` and a `url:`, which every server shares, or write `decline connection.memory_cache: "<why>"` above it if this is deliberate (human)
  rule: connection.memory_cache
  help: https://jadpo.dev/docs/errors/connection.memory_cache

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line means a person decides.

## How to fix it

**If the app runs on several servers**, keep the cache in Redis, which
every server shares:

```jadpo fragment
    connections: {
        cache: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
```

```jadpo check project=e-connection-memory-cache-fix-1 file=app.jadpo hidden
application {
    connections: {
        cache: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}
```

```jadpo check project=e-connection-memory-cache-fix-1 file=config.jadpo hidden
config {
    redis_url: RedisUrl
}
```

**If it runs on one server**, decline it with the reason:

```jadpo check project=e-connection-memory-cache-fix-2 file=app.jadpo
application {
    connections: {
        decline connection.memory_cache: "The shop runs on one server, so its one cache is every cache."
        cache: {
            driver: Driver.memory
        }
    }
}
```

A Worker keeps no memory between requests, so a deployable built for
Workers is refused a cache in memory whatever the decline says
([`deployable.target_unsupported`](deployable.target_unsupported.md)).

## Related

- [Cache drivers and what a cache may hold](../language/16-built-in-services.md#cache-drivers-and-what-a-cache-may-hold)
- [Caches](../language/05-data.md#caches)

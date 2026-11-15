# `data.cache_invalid`

An entity's `caches:` names no cache, keeps its records too long or too briefly, or caches records whose policy a cached copy can't be held to.

## Why Jadpo refuses it

An entity's `caches: { records: }` keeps a copy of each of its records,
by `id`, for the routes that read one record by `id`
([Caches](../language/05-data.md#caches)). It names the cache in
`store:`, a connection with `driver: Driver.redis` or
`driver: Driver.memory`, and how old a copy may get in `keep:`, from
`1s` to `1d`. A database, a store of files or a misspelt name keeps
nothing, and a copy kept longer than a day could outlive every change
meant to clear it.

One copy serves every caller, so policy is applied to it each time it is
read, by what the record holds. Some policy can't be applied that way:

- **A role in the entity's `read:` grants**, and **a tenant**: each reads
  the caller's memberships from the database as policy is checked.
- **A user field the cache leaves out**, a sensitive one, since a
  sensitive field is never put in a cache.

An entity kept in Redis is refused too, since its records are in Redis
already.

## What it looks like

```jadpo check project=e-data-cache-invalid file=app.jadpo hidden
application {
    connections: {
        cache: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}
```

```jadpo check project=e-data-cache-invalid file=config.jadpo hidden
config {
    redis_url: RedisUrl
}
```

```jadpo check project=e-data-cache-invalid file=domains/shop/types/product_name.jadpo hidden
type ProductName = Text {
    max_length: 100
}
```

```jadpo check project=e-data-cache-invalid file=domains/shop/entities/product.jadpo expect=data.cache_invalid
use shop/types { ProductName }

/// Something the shop sells.
entity Product {
    id: Uuid
    name: ProductName

    policy: {
        read: [Access.public]
    }
    caches: {
        records: {
            store: main
            keep: 1h
        }
    }
}
```

```text output
error: `Product` caches its records in `main`, which isn't a cache
 --> domains/shop/entities/product.jadpo:13:20
   |
13 |             store: main
   |                    ^^^^ not a cache
  reason: An entity's `caches: { records: }` keeps a copy of each of its records, by `id`, for the routes that read one: `store:` names a cache connection, one with `driver: Driver.redis` or `driver: Driver.memory`, and `keep:` the oldest a copy can be, from `1s` to `1d`. `main` isn't a cache connection.
  next: Name a cache connection, such as `store: cache` (agent)
  rule: data.cache_invalid
  help: https://jadpo.dev/docs/errors/data.cache_invalid

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `keep:` outside `1s` to `1d`, a setting other than `store:` and `keep:`,
either of them left out, a `caches:` holding anything but `records:`, an
entity in Redis, and an entity whose `read:` grants name a role or that
belongs to a tenant are this error too.

## How to fix it

**If the records should be cached**, name the cache, a connection with
`driver: Driver.redis`:

```jadpo fragment
    caches: {
        records: {
            store: cache
            keep: 1h
        }
    }
```

```jadpo check project=e-data-cache-invalid-fix-1 file=domains/shop/entities/product.jadpo hidden
use shop/types { ProductName }

/// Something the shop sells.
entity Product {
    id: Uuid
    name: ProductName

    policy: {
        read: [Access.public]
    }
    caches: {
        records: {
            store: cache
            keep: 1h
        }
    }
}
```

```jadpo check project=e-data-cache-invalid-fix-1 file=app.jadpo hidden
application {
    connections: {
        cache: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}
```

```jadpo check project=e-data-cache-invalid-fix-1 file=config.jadpo hidden
config {
    redis_url: RedisUrl
}
```

```jadpo check project=e-data-cache-invalid-fix-1 file=domains/shop/types/product_name.jadpo hidden migrate=initial
type ProductName = Text {
    max_length: 100
}
```

A route whose query reads one product by `id` now reads it from `cache`,
and a change to a product clears it.

An entity whose `read:` names a role, or that belongs to a tenant, leaves
`caches:` out. A route reads it through a query with a `cache:` of its own,
whose results are kept per caller
([`query.cache_invalid`](query.cache_invalid.md) shows one).

## Related

- [Caches](../language/05-data.md#caches)
- [Caches and personal data](../language/09-policy.md#caches-and-personal-data)
- [`query.cache_invalid`](query.cache_invalid.md)

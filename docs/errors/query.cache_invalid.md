# `query.cache_invalid`

A query's `cache:` names no cache, keeps its entries too long or too briefly, or caches what a cache may not hold.

## Why Jadpo refuses it

A query's `cache:` keeps its results for the routes that run it, so busy
pages read them quickly ([Caches](../language/05-data.md#caches)). It names
the cache in `store:`, a connection with `driver: Driver.redis` or
`driver: Driver.memory`, and how old an entry may get in `keep:`, from `1s`
to `1d`. A database, a store of files or a misspelt name keeps nothing, and
an entry kept longer than a day could outlive every change meant to clear
it.

Some records are never kept. A sensitive field, such as a health record,
is recorded in the access records each time it is read, which a cached
copy would skip. A `File` goes out as a link that lasts five minutes, which
a cache would keep long after it ends.

## What it looks like

```jadpo check project=e-query-cache-invalid file=app.jadpo hidden
application {
    connections: {
        cache: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}
```

```jadpo check project=e-query-cache-invalid file=config.jadpo hidden
config {
    redis_url: RedisUrl
}
```

```jadpo check project=e-query-cache-invalid file=domains/shop/types/product_name.jadpo hidden
type ProductName = Text {
    max_length: 100
}
```

```jadpo check project=e-query-cache-invalid file=domains/shop/entities/product.jadpo hidden migrate=initial
use shop/types { ProductName }

/// Something the shop sells.
entity Product {
    id: Uuid
    name: ProductName

    policy: {
        read: [Access.public]
    }
}
```

```jadpo check project=e-query-cache-invalid file=domains/shop/types/product_card.jadpo hidden
use shop/entities { Product }

/// A product as a list shows it.
type ProductCard = Output {
    pick: [Product.id, Product.name]
}
```

```jadpo check project=e-query-cache-invalid file=domains/shop/queries/products.jadpo expect=query.cache_invalid
use shop/entities { Product }
use shop/types { ProductCard }

/// Every product.
query products() -> List<ProductCard> {
    from: Product
    order: [Product.id asc]
    limit: 50
    cache: {
        store: main
        keep: 10m
    }
}
```

```text output
error: `products` keeps its cache in `main`, which isn't a cache
 --> domains/shop/queries/products.jadpo:10:16
   |
10 |         store: main
   |                ^^^^ not a cache
  reason: A query's `cache:` keeps its results for the routes that run it: `store:` names a cache connection, one with `driver: Driver.redis` or `driver: Driver.memory`, and `keep:` the oldest an entry can be, from `1s` to `1d`. `main` isn't a cache connection.
  next: Name a cache connection, such as `store: cache` (agent)
  rule: query.cache_invalid
  help: https://jadpo.dev/docs/errors/query.cache_invalid

check failed: 1 error, 6 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `keep:` outside `1s` to `1d`, a setting other than `store:` and `keep:`,
either of them left out, a `cache:` that holds no block, and a query whose
records give a sensitive field or a `File` are this error too.

## How to fix it

**If the query should be cached**, name the cache, a connection with `driver: Driver.redis`:

```jadpo fragment
    cache: {
        store: cache
        keep: 10m
    }
```

```jadpo check project=e-query-cache-invalid-fix-1 file=domains/shop/queries/products.jadpo hidden
use shop/entities { Product }
use shop/types { ProductCard }

/// Every product.
query products() -> List<ProductCard> {
    from: Product
    order: [Product.id asc]
    limit: 50
    cache: {
        store: cache
        keep: 10m
    }
}
```

```jadpo check project=e-query-cache-invalid-fix-1 file=app.jadpo hidden
application {
    connections: {
        cache: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}
```

```jadpo check project=e-query-cache-invalid-fix-1 file=config.jadpo hidden
config {
    redis_url: RedisUrl
}
```

```jadpo check project=e-query-cache-invalid-fix-1 file=domains/shop/types/product_name.jadpo hidden
type ProductName = Text {
    max_length: 100
}
```

```jadpo check project=e-query-cache-invalid-fix-1 file=domains/shop/entities/product.jadpo hidden
use shop/types { ProductName }

/// Something the shop sells.
entity Product {
    id: Uuid
    name: ProductName

    policy: {
        read: [Access.public]
    }
}
```

```jadpo check project=e-query-cache-invalid-fix-1 file=domains/shop/types/product_card.jadpo hidden migrate=initial
use shop/entities { Product }

/// A product as a list shows it.
type ProductCard = Output {
    pick: [Product.id, Product.name]
}
```

A route that runs `products` now reads it from `cache`, and a change to a
product clears it.

A query whose records give a sensitive field or a `File` stays uncached;
a route that needs a cache reads a query of its own that leaves the field
out.

## Related

- [Caches](../language/05-data.md#caches)
- [Cache drivers and what a cache may hold](../language/16-built-in-services.md#cache-drivers-and-what-a-cache-may-hold)

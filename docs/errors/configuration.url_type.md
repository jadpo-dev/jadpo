# `configuration.url_type`

A connection's `url:` isn't a setting of the type its driver needs.

## Why Jadpo refuses it

A database's address normally carries a password, so it comes from a secret
setting read from the deployment's environment, never from source. Each
driver has an address type of its own, which checks the scheme when the app
starts and is always secret. Any other type would let an address for the
wrong service, or one written in source, reach the connection.

## What it looks like

```jadpo check project=e-url-type expect=configuration.url_type file=config.jadpo
config {
    cache_url: RedisUrl
}
```

```jadpo check project=e-url-type expect=configuration.url_type file=app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.cache_url
        }
    }
}
```

```text output
error: `main`'s address must be a `PostgresUrl` setting
 --> app.jadpo:5:18
  |
5 |             url: config.cache_url
  |                  ^^^^^^^^^^^^^^^^ `cache_url` is `RedisUrl`
  reason: A connection opened with `driver: Driver.postgres` takes an address of its own type, which checks the scheme and is always secret; `config.cache_url` is `RedisUrl`.
  next: Declare `<name>: PostgresUrl` in `config.jadpo`, and write `url: config.<name>` (agent)
  rule: configuration.url_type
  help: https://jadpo.dev/docs/errors/configuration.url_type

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An address written in source is refused the same way:

```jadpo check project=e-url-type-source expect=configuration.url_type file=app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: "postgresql://localhost/app"
        }
    }
}
```

```text output
error: `main`'s address must be a `PostgresUrl` setting
 --> app.jadpo:5:18
  |
5 |             url: "postgresql://localhost/app"
  |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ an address written in source
  reason: A database address normally carries a password, so it is never written in source: it is a secret setting, `config.<name>`, read from the deployment's environment.
  next: Declare `<name>: PostgresUrl` in `config.jadpo`, and write `url: config.<name>` (agent)
  rule: configuration.url_type
  help: https://jadpo.dev/docs/errors/configuration.url_type

check failed: 1 error, 1 file
```

## How to fix it

**If the setting holds a different service's address**, declare one of the
type the driver needs and read that:

```jadpo check project=e-url-type-fix file=config.jadpo
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-url-type-fix file=app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}
```

The app now reads `DATABASE_URL`, won't start without a PostgreSQL address,
and treats the value as a secret.

## Related

- [Database and other connection URLs](../language/10-configuration-and-time.md#database-and-other-connection-urls)
- [Connection types](../language/10-configuration-and-time.md#connection-types)

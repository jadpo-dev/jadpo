# `query.redis_unsupported`

A query reads an entity kept in Redis in a way only a database can serve, such as a range, an `include` or an order.

## Why Jadpo refuses it

Redis keeps no indexes of its own. Jadpo finds a record there by its
`id`, by the key it keeps for each value of a field with `unique: true`,
or, for a field `indexes:` lists, by the set it keeps for each value,
which gives a page in creation order and its `count()`. A query that
asks anything else, a range, `in`, `or`, an `order:`, an `include:`,
`matches` or a `geo.` test, would have to read every record, and Redis
keeps no list of them.

## What it looks like

`app.jadpo` declares `drafts`, a Redis connection:

```jadpo check project=e-query-redis file=app.jadpo hidden
application {
    connections: {
        drafts: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}

config {
    redis_url: RedisUrl
}
```

```jadpo check project=e-query-redis file=domains/accounts/types/attempts.jadpo hidden
/// How many times someone has tried a code.
type Attempts = Int {
    min: 0
    max: 100
}
```

domains/accounts/entities/sign_up_draft.jadpo:

```jadpo check project=e-query-redis file=domains/accounts/entities/sign_up_draft.jadpo
use accounts/types { Attempts }

/// A sign-up someone has started and not yet finished.
entity SignUpDraft {
    id: Uuid
    email: Email
    attempts: Attempts

    store: drafts
    expires_after: 2h
    indexes: {
        by_attempts: [attempts]
    }
}
```

domains/accounts/queries/locked_out.jadpo:

```jadpo check project=e-query-redis file=domains/accounts/queries/locked_out.jadpo expect=query.redis_unsupported
use accounts/entities { SignUpDraft }

/// The sign-ups that have used up their tries.
query locked_out() -> List<SignUpDraft> {
    from: SignUpDraft
    where: SignUpDraft.attempts >= 5
    limit: 50
}
```

```text output
error: `locked_out` reads `SignUpDraft`, kept in Redis, with a range
 --> domains/accounts/queries/locked_out.jadpo:6:5
  |
6 |     where: SignUpDraft.attempts >= 5
  |     ^^^^^ only a database can
  reason: `drafts` is a Redis connection. Redis keeps no order of a field's values, only a set for each one, so it finds records holding a value and nothing between two. A query of an entity in Redis finds records by `id`, by a field with `unique: true`, or by `==` on one field `indexes:` lists, a page in creation order or its `count()`.
  next: Keep `SignUpDraft` in a PostgreSQL store, or find its records by `id`, a unique field or an indexed one (human)
  rule: query.redis_unsupported
  help: https://jadpo.dev/docs/errors/query.redis_unsupported

check failed: 1 error, 4 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides where the records are kept, and which of them the query finds.

## How to fix it

**If the records belong in a database**, keep the entity there, and the
query reads them as it is:

```jadpo fragment
entity SignUpDraft {
    id: Uuid
    email: Email
    attempts: Attempts
}
```

```jadpo check project=e-query-redis-fix-1 file=domains/accounts/entities/sign_up_draft.jadpo hidden
use accounts/types { Attempts }

/// A sign-up someone has started and not yet finished.
entity SignUpDraft {
    id: Uuid
    email: Email
    attempts: Attempts
}
```

```jadpo check project=e-query-redis-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=e-query-redis-fix-1 file=domains/accounts/types/attempts.jadpo hidden
/// How many times someone has tried a code.
type Attempts = Int {
    min: 0
    max: 100
}
```

```jadpo check project=e-query-redis-fix-1 file=domains/accounts/queries/locked_out.jadpo hidden migrate=initial
use accounts/entities { SignUpDraft }

/// The sign-ups that have used up their tries.
query locked_out() -> List<SignUpDraft> {
    from: SignUpDraft
    where: SignUpDraft.attempts >= 5
    order: [SignUpDraft.id asc]
    limit: 50
}
```

The range is now one `WHERE` in PostgreSQL.

**If they stay in Redis**, test one value with `==`, which the set Jadpo
keeps for each value of `attempts` serves:

```jadpo fragment
query locked_out() -> List<SignUpDraft> {
    from: SignUpDraft
    where: SignUpDraft.attempts == 5
    limit: 50
}
```

```jadpo check project=e-query-redis-fix-2 file=domains/accounts/queries/locked_out.jadpo hidden
use accounts/entities { SignUpDraft }

/// The sign-ups that have used up their tries.
query locked_out() -> List<SignUpDraft> {
    from: SignUpDraft
    where: SignUpDraft.attempts == 5
    limit: 50
}
```

```jadpo check project=e-query-redis-fix-2 file=app.jadpo hidden
application {
    connections: {
        drafts: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}
```

```jadpo check project=e-query-redis-fix-2 file=config.jadpo hidden
config {
    redis_url: RedisUrl
}
```

```jadpo check project=e-query-redis-fix-2 file=domains/accounts/types/attempts.jadpo hidden
/// How many times someone has tried a code.
type Attempts = Int {
    min: 0
    max: 100
}
```

```jadpo check project=e-query-redis-fix-2 file=domains/accounts/entities/sign_up_draft.jadpo hidden
use accounts/types { Attempts }

/// A sign-up someone has started and not yet finished.
entity SignUpDraft {
    id: Uuid
    email: Email
    attempts: Attempts

    store: drafts
    expires_after: 2h
    indexes: {
        by_attempts: [attempts]
    }
}
```

The query now reads the sign-ups that have tried exactly five codes, in
the order they were made.

## Related

- [Reading and writing entities in Redis](../language/06-reads-and-writes.md#reading-and-writing-entities-in-redis)
- [`data.redis_unsupported`](data.redis_unsupported.md)

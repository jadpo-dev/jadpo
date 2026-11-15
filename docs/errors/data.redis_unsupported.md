# `data.redis_unsupported`

An entity kept in Redis, or a write of one, uses what only a database keeps, such as a lifecycle, a reference or an index over several fields.

## Why Jadpo refuses it

Redis keeps no tables, foreign keys or indexes of its own. Jadpo keeps
each record whole, by its `id`, with a key for each unique field's value
and a set for each indexed field's, and expires it `expires_after:`
after its last write. Anything that needs more is refused where it is
written, naming the reason:

- **A `lifecycle:`**: expiry takes its place.
- **A unique rule or an index over several fields**, or an index with a
  `kind:`: Jadpo keeps a key or a set for one field's values.
- **A reference** other than a user field, the `id` of the principal's
  user or service, and any `on_delete:`: a reference is a foreign key.
  A reference to a person is refused too, since erasure finds a person's
  records by the references to them.
- **A role, a tenant or a field's own `policy:`**: policy on a record in
  Redis is applied to the record once it is read, by what it holds.
- **`retention:`, `audit:`, `search:`, `webhooks:` and `person: true`**,
  and a `File` or sealed field: each keeps what it needs in a database.
- **In a write**, a value worked out from what is stored other than
  whole numbers added and taken away, or a construction that refuses;
  and an action that writes one and also emits an event, keeps a record
  of several changing calls, or starts a batch over a query of one.

Jadpo won't choose between keeping the records in a database and doing
without what Redis can't keep: the first makes them last, and the second
changes what the app does.

## What it looks like

`app.jadpo` declares `drafts`, a Redis connection:

```jadpo check project=e-redis-unsupported file=app.jadpo
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

```jadpo check project=e-redis-unsupported file=domains/accounts/types/sign_up_step.jadpo hidden
/// How far someone has got through signing up.
type SignUpStep = Enum {
    started
    email_confirmed
}
```

domains/accounts/entities/sign_up_draft.jadpo:

```jadpo check project=e-redis-unsupported file=domains/accounts/entities/sign_up_draft.jadpo expect=data.redis_unsupported
use accounts/types { SignUpStep }

/// A sign-up someone has started and not yet finished.
entity SignUpDraft {
    id: Uuid
    email: Email
    step: SignUpStep

    store: drafts
    expires_after: 2h
    indexes: {
        by_email_and_step: [email, step]
    }
}
```

```text output
error: `SignUpDraft` is kept in Redis, which can't keep an index over several fields
 --> domains/accounts/entities/sign_up_draft.jadpo:11:5
   |
11 |     indexes: {
   |     ^^^^^^^ an index over several fields
  reason: `drafts` is a Redis connection. Jadpo keeps a set for each value of a field `indexes:` lists, which a query by equality on that one field reads; a set over several fields would need a query on all of them that Redis can't run. An entity in Redis keeps each record whole, by its `id`, with a key for each unique field's value and a set for each indexed field's, and nothing more.
  next: Keep `SignUpDraft` in a PostgreSQL store, or do without an index over several fields (human)
  rule: data.redis_unsupported
  help: https://jadpo.dev/docs/errors/data.redis_unsupported

check failed: 1 error, 3 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether the records last, and what the app can do with them.

## How to fix it

**If the records belong in a database**, keep the entity there, and the
index serves a query on both fields:

```jadpo fragment
entity SignUpDraft {
    ...
    indexes: {
        by_email_and_step: [email, step]
    }
}
```

```jadpo check project=e-redis-unsupported-fix-1 file=domains/accounts/entities/sign_up_draft.jadpo hidden
use accounts/types { SignUpStep }

/// A sign-up someone has started and not yet finished.
entity SignUpDraft {
    id: Uuid
    email: Email
    step: SignUpStep

    indexes: {
        by_email_and_step: [email, step]
    }
}
```

```jadpo check project=e-redis-unsupported-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=e-redis-unsupported-fix-1 file=domains/accounts/types/sign_up_step.jadpo hidden migrate=initial
/// How far someone has got through signing up.
type SignUpStep = Enum {
    started
    email_confirmed
}
```

Its records now last until something deletes them, in `main`.

**If they stay in Redis**, index each field a query tests on its own:

```jadpo fragment
entity SignUpDraft {
    ...
    indexes: {
        by_step: [step]
    }
}
```

```jadpo check project=e-redis-unsupported-fix-2 file=domains/accounts/entities/sign_up_draft.jadpo hidden
use accounts/types { SignUpStep }

/// A sign-up someone has started and not yet finished.
entity SignUpDraft {
    id: Uuid
    email: Email
    step: SignUpStep

    store: drafts
    expires_after: 2h
    indexes: {
        by_step: [step]
    }
}
```

```jadpo check project=e-redis-unsupported-fix-2 file=app.jadpo hidden
application {
    connections: {
        drafts: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}
```

```jadpo check project=e-redis-unsupported-fix-2 file=config.jadpo hidden
config {
    redis_url: RedisUrl
}
```

```jadpo check project=e-redis-unsupported-fix-2 file=domains/accounts/types/sign_up_step.jadpo hidden
/// How far someone has got through signing up.
type SignUpStep = Enum {
    started
    email_confirmed
}
```

A query now finds drafts by their step, and one draft by its `id`.

## Related

- [Entities in Redis](../language/05-data.md#entities-in-redis)
- [Reading and writing entities in Redis](../language/06-reads-and-writes.md#reading-and-writing-entities-in-redis)
- [`query.redis_unsupported`](query.redis_unsupported.md)

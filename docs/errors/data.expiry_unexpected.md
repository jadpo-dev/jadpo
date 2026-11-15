# `data.expiry_unexpected`

An entity kept in a database says with `expires_after:` when its records expire, which only an entity in Redis does.

## Why Jadpo refuses it

A database keeps a record until something deletes it. Only an entity
kept in Redis, for data that is short-lived and harmless to lose, says
how long its records live. A record in a database that should go after a
while is deleted by its lifecycle's `purge:` or by a `retention:`, which
say what goes and record it.

## What it looks like

```jadpo check project=e-expiry-unexpected file=app.jadpo hidden
application {}
```

domains/accounts/entities/sign_up_draft.jadpo:

```jadpo check project=e-expiry-unexpected file=domains/accounts/entities/sign_up_draft.jadpo expect=data.expiry_unexpected
/// A sign-up someone has started and not yet finished.
entity SignUpDraft {
    id: Uuid
    email: Email

    expires_after: 2h
}
```

```text output
error: `SignUpDraft` is kept in a database, where records don't expire
 --> domains/accounts/entities/sign_up_draft.jadpo:6:5
  |
6 |     expires_after: 2h
  |     ^^^^^^^^^^^^^ only an entity in Redis expires
  reason: `main` is a PostgreSQL store, which keeps a record until something deletes it. Only an entity kept in Redis, for data that is short-lived and harmless to lose, says how long its records live. A record that should go after a while is deleted by a lifecycle's `purge:` or a `retention:`.
  next: Remove `expires_after:` from `SignUpDraft`, or keep it in a Redis store (agent)
  rule: data.expiry_unexpected
  help: https://jadpo.dev/docs/errors/data.expiry_unexpected

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the records belong in the database**, remove `expires_after:`.
`domains/accounts/entities/sign_up_draft.jadpo` becomes:

```jadpo check project=e-expiry-unexpected-fix-1 file=domains/accounts/entities/sign_up_draft.jadpo
/// A sign-up someone has started and not yet finished.
entity SignUpDraft {
    id: Uuid
    email: Email
}
```

```jadpo check project=e-expiry-unexpected-fix-1 file=app.jadpo hidden migrate=initial
application {}
```

A record now stays until something deletes it; a `retention:` deletes
them after a while
([Retention](../language/15-personal-data.md#retention)).

**If they are short-lived and harmless to lose**, keep the entity in a
Redis store:

```jadpo fragment
entity SignUpDraft {
    ...
    store: drafts
    expires_after: 2h
}
```

```jadpo check project=e-expiry-unexpected-fix-2 file=domains/accounts/entities/sign_up_draft.jadpo hidden
/// A sign-up someone has started and not yet finished.
entity SignUpDraft {
    id: Uuid
    email: Email

    store: drafts
    expires_after: 2h
}
```

```jadpo check project=e-expiry-unexpected-fix-2 file=app.jadpo hidden
application {
    connections: {
        drafts: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}
```

```jadpo check project=e-expiry-unexpected-fix-2 file=config.jadpo hidden
config {
    redis_url: RedisUrl
}
```

A sign-up nobody touches for two hours is gone from Redis.

## Related

- [Entities in Redis](../language/05-data.md#entities-in-redis)
- [`data.expiry_required`](data.expiry_required.md)

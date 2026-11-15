# `data.expiry_required`

An entity kept in Redis doesn't say with `expires_after:` how long its records live.

## Why Jadpo refuses it

Redis keeps data that is short-lived and harmless to lose, so every
record of an entity kept there expires. `expires_after:` says how long a
record lives after its last write. Without it, a record would stay in
Redis until the server ran out of memory and dropped whatever it chose.

## What it looks like

`app.jadpo` declares `drafts`, a Redis connection:

```jadpo check project=e-expiry-required file=app.jadpo
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

domains/accounts/entities/sign_up_draft.jadpo:

```jadpo check project=e-expiry-required file=domains/accounts/entities/sign_up_draft.jadpo expect=data.expiry_required
/// A sign-up someone has started and not yet finished.
entity SignUpDraft {
    id: Uuid
    email: Email

    store: drafts
}
```

```text output
error: `SignUpDraft` is kept in Redis and doesn't say when its records expire
 --> domains/accounts/entities/sign_up_draft.jadpo:2:8
  |
2 | entity SignUpDraft {
  |        ^^^^^^^^^^^ no `expires_after:`
  reason: `drafts` is a Redis connection, which keeps data that is short-lived and harmless to lose, so every record kept there expires: `expires_after:` says how long one lives after its last write.
  next: Add `expires_after:` to `SignUpDraft`, such as `expires_after: 2h` (agent)
  rule: data.expiry_required
  help: https://jadpo.dev/docs/errors/data.expiry_required

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the records are short-lived and harmless to lose**, say how long one
lives after its last write:

```jadpo fragment
entity SignUpDraft {
    ...
    store: drafts
    expires_after: 2h
}
```

```jadpo check project=e-expiry-required-fix-1 file=domains/accounts/entities/sign_up_draft.jadpo hidden
/// A sign-up someone has started and not yet finished.
entity SignUpDraft {
    id: Uuid
    email: Email

    store: drafts
    expires_after: 2h
}
```

```jadpo check project=e-expiry-required-fix-1 file=app.jadpo hidden
application {
    connections: {
        drafts: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}
```

```jadpo check project=e-expiry-required-fix-1 file=config.jadpo hidden
config {
    redis_url: RedisUrl
}
```

A sign-up nobody touches for two hours is gone.

**If they should last until something deletes them**, keep the entity in
a PostgreSQL store. `domains/accounts/entities/sign_up_draft.jadpo`
becomes:

```jadpo check project=e-expiry-required-fix-2 file=domains/accounts/entities/sign_up_draft.jadpo
/// A sign-up someone has started and not yet finished.
entity SignUpDraft {
    id: Uuid
    email: Email
}
```

```jadpo check project=e-expiry-required-fix-2 file=app.jadpo hidden migrate=initial
application {}
```

Its records now live in `main`, and its first migration makes their table.

## Related

- [Entities in Redis](../language/05-data.md#entities-in-redis)
- [`data.expiry_unexpected`](data.expiry_unexpected.md)

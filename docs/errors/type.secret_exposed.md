# `type.secret_exposed`

A secret value, such as a password or an API key, is sent, stored, logged or read as text.

## Why Jadpo refuses it

A secret value, a `Password`, `ApiKey`, `BearerToken`, `Jwt`,
`TotpSecret`, `SecretText` or `OneTimeCode`, is something the app
receives and checks, but never lets go of. It arrives in a request's
`Input` and stays on the server. Everything written out of the program
is somewhere it could be read again: a response is logged by proxies and
kept by browsers, a failure's fields are logged, an event is kept in the
outbox, and a database is copied into backups. So an `Output`, a
failure, an event, an email, a message, a webhook's `reply:` and an
entity each refuse one, however deep in lists or objects it sits, and
so does any operation but the one made to take it, such as
`hash.sha256` or `crypto.hash_password`. What leaves the server is
something worked out from the secret, never the secret.

## What it looks like

main.jadpo:

```jadpo check project=e-secret-exposed expect=type.secret_exposed
type PartnerName = Text {
    max_length: 100
}

/// What the partners page shows of a partner.
type PartnerProfile = Output {
    name: PartnerName
    partner_key: ApiKey
}
```

```text output
error: `PartnerProfile.partner_key` holds an `ApiKey`
 --> main.jadpo:8:18
  |
8 |     partner_key: ApiKey
  |                  ^^^^^^ sent to a client
  reason: An `ApiKey` is a secret value: it is never sent, logged, stored or shown. A response is written out, logged by proxies and kept by browsers, and a client never needs a secret back.
  next: Remove `partner_key`, and send what the client needs instead, such as whether one is set (agent)
  rule: type.secret_exposed
  help: https://jadpo.dev/docs/errors/type.secret_exposed

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An entity that keeps a secret is the same error:

```jadpo check project=e-secret-exposed-stored expect=type.secret_exposed
type PartnerName = Text {
    max_length: 100
}

entity Partner {
    id: Uuid
    name: PartnerName
    partner_key: ApiKey

    policy: {
        create: [Access.authenticated]
        read: [Access.authenticated]
    }
}
```

```text output
error: `Partner.partner_key` keeps an `ApiKey`
 --> main.jadpo:8:18
  |
8 |     partner_key: ApiKey
  |                  ^^^^^^ a secret, stored
  reason: An `ApiKey` is never stored as itself, so a copy of the database holds none. A key or a token is checked against its hash, `hash.sha256` of it, which is what is kept.
  next: Store `Hash(hash.sha256(...))` of it in a `Hash` field in `partner_key`'s place, and look it up by that (agent)
  rule: type.secret_exposed
  help: https://jadpo.dev/docs/errors/type.secret_exposed

check failed: 1 error, 1 file
```

## How to fix it

**If the page needs to know whether a partner has a key**, send that
instead of the key. In `main.jadpo`:

```jadpo fragment
type PartnerProfile = Output {
    name: PartnerName
    has_partner_key: Bool
}
```

```jadpo check project=e-secret-exposed-fix-1 hidden
type PartnerName = Text {
    max_length: 100
}

/// What the partners page shows of a partner.
type PartnerProfile = Output {
    name: PartnerName
    has_partner_key: Bool
}
```

The partners page can now say whether a key is set, and no response
ever carries the key.

**If the app must recognise the key later**, keep its hash, and compare
the hash of the key a partner sends with it. In `main.jadpo`:

```jadpo fragment
entity Partner {
    ...
    partner_key_hash: Hash
    ...
}
```

```jadpo check project=e-secret-exposed-fix-2 hidden
type PartnerName = Text {
    max_length: 100
}

entity Partner {
    id: Uuid
    name: PartnerName
    partner_key_hash: Hash

    policy: {
        create: [Access.authenticated]
        read: [Access.authenticated]
    }
}

/// What is kept of a partner's key: its SHA-256, which signs no one in.
function key_hash(partner_key: ApiKey) -> Hash {
    return Hash(hash.sha256(partner_key))
}
```

The database now holds only `key_hash(...)` of each key, so a copy of it
lets no one call the app as a partner; a key a partner sends is
recognised when its hash matches the one kept.

## Related

- [Secret values](../language/19-standard-library.md#secret-values)
- [Where a secret can go](../language/19-standard-library.md#where-a-secret-can-go)
- [How a secret arrives](../language/19-standard-library.md#how-a-secret-arrives)

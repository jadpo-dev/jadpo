# `data.encrypted_field_invalid`

A field that can't be stored sealed, or that says it isn't sealed when its type always is.

## Why Jadpo refuses it

A sealed column holds bytes only the application can open, so anything the
database itself has to read can't be one: a record's `id`, a reference,
which is a foreign key, a generated field, and a `File`, whose column points
at bytes kept in a store. The reverse is refused too: a key type, a
`TotpSecret` and a `SecretText` are secrets, never stored as themselves, so a
field of one can't turn `encrypted:` off.

## What it looks like

A referral whose reference to a clinician says it is sealed:

```jadpo check project=e-encrypted-field-invalid expect=data.encrypted_field_invalid
config {
    field_encryption_key: EncryptionKey
}

application {
    field_encryption_key: config.field_encryption_key
}

/// A clinician.
entity Clinician {
    id: Uuid
}

/// A referral to a clinician.
entity Referral {
    id: Uuid
    clinician_id: Clinician.id {
        on_delete: OnDelete.restricted
        encrypted: true
    }
}
```

```text output
error: `Referral.clinician_id` can't be kept sealed
 --> main.jadpo:17:19
   |
17 |     clinician_id: Clinician.id {
   |                   ^^^^^^^^^^^^^^ a reference, read by the database
  reason: A reference is a foreign key, which the database itself compares.
  next: Remove `encrypted:` from `clinician_id` (agent)
  rule: data.encrypted_field_invalid
  help: https://jadpo.dev/docs/errors/data.encrypted_field_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make the
fix itself: what the database has to read can't be sealed.

A secret kept sealed that says it isn't:

```jadpo check project=e-encrypted-field-off expect=data.encrypted_field_invalid
config {
    field_encryption_key: EncryptionKey
}

application {
    field_encryption_key: config.field_encryption_key
}

/// A person's authenticator.
entity Authenticator {
    id: Uuid
    code_secret: TotpSecret {
        encrypted: false
    }
}
```

```text output
error: `Authenticator.code_secret` can't turn `encrypted:` off
 --> main.jadpo:13:9
   |
13 |         encrypted: false
   |         ^^^^^^^^^ always sealed
  reason: A `TotpSecret` is a secret, so it is never stored as itself: a key type, a `TotpSecret` and a `SecretText` are always kept sealed with the application's `field_encryption_key:`.
  next: Remove `encrypted: false` from `code_secret` (agent)
  rule: data.encrypted_field_invalid
  help: https://jadpo.dev/docs/errors/data.encrypted_field_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If the field has to be a reference, an `id` or a generated time**, remove
`encrypted:`:

```jadpo fragment
    clinician_id: Clinician.id {
        on_delete: OnDelete.restricted
    }
```

```jadpo check project=e-encrypted-field-invalid-fix hidden
/// A clinician.
entity Clinician {
    id: Uuid
}

/// A referral to a clinician.
entity Referral {
    id: Uuid
    clinician_id: Clinician.id {
        on_delete: OnDelete.restricted
    }
}
```

It stays readable to anyone with a copy of the database, so keep what must stay
private in a field of its own.

**If it is a secret**, remove `encrypted: false`:

```jadpo fragment
    code_secret: TotpSecret
```

```jadpo check project=e-encrypted-field-off-fix hidden
config {
    field_encryption_key: EncryptionKey
}

application {
    field_encryption_key: config.field_encryption_key
}

/// A person's authenticator.
entity Authenticator {
    id: Uuid
    code_secret: TotpSecret
}
```

It stays sealed, as it would have whichever it said.

## Related

- [Encrypted fields](../language/15-personal-data.md#encrypted-fields)
- [How a secret is kept](../language/19-standard-library.md#how-a-secret-is-kept)

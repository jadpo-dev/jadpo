# `data.field_encryption_key_invalid`

The application's `field_encryption_key:` reads something other than an `EncryptionKey` setting.

## Why Jadpo refuses it

The key encrypted fields are sealed with is 32 random bytes that never appear
in your source. A literal would put it in git, and a `SigningKey` is text of
any length, which isn't that.

## What it looks like

An application naming a signing key:

```jadpo check project=e-field-key-invalid expect=data.field_encryption_key_invalid
config {
    session_key: SigningKey
}

application {
    field_encryption_key: config.session_key
}
```

```text output
error: `field_encryption_key:` reads `session_key`, which isn't an `EncryptionKey`
 --> main.jadpo:6:27
  |
6 |     field_encryption_key: config.session_key
  |                           ^^^^^^^^^^^^^^^^^^ `session_key` is `SigningKey`
  reason: Encrypted fields are sealed with an `EncryptionKey` setting: 32 random bytes, written as 43 Base64url characters, and replaced by moving the old value to the variable with `_PREVIOUS` after its name (area 10). A `SigningKey` is text of any length, so it isn't one.
  next: Declare `session_key: EncryptionKey` in `config.jadpo` (agent)
  rule: data.field_encryption_key_invalid
  help: https://jadpo.dev/docs/errors/data.field_encryption_key_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make the
fix itself: only an `EncryptionKey` setting will do.

## How to fix it

**If this setting is the key**, declare it as an `EncryptionKey`:

```jadpo fragment
    session_key: EncryptionKey
```

```jadpo check project=e-field-key-invalid-fix-1 hidden
config {
    session_key: EncryptionKey
}

application {
    field_encryption_key: config.session_key
}
```

Its value now has to be 32 random bytes: `jadpo config set session_key
--generate` makes one for your machine.

**If that setting does something else**, keep it, and give the fields a key of
their own:

```jadpo fragment
    field_encryption_key: config.field_encryption_key
```

```jadpo check project=e-field-key-invalid-fix-2 hidden
config {
    field_encryption_key: EncryptionKey
    session_key: SigningKey
}

application {
    field_encryption_key: config.field_encryption_key
}
```

Each environment now has one more secret to be given.

## Related

- [Encrypted fields](../language/15-personal-data.md#encrypted-fields)
- [Hashing and cryptography](../language/19-standard-library.md#hashing-and-cryptography)

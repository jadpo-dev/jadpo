# `type.key_as_value`

A key held as data is read as a value, when it goes only into a key-type field or a `crypto` call's `key:`.

## Why Jadpo refuses it

A key made by `crypto.new_key`, or read from a key-type field, is opaque.
Returned, logged, compared or turned into text, it would leave the one place
that keeps it safe, and a key that has been read has to be treated as lost.

## What it looks like

A function that hands a device's key to its caller:

```jadpo check project=e-key-as-value expect=type.key_as_value
config {
    field_encryption_key: EncryptionKey
}

application {
    field_encryption_key: config.field_encryption_key
}

/// A tablet, with a key of its own.
entity Tablet {
    id: Uuid
    device_key: EncryptionKey
}

/// The device's key.
function key_of(tablet: Tablet) -> EncryptionKey {
    return tablet.device_key
}
```

```text output
error: `tablet.device_key` is a key, read as a value
 --> main.jadpo:17:12
   |
17 |     return tablet.device_key
   |            ^^^^^^^^^^^^^^^^^ a key, read here
  reason: A key held as data is opaque: code never reads, prints, compares, returns or passes on a key, so it goes only into a key-type field, which keeps it sealed, or a `crypto` call's `key:`.
  next: Give `tablet.device_key` only as a `crypto` call's `key:`, or set a key-type field to it (agent)
  rule: type.key_as_value
  help: https://jadpo.dev/docs/errors/type.key_as_value

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make the
fix itself: the key goes to the operation, so the operation moves to where the
key is.

## How to fix it

**If something needs the key**, do it where the record is, giving the key as
the call's `key:`:

```jadpo fragment
    return Sealed(crypto.encrypt(note, key: tablet.device_key, context: "note"))
```

```jadpo check project=e-key-as-value-fix hidden
config {
    field_encryption_key: EncryptionKey
}

application {
    field_encryption_key: config.field_encryption_key
}

type Sealed = Bytes {}

type Note = Text {
    max_length: 200
}

/// A tablet, with a key of its own.
entity Tablet {
    id: Uuid
    device_key: EncryptionKey
}

/// A note sealed with the device's key.
action seal_for(tablet: Tablet, note: Note) -> Sealed {
    return Sealed(crypto.encrypt(note, key: tablet.device_key, context: "note"))
}
```

The key never leaves the runtime, and what it sealed may go anywhere.

**If you only need to know whether a device has a key**, make the field
optional and compare it with `none`:

```jadpo fragment
    return Keyed(tablet.device_key != none)
```

```jadpo check project=e-key-as-value-none hidden
config {
    field_encryption_key: EncryptionKey
}

application {
    field_encryption_key: config.field_encryption_key
}

type Keyed = Bool {}

/// A tablet, which may have a key of its own.
entity Tablet {
    id: Uuid
    device_key: EncryptionKey?
}

/// Whether the device has a key.
function keyed(tablet: Tablet) -> Keyed {
    return Keyed(tablet.device_key != none)
}
```

That says nothing of the key itself, and a `match` on it binds the key under
another name, for a `crypto` call's `key:`.

## Related

- [A key is never a value](../language/19-standard-library.md#a-key-is-never-a-value)
- [`configuration.secret_flow`](configuration.secret_flow.md)

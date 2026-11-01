# `data.field_encryption_key_required`

A field is kept sealed, but the application names no key to seal it with.

## Why Jadpo refuses it

An encrypted field, a key-type field, a `TotpSecret` and a `SecretText` are all
sealed with the application's one key, which `application {
field_encryption_key: }` names. Without one there is nothing to seal them
with. Jadpo won't make up a secret for you: each environment has to be given
this key, and losing it loses every value sealed with it for good. The error is
reported once, at the first field that needs the key.

## What it looks like

An app that names no key:

```jadpo check project=e-field-key-required expect=data.field_encryption_key_required
type ClinicalNote = Text {
    max_length: 2000
}

/// A consultation, whose notes only the application can read.
entity Consultation {
    id: Uuid
    notes: ClinicalNote {
        encrypted: true
    }
}
```

```text output
error: `Consultation.notes` is kept sealed, but `application { }` names no key to seal it with
 --> main.jadpo:8:5
  |
8 |     notes: ClinicalNote {
  |     ^^^^^ kept sealed
  reason: An encrypted field, a key-type field, a `TotpSecret` and a `SecretText` are kept sealed with the application's one key, which `application { field_encryption_key: }` names, and `Consultation.notes` is the first that needs it. Jadpo won't make up a secret for you: losing this key loses every value sealed with it for good, so it is a new value each environment has to be given and keep safe.
  next: Add `field_encryption_key: config.field_encryption_key` to `application { }` in `app.jadpo`, declare `field_encryption_key: EncryptionKey` in `config.jadpo`, and run `jadpo config set field_encryption_key --generate` (human)
  rule: data.field_encryption_key_required
  help: https://jadpo.dev/docs/errors/data.field_encryption_key_required

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop and
ask you which fix you want, because the choice is yours to make: it decides a
new secret each environment must be given and kept safe.

## How to fix it

**If this field should be sealed**, name a key in `app.jadpo` and declare it in
`config.jadpo`:

```jadpo fragment
    field_encryption_key: config.field_encryption_key
```

```jadpo check project=e-field-key-required-fix hidden
config {
    field_encryption_key: EncryptionKey
}

application {
    field_encryption_key: config.field_encryption_key
}

type ClinicalNote = Text {
    max_length: 2000
}

/// A consultation, whose notes only the application can read.
entity Consultation {
    id: Uuid
    notes: ClinicalNote {
        encrypted: true
    }
}
```

Then run `jadpo config set field_encryption_key --generate`, keep an offline
copy of what it writes, apart from your backups, and give each environment a
key of its own.

**If it needn't be**, remove `encrypted: true`:

```jadpo fragment
    notes: ClinicalNote
```

```jadpo check project=e-field-key-required-plain hidden
type ClinicalNote = Text {
    max_length: 2000
}

/// A consultation.
entity Consultation {
    id: Uuid
    notes: ClinicalNote
}
```

Anyone with a copy of the database can then read it.

## Related

- [Encrypted fields](../language/15-personal-data.md#encrypted-fields)
- [What can't be undone](../language/15-personal-data.md#what-cant-be-undone)

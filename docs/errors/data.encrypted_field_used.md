# `data.encrypted_field_used`

An encrypted field is used somewhere the database or a log would have to read it.

## Why Jadpo refuses it

A sealed column holds bytes only the application can open, and one value
sealed twice gives different bytes. So the database can't compare, order,
index or search what is in it, and no unique rule could notice two equal
values. Accepted, each of these would quietly match nothing. A failure's
fields are written to the log, which is exactly what sealing the field was
meant to keep it out of.

## What it looks like

A query that filters on a consultation's sealed notes:

```jadpo check project=e-encrypted-field-used expect=data.encrypted_field_used
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

/// The consultations with these notes.
query with_notes(notes: ClinicalNote) -> List<Consultation> {
    from: Consultation
    where: Consultation.notes == notes
    order: [Consultation.id asc]
    limit: 10
}
```

```text output
error: A query's condition can't read `Consultation.notes`, which is encrypted
 --> main.jadpo:24:12
   |
24 |     where: Consultation.notes == notes
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^ kept sealed
  reason: `Consultation.notes` is kept sealed: its column holds bytes only the application can open, which differ every time one value is sealed. So it is kept out of unique rules, indexes, filters, orderings, searches and logs, and nothing but an action's write can set it.
  next: Find the records by another field, and compare this one in the action (agent)
  rule: data.encrypted_field_used
  help: https://jadpo.dev/docs/errors/data.encrypted_field_used

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make the
fix itself: only the application can read the field, so the work moves into
the application.

## How to fix it

**If you need to find records by the value**, keep what you search on in a
field of its own that isn't encrypted, such as a hash of it, and filter on
that:

```jadpo fragment
    notes_digest: NotesDigest
```

```jadpo check project=e-encrypted-field-used-fix hidden
config {
    field_encryption_key: EncryptionKey
}

application {
    field_encryption_key: config.field_encryption_key
}

type ClinicalNote = Text {
    max_length: 2000
}

type NotesDigest = Bytes {}

/// A consultation, whose notes only the application can read.
entity Consultation {
    id: Uuid
    notes: ClinicalNote {
        encrypted: true
    }
    notes_digest: NotesDigest
}

/// The consultations with these notes.
query with_notes(notes_digest: NotesDigest) -> List<Consultation> {
    from: Consultation
    where: Consultation.notes_digest == notes_digest
    order: [Consultation.id asc]
    limit: 10
}
```

Anyone with a copy of the database can read that field, and its hash can be
tested against a guess, so keep it only where that is acceptable.

**If you only need to compare or order what you have found**, find the records
by another field and do the rest in the action, where the value is open:

```jadpo fragment
    return Same(consultation.notes == notes)
```

```jadpo check project=e-encrypted-field-used-action hidden
config {
    field_encryption_key: EncryptionKey
}

application {
    field_encryption_key: config.field_encryption_key
}

type ClinicalNote = Text {
    max_length: 2000
}

type Same = Bool {}

/// A consultation, whose notes only the application can read.
entity Consultation {
    id: Uuid
    notes: ClinicalNote {
        encrypted: true
    }
}

/// Whether a consultation's notes say this.
function says(consultation: Consultation, notes: ClinicalNote) -> Same {
    return Same(consultation.notes == notes)
}
```

The database reads each record as before, and only your code sees the notes.

## Related

- [Encrypted fields](../language/15-personal-data.md#encrypted-fields)
- [Logging](../language/04-failures.md#logging)

# `cli.config_rotation_unavailable`

The sealed rows `jadpo config rotation` counts couldn't be counted, because their database can't be reached or has no table for them yet.

## Why Jadpo refuses it

`jadpo config rotation` says when the previous field key can go, by counting
each sealed field's rows in its database. With no count there is nothing it can
safely say, and removing the key too soon loses rows for good, so it stops
rather than guess.

Jadpo won't apply the migrations for you here or try another address. The
database may be production's, and whether it is the right one only you know.

## What it looks like

A project keeping one note sealed, against a new database to which its
migrations haven't been applied:

```jadpo check project=e-config-rotation file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
    field_encryption_key: config.field_encryption_key
}
```

```jadpo check project=e-config-rotation file=config.jadpo hidden
config {
    database_url: PostgresUrl
    field_encryption_key: EncryptionKey
}
```

```jadpo check project=e-config-rotation file=domains/clinic/types/clinical_note.jadpo hidden
/// What a clinician wrote.
type ClinicalNote = Text {
    max_length: 2000
}
```

```jadpo check project=e-config-rotation file=domains/clinic/entities/consultation.jadpo migrate=initial
use clinic/types { ClinicalNote }

/// A consultation, whose notes only the application can read.
entity Consultation {
    id: Uuid
    notes: ClinicalNote {
        encrypted: true
    }
}
```

```sh run project=e-config-rotation database expect=cli.config_rotation_unavailable
jadpo config set field_encryption_key --generate
jadpo config rotation
```

```text output
```

`(human)` at the end of the `next:` line tells your coding agent to stop and
ask you which fix you want, because the choice is yours to make: only you know
which database this is.

## How to fix it

**If the database hasn't applied the migrations**, apply them:

```sh
jadpo migrate apply
```

The command then counts its rows.

**If it can't be reached**, check it is running and its address is right, or
name it yourself:

```sh
jadpo config rotation --url postgresql://db.example:5432/clinic
```

Keep `FIELD_ENCRYPTION_KEY_PREVIOUS` in place until the command can answer.

## Related

- [Rotating the key](../language/15-personal-data.md#rotating-the-key)
- [Migrations](../language/05-data.md#migrations)

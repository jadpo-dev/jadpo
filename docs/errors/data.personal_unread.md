# `data.personal_unread`

No query, output or action reads a field marked as personal data, so the app holds data it doesn't use.

## Why Jadpo refuses it

Personal data an app holds and never uses is data the law says it
shouldn't hold (data minimisation, GDPR Article 5). A test or a fixture
reading the field doesn't count, and nor does an export, which reads every
field whether the app uses it or not. It is said only of a project with
nothing else wrong, since a body with an error may hold a read no walk can
see.

## What it looks like

```jadpo check project=e-data-personal-unread file=app.jadpo hidden
application {}
```

```jadpo check project=e-data-personal-unread file=domains/clinic/types/note.jadpo hidden
/// A note a clinician wrote.
type Note = Text {
    max_length: 200
}
```

```jadpo check project=e-data-personal-unread file=domains/clinic/entities/patient.jadpo expect=data.personal_unread migrate=initial
use clinic/types { Note }

/// Someone the clinic treats.
entity Patient {
    id: Uuid
    medical_note: Note {
        personal: PersonalData.health
    }
    person: true
}
```

```text output
error: Nothing reads `Patient.medical_note`, which is personal data
 --> domains/clinic/entities/patient.jadpo:6:5
  |
6 |     medical_note: Note {
  |     ^^^^^^^^^^^^ no query, output or action reads it
  reason: Personal data an app holds and never uses is data it shouldn't hold (data minimisation, GDPR Article 5). No query, output or action reads `Patient.medical_note`; an export doesn't count, since it reads every field whether the app uses it or not.
  next: Read `medical_note` where the app needs it, or remove it from `Patient`, or write `decline data.personal_unread: "<why>"` above it if this is deliberate (agent)
  rule: data.personal_unread
  help: https://jadpo.dev/docs/errors/data.personal_unread

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If the app needs it**, read it where it is used, and the error goes:

```jadpo check project=e-data-personal-unread-read file=app.jadpo hidden
application {}
```

```jadpo check project=e-data-personal-unread-read file=domains/clinic/types/note.jadpo hidden
/// A note a clinician wrote.
type Note = Text {
    max_length: 200
}
```

```jadpo check project=e-data-personal-unread-read file=domains/clinic/entities/patient.jadpo hidden migrate=initial
use clinic/types { Note }

/// Someone the clinic treats.
entity Patient {
    id: Uuid
    medical_note: Note {
        personal: PersonalData.health
    }
    person: true
}
```

```jadpo check project=e-data-personal-unread-read file=domains/clinic/queries/notes.jadpo
use clinic/entities { Patient }
use clinic/types { Note }

/// A patient's note, as a clinician sees it.
type NoteLine = Output {
    medical_note: Note
}

/// Each patient's note.
query notes() -> List<NoteLine> {
    from: Patient
    order: [Patient.id asc]
    limit: 20
}
```

Now a query reads the field, so the app uses what it holds.

**If it is kept for a legal reason**, it may be read only by export, so a
`decline` line above it keeps it, with the reason:

```jadpo check project=e-data-personal-unread-declined file=app.jadpo hidden
application {}
```

```jadpo check project=e-data-personal-unread-declined file=domains/clinic/types/note.jadpo hidden
/// A note a clinician wrote.
type Note = Text {
    max_length: 200
}
```

```jadpo check project=e-data-personal-unread-declined file=domains/clinic/entities/patient.jadpo migrate=initial
use clinic/types { Note }

/// Someone the clinic treats.
entity Patient {
    id: Uuid
    decline data.personal_unread: "Kept for the regulator, read only by an export"
    medical_note: Note {
        personal: PersonalData.health
    }
    person: true
}
```

The field stays, and the reason is shown in review with every other decline.

## Related

- [What the compiler checks](../language/15-personal-data.md#what-the-compiler-checks)
- [Keeping code as it is, with a reason](../language/01-project-layout-and-modules.md#keeping-code-as-it-is-with-a-reason)

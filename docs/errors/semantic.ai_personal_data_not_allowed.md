# `semantic.ai_personal_data_not_allowed`

Your code sends a model personal data of a kind, such as health records, that the app's `ai:` settings don't allow it to send.

## Why Jadpo refuses it

By default your app may send a model no personal data at all. Each
kind it sends, its category, is listed once in `app.jadpo`, under
`ai: { personal_data: { allowed: } }`, so a reviewer sees in one place
what the provider is given. A field's category comes from its
`personal:` mark, or from a standard type that is always personal data,
such as an `NhsNumber`.

The compiler follows each value a prompt is built from back to the
fields it was read from. A marked field copied into a variable, built
into text or passed through a function keeps its category. The error
names the field and the category, never a value.

Jadpo won't list a category for you. Sending health records to a
provider is a privacy decision, and it is yours.

## What it looks like

The clinic allows `identifying` data, and its handover summary sends a
patient's notes, which are `health`, through a variable.

app.jadpo:

```jadpo check project=e-ai-personal-data-not-allowed file=app.jadpo
application {
    connections: {
        local_model: {
            driver: Driver.openai_compatible
            url: config.model_url
        }
    }
    ai: {
        connection: local_model
        models: {
            default: config.drafting_model
        }
        limits: {
            per_day: {
                tokens: 2000000
            }
        }
        personal_data: {
            allowed: [PersonalData.identifying]
        }
    }
}
```

```jadpo check project=e-ai-personal-data-not-allowed file=config.jadpo hidden
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-ai-personal-data-not-allowed file=domains/clinic/types/clinical_note.jadpo hidden
/// What a clinician wrote about a visit.
type ClinicalNote = Text {
    max_length: 4000
}
```

```jadpo check project=e-ai-personal-data-not-allowed file=domains/clinic/failures/summary_unavailable.jadpo hidden
/// The model can't summarise the notes now.
type SummaryUnavailable = Failure {
    kind: FailureKind.unavailable
}
```

domains/clinic/entities/patient.jadpo:

```jadpo check project=e-ai-personal-data-not-allowed file=domains/clinic/entities/patient.jadpo migrate=initial
use clinic/types { ClinicalNote }

/// A patient of the clinic.
entity Patient {
    id: Uuid
    patient_nhs_number: NhsNumber {
        personal: PersonalData.government_id
        to_models: false
    }
    notes: ClinicalNote {
        personal: PersonalData.health
    }

    person: true
}
```

```jadpo check project=e-ai-personal-data-not-allowed file=domains/clinic/types/handover_summary.jadpo hidden
/// What the clinician taking over is told.
type HandoverSummary = Output {
    /// The notes in three lines.
    summary: Text {
        max_length: 600
    }
}
```

```jadpo check project=e-ai-personal-data-not-allowed file=domains/clinic/prompts/summarise_notes.jadpo hidden
use clinic/types { ClinicalNote }

/// Summarises a patient's notes for the clinician taking over.
type SummariseNotes = Prompt {
    notes: ClinicalNote
}
```

```jadpo check project=e-ai-personal-data-not-allowed file=domains/clinic/prompts/summarise_notes.prompt hidden
# Instructions

Summarise the notes for the clinician taking over, in three lines.

# Request

{notes}
```

domains/clinic/actions/hand_over.jadpo:

```jadpo check project=e-ai-personal-data-not-allowed file=domains/clinic/actions/hand_over.jadpo expect=semantic.ai_personal_data_not_allowed
use clinic/entities { Patient, PatientNotFound }
use clinic/types { ClinicalNote, HandoverSummary }
use clinic/failures { SummaryUnavailable }
use clinic/prompts { SummariseNotes }

/// Summarises a patient's notes for the clinician taking over.
action hand_over(
    patient_id: Patient.id
) fails PatientNotFound, SummaryUnavailable -> HandoverSummary {
    var patient: Patient = attempt Patient.get(patient_id)
    var handover_notes: ClinicalNote = patient.notes
    return attempt ai.generate(
        SummariseNotes {
            notes: handover_notes
        },
        as: HandoverSummary
    ) {
        refused: SummaryUnavailable
        malformed: SummaryUnavailable
        timed_out: SummaryUnavailable
        over_budget: SummaryUnavailable
        too_long: SummaryUnavailable
    }
}
```

```text output
error: `Patient.notes` reaches a model, and `ai:` doesn't allow `health` data
 --> domains/clinic/actions/hand_over.jadpo:14:20
   |
14 |             notes: handover_notes
   |                    ^^^^^^^^^^^^^^ holds `Patient.notes`
  reason: `SummariseNotes`'s `{notes}` is built from `Patient.notes`, marked `personal: PersonalData.health`, a sensitive category. Each personal-data category sent to a model is listed in `ai: { personal_data: { allowed: } }`, so what the provider is given is decided in `app.jadpo`, where a reviewer sees it.
  next: List `PersonalData.health` in `ai: { personal_data: { allowed: } }`, or leave `Patient.notes` out of `SummariseNotes`'s `{notes}` (human)
  rule: semantic.ai_personal_data_not_allowed
  help: https://jadpo.dev/docs/errors/semantic.ai_personal_data_not_allowed

check failed: 1 error, 8 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether the model's provider is given patients' health records.

## How to fix it

The question is whether the provider may be given this kind of data.

**If it may**, list the category in `app.jadpo`:

```jadpo fragment
            allowed: [PersonalData.identifying, PersonalData.health]
```

```jadpo check project=e-ai-personal-data-not-allowed-fix-1 file=app.jadpo hidden
application {
    connections: {
        local_model: {
            driver: Driver.openai_compatible
            url: config.model_url
        }
    }
    ai: {
        connection: local_model
        models: {
            default: config.drafting_model
        }
        limits: {
            per_day: {
                tokens: 2000000
            }
        }
        personal_data: {
            allowed: [PersonalData.identifying, PersonalData.health]
        }
    }
}
```

```jadpo check project=e-ai-personal-data-not-allowed-fix-1 file=config.jadpo hidden
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-ai-personal-data-not-allowed-fix-1 file=domains/clinic/types/clinical_note.jadpo hidden
/// What a clinician wrote about a visit.
type ClinicalNote = Text {
    max_length: 4000
}
```

```jadpo check project=e-ai-personal-data-not-allowed-fix-1 file=domains/clinic/failures/summary_unavailable.jadpo hidden
/// The model can't summarise the notes now.
type SummaryUnavailable = Failure {
    kind: FailureKind.unavailable
}
```

```jadpo check project=e-ai-personal-data-not-allowed-fix-1 file=domains/clinic/entities/patient.jadpo hidden migrate=initial
use clinic/types { ClinicalNote }

/// A patient of the clinic.
entity Patient {
    id: Uuid
    patient_nhs_number: NhsNumber {
        personal: PersonalData.government_id
        to_models: false
    }
    notes: ClinicalNote {
        personal: PersonalData.health
    }

    person: true
}
```

```jadpo check project=e-ai-personal-data-not-allowed-fix-1 file=domains/clinic/types/handover_summary.jadpo hidden
/// What the clinician taking over is told.
type HandoverSummary = Output {
    /// The notes in three lines.
    summary: Text {
        max_length: 600
    }
}
```

```jadpo check project=e-ai-personal-data-not-allowed-fix-1 file=domains/clinic/prompts/summarise_notes.jadpo hidden
use clinic/types { ClinicalNote }

/// Summarises a patient's notes for the clinician taking over.
type SummariseNotes = Prompt {
    notes: ClinicalNote
}
```

```jadpo check project=e-ai-personal-data-not-allowed-fix-1 file=domains/clinic/prompts/summarise_notes.prompt hidden
# Instructions

Summarise the notes for the clinician taking over, in three lines.

# Request

{notes}
```

```jadpo check project=e-ai-personal-data-not-allowed-fix-1 file=domains/clinic/actions/hand_over.jadpo hidden
use clinic/entities { Patient, PatientNotFound }
use clinic/types { ClinicalNote, HandoverSummary }
use clinic/failures { SummaryUnavailable }
use clinic/prompts { SummariseNotes }

/// Summarises a patient's notes for the clinician taking over.
action hand_over(
    patient_id: Patient.id
) fails PatientNotFound, SummaryUnavailable -> HandoverSummary {
    var patient: Patient = attempt Patient.get(patient_id)
    var handover_notes: ClinicalNote = patient.notes
    return attempt ai.generate(
        SummariseNotes {
            notes: handover_notes
        },
        as: HandoverSummary
    ) {
        refused: SummaryUnavailable
        malformed: SummaryUnavailable
        timed_out: SummaryUnavailable
        over_budget: SummaryUnavailable
        too_long: SummaryUnavailable
    }
}
```

The handover summary now sends each patient's notes to the model, and
`app.jadpo` says so where a reviewer reads it. Any other prompt that
sends `health` data is allowed too, since the list is for the whole app.

If it may not, build the prompt only from fields whose categories are
listed, and leave `Patient.notes` out of it. The NHS number is never
sent either way, since its field is marked `to_models: false`.

## Related

- [Personal data sent to a model](../language/21-ai.md#personal-data-sent-to-a-model)
- [Following personal data](../language/21-ai.md#following-personal-data)
- [Marking personal fields](../language/15-personal-data.md#marking-personal-fields)
- [`semantic.ai_field_refused`](../errors/semantic.ai_field_refused.md)

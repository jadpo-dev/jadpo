# `semantic.ai_personal_data_not_allowed`

Each personal-data category a prompt or an embedded text sends the model
is listed in `ai: { personal_data: { allowed: } }`, so what the
provider is given is decided once, in `app.jadpo`, where a reviewer sees
it; `personal_data:` left out allows none. The compiler follows each
placeholder back to the fields it was read or built from, so a marked
field copied into a variable, built into text or passed through a
function keeps its category, and a standard type that is personal data
whatever holds it (an `NhsNumber`, a `ChatMessage`) is its own. The
problem names the field and the category, never a value. Here the
clinic lists `identifying` but sends `health`:

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

```jadpo check project=e-ai-personal-data-not-allowed file=domains/clinic/entities/patient.jadpo hidden migrate=initial
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

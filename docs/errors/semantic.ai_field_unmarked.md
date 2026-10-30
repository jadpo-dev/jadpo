# `semantic.ai_field_unmarked`

A field that stores a marked field's value keeps its marks: each
category, from `personal:` or the field's type, and `to_models: false`.
Whatever reads the field later, a prompt among them, sends what it
holds, so a copy into a field without them would reach a model unchecked. A value one decided, under an `if` or
a `match`, is stored with it too:

```jadpo check project=e-ai-field-unmarked file=app.jadpo hidden
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
            allowed: [PersonalData.health]
        }
    }
}
```

```jadpo check project=e-ai-field-unmarked file=config.jadpo hidden
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-ai-field-unmarked file=domains/clinic/types/clinical_note.jadpo hidden
/// What a clinician wrote about a visit.
type ClinicalNote = Text {
    max_length: 4000
}
```

```jadpo check project=e-ai-field-unmarked file=domains/clinic/entities/patient.jadpo migrate=initial
use clinic/types { ClinicalNote }

/// A patient of the clinic.
entity Patient {
    id: Uuid
    notes: ClinicalNote {
        personal: PersonalData.health
    }
    handover_remark: ClinicalNote?

    policy: {
        read: [Access.authenticated]
        update: [Access.authenticated]
    }
}
```

```jadpo check project=e-ai-field-unmarked file=domains/clinic/actions/remark_on_notes.jadpo expect=semantic.ai_field_unmarked
use clinic/entities { Patient, PatientNotFound }

/// Copies a patient's notes into the remark the next clinician reads.
action remark_on_notes(patient_id: Patient.id) fails PatientNotFound -> Patient {
    var patient: Patient = attempt Patient.get(patient_id)
    return Patient.update(patient) {
        set: {
            handover_remark: patient.notes
        }
    }
}
```

```text output
error: `Patient.notes` is stored in `Patient.handover_remark`, which isn't marked `health`
 --> domains/clinic/actions/remark_on_notes.jadpo:8:30
  |
8 |             handover_remark: patient.notes
  |                              ^^^^^^^^^^^^^ holds `Patient.notes`
  reason: `Patient.notes` is marked `personal: PersonalData.health`, a sensitive category. Whatever reads `Patient.handover_remark` later, a prompt among them, sends what it holds, so a field that stores a marked value keeps its marks: each category it is, in `personal:` or its type, and `to_models: false`.
  next: Give `handover_remark` `personal: PersonalData.health`, or leave `Patient.notes` out of it (agent)
  rule: semantic.ai_field_unmarked
  help: https://jadpo.dev/docs/errors/semantic.ai_field_unmarked

check failed: 1 error, 5 files
```

A model's answer carries what its prompt was built from, so a field read
off it is checked like those fields when it is stored or sent again: the summary of a patient's notes is `health` wherever it
is kept.

```jadpo check project=e-ai-answer-unmarked file=app.jadpo hidden
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
            allowed: [PersonalData.health]
        }
    }
}
```

```jadpo check project=e-ai-answer-unmarked file=config.jadpo hidden
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-ai-answer-unmarked file=domains/clinic/types/clinical_note.jadpo hidden
/// What a clinician wrote about a visit.
type ClinicalNote = Text {
    max_length: 4000
}
```

```jadpo check project=e-ai-answer-unmarked file=domains/clinic/entities/patient.jadpo migrate=initial hidden
use clinic/types { ClinicalNote }

/// A patient of the clinic.
entity Patient {
    id: Uuid
    notes: ClinicalNote {
        personal: PersonalData.health
    }
    handover_remark: ClinicalNote?

    policy: {
        read: [Access.authenticated]
        update: [Access.authenticated]
    }
}
```

```jadpo check project=e-ai-answer-unmarked file=domains/clinic/types/handover_summary.jadpo hidden
/// What the clinician taking over is told.
type HandoverSummary = Output {
    /// The notes in three lines.
    summary: Text {
        max_length: 600
    }
}
```

```jadpo check project=e-ai-answer-unmarked file=domains/clinic/failures/summary_unavailable.jadpo hidden
/// The model can't summarise the notes now.
type SummaryUnavailable = Failure {
    kind: FailureKind.unavailable
}
```

```jadpo check project=e-ai-answer-unmarked file=domains/clinic/prompts/summarise_notes.jadpo hidden
use clinic/types { ClinicalNote }

/// Summarises a patient's notes for the clinician taking over.
type SummariseNotes = Prompt {
    notes: ClinicalNote
}
```

```jadpo check project=e-ai-answer-unmarked file=domains/clinic/prompts/summarise_notes.prompt hidden
# Instructions

Summarise the notes for the clinician taking over, in three lines. Leave
out anything the notes don't say.

# Request

{notes}
```

```jadpo check project=e-ai-answer-unmarked file=domains/clinic/actions/keep_handover.jadpo expect=semantic.ai_field_unmarked
use clinic/entities { Patient, PatientNotFound }
use clinic/types { ClinicalNote, HandoverSummary }
use clinic/failures { SummaryUnavailable }
use clinic/prompts { SummariseNotes }

/// Keeps a summary of a patient's notes as the next clinician's remark.
action keep_handover(patient_id: Patient.id) fails PatientNotFound, SummaryUnavailable -> Patient {
    var patient: Patient = attempt Patient.get(patient_id)
    var handover: HandoverSummary = attempt ai.generate(
        SummariseNotes {
            notes: patient.notes
        },
        as: HandoverSummary
    ) {
        refused: SummaryUnavailable
        malformed: SummaryUnavailable
        timed_out: SummaryUnavailable
        over_budget: SummaryUnavailable
        too_long: SummaryUnavailable
    }
    return Patient.update(patient) {
        set: {
            handover_remark: ClinicalNote(handover.summary)
        }
    }
}
```

```text output
error: `Patient.notes` is stored in `Patient.handover_remark`, which isn't marked `health`
 --> domains/clinic/actions/keep_handover.jadpo:23:30
   |
23 |             handover_remark: ClinicalNote(handover.summary)
   |                              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ holds `Patient.notes`
  reason: `Patient.notes` is marked `personal: PersonalData.health`, a sensitive category. Whatever reads `Patient.handover_remark` later, a prompt among them, sends what it holds, so a field that stores a marked value keeps its marks: each category it is, in `personal:` or its type, and `to_models: false`.
  next: Give `handover_remark` `personal: PersonalData.health`, or leave `Patient.notes` out of it (agent)
  rule: semantic.ai_field_unmarked
  help: https://jadpo.dev/docs/errors/semantic.ai_field_unmarked

check failed: 1 error, 8 files
```

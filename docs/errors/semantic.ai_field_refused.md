# `semantic.ai_field_refused`

A value from a field marked `to_models: false`, which must never be sent to a model, reaches a prompt or text sent for an embedding.

## Why Jadpo refuses it

`to_models: false` on a field says its value is never sent to any
model's provider, whatever personal data `ai: { personal_data: { allowed: } }`
lets through. Use it for a value no provider should ever hold, such as
a patient's NHS number.

The compiler follows each value a prompt is built from back to the
fields it was read from, through variables, functions and text built
from it. So a marked field can't reach a model by being copied first.
The error names the field, never its value.

## What it looks like

The clinic's patients have an NHS number marked `to_models: false`, and
a prompt that checks a referral is given it.

```jadpo check project=e-ai-field-refused file=app.jadpo hidden
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
            allowed: [PersonalData.government_id, PersonalData.health]
        }
    }
}
```

```jadpo check project=e-ai-field-refused file=config.jadpo hidden
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-ai-field-refused file=domains/clinic/types/clinical_note.jadpo hidden
/// What a clinician wrote about a visit.
type ClinicalNote = Text {
    max_length: 4000
}
```

```jadpo check project=e-ai-field-refused file=domains/clinic/failures/summary_unavailable.jadpo hidden
/// The model can't summarise the notes now.
type SummaryUnavailable = Failure {
    kind: FailureKind.unavailable
}
```

domains/clinic/entities/patient.jadpo:

```jadpo check project=e-ai-field-refused file=domains/clinic/entities/patient.jadpo migrate=initial
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

```jadpo check project=e-ai-field-refused file=domains/clinic/types/handover_summary.jadpo hidden
/// What the clinician taking over is told.
type HandoverSummary = Output {
    /// The notes in three lines.
    summary: Text {
        max_length: 600
    }
}
```

domains/clinic/prompts/check_referral.jadpo:

```jadpo check project=e-ai-field-refused file=domains/clinic/prompts/check_referral.jadpo
use clinic/types { ClinicalNote }

/// Checks a referral's notes against the patient it names.
type CheckReferral = Prompt {
    notes: ClinicalNote
    number: NhsNumber
}
```

domains/clinic/prompts/check_referral.prompt:

```jadpo check project=e-ai-field-refused file=domains/clinic/prompts/check_referral.prompt
# Instructions

Say whether the notes are about the patient with this NHS number.

# Request

{number}

{notes}
```

domains/clinic/actions/check_referral_notes.jadpo:

```jadpo check project=e-ai-field-refused file=domains/clinic/actions/check_referral_notes.jadpo expect=semantic.ai_field_refused
use clinic/entities { Patient, PatientNotFound }
use clinic/types { HandoverSummary }
use clinic/failures { SummaryUnavailable }
use clinic/prompts { CheckReferral }

/// Checks a patient's notes against their NHS number.
action check_referral_notes(
    patient_id: Patient.id
) fails PatientNotFound, SummaryUnavailable -> HandoverSummary {
    var patient: Patient = attempt Patient.get(patient_id)
    return attempt ai.generate(
        CheckReferral {
            notes: patient.notes
            number: patient.patient_nhs_number
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
error: `Patient.patient_nhs_number` reaches a model, and is `to_models: false`
 --> domains/clinic/actions/check_referral_notes.jadpo:14:21
   |
14 |             number: patient.patient_nhs_number
   |                     ^^^^^^^^^^^^^^^^^^^^^^^^^^ holds `Patient.patient_nhs_number`
  reason: `to_models: false` keeps a field out of every prompt and every text sent for an embedding, whatever `ai: { personal_data: { allowed: } }` lists; `CheckReferral`'s `{number}` is built from `Patient.patient_nhs_number`.
  next: Leave `Patient.patient_nhs_number` out of `CheckReferral`'s `{number}` (agent)
  rule: semantic.ai_field_refused
  help: https://jadpo.dev/docs/errors/semantic.ai_field_refused

check failed: 1 error, 8 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the prompt can do its job without the number**, leave it out. In `domains/clinic/actions/check_referral_notes.jadpo`, the
prompt is built from the notes alone:

```jadpo fragment
    return attempt ai.generate(
        CheckReferral {
            notes: patient.notes
        },
        as: HandoverSummary
    ) {
        ...
    }
```

```jadpo check project=e-ai-field-refused-fix-1 file=domains/clinic/actions/check_referral_notes.jadpo hidden
use clinic/entities { Patient, PatientNotFound }
use clinic/types { HandoverSummary }
use clinic/failures { SummaryUnavailable }
use clinic/prompts { CheckReferral }

/// Summarises a patient's notes for the clinician taking over.
action check_referral_notes(
    patient_id: Patient.id
) fails PatientNotFound, SummaryUnavailable -> HandoverSummary {
    var patient: Patient = attempt Patient.get(patient_id)
    return attempt ai.generate(
        CheckReferral {
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
}
```

Then remove the field from the prompt's type and its text.
domains/clinic/prompts/check_referral.jadpo becomes:

```jadpo check project=e-ai-field-refused-fix-1 file=domains/clinic/prompts/check_referral.jadpo
use clinic/types { ClinicalNote }

/// Summarises a referral's notes for the clinician taking over.
type CheckReferral = Prompt {
    notes: ClinicalNote
}
```

domains/clinic/prompts/check_referral.prompt:

```jadpo check project=e-ai-field-refused-fix-1 file=domains/clinic/prompts/check_referral.prompt
# Instructions

Summarise the notes for the clinician taking over, in three lines.

# Request

{notes}
```

```jadpo check project=e-ai-field-refused-fix-1 file=app.jadpo hidden
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
            allowed: [PersonalData.government_id, PersonalData.health]
        }
    }
}
```

```jadpo check project=e-ai-field-refused-fix-1 file=config.jadpo hidden
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-ai-field-refused-fix-1 file=domains/clinic/types/clinical_note.jadpo hidden
/// What a clinician wrote about a visit.
type ClinicalNote = Text {
    max_length: 4000
}
```

```jadpo check project=e-ai-field-refused-fix-1 file=domains/clinic/failures/summary_unavailable.jadpo hidden
/// The model can't summarise the notes now.
type SummaryUnavailable = Failure {
    kind: FailureKind.unavailable
}
```

```jadpo check project=e-ai-field-refused-fix-1 file=domains/clinic/types/handover_summary.jadpo hidden
/// What the clinician taking over is told.
type HandoverSummary = Output {
    /// The notes in three lines.
    summary: Text {
        max_length: 600
    }
}
```

```jadpo check project=e-ai-field-refused-fix-1 file=domains/clinic/entities/patient.jadpo hidden migrate=initial
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

The model is sent only the notes, and the NHS number stays with your
app. A job that needs the number, such as matching a referral to a
patient, is done in your own code, never by the model.

## Related

- [Personal data sent to a model](../language/21-ai.md#personal-data-sent-to-a-model)
- [Following personal data](../language/21-ai.md#following-personal-data)
- [Marking personal fields](../language/15-personal-data.md#marking-personal-fields)
- [`semantic.ai_personal_data_not_allowed`](../errors/semantic.ai_personal_data_not_allowed.md)

//! Area 21, "Personal data sent to a model" (chunk 5.5v5b), in the
//! checker: each placeholder of a prompt an action or function builds,
//! and each text `ai.embed` sends, is followed back to the fields it was
//! read or built from, through variables, calls, text built from it and
//! lists. Every personal-data category reaching a model is listed in `ai:
//! { personal_data: { allowed: } }` (`semantic.ai_personal_data_not_allowed`),
//! a field's own `personal:` and the standard types' own categories alike,
//! and a field marked `to_models: false` reaches none
//! (`semantic.ai_field_refused`).

use jadpo_db::{Database, Layout};

use crate::support;

const CONFIG: (&str, &str) = (
    "config.jadpo",
    "config {\n    model_url: Url {\n        default: \"http://localhost:11434/v1\"\n    }\n    drafting_model: ModelId {\n        default: \"local/llama3.2\"\n    }\n    search_model: ModelId {\n        default: \"local/nomic-embed-text\"\n    }\n}\n",
);

/// `app.jadpo`, with `personal` written in `ai:` after its limits.
fn app(personal: &str) -> String {
    format!(
        "application {{\n    connections: {{\n        local_model: {{\n            driver: Driver.openai_compatible\n            url: config.model_url\n        }}\n    }}\n    ai: {{\n        connection: local_model\n        models: {{\n            default: config.drafting_model\n            embedding: config.search_model\n        }}\n        embedding_dimensions: 768\n        limits: {{\n            per_day: {{\n                tokens: 2000000\n            }}\n        }}\n{personal}    }}\n}}\n"
    )
}

/// `personal_data:` allowing `categories`.
fn allowing(categories: &[&str]) -> String {
    let listed: Vec<String> = categories
        .iter()
        .map(|category| format!("PersonalData.{category}"))
        .collect();
    format!(
        "        personal_data: {{\n            allowed: [{}]\n        }}\n",
        listed.join(", ")
    )
}

const PATIENT: (&str, &str) = (
    "domains/clinic/entities/patient.jadpo",
    "use clinic/types { ClinicalNote, WardName }\n\n/// A patient of the clinic.\nentity Patient {\n    id: Uuid\n    ward: WardName\n    patient_nhs_number: NhsNumber {\n        personal: PersonalData.government_id\n        to_models: false\n    }\n    notes: ClinicalNote {\n        personal: PersonalData.health\n    }\n    person: true\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
);

const TYPES: [(&str, &str); 6] = [
    (
        "domains/clinic/types/clinical_note.jadpo",
        "/// What a clinician wrote about a visit.\ntype ClinicalNote = Text {}\n",
    ),
    (
        "domains/clinic/types/ward_name.jadpo",
        "/// A ward's name.\ntype WardName = Text {\n    max_length: 80\n}\n",
    ),
    (
        "domains/clinic/types/note_summary.jadpo",
        "/// A summary of a patient's notes.\ntype NoteSummary = Output {\n    /// The summary.\n    summary: Text {\n        max_length: 2000\n    }\n}\n",
    ),
    (
        "domains/clinic/failures/summary_unavailable.jadpo",
        "/// The model can't answer now.\ntype SummaryUnavailable = Failure {\n    kind: FailureKind.unavailable\n}\n",
    ),
    (
        "domains/clinic/prompts/summarise_notes.jadpo",
        "use clinic/types { ClinicalNote }\n\n/// Summarises a patient's notes for the next clinician.\ntype SummariseNotes = Prompt {\n    notes: ClinicalNote\n}\n",
    ),
    (
        "domains/clinic/prompts/check_number.jadpo",
        "/// Checks an NHS number's format is explained.\ntype CheckNumber = Prompt {\n    number: NhsNumber\n}\n",
    ),
];

const NUMBER_PROMPT: (&str, &str) = (
    "domains/clinic/prompts/check_number.prompt",
    "# Instructions\n\nSay what kind of number this is.\n\n# Request\n\n{number}\n",
);

const PROMPT: (&str, &str) = (
    "domains/clinic/prompts/summarise_notes.prompt",
    "# Instructions\n\nSummarise the notes for the next clinician in three lines.\n\n# Request\n\n{notes}\n",
);

const ACTION_PATH: &str = "domains/clinic/actions/summarise.jadpo";

const ARMS: &str = "        refused: SummaryUnavailable\n        malformed: SummaryUnavailable\n        timed_out: SummaryUnavailable\n        over_budget: SummaryUnavailable\n        too_long: SummaryUnavailable\n";

/// The action, reading the patient, running `before`, and asking the
/// model with `prompt`; `after` follows it in its file.
fn action_asking(before: &str, prompt: &str, after: &str) -> String {
    format!(
        "use clinic/entities {{ Patient, PatientNotFound }}\nuse clinic/failures {{ SummaryUnavailable }}\nuse clinic/prompts {{ CheckNumber, SummariseNotes }}\nuse clinic/types {{ ClinicalNote, NoteSummary }}\n\n/// Summarises a patient's notes.\naction summarise(patient_id: Patient.id) fails PatientNotFound, SummaryUnavailable -> NoteSummary {{\n    var patient: Patient = attempt Patient.get(patient_id)\n{before}    return attempt ai.generate({prompt}, as: NoteSummary) {{\n{ARMS}    }}\n}}\n{after}"
    )
}

/// [`action_asking`] with `SummariseNotes { notes: <notes> }`.
fn action(before: &str, notes: &str) -> String {
    action_asking(before, &format!("SummariseNotes {{ notes: {notes} }}"), "")
}

/// Each problem as `path rule: message`, for the clinic with `app.jadpo`
/// and the files given, past what a fragment leaves unused.
fn problems_with(app: &str, files: &[(&str, &str)]) -> Vec<String> {
    let db = Database::default();
    let mut all: Vec<(&str, &str)> = vec![("app.jadpo", app), CONFIG, PATIENT];
    all.extend_from_slice(&TYPES);
    for (path, text) in files {
        match all.iter_mut().find(|(known, _)| known == path) {
            Some(file) => file.1 = text,
            None => all.push((path, text)),
        }
    }
    // Each domain's event record, as `jadpo records update` writes it.
    let scratch = Database::default();
    let scratched = support::project_with_migrated_records(
        &scratch,
        Layout::Project,
        &all,
        &[PROMPT, NUMBER_PROMPT],
    );
    let events: Vec<(String, String)> =
        jadpo_check::events::record::record_texts(&scratch, scratched)
            .iter()
            .map(|(domain, text)| (jadpo_check::events::record::path(domain), text.clone()))
            .collect();
    let mut records = vec![PROMPT, NUMBER_PROMPT];
    records.extend(
        events
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str())),
    );
    let project = support::project_with_migrated_records(&db, Layout::Project, &all, &records);
    jadpo_check::check(&db, project)
        .iter()
        .filter(|d| {
            !matches!(
                d.rule.id,
                "layout.unused_declaration" | "import.unused" | "semantic.unused_variable"
            )
        })
        .map(|d| {
            let path = d
                .primary
                .as_ref()
                .map_or("", |label| label.span.path.as_str());
            format!("{path} {}: {}", d.rule.id, d.message)
        })
        .collect()
}

fn problems(allowed: &[&str], files: &[(&str, &str)]) -> Vec<String> {
    problems_with(&app(&allowing(allowed)), files)
}

const NONE: &[&str] = &[];

const NOTES_UNLISTED: &str = "domains/clinic/actions/summarise.jadpo semantic.ai_personal_data_not_allowed: `Patient.notes` reaches a model, and `ai:` doesn't allow `health` data";

/// A field marked `health` read straight into a placeholder needs
/// `health` listed; listed, the clinic checks clean.
#[test]
fn a_marked_field_reaching_a_prompt_unlisted_fails() {
    let summarise = action("", "patient.notes");
    let files = [(ACTION_PATH, summarise.as_str())];
    assert_eq!(problems(&["health"], &files), NONE);
    assert_eq!(problems(&[], &files), [NOTES_UNLISTED]);
    assert_eq!(
        problems(&["contact", "financial"], &files),
        [NOTES_UNLISTED]
    );
    // `personal_data:` left out allows nothing.
    assert_eq!(problems_with(&app(""), &files), [NOTES_UNLISTED]);
    // A field nobody marked sends nothing personal.
    let ward = action("", "ClinicalNote(patient.ward)");
    assert_eq!(problems(&[], &[(ACTION_PATH, ward.as_str())]), NONE);
}

/// The value is followed through variables, text built from it, lists
/// and the functions it passes through, and what decided it is sent
/// with it.
#[test]
fn a_marked_field_is_followed_through_the_body() {
    let check = |before: &str, notes: &str, after: &str| {
        let summarise = action_asking(
            before,
            &format!("SummariseNotes {{ notes: {notes} }}"),
            after,
        );
        problems(&[], &[(ACTION_PATH, summarise.as_str())])
    };
    assert_eq!(
        check(
            "    var copied: ClinicalNote = patient.notes\n",
            "copied",
            ""
        ),
        [NOTES_UNLISTED]
    );
    assert_eq!(
        check(
            "    var parts: List<Text> = [text.trim(patient.ward), text.trim(patient.notes)]\n",
            "ClinicalNote(text.join(parts, \"\\n\"))",
            ""
        ),
        [NOTES_UNLISTED]
    );
    assert_eq!(
        check(
            "    var mut said: ClinicalNote = ClinicalNote(\"\")\n    said = ClinicalNote(text.join([\"Notes:\", patient.notes], \" \"))\n",
            "said",
            ""
        ),
        [NOTES_UNLISTED]
    );
    // What decided a value is sent with it.
    assert_eq!(
        check(
            "    var mut said: ClinicalNote = ClinicalNote(\"short\")\n    if (text.length(patient.notes) > 100) {\n        said = ClinicalNote(\"long\")\n    }\n",
            "said",
            ""
        ),
        [NOTES_UNLISTED]
    );
    // Through a function, called with the field.
    assert_eq!(
        check(
            "",
            "quoted(patient.notes)",
            "\n/// The notes, as they were written.\nfunction quoted(notes: ClinicalNote) -> ClinicalNote {\n    return notes\n}\n"
        ),
        [NOTES_UNLISTED]
    );
}

/// A prompt built in a function from its parameter is checked where the
/// function is called, against what it is passed.
#[test]
fn a_parameter_reaching_a_prompt_is_checked_where_it_is_passed() {
    let ask = "\n/// The prompt for some notes.\nfunction ask(notes: ClinicalNote) -> SummariseNotes {\n    return SummariseNotes {\n        notes: notes\n    }\n}\n";
    let with = |value: &str| {
        let summarise = action_asking("", &format!("ask({value})"), ask);
        problems(&[], &[(ACTION_PATH, summarise.as_str())])
    };
    assert_eq!(with("ClinicalNote(patient.ward)"), NONE);
    assert_eq!(with("patient.notes"), [NOTES_UNLISTED]);
}

/// `to_models: false` keeps a field from every prompt, whatever `allowed:`
/// lists.
#[test]
fn a_field_kept_from_models_is_refused() {
    let summarise = action_asking("", "CheckNumber { number: patient.patient_nhs_number }", "");
    let files = [(ACTION_PATH, summarise.as_str())];
    assert_eq!(
        problems(&["government_id", "health"], &files),
        [
            "domains/clinic/actions/summarise.jadpo semantic.ai_field_refused: `Patient.patient_nhs_number` reaches a model, and is `to_models: false`"
        ]
    );
}

/// A standard type that is personal data whatever holds it is its
/// category wherever it goes: an `NhsNumber` is `health`, and a
/// conversation is `health` too.
#[test]
fn a_standard_type_s_own_category_is_checked() {
    let ask = format!(
        "use clinic/failures {{ SummaryUnavailable }}\nuse clinic/prompts {{ CheckNumber }}\nuse clinic/types {{ NoteSummary }}\n\n/// Asks about a number.\naction ask_about(number: NhsNumber) fails SummaryUnavailable -> NoteSummary {{\n    return attempt ai.generate(CheckNumber {{ number: number }}, as: NoteSummary) {{\n{ARMS}    }}\n}}\n"
    );
    let files = [("domains/clinic/actions/ask_about.jadpo", ask.as_str())];
    assert_eq!(problems(&["health"], &files), NONE);
    assert_eq!(
        problems(&[], &files),
        [
            "domains/clinic/actions/ask_about.jadpo semantic.ai_personal_data_not_allowed: a `NhsNumber` reaches a model, and `ai:` doesn't allow `health` data"
        ]
    );
    let chat_type = "use clinic/types { WardName }\n\n/// Answers the last message of a conversation.\ntype ChatReply = Prompt {\n    history: List<ChatMessage>\n    question: WardName\n}\n";
    let chat_prompt =
        "# Instructions\n\nAnswer the patient's question kindly.\n\n# Request\n\n{question}\n";
    let chat = format!(
        "use clinic/failures {{ SummaryUnavailable }}\nuse clinic/prompts {{ ChatReply }}\nuse clinic/types {{ NoteSummary, WardName }}\n\n/// Answers a question.\naction reply(question: WardName) fails SummaryUnavailable -> NoteSummary {{\n    return attempt ai.generate(ChatReply {{ history: []  question: question }}, as: NoteSummary) {{\n{ARMS}    }}\n}}\n"
    );
    let db = Database::default();
    let app = app(&allowing(&[]));
    let mut all: Vec<(&str, &str)> = vec![("app.jadpo", &app), CONFIG, PATIENT];
    all.extend_from_slice(&TYPES);
    all.push(("domains/clinic/prompts/chat_reply.jadpo", chat_type));
    all.push(("domains/clinic/actions/reply.jadpo", &chat));
    let project = support::project_with_migrated_records(
        &db,
        Layout::Project,
        &all,
        &[
            PROMPT,
            NUMBER_PROMPT,
            ("domains/clinic/prompts/chat_reply.prompt", chat_prompt),
        ],
    );
    let found: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .filter(|d| !matches!(d.rule.id, "layout.unused_declaration" | "import.unused"))
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect();
    assert_eq!(
        found,
        [
            "semantic.ai_personal_data_not_allowed: a `ChatMessage` reaches a model, and `ai:` doesn't allow `health` data"
        ]
    );
}

/// The text `ai.embed` sends reaches a model too.
#[test]
fn an_embedded_text_is_checked() {
    let meaning = (
        "domains/clinic/types/note_meaning.jadpo",
        "/// A note's meaning.\ntype NoteMeaning = Vector {\n    dimensions: 768\n}\n",
    );
    let embed = "use clinic/entities { Patient, PatientNotFound }\nuse clinic/failures { SummaryUnavailable }\nuse clinic/types { NoteMeaning }\n\n/// A patient's notes' meaning.\naction meaning(patient_id: Patient.id) fails PatientNotFound, SummaryUnavailable -> NoteMeaning {\n    var patient: Patient = attempt Patient.get(patient_id)\n    return attempt ai.embed(patient.notes, as: NoteMeaning) {\n        timed_out: SummaryUnavailable\n        over_budget: SummaryUnavailable\n        too_long: SummaryUnavailable\n    }\n}\n";
    let files = [meaning, ("domains/clinic/actions/meaning.jadpo", embed)];
    assert_eq!(problems(&["health"], &files), NONE);
    assert_eq!(
        problems(&[], &files),
        [
            "domains/clinic/actions/meaning.jadpo semantic.ai_personal_data_not_allowed: `Patient.notes` reaches a model, and `ai:` doesn't allow `health` data"
        ]
    );
}

/// `personal_data:` holds `allowed:`, a list of area 15's categories;
/// a field's `personal:` is one of them and its `to_models:` a `Bool`.
#[test]
fn the_categories_are_read() {
    let summarise = action("", "ClinicalNote(patient.ward)");
    let files = [(ACTION_PATH, summarise.as_str())];
    let with = |personal: &str| problems_with(&app(personal), &files);
    assert_eq!(
        with("        personal_data: {\n            allowed: [PersonalData.helth]\n        }\n"),
        ["app.jadpo ai.settings_invalid: `PersonalData.helth` isn't a personal-data category"]
    );
    assert_eq!(
        with(
            "        personal_data: {\n            allowed: [PersonalData.health, PersonalData.health]\n        }\n"
        ),
        ["app.jadpo ai.settings_invalid: `allowed:` lists `PersonalData.health` twice"]
    );
    assert_eq!(
        with("        personal_data: {\n            allow: []\n        }\n"),
        [
            "app.jadpo ai.settings_invalid: `personal_data:` has no `allowed:`",
            "app.jadpo ai.settings_invalid: `allow` isn't a setting of `personal_data:`",
        ]
    );
    let misspelt = PATIENT
        .1
        .replace("PersonalData.health", "PersonalData.helth");
    assert_eq!(
        problems(
            &["health"],
            &[
                (ACTION_PATH, summarise.as_str()),
                (PATIENT.0, misspelt.as_str())
            ]
        ),
        [
            "domains/clinic/entities/patient.jadpo type.setting_value: `personal` takes one of area 15's categories"
        ]
    );
    let unclear = PATIENT.1.replace("to_models: false", "to_models: 0");
    assert_eq!(
        problems(
            &["health"],
            &[
                (ACTION_PATH, summarise.as_str()),
                (PATIENT.0, unclear.as_str())
            ]
        ),
        [
            "domains/clinic/entities/patient.jadpo type.setting_value: `to_models` takes `true` or `false`"
        ]
    );
}

/// A query's `Output` is followed to the fields it was read from.
#[test]
fn a_query_s_output_is_followed_to_its_fields() {
    let card = (
        "domains/clinic/types/patient_card.jadpo",
        "use clinic/entities { Patient }\n\n/// A patient as a ward list shows them.\ntype PatientCard = Output {\n    pick: [Patient.ward, Patient.notes]\n}\n",
    );
    let query = (
        "domains/clinic/queries/ward_cards.jadpo",
        "use clinic/entities { Patient }\nuse clinic/types { PatientCard }\n\n/// Every patient's card, by ward.\nquery ward_cards() -> List<PatientCard> {\n    from: Patient\n    order: [Patient.ward asc]\n    limit: 50\n}\n",
    );
    let check = |field: &str| {
        let summarise = action(
            &format!(
                "    var cards: List<PatientCard> = ward_cards()\n    var lines: List<Text> = list.map(cards, card -> text.trim(card.{field}))\n"
            ),
            "ClinicalNote(text.join(lines, \"\\n\"))",
        )
        .replace(
            "use clinic/types { ClinicalNote, NoteSummary }",
            "use clinic/queries { ward_cards }\nuse clinic/types { ClinicalNote, NoteSummary, PatientCard }",
        );
        problems(&[], &[card, query, (ACTION_PATH, summarise.as_str())])
    };
    assert_eq!(check("ward"), NONE);
    assert_eq!(check("notes"), [NOTES_UNLISTED]);
}

/// A subscriber passing part of its event to an action that sends it to
/// a model is checked against the event's own marks and what every
/// `emit` of it put there.
#[test]
fn an_event_s_part_passed_to_a_model_is_checked() {
    let event = |marked: bool| {
        format!(
            "use clinic/types {{ ClinicalNote }}\n\n/// Notes were added for a patient.\ntype NotesAdded = Event {{\n    notes: ClinicalNote{}\n}}\n",
            if marked {
                " {\n        personal: PersonalData.health\n    }"
            } else {
                ""
            }
        )
    };
    let emit = |from: &str| {
        format!(
            "use clinic/entities {{ Patient, PatientNotFound }}\nuse clinic/events {{ NotesAdded }}\nuse clinic/types {{ ClinicalNote }}\n\n/// Tells the clinic about a patient's notes.\naction announce(patient_id: Patient.id, written: ClinicalNote) fails PatientNotFound -> Unit {{\n    var patient: Patient = attempt Patient.get(patient_id)\n    emit NotesAdded {{\n        notes: {from}\n    }}\n}}\n"
        )
    };
    let draft = format!(
        "use clinic/failures {{ SummaryUnavailable }}\nuse clinic/prompts {{ SummariseNotes }}\nuse clinic/types {{ ClinicalNote, NoteSummary }}\n\n/// Drafts a summary of new notes.\naction draft(notes: ClinicalNote) fails SummaryUnavailable -> NoteSummary {{\n    return attempt ai.generate(SummariseNotes {{ notes: notes }}, as: NoteSummary) {{\n{ARMS}    }}\n}}\n"
    );
    let subscriber = "use clinic/actions { draft }\nuse clinic/events { NotesAdded }\n\n/// Drafts a summary of each patient's new notes.\nsubscriber draft_summary {\n    on: NotesAdded\n    run: draft(event.notes)\n}\n";
    let app = app(&allowing(&[])).replace(
        "    ai: {",
        "    events: {\n        store: main\n    }\n    ai: {",
    );
    let check = |marked: bool, from: &str| {
        let event = event(marked);
        let emit = emit(from);
        problems_with(
            &app,
            &[
                ("domains/clinic/events/notes_added.jadpo", event.as_str()),
                ("domains/clinic/actions/announce.jadpo", emit.as_str()),
                ("domains/clinic/actions/draft.jadpo", draft.as_str()),
                ("domains/clinic/subscribers/draft_summary.jadpo", subscriber),
            ],
        )
    };
    assert_eq!(check(false, "written"), NONE);
    assert_eq!(
        check(true, "written"),
        [
            "domains/clinic/subscribers/draft_summary.jadpo semantic.ai_personal_data_not_allowed: `NotesAdded.notes` reaches a model, and `ai:` doesn't allow `health` data"
        ]
    );
    assert_eq!(
        check(false, "patient.notes"),
        [
            "domains/clinic/subscribers/draft_summary.jadpo semantic.ai_personal_data_not_allowed: `Patient.notes` reaches a model, and `ai:` doesn't allow `health` data"
        ]
    );
}

/// A marked field of a standard type that is personal data itself is
/// reported once, with both categories.
#[test]
fn a_marked_field_of_a_personal_type_is_one_problem() {
    let unrefused = PATIENT.1.replace("        to_models: false\n", "");
    let summarise = action_asking("", "CheckNumber { number: patient.patient_nhs_number }", "");
    assert_eq!(
        problems(
            &[],
            &[
                (PATIENT.0, unrefused.as_str()),
                (ACTION_PATH, summarise.as_str())
            ]
        ),
        [
            "domains/clinic/actions/summarise.jadpo semantic.ai_personal_data_not_allowed: `Patient.patient_nhs_number` reaches a model, and `ai:` doesn't allow `government_id` and `health` data"
        ]
    );
}

/// The clinic's patient with a diagnosis, marked `health`, and a remark
/// marked as `remark` says: nothing, or one of its marks.
fn patient_with(remark: &str) -> String {
    format!(
        "use clinic/types {{ ClinicalNote, Diagnosis, WardName }}\n\n/// A patient of the clinic.\nentity Patient {{\n    id: Uuid\n    ward: WardName\n    patient_nhs_number: NhsNumber {{\n        personal: PersonalData.government_id\n        to_models: false\n    }}\n    notes: ClinicalNote {{\n        personal: PersonalData.health\n    }}\n    diagnosis: Diagnosis {{\n        personal: PersonalData.health\n    }}\n    remark: ClinicalNote?{remark}\n    person: true\n\n    policy: {{\n        read: [Access.public]\n        update: [Access.public]\n    }}\n}}\n"
    )
}

const DIAGNOSIS: (&str, &str) = (
    "domains/clinic/types/diagnosis.jadpo",
    "/// What a patient was diagnosed with.\ntype Diagnosis = Enum {\n    hiv\n    flu\n}\n",
);

/// [`action_asking`] for the patient of [`patient_with`], with
/// `SummariseNotes { notes: <notes> }`.
fn diagnosed(before: &str, notes: &str, after: &str) -> Vec<String> {
    let patient = patient_with("");
    let summarise = action_asking(
        before,
        &format!("SummariseNotes {{ notes: {notes} }}"),
        after,
    )
    .replace(
        "use clinic/types { ClinicalNote, NoteSummary }",
        "use clinic/types { ClinicalNote, Diagnosis, NoteSummary }",
    );
    problems(
        &[],
        &[
            DIAGNOSIS,
            (PATIENT.0, patient.as_str()),
            (ACTION_PATH, summarise.as_str()),
        ],
    )
}

const DIAGNOSIS_UNLISTED: &str = "domains/clinic/actions/summarise.jadpo semantic.ai_personal_data_not_allowed: `Patient.diagnosis` reaches a model, and `ai:` doesn't allow `health` data";

/// What a condition decides is sent with it: a
/// `match` on a marked field sends what it holds, and so does a filter
/// testing one, an `if`, and what follows a `return` it may have taken.
#[test]
fn what_a_condition_decides_reaches_the_model() {
    assert_eq!(
        diagnosed(
            "    var said: ClinicalNote = match patient.diagnosis {\n        Diagnosis.hiv -> ClinicalNote(\"hiv\")\n        Diagnosis.flu -> ClinicalNote(\"flu\")\n    }\n",
            "said",
            ""
        ),
        [DIAGNOSIS_UNLISTED]
    );
    assert_eq!(
        diagnosed(
            "    var flagged: List<Text> = list.filter([\"fever\", \"cough\"], word -> text.contains(patient.notes, word))\n",
            "ClinicalNote(text.join(flagged, \", \"))",
            ""
        ),
        [NOTES_UNLISTED]
    );
    // How many a filter kept shows what it tested.
    assert_eq!(
        diagnosed(
            "    var flagged: List<Text> = list.filter([\"fever\", \"cough\"], word -> text.contains(patient.notes, word))\n",
            "ClinicalNote(text.from(list.count(flagged)))",
            ""
        ),
        [NOTES_UNLISTED]
    );
    // A field kept from models decides nothing sent either.
    assert_eq!(
        diagnosed(
            "    var mut said: ClinicalNote = ClinicalNote(\"none\")\n    if (patient.patient_nhs_number == patient.patient_nhs_number) {\n        said = ClinicalNote(\"some\")\n    }\n",
            "said",
            ""
        ),
        [
            "domains/clinic/actions/summarise.jadpo semantic.ai_field_refused: `Patient.patient_nhs_number` reaches a model, and is `to_models: false`"
        ]
    );
}

/// What a loop goes through isn't sent with what is made in it: the
/// records of a `for`'s list, and `list.map`'s and `list.count`'s, show
/// only what decided which records they are.
#[test]
fn what_a_loop_goes_through_stays_unsent() {
    let query = (
        "domains/clinic/queries/ward_patients.jadpo",
        "use clinic/entities { Patient }\n\n/// Every patient, by ward.\nquery ward_patients() -> List<Patient> {\n    from: Patient\n    order: [Patient.ward asc]\n    limit: 50\n}\n",
    );
    let check = |before: &str, notes: &str| {
        let patient = patient_with("");
        let summarise = action_asking(
            before,
            &format!("SummariseNotes {{ notes: {notes} }}"),
            "",
        )
        .replace(
            "use clinic/types { ClinicalNote, NoteSummary }",
            "use clinic/queries { ward_patients }\nuse clinic/types { ClinicalNote, NoteSummary }",
        );
        problems(
            &[],
            &[
                DIAGNOSIS,
                query,
                (PATIENT.0, patient.as_str()),
                (ACTION_PATH, summarise.as_str()),
            ],
        )
    };
    let wards = "    var mut wards: List<Text> = []\n    for each in ward_patients() {\n        wards = list.append(wards, text.trim(each.ward))\n    }\n";
    assert_eq!(check(wards, "ClinicalNote(text.join(wards, \", \"))"), NONE);
    assert_eq!(
        check("", "ClinicalNote(text.from(list.count(ward_patients())))"),
        NONE
    );
    // A list a filter kept carries what it tested into the loop.
    let kept = "    var mut wards: List<Text> = []\n    for each in list.filter(ward_patients(), one -> text.length(one.notes) > 100) {\n        wards = list.append(wards, text.trim(each.ward))\n    }\n";
    assert_eq!(
        check(kept, "ClinicalNote(text.join(wards, \", \"))"),
        [NOTES_UNLISTED]
    );
}

const PATIENT_ACTION_PATH: &str = "domains/clinic/actions/note_remark.jadpo";

/// A write storing a marked field's value in a field without its marks
/// is refused: what reads the field later, a prompt
/// among them, would send it unchecked.
#[test]
fn a_write_storing_marked_content_without_its_marks_fails() {
    let check = |remark: &str, set: &str, after: &str| {
        let patient = patient_with(remark);
        let note = format!(
            "use clinic/entities {{ Patient, PatientNotFound }}\nuse clinic/types {{ ClinicalNote, Diagnosis }}\n\n/// Notes a remark on a patient.\naction note_remark(patient_id: Patient.id) fails PatientNotFound -> Unit {{\n    var patient: Patient = attempt Patient.get(patient_id)\n    var noted: Patient = Patient.update(patient) {{\n        set: {{\n            remark: {set}\n        }}\n    }}\n}}\n{after}"
        );
        let summarise = action("", "ClinicalNote(patient.ward)");
        problems(
            &["health"],
            &[
                DIAGNOSIS,
                (PATIENT.0, patient.as_str()),
                (PATIENT_ACTION_PATH, note.as_str()),
                (ACTION_PATH, summarise.as_str()),
            ],
        )
    };
    let marked = " {\n        personal: PersonalData.health\n    }";
    assert_eq!(check(marked, "patient.notes", ""), NONE);
    assert_eq!(
        check("", "patient.notes", ""),
        [
            "domains/clinic/actions/note_remark.jadpo semantic.ai_field_unmarked: `Patient.notes` is stored in `Patient.remark`, which isn't marked `health`"
        ]
    );
    // What decided the value is stored with it.
    assert_eq!(
        check(
            "",
            "match patient.diagnosis {\n                Diagnosis.hiv -> ClinicalNote(\"hiv\")\n                Diagnosis.flu -> ClinicalNote(\"flu\")\n            }",
            ""
        ),
        [
            "domains/clinic/actions/note_remark.jadpo semantic.ai_field_unmarked: a value decided by `Patient.diagnosis` is stored in `Patient.remark`, which isn't marked `health`"
        ]
    );
    // A field kept from models keeps the field it is stored in from them.
    assert_eq!(
        check(
            marked,
            "match patient.patient_nhs_number == patient.patient_nhs_number {\n                true -> ClinicalNote(\"same\")\n                false -> ClinicalNote(\"other\")\n            }",
            ""
        ),
        [
            "domains/clinic/actions/note_remark.jadpo semantic.ai_field_unmarked: a value decided by `Patient.patient_nhs_number` is stored in `Patient.remark`, which isn't marked `government_id` or `to_models: false`"
        ]
    );
    // A parameter stored is checked where the action is called.
    let keep = "use clinic/entities { Patient }\nuse clinic/types { ClinicalNote }\n\n/// Keeps a remark.\naction keep_remark(patient: Patient, said: ClinicalNote) -> Patient {\n    return Patient.update(patient) {\n        set: {\n            remark: said\n        }\n    }\n}\n";
    let calling = |passed: &str| {
        let copy = format!(
            "use clinic/actions {{ keep_remark }}\nuse clinic/entities {{ Patient, PatientNotFound }}\nuse clinic/types {{ ClinicalNote }}\n\n/// Copies a patient's notes into the remark.\naction copy_notes(patient_id: Patient.id) fails PatientNotFound -> Patient {{\n    var patient: Patient = attempt Patient.get(patient_id)\n    return keep_remark(patient, {passed})\n}}\n"
        );
        let patient = patient_with("");
        let summarise = action("", "ClinicalNote(patient.ward)");
        problems(
            &["health"],
            &[
                DIAGNOSIS,
                (PATIENT.0, patient.as_str()),
                ("domains/clinic/actions/keep_remark.jadpo", keep),
                ("domains/clinic/actions/copy_notes.jadpo", copy.as_str()),
                (ACTION_PATH, summarise.as_str()),
            ],
        )
    };
    assert_eq!(calling("ClinicalNote(patient.ward)"), NONE);
    assert_eq!(
        calling("patient.notes"),
        [
            "domains/clinic/actions/copy_notes.jadpo semantic.ai_field_unmarked: `copy_notes` passes `Patient.notes` to `keep_remark`, which stores it in `Patient.remark`, which isn't marked `health`"
        ]
    );
}

/// A subscriber passing a marked part of its event to an action that
/// stores it in a field without its marks is refused at the subscriber.
#[test]
fn a_subscriber_storing_its_event_s_marked_part_without_its_marks_fails() {
    let patient = patient_with("").replace(
        "        read: [Access.public]\n        update: [Access.public]\n",
        "        read: [Access.public, Access.system]\n        update: [Access.system]\n",
    );
    let event = "use clinic/entities { Patient }\nuse clinic/types { ClinicalNote }\n\n/// Notes were added for a patient.\ntype NotesAdded = Event {\n    patient_id: Patient.id\n    notes: ClinicalNote {\n        personal: PersonalData.health\n    }\n}\n";
    let announce = "use clinic/entities { Patient, PatientNotFound }\nuse clinic/events { NotesAdded }\nuse clinic/types { ClinicalNote }\n\n/// Tells the clinic about a patient's notes.\naction announce(patient_id: Patient.id, written: ClinicalNote) fails PatientNotFound -> Unit {\n    var patient: Patient = attempt Patient.get(patient_id)\n    emit NotesAdded {\n        patient_id: patient.id\n        notes: written\n    }\n}\n";
    let remember = "use clinic/entities { Patient, PatientNotFound }\nuse clinic/types { ClinicalNote }\n\n/// Keeps what was said about a patient.\naction remember(patient_id: Patient.id, said: ClinicalNote) fails PatientNotFound -> Unit {\n    var patient: Patient = attempt Patient.get(patient_id)\n    var kept: Patient = Patient.update(patient) {\n        set: {\n            remark: said\n        }\n    }\n}\n";
    let subscriber = |passed: &str| {
        format!(
            "use clinic/actions {{ remember }}\nuse clinic/events {{ NotesAdded }}\n\n/// Keeps each patient's new notes as a remark.\nsubscriber remember_notes {{\n    on: NotesAdded\n    run: remember(event.patient_id, {passed})\n}}\n"
        )
    };
    let app = app(&allowing(&["health"])).replace(
        "    ai: {",
        "    events: {\n        store: main\n    }\n    ai: {",
    );
    let summarise = action("", "ClinicalNote(patient.ward)");
    let check = |passed: &str| {
        let subscriber = subscriber(passed);
        problems_with(
            &app,
            &[
                DIAGNOSIS,
                (PATIENT.0, patient.as_str()),
                ("domains/clinic/events/notes_added.jadpo", event),
                ("domains/clinic/actions/announce.jadpo", announce),
                ("domains/clinic/actions/remember.jadpo", remember),
                (
                    "domains/clinic/subscribers/remember_notes.jadpo",
                    subscriber.as_str(),
                ),
                (ACTION_PATH, summarise.as_str()),
            ],
        )
    };
    assert_eq!(
        check("event.notes"),
        [
            "domains/clinic/subscribers/remember_notes.jadpo semantic.ai_field_unmarked: `remember_notes` passes `NotesAdded.notes` to `remember`, which stores it in `Patient.remark`, which isn't marked `health`"
        ]
    );
}

/// The clinic's action asking the model for a summary of the patient's
/// notes, then running `after` with the `answer`.
fn answered(after: &str) -> String {
    format!(
        "use clinic/entities {{ Patient, PatientNotFound }}\nuse clinic/failures {{ SummaryUnavailable }}\nuse clinic/prompts {{ SummariseNotes }}\nuse clinic/types {{ ClinicalNote, NoteSummary }}\n\n/// Summarises a patient's notes.\naction summarise(patient_id: Patient.id) fails PatientNotFound, SummaryUnavailable -> NoteSummary {{\n    var patient: Patient = attempt Patient.get(patient_id)\n    var answer: NoteSummary = attempt ai.generate(SummariseNotes {{ notes: patient.notes }}, as: NoteSummary) {{\n{ARMS}    }}\n{after}    return answer\n}}\n"
    )
}

const KEEP_SUMMARY: &str = "    var kept: Patient = Patient.update(patient) {\n        set: {\n            remark: ClinicalNote(answer.summary)\n        }\n    }\n";

/// A model's answer carries what its prompt was built from (chunk
/// 7.9g): a field read off it, the summary of a patient's notes, is
/// checked like the notes when it is stored or sent again.
#[test]
fn a_model_s_answer_carries_what_its_prompt_was_built_from() {
    let stored = |remark: &str| {
        let patient = patient_with(remark);
        let summarise = answered(KEEP_SUMMARY);
        problems(
            &["health"],
            &[
                DIAGNOSIS,
                (PATIENT.0, patient.as_str()),
                (ACTION_PATH, summarise.as_str()),
            ],
        )
    };
    assert_eq!(
        stored(""),
        [
            "domains/clinic/actions/summarise.jadpo semantic.ai_field_unmarked: `Patient.notes` is stored in `Patient.remark`, which isn't marked `health`"
        ]
    );
    assert_eq!(
        stored(" {\n        personal: PersonalData.health\n    }"),
        NONE
    );
    // Sent again, from another action given the answer.
    let patient = patient_with("");
    let summarise = answered("");
    let again = format!(
        "use clinic/actions {{ summarise }}\nuse clinic/entities {{ Patient, PatientNotFound }}\nuse clinic/failures {{ SummaryUnavailable }}\nuse clinic/prompts {{ SummariseNotes }}\nuse clinic/types {{ ClinicalNote, NoteSummary }}\n\n/// Summarises a patient's summary again.\naction resummarise(patient_id: Patient.id) fails PatientNotFound, SummaryUnavailable -> NoteSummary {{\n    var first: NoteSummary = attempt summarise(patient_id)\n    return attempt ai.generate(SummariseNotes {{ notes: ClinicalNote(first.summary) }}, as: NoteSummary) {{\n{ARMS}    }}\n}}\n"
    );
    let mut sent = problems(
        &[],
        &[
            DIAGNOSIS,
            (PATIENT.0, patient.as_str()),
            (ACTION_PATH, summarise.as_str()),
            ("domains/clinic/actions/resummarise.jadpo", again.as_str()),
        ],
    );
    sent.sort();
    assert_eq!(
        sent,
        [
            "domains/clinic/actions/resummarise.jadpo semantic.ai_personal_data_not_allowed: `Patient.notes` reaches a model, and `ai:` doesn't allow `health` data",
            NOTES_UNLISTED,
        ]
    );
}

/// Stored where more callers read it than may read the notes, the
/// answer is refused as the notes would be (area 9, "What a write
/// stores").
#[test]
fn a_model_s_answer_keeps_its_sources_read_rules() {
    let patient = patient_with(" {\n        personal: PersonalData.health\n    }").replace(
        "    notes: ClinicalNote {\n        personal: PersonalData.health\n    }",
        "    notes: ClinicalNote {\n        personal: PersonalData.health\n        policy: {\n            read: [Access.authenticated]\n        }\n    }",
    );
    assert!(patient.contains("Access.authenticated"));
    let summarise = answered(KEEP_SUMMARY);
    let found: Vec<String> = problems(
        &["health"],
        &[
            DIAGNOSIS,
            (PATIENT.0, patient.as_str()),
            (ACTION_PATH, summarise.as_str()),
        ],
    )
    .into_iter()
    .filter(|problem| problem.contains("policy.output_field_unproved"))
    .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`Patient.notes`"), "{found:?}");
}

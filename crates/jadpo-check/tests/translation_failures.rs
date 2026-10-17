//! Failures worded for client apps (area 10, chunk 5.4d): a failure's text
//! is a member of its domain's language files keyed by its code, its
//! `client:` fields the placeholders, required in every supported locale
//! for each failure a route can answer with and for none that never leaves
//! the server.

use jadpo_db::Layout;

use crate::support::problems;

const APP: &str = "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.fr_fr]\n}\n";

const POLICY: &str =
    "use planning\n\npolicy {\n    actions: {\n        planning.find: [Access.public]\n    }\n}\n";

const TYPES: &[(&str, &str)] = &[
    (
        "domains/planning/types/todo_ref.jadpo",
        "/// A todo's reference.\ntype TodoRef = Text {\n    max_length: 40\n}\n",
    ),
    (
        "domains/planning/types/due_at.jadpo",
        "/// When a todo is due.\ntype DueAt = Instant {}\n",
    ),
    (
        "domains/planning/types/found.jadpo",
        "use planning/types { TodoRef }\n\n/// A todo found.\ntype Found = Output {\n    reference: TodoRef\n}\n",
    ),
    (
        "domains/planning/types/find_request.jadpo",
        "use planning/types { TodoRef }\n\n/// A todo to find.\ntype FindRequest = Input {\n    reference: TodoRef\n}\n",
    ),
];

const FAILURE: &str = "use planning/types { DueAt, TodoRef }\n\n/// No todo has the reference.\ntype TodoMissing = Failure {\n    kind: FailureKind.not_found\n    client: {\n        reference: TodoRef\n        due: DueAt\n    }\n    internal: {\n        hint: TodoRef\n    }\n}\n";

const ACTION: &str = "use planning/types { Found, TodoRef }\nuse planning/failures { TodoMissing }\n\n/// Finds a todo.\naction find(reference: TodoRef) fails TodoMissing -> Found {\n    reject TodoMissing {\n        reference: reference\n        due: \"2026-10-20T18:30:00Z\"\n        hint: reference\n    }\n}\n";

const ROUTE: &str = "use planning/types { FindRequest, Found }\nuse planning/actions { find }\n\nroutes Todos {\n    prefix: /todos\n\n    POST / {\n        authentication: RouteAuthentication.none\n        input: FindRequest\n        output: Found\n        run: find(input.reference)\n    }\n}\n";

const EN: &str =
    "translations {\n    todo_missing: \"No todo {reference}; it was due {due}.\"\n}\n";
const JADPO_FR: &str = "translations {\n    jadpo: {\n        deadline_exceeded: \"fr: deadline_exceeded.\"\n        idempotency_key_reused: \"fr: idempotency_key_reused.\"\n        internal_fault: \"fr: internal_fault.\"\n        invalid_input: \"fr: invalid_input.\"\n        invalid_request: \"fr: invalid_request.\"\n        method_not_allowed: \"fr: method_not_allowed.\"\n        rate_limited: \"fr: rate_limited.\"\n        request_too_large: \"fr: request_too_large.\"\n        route_not_found: \"fr: route_not_found.\"\n        unavailable: \"fr: unavailable.\"\n    }\n}\n";
const FR: &str =
    "translations {\n    todo_missing: \"Aucune tâche {reference}, attendue {due}.\"\n}\n";

fn project(replace: &[(&str, &str)]) -> Vec<String> {
    let mut files: Vec<(String, String)> = vec![
        ("app.jadpo", APP),
        ("policy.jadpo", POLICY),
        ("domains/planning/failures/todo_missing.jadpo", FAILURE),
        ("domains/planning/actions/find.jadpo", ACTION),
        ("domains/planning/routes/todos.jadpo", ROUTE),
        ("domains/planning/translations/en_gb.jadpo", EN),
        ("domains/planning/translations/fr_fr.jadpo", FR),
        ("translations/fr_fr.jadpo", JADPO_FR),
    ]
    .into_iter()
    .chain(TYPES.iter().copied())
    .map(|(path, text)| (path.to_string(), text.to_string()))
    .collect();
    for (path, text) in replace {
        match files.iter_mut().find(|(seen, _)| seen == path) {
            Some(found) => found.1 = (*text).to_string(),
            None => files.push(((*path).to_string(), (*text).to_string())),
        }
    }
    files.retain(|(_, text)| !text.is_empty());
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    problems(Layout::Project, &borrowed)
        .into_iter()
        // The API record is `jadpo api update`'s, not what is tested here.
        .filter(|line| !line.contains("api.record_out_of_date"))
        .collect()
}

const CLEAN: &[&str] = &[];

#[test]
fn a_failure_worded_in_every_locale_checks() {
    assert_eq!(project(&[]), CLEAN);
}

#[test]
fn a_failure_a_route_answers_with_needs_wording_in_each_locale() {
    let gone = project(&[(
        "domains/planning/translations/fr_fr.jadpo",
        "translations {\n}\n",
    )]);
    assert_eq!(
        gone,
        [
            "domains/planning/translations/fr_fr.jadpo translation.wording_missing: `todo_missing` has no wording in `domains/planning/translations/fr_fr.jadpo`"
        ]
    );
    // With no language file at all for the locale, the file is missing.
    let no_file = project(&[("domains/planning/translations/fr_fr.jadpo", "")]);
    assert_eq!(no_file.len(), 1, "{no_file:?}");
    assert!(
        no_file[0].contains("translation.file_missing"),
        "{no_file:?}"
    );
}

#[test]
fn a_domain_with_no_translations_at_all_is_asked_for_them() {
    let none = project(&[
        ("domains/planning/translations/en_gb.jadpo", ""),
        ("domains/planning/translations/fr_fr.jadpo", ""),
    ]);
    assert_eq!(none.len(), 2, "{none:?}");
    assert!(
        none.iter()
            .all(|line| line.contains("translation.file_missing")),
        "{none:?}"
    );
}

#[test]
fn a_failure_no_route_answers_with_needs_none() {
    // The route runs an action that never fails; `find`, which raises
    // `TodoMissing`, is left to the tests, so the failure never leaves the
    // server.
    let calm = "use planning/types { Found, TodoRef }\n\n/// Echoes a todo.\naction echo(reference: TodoRef) -> Found {\n    return Found {\n        reference: reference\n    }\n}\n";
    let route = ROUTE
        .replace(
            "use planning/actions { find }",
            "use planning/actions { echo }",
        )
        .replace("run: find(input.reference)", "run: echo(input.reference)");
    let test =
        "test \"a missing todo is refused\" {\n    assert find(\"A\").reference == \"A\"\n}\n";
    let found = project(&[
        ("domains/planning/actions/echo.jadpo", calm),
        ("domains/planning/actions/find.test.jadpo", test),
        ("domains/planning/routes/todos.jadpo", &route),
        ("domains/planning/translations/en_gb.jadpo", ""),
        ("domains/planning/translations/fr_fr.jadpo", ""),
    ]);
    assert!(
        !found.iter().any(|line| line.contains("translation.")),
        "{found:?}"
    );
}

#[test]
fn only_client_fields_are_placeholders() {
    let en = "translations {\n    todo_missing: \"No todo {reference}, hint {hint}.\"\n}\n";
    let found = project(&[("domains/planning/translations/en_gb.jadpo", en)]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0]
            .contains("translation.placeholder_unknown: `{hint}` isn't a field of `TodoMissing`"),
        "{found:?}"
    );
}

#[test]
fn a_member_is_the_failures_code_not_its_name() {
    let en = "translations {\n    todo_not_found: \"No todo.\"\n}\n";
    let found = project(&[("domains/planning/translations/en_gb.jadpo", en)]);
    assert!(
        found
            .iter()
            .any(|line| line.contains("translation.member_unknown")),
        "{found:?}"
    );
}

#[test]
fn an_explicit_code_is_the_member() {
    let failure = FAILURE.replace(
        "    kind: FailureKind.not_found\n",
        "    kind: FailureKind.not_found\n    code: \"todo_gone\"\n",
    );
    let en = EN.replace("todo_missing", "todo_gone");
    let fr = FR.replace("todo_missing", "todo_gone");
    assert_eq!(
        project(&[
            ("domains/planning/failures/todo_missing.jadpo", &failure),
            ("domains/planning/translations/en_gb.jadpo", &en),
            ("domains/planning/translations/fr_fr.jadpo", &fr),
        ]),
        CLEAN
    );
}

#[test]
fn a_client_field_a_client_cannot_format_is_refused() {
    let failure = FAILURE.replace("        due: DueAt\n", "        due: DueAt?\n");
    let found = project(&[("domains/planning/failures/todo_missing.jadpo", &failure)]);
    assert!(
        found
            .iter()
            .any(|line| line.contains("translation.field_type")),
        "{found:?}"
    );
}

#[test]
fn an_unfinished_locale_needs_no_wording_for_a_failure() {
    let app = "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.fr_fr]\n    in_progress: [Locale.fr_fr]\n}\n";
    assert_eq!(
        project(&[
            ("app.jadpo", app),
            ("domains/planning/translations/fr_fr.jadpo", ""),
            ("translations/fr_fr.jadpo", ""),
        ]),
        CLEAN
    );
}

#[test]
fn a_project_without_locales_words_no_failures() {
    assert_eq!(
        project(&[
            ("app.jadpo", "application {}\n"),
            ("domains/planning/translations/en_gb.jadpo", ""),
            ("domains/planning/translations/fr_fr.jadpo", ""),
            ("translations/fr_fr.jadpo", ""),
        ]),
        CLEAN
    );
}

/// The not-found failure the compiler provides for an entity is worded like
/// one the project declares, in the entity's domain.
fn notes(en: &str, fr: &str) -> Vec<String> {
    let files = [
        ("app.jadpo", APP),
        ("translations/fr_fr.jadpo", JADPO_FR),
        (
            "domains/notes/types/note_text.jadpo",
            "/// A note's text.\ntype NoteText = Text {\n    max_length: 200\n}\n",
        ),
        (
            "domains/notes/types/note_view.jadpo",
            "use notes/types { NoteText }\n\n/// A note shown.\ntype NoteView = Output {\n    text: NoteText\n}\n",
        ),
        (
            "domains/notes/entities/note.jadpo",
            "use notes/types { NoteText }\n\nentity Note {\n    id: Uuid\n    text: NoteText\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n        update: [Access.public]\n        delete: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/notes/queries/find_note.jadpo",
            "use notes/entities { Note, NoteNotFound }\nuse notes/types { NoteView }\n\n/// One note.\nquery find_note(note_id: Note.id) fails NoteNotFound -> NoteView {\n    from: Note\n    where: Note.id == note_id\n    missing: NoteNotFound\n}\n",
        ),
        (
            "domains/notes/routes/notes.jadpo",
            "use notes/entities { Note }\nuse notes/types { NoteView }\nuse notes/queries { find_note }\n\nroutes Notes {\n    prefix: /notes\n\n    /// One note.\n    GET /{note_id: Note.id} {\n        authentication: RouteAuthentication.none\n        output: NoteView\n        run: find_note(note_id)\n    }\n}\n",
        ),
        ("domains/notes/translations/en_gb.jadpo", en),
        ("domains/notes/translations/fr_fr.jadpo", fr),
    ];
    let files: Vec<(&str, &str)> = files
        .into_iter()
        .filter(|(_, text)| !text.is_empty())
        .collect();
    problems(Layout::Project, &files)
        .into_iter()
        .filter(|line| !line.contains("api.record_out_of_date"))
        .collect()
}

#[test]
fn the_not_found_failure_the_compiler_provides_is_worded_in_its_domain() {
    let en = "translations {\n    note_not_found: \"No such note.\"\n}\n";
    let fr = "translations {\n    note_not_found: \"Note introuvable.\"\n}\n";
    assert_eq!(notes(en, fr), CLEAN);
    let missing = notes(en, "translations {\n}\n");
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert!(
        missing[0].contains("translation.wording_missing: `note_not_found` has no wording"),
        "{missing:?}"
    );
}

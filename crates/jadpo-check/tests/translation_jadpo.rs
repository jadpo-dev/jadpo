//! Jadpo's own text (area 10, "Jadpo's and packages' text"; chunk 5.4e):
//! Jadpo's messages are shipped in `en_gb` and `en_us`; an app that supports
//! another locale words them in the root `translations/<locale>.jadpo`,
//! under `jadpo: { }`, and that file is checked against Jadpo's own message
//! types. Only the messages of features the app has switched on count.

use jadpo_check::lookups::Name;
use jadpo_check::translations::sources::{Entry, ROOT, hash, record_path, render, standings};
use jadpo_db::{Database, Layout};

use crate::support;

const POLICY: &str =
    "use planning\n\npolicy {\n    actions: {\n        planning.find: [Access.public]\n    }\n}\n";

const TYPES: &[(&str, &str)] = &[
    (
        "domains/planning/types/todo_ref.jadpo",
        "/// A todo's reference.\ntype TodoRef = Text {\n    max_length: 40\n}\n",
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

const ACTION: &str = "use planning/types { Found, TodoRef }\n\n/// Finds a todo.\naction find(reference: TodoRef) -> Found {\n    return Found {\n        reference: reference\n    }\n}\n";

const ROUTE: &str = "use planning/types { FindRequest, Found }\nuse planning/actions { find }\n\nroutes Todos {\n    prefix: /todos\n\n    POST / {\n        authentication: RouteAuthentication.none\n        input: FindRequest\n        output: Found\n        run: find(input.reference)\n    }\n}\n";

fn app(supported: &str, in_progress: &str) -> String {
    format!(
        "application {{}}\n\nlocales {{\n    default: Locale.en_gb\n    supported: [{supported}]\n{in_progress}}}\n"
    )
}

/// The messages Jadpo needs worded for an app with routes that need no
/// caller.
const REQUESTS: [&str; 10] = [
    "deadline_exceeded",
    "idempotency_key_reused",
    "internal_fault",
    "invalid_input",
    "invalid_request",
    "method_not_allowed",
    "rate_limited",
    "request_too_large",
    "route_not_found",
    "unavailable",
];

/// A root language file wording `members` under `jadpo: { }`.
fn words(members: &[&str]) -> String {
    let mut out = String::from("translations {\n    jadpo: {\n");
    for member in members {
        out.push_str(&format!("        {member}: \"Texte de {member}.\"\n"));
    }
    out.push_str("    }\n}\n");
    out
}

fn routes(extra: &[(&str, &str)], supported: &str, in_progress: &str) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = vec![
        ("app.jadpo".into(), app(supported, in_progress)),
        ("policy.jadpo".into(), POLICY.into()),
        ("domains/planning/actions/find.jadpo".into(), ACTION.into()),
        ("domains/planning/routes/todos.jadpo".into(), ROUTE.into()),
    ];
    files.extend(
        TYPES
            .iter()
            .map(|(path, text)| (path.to_string(), text.to_string())),
    );
    for (path, text) in extra {
        match files.iter_mut().find(|(seen, _)| seen == path) {
            Some(found) => found.1 = (*text).to_string(),
            None => files.push(((*path).to_string(), (*text).to_string())),
        }
    }
    files.retain(|(_, text)| !text.is_empty());
    files
}

fn problems(files: &[(String, String)]) -> Vec<String> {
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    support::problems(Layout::Project, &borrowed)
        .into_iter()
        // The API record is `jadpo api update`'s, not what is tested here.
        .filter(|line| !line.contains("api.record_out_of_date"))
        .collect()
}

/// Each problem's rule and reason.
fn reasons(files: &[(String, String)]) -> Vec<(String, String)> {
    let db = Database::default();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let project = support::project(&db, Layout::Project, &borrowed);
    jadpo_check::check(&db, project)
        .iter()
        .filter(|d| !d.rule.id.starts_with("api."))
        .map(|d| (d.rule.id.to_string(), d.reason.clone().unwrap_or_default()))
        .collect()
}

const EN: &str = "Locale.en_gb, Locale.en_us";
const FR: &str = "Locale.en_gb, Locale.fr_fr";
const CLEAN: &[&str] = &[];

#[test]
fn jadpo_ships_its_wording_in_british_and_american_english() {
    assert_eq!(problems(&routes(&[], EN, "")), CLEAN);
}

#[test]
fn a_locale_jadpo_does_not_ship_is_one_error_listing_the_messages() {
    let found = problems(&routes(&[], FR, ""));
    assert_eq!(found.len(), 1, "{found:?}");
    let line = &found[0];
    assert!(
        line.starts_with("app.jadpo translation.jadpo_missing:"),
        "{line}"
    );
    assert!(line.contains("Locale.fr_fr"), "{line}");
}

#[test]
fn the_root_file_words_jadpo_for_that_locale() {
    let files = routes(&[("translations/fr_fr.jadpo", &words(&REQUESTS))], FR, "");
    assert_eq!(problems(&files), CLEAN);
}

#[test]
fn a_message_left_out_is_the_one_the_error_names() {
    let some: Vec<&str> = REQUESTS
        .iter()
        .copied()
        .filter(|key| *key != "rate_limited")
        .collect();
    let files = routes(&[("translations/fr_fr.jadpo", &words(&some))], FR, "");
    let found = reasons(&files);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].0 == "translation.jadpo_missing", "{found:?}");
    assert!(
        found[0].1.contains("Still to word: `rate_limited`."),
        "{found:?}"
    );
}

#[test]
fn an_app_with_no_routes_shows_none_of_jadpo_failures() {
    let files = vec![("app.jadpo".to_string(), app(FR, ""))];
    assert_eq!(problems(&files), CLEAN);
}

#[test]
fn an_in_progress_locale_falls_back_to_the_wording_jadpo_ships() {
    let files = routes(&[], FR, "    in_progress: [Locale.fr_fr]\n");
    assert_eq!(problems(&files), CLEAN);
}

#[test]
fn an_app_may_reword_what_jadpo_ships() {
    let files = routes(
        &[(
            "translations/en_gb.jadpo",
            "translations {\n    jadpo: {\n        invalid_input: \"Check the form, then send it again.\"\n    }\n}\n",
        )],
        EN,
        "",
    );
    assert_eq!(problems(&files), CLEAN);
}

#[test]
fn a_member_jadpo_has_no_message_for_is_named_with_the_nearest() {
    let files = routes(
        &[(
            "translations/en_gb.jadpo",
            "translations {\n    jadpo: {\n        invalid_inptu: \"Check the form.\"\n    }\n}\n",
        )],
        EN,
        "",
    );
    let found = problems(&files);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("translations/en_gb.jadpo translation.member_unknown:"),
        "{found:?}"
    );
}

#[test]
fn a_message_of_a_feature_that_is_off_is_an_error_saying_so() {
    let files = routes(
        &[(
            "translations/en_gb.jadpo",
            "translations {\n    jadpo: {\n        magic_link_email: \"Your link\"\n    }\n}\n",
        )],
        EN,
        "",
    );
    let found = problems(&files);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("translations/en_gb.jadpo translation.feature_off:"),
        "{found:?}"
    );
}

#[test]
fn a_group_that_is_not_an_owner_is_an_error() {
    let files = routes(
        &[(
            "translations/en_gb.jadpo",
            "translations {\n    postcodes: {\n        invalid_postcode: \"No such postcode.\"\n    }\n}\n",
        )],
        EN,
        "",
    );
    let found = problems(&files);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("translations/en_gb.jadpo translation.group_unknown:"),
        "{found:?}"
    );
}

#[test]
fn a_member_outside_any_group_is_a_shape_error() {
    let files = routes(
        &[(
            "translations/en_gb.jadpo",
            "translations {\n    invalid_input: \"Check the form.\"\n}\n",
        )],
        EN,
        "",
    );
    let found = problems(&files);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("translations/en_gb.jadpo translation.wording_shape:"),
        "{found:?}"
    );
}

#[test]
fn jadpo_words_may_write_the_app_name_and_no_other_field() {
    let ok = routes(
        &[(
            "translations/en_gb.jadpo",
            "translations {\n    jadpo: {\n        rate_limited: \"{app_name} is busy.\"\n    }\n}\n",
        )],
        EN,
        "",
    );
    assert_eq!(problems(&ok), CLEAN);
    let bad = routes(
        &[(
            "translations/en_gb.jadpo",
            "translations {\n    jadpo: {\n        rate_limited: \"{count} is busy.\"\n    }\n}\n",
        )],
        EN,
        "",
    );
    let found = problems(&bad);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("translation.placeholder_unknown"),
        "{found:?}"
    );
}

#[test]
fn text_direction_is_worded_for_each_value() {
    let ok = routes(
        &[(
            "translations/fr_fr.jadpo",
            &format!(
                "translations {{\n    jadpo: {{\n        text_direction: {{\n            left_to_right: \"ltr\"\n            right_to_left: \"rtl\"\n        }}\n{}    }}\n}}\n",
                REQUESTS
                    .iter()
                    .map(|key| format!("        {key}: \"Texte.\"\n"))
                    .collect::<String>()
            ),
        )],
        FR,
        "",
    );
    assert_eq!(problems(&ok), CLEAN);
    let missing = routes(
        &[(
            "translations/en_gb.jadpo",
            "translations {\n    jadpo: {\n        text_direction: {\n            left_to_right: \"ltr\"\n        }\n    }\n}\n",
        )],
        EN,
        "",
    );
    let found = problems(&missing);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("translation.variant_missing"),
        "{found:?}"
    );
}

#[test]
fn a_root_file_for_a_locale_the_app_does_not_support_is_an_error() {
    let files = routes(&[("translations/de_de.jadpo", &words(&REQUESTS))], EN, "");
    let found = problems(&files);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("translation.locale_unsupported"),
        "{found:?}"
    );
}

// --- the record -----------------------------------------------------------

const SOURCE: &str = "Check the form.";

fn standing(from: &str, wording: &str, record: bool) -> (Vec<String>, bool) {
    let db = Database::default();
    let en = format!(
        "translations {{\n    jadpo: {{\n        invalid_input: \"{SOURCE}\"\n    }}\n}}\n"
    );
    let fr = words(&REQUESTS);
    let files = routes(
        &[
            ("translations/en_gb.jadpo", &en),
            ("translations/fr_fr.jadpo", &fr),
        ],
        FR,
        "",
    );
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let line = render(&[Entry {
        locale: "fr_fr".to_string(),
        member: "jadpo.invalid_input".to_string(),
        from: hash(from),
        wording: hash(wording),
        accepted: false,
    }]);
    let path = record_path(ROOT);
    assert_eq!(path, "translations/sources.jadpo_record");
    let records: Vec<(&str, &str)> = if record {
        vec![(path.as_str(), line.as_str())]
    } else {
        Vec::new()
    };
    let project = support::project_with_records(&db, Layout::Project, &borrowed, &records);
    let found = jadpo_check::check(&db, project)
        .iter()
        .filter(|d| !d.rule.id.starts_with("api."))
        .map(|d| format!("{} {}", d.rule.id, d.message))
        .collect();
    let stale = standings(&db, project, Name::new(&db, ROOT.to_string()))
        .list
        .iter()
        .any(|standing| standing.stale);
    (found, stale)
}

#[test]
fn jadpo_wording_made_from_the_current_default_is_fresh() {
    let (found, stale) = standing(SOURCE, "Texte de invalid_input.", true);
    assert_eq!(found, CLEAN);
    assert!(!stale);
}

#[test]
fn jadpo_wording_made_from_wording_that_changed_is_stale() {
    let (found, stale) = standing("Another wording.", "Texte de invalid_input.", true);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("translation.stale"), "{found:?}");
    assert!(found[0].contains("`jadpo.invalid_input`"), "{found:?}");
    assert!(stale);
}

#[test]
fn jadpo_wording_with_no_line_in_the_record_is_not_stale() {
    let (found, stale) = standing("Another wording.", "x", false);
    assert_eq!(found, CLEAN);
    assert!(!stale);
}

// --- signing in -----------------------------------------------------------

/// A route that needs a caller, signed in with API tokens only.
fn signed_in(supported: &str, extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = vec![
        ("app.jadpo", app(supported, "")),
        (
            "config.jadpo",
            "config {\n    session_signing_key: SigningKey\n}\n".to_string(),
        ),
        (
            "app.jadpo",
            format!(
                "use notes/entities {{ Member }}\nuse notes/types {{ MemberStatus }}\n\nprincipal {{\n    user: {{\n        subject: Member.handle\n        active: Member.status == MemberStatus.active\n    }}\n}}\n\n{}",
                app(supported, "")
            ),
        ),
        (
            "authentication.jadpo",
            "authentication {\n    api_token: {\n        signing_key: config.session_signing_key\n    }\n}\n".to_string(),
        ),
        (
            "policy.jadpo",
            "use notes\n\npolicy {\n    actions: {\n        notes.say_who: [Access.authenticated]\n    }\n}\n".to_string(),
        ),
        (
            "domains/notes/entities/member.jadpo",
            "use notes/types { Handle, MemberStatus }\n\nentity Member {\n    id: Uuid\n    handle: Handle {\n        unique: true\n    }\n    status: MemberStatus\n\n    policy: {\n        create: [Access.public]\n        read: [id]\n        update: [id]\n    }\n}\n".to_string(),
        ),
        (
            "domains/notes/types/handle.jadpo",
            "type Handle = Text {\n    min_length: 3\n    max_length: 40\n}\n".to_string(),
        ),
        (
            "domains/notes/types/member_status.jadpo",
            "type MemberStatus = Enum {\n    active\n    suspended\n}\n".to_string(),
        ),
        (
            "domains/notes/types/said.jadpo",
            "use notes/types { Handle }\n\ntype Said = Output {\n    who: Handle\n}\n".to_string(),
        ),
        (
            "domains/notes/actions/say_who.jadpo",
            "use notes/types { Said }\n\n/// Says who is signed in.\naction say_who(caller: Principal.user) -> Said {\n    return Said {\n        who: caller.subject\n    }\n}\n".to_string(),
        ),
        (
            "domains/notes/routes/echo.jadpo",
            "use notes/types { Said }\nuse notes/actions { say_who }\n\nroutes Echo {\n    prefix: /echo\n\n    POST / {\n        output: Said\n        run: say_who(current_principal.user)\n    }\n}\n".to_string(),
        ),
    ]
    .into_iter()
    .map(|(path, text)| (path.to_string(), text))
    .collect();
    // The first app.jadpo above is replaced by the one with the principal.
    files.remove(0);
    for (path, text) in extra {
        files.push(((*path).to_string(), (*text).to_string()));
    }
    files
}

#[test]
fn a_route_that_needs_a_caller_switches_on_the_failures_of_signing_in() {
    // English is shipped, so nothing is asked for.
    assert_eq!(problems(&signed_in(EN, &[])), CLEAN);
    // French is not: the sign-in failures join the request ones.
    let found = reasons(&signed_in(FR, &[]));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "translation.jadpo_missing");
    for key in [
        "authentication_required",
        "invalid_credentials",
        "ambiguous_credentials",
        "not_permitted",
        "authentication_unavailable",
        "user_disabled",
    ] {
        assert!(found[0].1.contains(&format!("`{key}`")), "{key}: {found:?}");
    }
    // API tokens only: no service key, no browser session.
    for key in ["service_disabled", "csrf_rejected"] {
        assert!(
            !found[0].1.contains(&format!("`{key}`")),
            "{key}: {found:?}"
        );
    }
}

#[test]
fn only_the_root_translations_folder_may_hold_language_files() {
    let files = routes(
        &[("translations/fr_fr.jadpo", "type Stray = Text {}\n")],
        FR,
        "",
    );
    let found = problems(&files);
    assert!(
        found
            .iter()
            .any(|line| line.contains("layout.wrong_folder")),
        "{found:?}"
    );
}

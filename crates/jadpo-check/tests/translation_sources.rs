//! Stale translations and review (area 10, chunk 5.4d): each `translations/`
//! folder's `sources.jadpo_record` says which default wording a translation
//! was made from and who accepted it; a translation made from wording that
//! has changed is an error, unless its locale is in progress, and
//! `translations: { review: ... }` in `app.jadpo` is read.

use jadpo_check::lookups::Name;
use jadpo_check::translations::sources::{
    Entry, HEADER, hash, parse, record_path, render, standings,
};
use jadpo_db::{Database, Layout};

use crate::support;

const APP: &str = "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.fr_fr]\n}\n";

const MESSAGE: &str = "/// Says hello.\ntype Hello = Message {\n    name: Text\n}\n";

const EN: &str = "translations {\n    hello: \"Hello, {name}.\"\n}\n";
const FR: &str = "translations {\n    hello: \"Bonjour, {name}.\"\n}\n";

fn files<'a>(app: &'a str, en: &'a str, fr: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("app.jadpo", app),
        ("domains/greeting/translations/hello.jadpo", MESSAGE),
        ("domains/greeting/translations/en_gb.jadpo", en),
        ("domains/greeting/translations/fr_fr.jadpo", fr),
    ]
}

fn lines(app: &str, en: &str, fr: &str, record: Option<&str>) -> Vec<String> {
    let db = Database::default();
    let files = files(app, en, fr);
    let path = record_path("greeting");
    let records: Vec<(&str, &str)> = record
        .map(|text| (path.as_str(), text))
        .into_iter()
        .collect();
    let project = support::project_with_records(&db, Layout::Project, &files, &records);
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| format!("{} {}", d.rule.id, d.message))
        .collect()
}

/// A record saying `fr_fr.hello` was made from `from` and is worded `fr`.
fn recorded(from: &str, wording: &str, accepted: bool) -> String {
    render(&[Entry {
        locale: "fr_fr".to_string(),
        member: "hello".to_string(),
        from: hash(from),
        wording: hash(wording),
        accepted,
    }])
}

const CLEAN: &[&str] = &[];

#[test]
fn a_record_is_written_and_read_back() {
    let entries = vec![
        Entry {
            locale: "fr_fr".to_string(),
            member: "hello".to_string(),
            from: hash("a"),
            wording: hash("b"),
            accepted: true,
        },
        Entry {
            locale: "de_de".to_string(),
            member: "hello".to_string(),
            from: hash("a"),
            wording: hash("c"),
            accepted: false,
        },
    ];
    let text = render(&entries);
    assert!(text.starts_with(&format!(
        "{HEADER}\nrecord translations 1\n\ntranslation de_de.hello from "
    )));
    assert!(text.ends_with(" accepted\n"), "{text}");
    let mut sorted = entries.clone();
    sorted.reverse();
    assert_eq!(parse(&text), Ok(sorted), "sorted by locale, then member");
    assert_eq!(hash("a").len(), 12);
    assert_ne!(hash("a"), hash("b"));
}

#[test]
fn a_record_the_compiler_cannot_read_is_refused_with_its_line() {
    assert!(parse("").is_err());
    assert!(parse(&format!("{HEADER}\nrecord translations 2\n")).is_err());
    assert_eq!(
        parse(&format!(
            "{HEADER}\nrecord translations 1\n\ntranslation fr_fr.hello from 1 wording 2\n"
        ))
        .map_err(|(line, _)| line),
        Err(3)
    );
}

#[test]
fn a_project_with_no_record_has_nothing_stale() {
    assert_eq!(lines(APP, EN, FR, None), CLEAN);
}

#[test]
fn a_translation_recorded_from_the_current_wording_is_fresh() {
    let record = recorded("Hello, {name}.", "Bonjour, {name}.", false);
    assert_eq!(lines(APP, EN, FR, Some(&record)), CLEAN);
}

#[test]
fn a_translation_made_from_wording_that_changed_is_stale() {
    let record = recorded("Hello, {name}.", "Bonjour, {name}.", true);
    let en = EN.replace("Hello,", "Good day,");
    assert_eq!(
        lines(APP, &en, FR, Some(&record)),
        [
            "translation.stale `hello` in `fr_fr` was translated from `en_gb` wording that has since changed"
        ]
    );
}

#[test]
fn a_stale_translation_of_a_locale_in_progress_is_not_an_error() {
    let record = recorded("Hello, {name}.", "Bonjour, {name}.", true);
    let en = EN.replace("Hello,", "Good day,");
    let app = APP.replace(
        "supported: [Locale.en_gb, Locale.fr_fr]",
        "supported: [Locale.en_gb, Locale.fr_fr]\n    in_progress: [Locale.fr_fr]",
    );
    assert_eq!(lines(&app, &en, FR, Some(&record)), CLEAN);
}

#[test]
fn reordering_a_blocks_values_changes_no_hash() {
    let message = "/// Says hello.\ntype Hello = Message {\n    count: PluralCount\n}\n";
    let en = "translations {\n    hello: {\n        one: \"{count} hello\"\n        other: \"{count} hellos\"\n    }\n}\n";
    let swapped = "translations {\n    hello: {\n        other: \"{count} hellos\"\n        one: \"{count} hello\"\n    }\n}\n";
    let fr = "translations {\n    hello: {\n        one: \"{count} bonjour\"\n        many: \"{count} de bonjours\"\n        other: \"{count} bonjours\"\n    }\n}\n";
    let db = Database::default();
    let make = |en: &str| {
        let files = vec![
            ("app.jadpo", APP),
            ("domains/greeting/translations/hello.jadpo", message),
            ("domains/greeting/translations/en_gb.jadpo", en),
            ("domains/greeting/translations/fr_fr.jadpo", fr),
        ];
        let project = support::project_with_records(&db, Layout::Project, &files, &[]);
        standings(&db, project, Name::new(&db, "greeting".to_string()))
            .list
            .iter()
            .map(|s| s.default_hash.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(make(en), make(swapped));
}

#[test]
fn an_unreadable_record_is_an_error_pointing_at_its_line() {
    let found = lines(APP, EN, FR, Some("not a record\n"));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("translation.sources_invalid "),
        "{found:?}"
    );
}

#[test]
fn what_is_unaccepted_is_what_is_new_or_changed() {
    let db = Database::default();
    let path = record_path("greeting");
    let standing = |record: Option<String>, fr: &str| {
        let files = files(APP, EN, fr);
        let records: Vec<(&str, &str)> = record
            .as_deref()
            .map(|text| (path.as_str(), text))
            .into_iter()
            .collect();
        let project = support::project_with_records(&db, Layout::Project, &files, &records);
        standings(&db, project, Name::new(&db, "greeting".to_string()))
            .list
            .clone()
    };
    // No line: new, so unaccepted.
    let new = standing(None, FR);
    assert!(new[0].changed() && !new[0].accepted());
    // Accepted exactly this wording.
    let accepted = standing(
        Some(recorded("Hello, {name}.", "Bonjour, {name}.", true)),
        FR,
    );
    assert!(!accepted[0].changed() && accepted[0].accepted());
    // Reworded since: changed, so unaccepted though the line says accepted.
    let reworded = standing(
        Some(recorded("Hello, {name}.", "Bonjour, {name}.", true)),
        &FR.replace("Bonjour", "Salut"),
    );
    assert!(reworded[0].changed() && !reworded[0].accepted());
    // Recorded but not accepted.
    let waiting = standing(
        Some(recorded("Hello, {name}.", "Bonjour, {name}.", false)),
        FR,
    );
    assert!(!waiting[0].changed() && !waiting[0].accepted());
}

// --- review in app.jadpo ---------------------------------------------------------------

fn with_review(option: &str) -> String {
    format!(
        "application {{\n    translations: {{\n        {option}\n    }}\n}}\n\nlocales {{\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.fr_fr]\n}}\n"
    )
}

#[test]
fn review_can_be_required_or_none() {
    assert_eq!(
        lines(
            &with_review("review: TranslationReview.required"),
            EN,
            FR,
            None
        ),
        CLEAN
    );
    assert_eq!(
        lines(&with_review("review: TranslationReview.none"), EN, FR, None),
        CLEAN
    );
}

#[test]
fn review_is_read_from_the_application() {
    let db = Database::default();
    let required = with_review("review: TranslationReview.required");
    let project =
        support::project_with_records(&db, Layout::Project, &files(&required, EN, FR), &[]);
    assert!(jadpo_check::translations::sources::review(&db, project).required);
    let project = support::project_with_records(&db, Layout::Project, &files(APP, EN, FR), &[]);
    assert!(!jadpo_check::translations::sources::review(&db, project).required);
}

#[test]
fn an_option_review_does_not_take_is_an_error() {
    assert_eq!(
        lines(
            &with_review("reviews: TranslationReview.required"),
            EN,
            FR,
            None
        ),
        ["translation.option_unknown `reviews` isn't an option of `translations`"]
    );
}

#[test]
fn a_review_that_is_not_a_translation_review_is_an_error() {
    assert_eq!(
        lines(
            &with_review("review: TranslationReview.always"),
            EN,
            FR,
            None
        ),
        ["translation.review_invalid `review:` isn't a `TranslationReview`"]
    );
    assert_eq!(
        lines(&with_review("review: true"), EN, FR, None),
        ["translation.review_invalid `review:` isn't a `TranslationReview`"]
    );
}

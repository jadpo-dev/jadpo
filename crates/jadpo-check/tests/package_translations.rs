//! Library packages' translations (area 10, "Jadpo's and packages' text";
//! area 11, "The package format"; chunk 7.1b3): a library package's
//! `translations/` holds its messages and its failures' wording, laid out
//! as a domain's; the app calls a message as
//! `Postcodes.translations.<message>(...)`, words what the package doesn't
//! in the root `translations/<locale>.jadpo` under the package's name, and
//! may reword what it does. The package's messages are in its surface.

use crate::support;

use jadpo_db::{Database, Layout, Locked, Source, package_digest, write_lock};
use support::project_with_records;

const APP_EN: (&str, &str) = (
    "app.jadpo",
    "application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
",
);

const APP_EN_DE: (&str, &str) = (
    "app.jadpo",
    "application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.de_de]
}
",
);

const INDEX: (&str, &str) = (
    "packages/postcodes/v2/package.jadpo",
    "/// UK postcodes: reading them as people write them.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: \"1.0\"
}
",
);

const INVALID: (&str, &str) = (
    "packages/postcodes/v2/failures/invalid_postcode.jadpo",
    "/// The text isn't a UK postcode.
public type InvalidPostcode = Failure {
    kind: FailureKind.invalid_value
}
",
);

const HINT: (&str, &str) = (
    "packages/postcodes/v2/translations/postcode_hint.jadpo",
    "/// Shown under a postcode field.
public type PostcodeHint = Message {}
",
);

const FOUND: (&str, &str) = (
    "packages/postcodes/v2/translations/postcodes_found.jadpo",
    "/// How many postcodes a search found.
public type PostcodesFound = Message {
    count: PluralCount
}
",
);

const EN: (&str, &str) = (
    "packages/postcodes/v2/translations/en_gb.jadpo",
    "translations {
    postcode_hint: \"Write it as it is on a letter, such as SW1A 1AA.\"
    postcodes_found: {
        one: \"{count} postcode\"
        other: \"{count} postcodes\"
    }
    invalid_postcode: \"That isn't a UK postcode.\"
}
",
);

/// The app's actions that show the package's messages.
const SHOW: (&str, &str) = (
    "domains/intake/actions/postcode_text.jadpo",
    "use packages/postcodes

/// What the postcode field says, in the reader's language.
action postcode_text(reader_locale: Locale) -> PresentationText {
    return Postcodes.translations.postcode_hint(locale: reader_locale)
}
",
);

const COUNT: (&str, &str) = (
    "domains/intake/actions/found_text.jadpo",
    "use intake/types { PostcodeCount }
use packages/postcodes

/// How many postcodes a search found, in the reader's language.
action found_text(found: PostcodeCount, reader_locale: Locale) -> PresentationText {
    return Postcodes.translations.postcodes_found(count: found, locale: reader_locale)
}
",
);

const POSTCODE_COUNT: (&str, &str) = (
    "domains/intake/types/postcode_count.jadpo",
    "/// How many postcodes.
type PostcodeCount = Int {
    min: 0
}
",
);

/// The German the package doesn't have, in the app's root file.
const ROOT_DE: (&str, &str) = (
    "translations/de_de.jadpo",
    "translations {
    postcodes: {
        postcode_hint: \"Schreiben Sie sie wie auf einem Brief, etwa SW1A 1AA.\"
        postcodes_found: {
            one: \"{count} Postleitzahl\"
            other: \"{count} Postleitzahlen\"
        }
    }
}
",
);

fn package() -> Vec<(&'static str, &'static str)> {
    vec![INDEX, INVALID, HINT, FOUND, EN, POSTCODE_COUNT]
}

/// Each problem as `path rule: message`, with the package locked as
/// installed.
fn problems(files: &[(&str, &str)]) -> Vec<String> {
    let prefix = "packages/postcodes/v2/";
    let held: Vec<(&str, &[u8])> = files
        .iter()
        .filter_map(|(path, text)| Some((path.strip_prefix(prefix)?, text.as_bytes())))
        .collect();
    let lock = write_lock(&[Locked {
        package: "Postcodes.v2".to_string(),
        source: Source::Path {
            version: "2.0.0".to_string(),
            path: "../postcodes".to_string(),
            digest: package_digest(&held),
        },
    }]);
    let db = Database::default();
    let project = project_with_records(&db, Layout::Project, files, &[("jadpo.lock", &lock)]);
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| {
            let path = d
                .primary
                .as_ref()
                .map_or("", |label| label.span.path.as_str());
            format!("{path} {}: {}", d.rule.id, d.message)
        })
        .collect()
}

fn with(
    app: (&'static str, &'static str),
    extra: &[(&'static str, &'static str)],
) -> Vec<(&'static str, &'static str)> {
    let mut files = vec![app];
    files.extend(package());
    files.extend_from_slice(extra);
    files
}

fn has(found: &[String], wanted: &str) {
    assert!(
        found.iter().any(|problem| problem.contains(wanted)),
        "no problem with {wanted:?} in {found:#?}"
    );
}

#[test]
fn a_library_package_s_messages_are_called_through_its_name() {
    assert_eq!(
        problems(&with(APP_EN, &[SHOW, COUNT])),
        Vec::<String>::new()
    );
}

#[test]
fn a_locale_the_package_doesn_t_word_is_the_app_s_to_word() {
    let found = problems(&with(APP_EN_DE, &[SHOW, COUNT]));
    assert_eq!(found.len(), 1, "{found:#?}");
    has(
        &found,
        "app.jadpo translation.file_missing: `Locale.de_de` is supported but package `Postcodes` has no wording in it for `postcode_hint` and `postcodes_found`",
    );
    // The root file words both, under the package's name.
    assert_eq!(
        problems(&with(APP_EN_DE, &[SHOW, COUNT, ROOT_DE])),
        Vec::<String>::new()
    );
    // A root file that words one of them still lacks the other.
    let partial = (
        "translations/de_de.jadpo",
        "translations {
    postcodes: {
        postcode_hint: \"Schreiben Sie sie wie auf einem Brief.\"
    }
}
",
    );
    let found = problems(&with(APP_EN_DE, &[SHOW, COUNT, partial]));
    assert_eq!(found.len(), 1, "{found:#?}");
    has(
        &found,
        "translations/de_de.jadpo translation.wording_missing: `postcodes.postcodes_found` has no wording in `translations/de_de.jadpo`",
    );
}

#[test]
fn a_locale_in_progress_falls_back_to_the_package_s_wording() {
    let app = (
        "app.jadpo",
        "application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.de_de]
    in_progress: [Locale.de_de]
}
",
    );
    assert_eq!(problems(&with(app, &[SHOW, COUNT])), Vec::<String>::new());
}

#[test]
fn the_root_file_s_group_is_checked_against_the_package_s_messages() {
    let root = (
        "translations/de_de.jadpo",
        "translations {
    postcodes: {
        postcode_hnt: \"Schreiben Sie sie wie auf einem Brief.\"
        postcodes_found: {
            one: \"{cuont} Postleitzahl\"
            other: \"{count} Postleitzahlen\"
        }
    }
}
",
    );
    let found = problems(&with(APP_EN_DE, &[SHOW, COUNT, root]));
    has(
        &found,
        "translations/de_de.jadpo translation.member_unknown: `postcode_hnt` words nothing of `Postcodes`'s",
    );
    has(
        &found,
        "translations/de_de.jadpo translation.placeholder_unknown: `{cuont}` isn't a field of `Postcodes.PostcodesFound`",
    );
    // The hint is then missing, since the member that was meant to word it
    // names nothing.
    has(
        &found,
        "translation.wording_missing: `postcodes.postcode_hint` has no wording in `translations/de_de.jadpo`",
    );
}

#[test]
fn a_plural_form_the_root_file_leaves_out_is_missing() {
    let root = (
        "translations/de_de.jadpo",
        "translations {
    postcodes: {
        postcode_hint: \"Schreiben Sie sie wie auf einem Brief.\"
        postcodes_found: {
            other: \"{count} Postleitzahlen\"
        }
    }
}
",
    );
    let found = problems(&with(APP_EN_DE, &[SHOW, COUNT, root]));
    has(
        &found,
        "translations/de_de.jadpo translation.variant_missing: `postcodes_found` in `translations/de_de.jadpo` has no wording for the plural form `one`",
    );
}

#[test]
fn a_group_that_names_no_package_is_unknown() {
    let root = (
        "translations/en_gb.jadpo",
        "translations {
    postcode: {
        postcode_hint: \"Type the postcode.\"
    }
}
",
    );
    let found = problems(&with(APP_EN, &[SHOW, COUNT, root]));
    has(
        &found,
        "translations/en_gb.jadpo translation.group_unknown: `postcode` isn't a group Jadpo reads",
    );
}

#[test]
fn the_package_s_own_wording_is_checked_where_it_is_written() {
    let en = (
        "packages/postcodes/v2/translations/en_gb.jadpo",
        "translations {
    postcode_hint: \"Write it as it is on a letter.\"
    postcodes_found: {
        one: \"{count} postcode\"
        other: \"{total} postcodes\"
    }
}
",
    );
    let mut files = vec![
        APP_EN,
        INDEX,
        INVALID,
        HINT,
        FOUND,
        en,
        SHOW,
        COUNT,
        POSTCODE_COUNT,
    ];
    let found = problems(&files);
    has(
        &found,
        "packages/postcodes/v2/translations/en_gb.jadpo translation.placeholder_unknown: `{total}` isn't a field of `Postcodes.PostcodesFound`",
    );
    // A language the app doesn't support is the package's own business.
    files.retain(|(path, _)| *path != en.0);
    files.push(EN);
    files.push((
        "packages/postcodes/v2/translations/fr_fr.jadpo",
        "translations {
    postcode_hint: \"Écrivez-le comme sur une lettre.\"
    postcodes_found: {
        one: \"{count} code postal\"
        many: \"{count} codes postaux\"
        other: \"{count} codes postaux\"
    }
}
",
    ));
    assert_eq!(problems(&files), Vec::<String>::new());
    // A file named for no locale is still a mistake.
    files.push((
        "packages/postcodes/v2/translations/english.jadpo",
        "translations {\n}\n",
    ));
    has(
        &problems(&files),
        "packages/postcodes/v2/translations/english.jadpo translation.file_name_invalid: `packages/postcodes/v2/translations/english.jadpo` isn't named for a locale",
    );
}

#[test]
fn a_call_names_a_public_message_of_the_package() {
    let private = (
        "packages/postcodes/v2/translations/postcode_hint.jadpo",
        "/// Shown under a postcode field.
type PostcodeHint = Message {}
",
    );
    let files = vec![
        APP_EN,
        INDEX,
        INVALID,
        private,
        FOUND,
        EN,
        SHOW,
        COUNT,
        POSTCODE_COUNT,
    ];
    has(
        &problems(&files),
        "domains/intake/actions/postcode_text.jadpo package.not_public: `Postcodes.translations.postcode_hint` isn't public",
    );
    let unknown = (
        "domains/intake/actions/postcode_text.jadpo",
        "use packages/postcodes

/// What the postcode field says.
action postcode_text(reader_locale: Locale) -> PresentationText {
    return Postcodes.translations.postcode_hnt(locale: reader_locale)
}
",
    );
    has(
        &problems(&with(APP_EN, &[unknown])),
        "translation.message_unknown: `Postcodes.translations.postcode_hnt` is not a message",
    );
    let through_translations = (
        "domains/intake/actions/postcode_text.jadpo",
        "use packages/postcodes

/// What the postcode field says.
action postcode_text(reader_locale: Locale) -> PresentationText {
    return translations.postcodes.postcode_hint(locale: reader_locale)
}
",
    );
    has(
        &problems(&with(APP_EN, &[through_translations])),
        "translation.message_unknown: `translations.postcodes.postcode_hint` is not a message",
    );
}

#[test]
fn a_package_s_messages_and_locales_are_in_its_surface() {
    let db = Database::default();
    let project = project_with_records(&db, Layout::Project, &with(APP_EN, &[]), &[]);
    let surface = jadpo_check::surface::text(&db, project, "packages/postcodes/v2", Some("2.0.0"))
        .expect("a surface");
    assert!(
        surface.contains(
            "package Postcodes.v2
    /// UK postcodes: reading them as people write them.
    file package.jadpo
    kind library
    version 2.0.0
    licence mit
    jadpo 1.0
    locale en_gb
"
        ),
        "{surface}"
    );
    assert!(
        surface.ends_with(
            "message translations.postcode_hint
    /// Shown under a postcode field.
    file translations/postcode_hint.jadpo

message translations.postcodes_found
    /// How many postcodes a search found.
    file translations/postcodes_found.jadpo
    count: PluralCount

failure InvalidPostcode
    /// The text isn't a UK postcode.
    file failures/invalid_postcode.jadpo
    kind invalid_value
    code invalid_postcode
"
        ),
        "{surface}"
    );
}

#[test]
fn a_message_s_fields_and_locales_decide_the_version() {
    use jadpo_check::publishing::version;
    let surface = |files: &[(&str, &str)]| {
        let db = Database::default();
        let project = project_with_records(&db, Layout::Project, files, &[]);
        jadpo_check::surface::text(&db, project, "packages/postcodes/v2", None).expect("surface")
    };
    let before = surface(&with(APP_EN, &[]));
    // A new locale is the next minor version.
    let french = (
        "packages/postcodes/v2/translations/fr_fr.jadpo",
        "translations {
    postcode_hint: \"Écrivez-le comme sur une lettre.\"
    postcodes_found: {
        one: \"{count} code postal\"
        many: \"{count} codes postaux\"
        other: \"{count} codes postaux\"
    }
}
",
    );
    let more = surface(&with(APP_EN, &[french]));
    let bump = version("v2", Some(("2.0.0", &before)), &more).expect("allowed");
    assert_eq!(bump.version, "2.1.0");
    // Dropping a locale breaks an app that shows it.
    let dropped = version("v2", Some(("2.1.0", &more)), &before).expect_err("breaking");
    assert_eq!(
        dropped,
        vec![("changed".to_string(), "package Postcodes.v2".to_string())]
    );
    // A message's new field breaks the app's call and its rewording.
    let fields = (
        "packages/postcodes/v2/translations/postcode_hint.jadpo",
        "/// Shown under a postcode field.
public type PostcodeHint = Message {
    example: Text
}
",
    );
    let en = (
        "packages/postcodes/v2/translations/en_gb.jadpo",
        "translations {
    postcode_hint: \"Write it as it is on a letter, such as {example}.\"
    postcodes_found: {
        one: \"{count} postcode\"
        other: \"{count} postcodes\"
    }
}
",
    );
    let files = vec![APP_EN, INDEX, INVALID, fields, FOUND, en];
    let changed = version("v2", Some(("2.0.0", &before)), &surface(&files)).expect_err("breaking");
    assert_eq!(
        changed,
        vec![(
            "changed".to_string(),
            "message translations.postcode_hint".to_string()
        )]
    );
}

#[test]
fn a_package_rewording_its_own_text_makes_the_app_s_translation_stale() {
    use jadpo_check::translations::sources::{Entry, hash, render};
    let german = "Schreiben Sie sie wie auf einem Brief, etwa SW1A 1AA.";
    let record = |from: &str| {
        render(&[Entry {
            locale: "de_de".to_string(),
            member: "postcodes.postcode_hint".to_string(),
            from: hash(from),
            wording: hash(german),
            accepted: false,
        }])
    };
    let check = |from: &str| {
        let files = with(APP_EN_DE, &[SHOW, COUNT, ROOT_DE]);
        let prefix = "packages/postcodes/v2/";
        let held: Vec<(&str, &[u8])> = files
            .iter()
            .filter_map(|(path, text)| Some((path.strip_prefix(prefix)?, text.as_bytes())))
            .collect();
        let lock = write_lock(&[Locked {
            package: "Postcodes.v2".to_string(),
            source: Source::Path {
                version: "2.0.0".to_string(),
                path: "../postcodes".to_string(),
                digest: package_digest(&held),
            },
        }]);
        let sources = record(from);
        let db = Database::default();
        let project = project_with_records(
            &db,
            Layout::Project,
            &files,
            &[
                ("jadpo.lock", &lock),
                ("translations/sources.jadpo_record", &sources),
            ],
        );
        jadpo_check::check(&db, project)
            .iter()
            .map(|d| format!("{}: {}", d.rule.id, d.message))
            .collect::<Vec<_>>()
    };
    // Made from the package's English as it is today.
    assert_eq!(
        check("Write it as it is on a letter, such as SW1A 1AA."),
        Vec::<String>::new()
    );
    // Made from English the package has since reworded.
    assert_eq!(
        check("Write it as on an envelope."),
        vec![
            "translation.stale: `postcodes.postcode_hint` in `de_de` was translated from `en_gb` wording that has since changed"
                .to_string()
        ]
    );
}

#[test]
fn a_package_failure_a_route_answers_with_is_worded_in_every_locale() {
    let uk = (
        "packages/postcodes/v2/libraries/uk.jadpo",
        "/// Reading UK postcodes.
public library uk {
    /// Reads a postcode in upper case.
    function tidy(entered: EnteredPostcode) fails InvalidPostcode -> EnteredPostcode {
        return attempt EnteredPostcode(text.upper(entered)) {
            invalid: InvalidPostcode
        }
    }
}
",
    );
    let entered_postcode = (
        "packages/postcodes/v2/types/entered_postcode.jadpo",
        "/// A postcode as someone typed it.
public type EnteredPostcode = Text {
    max_length: 20
}
",
    );
    let read = (
        "domains/intake/actions/read_postcode.jadpo",
        "use intake/types { EnteredText, PostcodeAnswer }
use packages/postcodes

/// Reads a customer's postcode.
action read_postcode(entered: EnteredText) fails Postcodes.InvalidPostcode -> PostcodeAnswer {
    var tidied: Postcodes.EnteredPostcode = attempt Postcodes.uk.tidy(Postcodes.EnteredPostcode(entered))
    return PostcodeAnswer {
        postcode: EnteredText(tidied)
    }
}
",
    );
    let entered = (
        "domains/intake/types/entered_text.jadpo",
        "/// What someone typed.
type EnteredText = Text {
    max_length: 20
}
",
    );
    let request = (
        "domains/intake/types/postcode_request.jadpo",
        "use intake/types { EnteredText }

/// A postcode to read.
type PostcodeRequest = Input {
    entered: EnteredText
}
",
    );
    let answer = (
        "domains/intake/types/postcode_answer.jadpo",
        "use intake/types { EnteredText }

/// A postcode read.
type PostcodeAnswer = Output {
    postcode: EnteredText
}
",
    );
    let routes = (
        "domains/intake/routes/postcode_checks.jadpo",
        "use intake/types { PostcodeAnswer, PostcodeRequest }
use intake/actions { read_postcode }

routes PostcodeChecks {
    prefix: /postcode-checks

    POST / {
        authentication: RouteAuthentication.none
        input: PostcodeRequest
        output: PostcodeAnswer
        run: read_postcode(input.entered)
    }
}
",
    );
    let policy = (
        "policy.jadpo",
        "use intake

policy {
    actions: {
        intake.read_postcode: [Access.public]
    }
}
",
    );
    let found = problems(&with(
        APP_EN_DE,
        &[
            SHOW,
            COUNT,
            ROOT_DE,
            uk,
            entered_postcode,
            read,
            entered,
            request,
            answer,
            routes,
            policy,
        ],
    ));
    // The API's record and Jadpo's own German are beside the point here.
    let found: Vec<String> = found
        .into_iter()
        .filter(|problem| {
            !problem.contains(" record.") && !problem.contains("translation.jadpo_missing")
        })
        .collect();
    assert_eq!(found.len(), 1, "{found:#?}");
    has(
        &found,
        "translations/de_de.jadpo translation.wording_missing: `postcodes.invalid_postcode` has no wording in `translations/de_de.jadpo`",
    );
}

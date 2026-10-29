//! Locales (area 10, chunk 5.3): `locales { default: ... supported: [...] }`
//! says which locales formatting and translations accept. The plan it
//! becomes, the errors each wrong spelling gives, and what a call that
//! writes text for a person checks against it.

use jadpo_db::{Database, Layout};
use jadpo_std::{LocalePlan, locale_ordinal};

use crate::support;

fn rules(locales: &str) -> Vec<String> {
    support::rules(Layout::Loose, &[("main.jadpo", locales)])
}

fn plan(locales: &str) -> Option<LocalePlan> {
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", locales)]);
    jadpo_check::locales::locales(&db, project).plan.clone()
}

fn ordinal(variant: &str) -> u32 {
    locale_ordinal(variant).unwrap_or(u32::MAX)
}

const CLEAN: &[&str] = &[];

const PAGE: &str = "locales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.en_us, Locale.fr_fr]\n}\n";

// --- what it reads ------------------------------------------------------------------

#[test]
fn the_page_example_declares_cleanly() {
    assert_eq!(rules(PAGE), CLEAN);
    assert_eq!(
        plan(PAGE),
        Some(LocalePlan {
            default: ordinal("en_gb"),
            supported: vec![ordinal("en_gb"), ordinal("en_us"), ordinal("fr_fr")],
            in_progress: Vec::new(),
        })
    );
    assert_eq!(plan(""), None, "nothing declared");
}

#[test]
fn a_right_to_left_and_a_script_locale_are_locales_like_any() {
    let source = "locales {\n    default: Locale.ar_eg\n    supported: [Locale.ar_eg, Locale.he, Locale.zh_hans]\n}\n";
    assert_eq!(rules(source), CLEAN);
}

// --- the two options ------------------------------------------------------------------

#[test]
fn a_missing_default_or_supported_is_its_own_error() {
    assert_eq!(
        rules("locales {\n    supported: [Locale.en_gb]\n}\n"),
        ["syntax.locales_default_required"]
    );
    assert_eq!(
        rules("locales {\n    default: Locale.en_gb\n}\n"),
        ["syntax.locales_supported_required"]
    );
    assert_eq!(
        rules("locales {\n}\n"),
        [
            "syntax.locales_default_required",
            "syntax.locales_supported_required"
        ]
    );
}

#[test]
fn there_is_no_unsupported_setting() {
    assert_eq!(
        rules(
            "locales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb]\n    unsupported: Locale.fr_fr\n}\n"
        ),
        ["syntax.locales_unknown_option"]
    );
    assert_eq!(
        rules(
            "locales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb]\n    suported: [Locale.fr_fr]\n}\n"
        ),
        ["syntax.locales_unknown_option"]
    );
}

#[test]
fn an_option_given_twice_is_an_error() {
    assert_eq!(
        rules(
            "locales {\n    default: Locale.en_gb\n    default: Locale.fr_fr\n    supported: [Locale.en_gb, Locale.fr_fr]\n}\n"
        ),
        ["syntax.locales_duplicate_option"]
    );
}

// --- the locales ----------------------------------------------------------------------

#[test]
fn a_locale_is_a_name_not_text() {
    assert_eq!(
        rules("locales {\n    default: \"en-GB\"\n    supported: [Locale.en_gb]\n}\n"),
        ["type.locale_invalid"]
    );
    assert_eq!(
        rules(
            "locales {\n    default: Locale.en_gb\n    supported: [\"en-GB\", Locale.fr_fr]\n}\n"
        ),
        ["type.locale_invalid"]
    );
    assert_eq!(
        rules(
            "locales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.xx_yy]\n}\n"
        ),
        ["type.locale_invalid"]
    );
    assert_eq!(
        rules("locales {\n    default: Locale.en_gb\n    supported: Locale.en_gb\n}\n"),
        ["type.locale_invalid"]
    );
    assert_eq!(
        rules("locales {\n    default: Zone.utc\n    supported: [Locale.en_gb]\n}\n"),
        ["type.locale_invalid"]
    );
    assert_eq!(
        plan("locales {\n    default: \"en-GB\"\n    supported: [Locale.en_gb]\n}\n"),
        None
    );
}

#[test]
fn a_locale_listed_twice_is_an_error() {
    assert_eq!(
        rules(
            "locales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.en_gb]\n}\n"
        ),
        ["type.locale_duplicate"]
    );
}

#[test]
fn the_default_is_one_of_the_supported() {
    assert_eq!(
        rules("locales {\n    default: Locale.fr_fr\n    supported: [Locale.en_gb]\n}\n"),
        ["type.locale_default_unsupported"]
    );
    assert_eq!(
        rules("locales {\n    default: Locale.fr_fr\n    supported: []\n}\n"),
        ["type.locale_default_unsupported"]
    );
}

#[test]
fn a_second_declaration_is_an_error() {
    let source = format!(
        "{PAGE}\nlocales {{\n    default: Locale.en_us\n    supported: [Locale.en_us]\n}}\n"
    );
    assert_eq!(rules(&source), ["semantic.multiple_locale_declarations"]);
}

// --- what formatting accepts -----------------------------------------------------------

const TYPES: &str = "type OrderedAt = Instant {}\n\n";

fn formats(locales: &str, locale: &str) -> Vec<String> {
    let source = format!(
        "{locales}\n{TYPES}function label(value: OrderedAt, reader_locale: Locale) -> PresentationText {{\n    return text.format(value, zone: Zone.utc, locale: {locale}, format: TimeFormat.date_long)\n}}\n"
    );
    rules(&source)
}

#[test]
fn a_locale_written_out_is_one_the_application_supports() {
    assert_eq!(formats(PAGE, "Locale.fr_fr"), CLEAN);
    assert_eq!(
        formats(PAGE, "reader_locale"),
        CLEAN,
        "a Locale passed in is read as a supported one"
    );
    assert_eq!(formats(PAGE, "Locale.de_de"), ["type.locale_unsupported"]);
    assert_eq!(formats(PAGE, "Locale.ar_eg"), ["type.locale_unsupported"]);
}

#[test]
fn a_locale_in_a_declaration_with_a_problem_is_not_checked_again() {
    assert_eq!(
        formats(
            "locales {\n    default: Locale.fr_fr\n    supported: [Locale.en_gb]\n}\n",
            "Locale.de_de"
        ),
        ["type.locale_default_unsupported"]
    );
}

#[test]
fn files_on_their_own_have_no_locales_to_require() {
    assert_eq!(formats("", "Locale.fr_fr"), CLEAN);
}

/// An application with `locales` or without, a type, and an action that
/// formats.
fn application(locales: &str) -> Vec<(&'static str, String)> {
    vec![
        ("app.jadpo", format!("application {{\n}}\n{locales}")),
        (
            "domains/shop/types/ordered_at.jadpo",
            "/// When an order was placed.\ntype OrderedAt = Instant {}\n".into(),
        ),
        (
            "domains/shop/types/placed_text.jadpo",
            "/// An order's placing, shown.\ntype PlacedText = Output {\n    placed: PresentationText\n}\n".into(),
        ),
        (
            "domains/shop/actions/show_placed.jadpo",
            "use shop/types { OrderedAt, PlacedText }\n\n/// The placing, in the customer's language.\naction show_placed(ordered_at: OrderedAt, reader_locale: Locale) -> PlacedText {\n    return PlacedText {\n        placed: text.format(ordered_at, zone: Zone.utc, locale: reader_locale, format: TimeFormat.date_long)\n    }\n}\n".into(),
        ),
        (
            "domains/shop/actions/show_placed.test.jadpo",
            "test \"the placing is shown\" {\n    assert show_placed(\"2026-10-25T09:30:00Z\", Locale.en_gb).placed == \"25 October 2026\"\n}\n".into(),
        ),
    ]
}

fn project_rules(locales: &str) -> Vec<String> {
    let files = application(locales);
    let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    support::rules(Layout::Project, &files)
}

#[test]
fn an_application_that_formats_declares_its_locales() {
    assert_eq!(
        project_rules("\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb]\n}\n"),
        CLEAN
    );
    assert_eq!(project_rules(""), ["type.temporal_locales_required"]);
}

#[test]
fn a_setting_of_type_locale_starts_with_a_supported_one() {
    let config = "config {\n    ui_locale: Locale {\n        default: Locale.fr_fr\n    }\n}\n";
    assert_eq!(rules(&format!("{PAGE}\n{config}")), CLEAN);
    let config = "config {\n    ui_locale: Locale {\n        default: Locale.de_de\n    }\n}\n";
    assert_eq!(
        rules(&format!("{PAGE}\n{config}")),
        ["type.locale_unsupported"]
    );
    // With no locales declared there is nothing to start from.
    assert_eq!(rules(config), CLEAN);
}

// --- choosing a locale from a request (chunk 5.3c) -----------------------------------------

const DECLARED: &str =
    "\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.fr_fr]\n}\n";

/// An application whose action takes `params` and runs `body`, with the
/// types a request's header and a shown text are.
fn choosing(locales: &str, params: &str, body: &str) -> Vec<String> {
    choosing_to(locales, params, "Locale", body)
}

fn choosing_to(locales: &str, params: &str, result: &str, body: &str) -> Vec<String> {
    let files = [
        ("app.jadpo", format!("application {{\n}}\n{locales}")),
        (
            "domains/shop/types/accept_language.jadpo",
            "/// An `Accept-Language` header.\ntype AcceptLanguage = Text {}\n".into(),
        ),
        (
            "domains/shop/types/placed_at.jadpo",
            "/// When an order was placed.\ntype PlacedAt = Instant {}\n".into(),
        ),
        (
            "domains/shop/types/placed_text.jadpo",
            "/// An order's placing, shown.\ntype PlacedText = Output {\n    placed: PresentationText\n}\n".into(),
        ),
        (
            "domains/shop/actions/choose.jadpo",
            format!(
                "use shop/types {{ AcceptLanguage, PlacedAt, PlacedText }}\n\n/// Whatever the body gives.\naction choose({params}) -> {result} {{\n    {body}\n}}\n"
            ),
        ),
    ];
    let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    support::rules(Layout::Project, &files)
        .into_iter()
        .filter(|rule| rule != "import.unused" && rule != "layout.unused_declaration")
        .collect()
}

#[test]
fn a_locale_is_chosen_from_a_header() {
    for header in ["header: AcceptLanguage?", "header: AcceptLanguage"] {
        assert_eq!(
            choosing(
                DECLARED,
                header,
                "return locale.from_accept_language(header)"
            ),
            CLEAN,
            "{header}"
        );
    }
    assert_eq!(
        choosing(
            DECLARED,
            "header: AcceptLanguage?",
            "return locale.from_accept_language(header: header)"
        ),
        CLEAN,
        "by name"
    );
    assert_eq!(
        choosing(DECLARED, "", "return locale.from_accept_language(none)"),
        CLEAN
    );
    assert_eq!(
        choosing(
            DECLARED,
            "",
            "return locale.from_accept_language(\"fr-FR, en;q=0.5\")"
        ),
        CLEAN
    );
}

#[test]
fn the_locale_chosen_is_one_the_application_supports() {
    // It is a `Locale` like any other, so it goes where one is wanted,
    // formatting included, without being written out and checked.
    assert_eq!(
        choosing_to(
            DECLARED,
            "header: AcceptLanguage?, placed_at: PlacedAt",
            "PresentationText",
            "var chosen: Locale = locale.from_accept_language(header)\n    return text.format(placed_at, zone: Zone.utc, locale: chosen, format: TimeFormat.date_long)"
        ),
        CLEAN
    );
}

#[test]
fn choosing_needs_the_application_s_locales() {
    assert_eq!(
        choosing(
            "",
            "header: AcceptLanguage?",
            "return locale.from_accept_language(header)"
        ),
        ["type.temporal_locales_required"]
    );
}

#[test]
fn files_on_their_own_have_no_locales_to_choose_among() {
    let source = "type AcceptLanguage = Text {}\n\nfunction choose(header: AcceptLanguage?) -> Locale {\n    return locale.from_accept_language(header)\n}\n";
    assert_eq!(rules(source), CLEAN);
}

#[test]
fn the_header_is_text_that_may_be_none_and_is_always_passed() {
    assert_eq!(
        choosing(
            DECLARED,
            "placed_at: PlacedAt",
            "return locale.from_accept_language(placed_at)"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        choosing(DECLARED, "", "return locale.from_accept_language()"),
        ["type.argument_count"]
    );
    assert_eq!(
        choosing(
            DECLARED,
            "header: AcceptLanguage?",
            "return locale.from_accept_language(header, header)"
        ),
        ["type.argument_count"]
    );
    assert_eq!(
        choosing(
            DECLARED,
            "header: AcceptLanguage?",
            "return locale.from_accept_language(language: header)"
        ),
        ["type.named_argument_unknown"]
    );
}

#[test]
fn the_choice_is_a_locale_and_nothing_else() {
    let files = [
        ("app.jadpo", format!("application {{\n}}\n{DECLARED}")),
        (
            "domains/shop/types/accept_language.jadpo",
            "/// An `Accept-Language` header.\ntype AcceptLanguage = Text {}\n".into(),
        ),
        (
            "domains/shop/types/shown.jadpo",
            "/// A label.\ntype Shown = Text {}\n".into(),
        ),
        (
            "domains/shop/actions/choose.jadpo",
            "use shop/types { AcceptLanguage, Shown }\n\n/// The language a request asks for, as text.\naction choose(header: AcceptLanguage?) -> Shown {\n    return locale.from_accept_language(header)\n}\n".to_string(),
        ),
    ];
    let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    assert_eq!(support::rules(Layout::Project, &files), ["type.mismatch"]);
}

#[test]
fn a_variable_cannot_be_called_locale_since_that_is_the_namespace() {
    assert_eq!(
        choosing(DECLARED, "locale: Locale", "return locale"),
        ["semantic.standard_namespace_reserved"]
    );
}

#[test]
fn a_query_computes_nothing_so_it_cannot_choose_a_locale() {
    // The choice is made before the query, and its result passed in, as
    // every other call is.
    let files = [
        ("app.jadpo", format!("application {{\n}}\n{DECLARED}")),
        (
            "domains/shop/types/accept_language.jadpo",
            "/// An `Accept-Language` header.\ntype AcceptLanguage = Text {}\n".into(),
        ),
        (
            "domains/shop/entities/greeting.jadpo",
            "/// A greeting in one language.\nentity Greeting {\n    id: Uuid\n    language: Locale\n}\n".into(),
        ),
        (
            "domains/shop/queries/greeting_for.jadpo",
            "use shop/types { AcceptLanguage }\nuse shop/entities { Greeting }\n\n/// The greeting in the language a request asks for.\nquery greeting_for(header: AcceptLanguage?) -> List<Greeting> {\n    from: Greeting\n    where: Greeting.language == locale.from_accept_language(header)\n    order: [Greeting.id asc]\n    limit: 1\n}\n".into(),
        ),
    ];
    let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    let rules = support::rules(Layout::Project, &files);
    assert!(
        rules.contains(&"query.value_unsupported".to_string()),
        "{rules:?}"
    );
}

// --- sorting by a locale (chunk 8.4j) ------------------------------------------------

/// An application whose action sorts names with `body`'s call.
fn sorting(locales: &str, body: &str) -> Vec<String> {
    let files = [
        ("app.jadpo", format!("application {{\n}}\n{locales}")),
        (
            "domains/shop/types/loaf_name.jadpo",
            "/// A loaf's name.\ntype LoafName = Text {\n    min_length: 1\n    max_length: 60\n}\n".into(),
        ),
        (
            "domains/shop/actions/in_order.jadpo",
            format!(
                "use shop/types {{ LoafName }}\n\n/// The loaves in order.\naction in_order(loaves: List<LoafName>, shopper: Locale) -> List<LoafName> {{\n    return {body}\n}}\n"
            ),
        ),
    ];
    let files: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    support::rules(Layout::Project, &files)
}

#[test]
fn sorting_by_a_locale_needs_the_application_s_locales() {
    // a deployable keeps collation for the declared
    // locales only, so an application with none sorts by none.
    let by_locale = "list.sort_by(loaves, loaf -> loaf, locale: shopper)";
    assert_eq!(sorting(DECLARED, by_locale), CLEAN);
    assert_eq!(sorting("", by_locale), ["type.temporal_locales_required"]);
    let written_out = "list.sort_by(loaves, loaf -> loaf, locale: Locale.sv_se)";
    assert_eq!(sorting("", written_out), ["type.temporal_locales_required"]);
    // By code point, there is no locale to need.
    assert_eq!(sorting("", "list.sort_by(loaves, loaf -> loaf)"), CLEAN);
}

#[test]
fn files_on_their_own_sort_by_any_locale() {
    let source = "type LoafName = Text {}\n\nfunction in_order(loaves: List<LoafName>, shopper: Locale) -> List<LoafName> {\n    return list.sort_by(loaves, loaf -> loaf, locale: shopper)\n}\n";
    assert_eq!(rules(source), CLEAN);
}

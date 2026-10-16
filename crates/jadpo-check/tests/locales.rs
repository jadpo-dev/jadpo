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
        "{locales}\n{TYPES}function label(value: OrderedAt, locale: Locale) -> PresentationText {{\n    return temporal.format(value, zone: Zone.utc, locale: {locale}, format: TimeFormat.date_long)\n}}\n"
    );
    rules(&source)
}

#[test]
fn a_locale_written_out_is_one_the_application_supports() {
    assert_eq!(formats(PAGE, "Locale.fr_fr"), CLEAN);
    assert_eq!(
        formats(PAGE, "locale"),
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
            "use shop/types { OrderedAt, PlacedText }\n\n/// The placing, in the customer's language.\naction show_placed(ordered_at: OrderedAt, locale: Locale) -> PlacedText {\n    return PlacedText {\n        placed: temporal.format(ordered_at, zone: Zone.utc, locale: locale, format: TimeFormat.date_long)\n    }\n}\n".into(),
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

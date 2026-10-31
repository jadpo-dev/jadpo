//! Area 2, "Units and money in plain numbers" (roadmap F1): a named type
//! built on `Int` or `Decimal` whose whole name is one of a standard
//! type's units, a currency's minor unit or a percentage is
//! `type.unit_as_number`, a declinable error naming the standard type,
//! read from the same name data `jadpo suggest` uses.

use jadpo_db::{Database, Layout};
use jadpo_diag::Diagnostic;

use crate::support;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", source)]);
    jadpo_check::check(&db, project)
}

fn messages(source: &str) -> Vec<String> {
    diagnostics(source)
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect()
}

const NONE: &[&str] = &[];

#[test]
fn a_type_named_for_a_unit_is_the_standard_type_held_as_a_number() {
    for (source, standard) in [
        (
            "type Grams = Int {}\n",
            "`Grams` holds a `Mass` as a plain number",
        ),
        (
            "type Metres = Decimal {\n    scale: 2\n}\n",
            "`Metres` holds a `Length` as a plain number",
        ),
        (
            "type Pence = Int {}\n",
            "`Pence` holds a `Money` as a plain number",
        ),
        (
            "type Cents = Int {}\n",
            "`Cents` holds a `Money` as a plain number",
        ),
        (
            "type Percent = Int {\n    min: 0\n    max: 100\n}\n",
            "`Percent` holds a `Percentage` as a plain number",
        ),
        (
            "type Seconds = Int {}\n",
            "`Seconds` holds a `Duration` as a plain number",
        ),
        (
            "type Kg = Decimal {\n    scale: 3\n}\n",
            "`Kg` holds a `Mass` as a plain number",
        ),
        (
            "type NauticalMiles = Decimal {\n    scale: 2\n}\n",
            "`NauticalMiles` holds a `Length` as a plain number",
        ),
        (
            "type MilesPerHour = Decimal {\n    scale: 1\n}\n",
            "`MilesPerHour` holds a `Speed` as a plain number",
        ),
    ] {
        assert_eq!(
            messages(source),
            [format!("type.unit_as_number: {standard}")],
            "{source}"
        );
    }
}

#[test]
fn the_error_names_the_unit_and_the_standard_type_at_the_name() {
    let found = diagnostics("type Grams = Int {\n    min: 0\n}\n");
    assert_eq!(found.len(), 1);
    let error = &found[0];
    assert!(error.rule.declinable, "a judgement call");
    let primary = error.primary.as_ref().expect("at the name");
    assert_eq!((primary.span.start, primary.span.end), (5, 10));
    assert_eq!(primary.text, "named for a unit");
    let reason = error.reason.as_deref().unwrap_or_default();
    assert!(
        reason.starts_with("`Grams` is named for a unit of `Mass` but is a plain `Int`"),
        "{reason}"
    );
    let next = error
        .next
        .as_ref()
        .map(|n| n.text.as_str())
        .unwrap_or_default();
    assert!(
        next.starts_with("Build it on `Mass`, named for what it holds rather than its unit"),
        "{next}"
    );
}

#[test]
fn a_type_built_on_a_number_type_is_found_too() {
    assert_eq!(
        messages("type Whole = Int {}\n\ntype Grams = Whole {}\n"),
        ["type.unit_as_number: `Grams` holds a `Mass` as a plain number"]
    );
}

#[test]
fn a_name_that_is_more_than_a_unit_or_a_type_that_holds_its_unit_is_left_alone() {
    for source in [
        "type ParcelWeight = Mass {}\n",
        "type Kilograms = Mass {}\n",
        "type Wait = Duration {}\n",
        "type PricePence = Int {}\n",
        "type Grams = Text {}\n",
        "type Count = Int {}\n",
        "type PassRate = Percentage {}\n",
    ] {
        assert_eq!(messages(source), NONE, "{source}");
    }
}

#[test]
fn it_can_be_declined_with_a_reason() {
    let declined = "/// Knots the scouts can tie.\ndecline type.unit_as_number: \"Knots the scouts can tie, not a speed\"\ntype Knots = Int {\n    min: 0\n}\n";
    assert_eq!(messages(declined), NONE);
    assert_eq!(
        messages("/// Knots the scouts can tie.\ntype Knots = Int {\n    min: 0\n}\n"),
        ["type.unit_as_number: `Knots` holds a `Speed` as a plain number"]
    );
}

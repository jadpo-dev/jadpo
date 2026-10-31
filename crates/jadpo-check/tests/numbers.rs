//! Area 19, "Numbers" (chunk 5.5d): every `integer` and `decimal`
//! operation checks, `min` and `max` keep their values' named type and take
//! two of one type as `+` does, `abs` and the roundings prove what they
//! can, `scale:` is written out, `Rounding.down` exists, and `text.format`
//! shows a number.

use crate::support;

use jadpo_db::{Database, Layout};
use jadpo_diag::Diagnostic;

const NAMES: &str = "type Heaps = Int {\n    min: 0\n}\n\ntype TrayCount = Int {\n    min: 1\n    max: 12\n}\n\ntype Swing = Int {\n    min: -5\n    max: 5\n}\n\ntype Places = Int {\n    min: 0\n    max: 3\n}\n\ntype Heaviness = Decimal {\n    scale: 3\n    min: 0.000\n}\n\ntype Change = Decimal {\n    scale: 2\n}\n\ntype LoafPrice = Decimal {\n    scale: 2\n    min: 0.00\n}\n\n";

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    );
    jadpo_check::check(&db, project)
}

fn rules(source: &str) -> Vec<String> {
    diagnostics(source)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect()
}

const NONE: &[&str] = &[];

#[test]
fn every_number_operation_checks() {
    let source = "function drift(a: Heaps, b: Heaps) -> Heaps {\n    return Heaps(integer.abs(a - b))\n}\n\nfunction trays(a: TrayCount, b: TrayCount) -> List<TrayCount> {\n    return [integer.min(a, b), integer.max(a, b), integer.min(a, 3), integer.max(12, b)]\n}\n\nfunction loaves(flour: Heaviness) -> Heaps {\n    return Heaps(integer.from_decimal(flour / 0.450, rounding: Rounding.down))\n}\n\nfunction prices(a: LoafPrice, b: LoafPrice) -> List<LoafPrice> {\n    return [decimal.min(a, b), decimal.max(a, 0.50), LoafPrice(decimal.abs(a - b))]\n}\n\nfunction label(weight: Heaviness) -> Heaviness {\n    return Heaviness(decimal.round(weight, scale: 1, rounding: Rounding.half_up))\n}\n\nfunction share(total: LoafPrice, people: TrayCount) -> LoafPrice {\n    return LoafPrice(decimal.divide(total, people, rounding: Rounding.down))\n}\n\nfunction shown(price: LoafPrice, count: TrayCount) -> List<PresentationText> {\n    return [text.format(price, locale: Locale.en_gb), text.format(count, locale: Locale.fr_fr)]\n}\n";
    assert_eq!(rules(source), NONE);
}

#[test]
fn min_and_max_take_two_values_of_one_type_and_keep_it() {
    // Two different named types aren't one, as for `+`.
    let found = diagnostics(
        "function f(a: Heaps, b: TrayCount) -> Heaps {\n    return integer.min(a, b)\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.mismatch");
    assert_eq!(
        found[0].message,
        "`Heaps` and `TrayCount` can't both be given to `integer.min`"
    );
    // A literal takes the other value's type, so it must be one.
    assert_eq!(
        rules("function f(a: TrayCount) -> TrayCount {\n    return integer.min(a, 20)\n}\n"),
        ["type.invalid_literal"]
    );
    // A plain number makes the result plain, as `+` does.
    assert_eq!(
        rules(
            "function f(a: Heaps, b: TrayCount) -> Heaps {\n    return integer.max(a, b + 1)\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(
            "function f(a: Heaps, b: TrayCount) -> Heaps {\n    return Heaps(integer.max(a, b + 1))\n}\n"
        ),
        NONE,
        "both at least 0, so the larger is"
    );
    // Each value is built on the namespace's representation.
    assert_eq!(
        rules("function f(a: LoafPrice) -> LoafPrice {\n    return integer.max(a, a)\n}\n"),
        ["type.mismatch", "type.mismatch"]
    );
    assert_eq!(
        rules("function f(a: LoafPrice) -> LoafPrice {\n    return decimal.max(a, 1)\n}\n"),
        ["type.mismatch"]
    );
    // Two literals give the representation.
    assert_eq!(
        rules("function f() -> TrayCount {\n    return TrayCount(integer.min(3, 4))\n}\n"),
        NONE
    );
}

#[test]
fn bounds_follow_through_min_max_abs_and_rounding() {
    // `min` with 12 is never above 12, `max` with 1 never below 1.
    assert_eq!(
        rules(
            "function f(a: Heaps) -> TrayCount {\n    return TrayCount(integer.max(integer.min(a, 12), 1))\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules("function f(a: Heaps) -> TrayCount {\n    return TrayCount(integer.min(a, 12))\n}\n"),
        ["failure.attempt_required"],
        "0 is below `min: 1`"
    );
    // `abs` is never below 0, but has no top.
    assert_eq!(
        rules("function f(a: Swing) -> Swing {\n    return Swing(integer.abs(a))\n}\n"),
        ["failure.attempt_required"]
    );
    // A rounding never crosses zero; a value that may be negative may
    // round to one.
    assert_eq!(
        rules(
            "function f(a: Change) -> Heaps {\n    return Heaps(integer.from_decimal(a, rounding: Rounding.up))\n}\n"
        ),
        ["failure.attempt_required"]
    );
    assert_eq!(
        rules(
            "function f(a: Change) -> LoafPrice {\n    return LoafPrice(decimal.round(decimal.abs(a), scale: 0, rounding: Rounding.down))\n}\n"
        ),
        NONE
    );
}

#[test]
fn scale_is_a_whole_number_written_out() {
    let base = |scale: &str| {
        format!(
            "function f(w: Heaviness, p: Places) -> Heaviness {{\n    return Heaviness(decimal.round(w, scale: {scale}, rounding: Rounding.down))\n}}\n"
        )
    };
    assert_eq!(rules(&base("0")), NONE);
    assert_eq!(rules(&base("18")), NONE);
    let value = diagnostics(&base("p"));
    assert_eq!(value.len(), 1, "{value:?}");
    assert_eq!(value[0].rule.id, "type.mismatch");
    assert_eq!(
        value[0].message,
        "`decimal.round`'s `scale:` is a whole number written out, not a value"
    );
    assert_eq!(rules(&base("-1")), ["type.argument_negative"]);
    assert_eq!(rules(&base("19")), ["type.mismatch"]);
    assert_eq!(rules(&base("1.5")), ["type.mismatch"]);
    // It is given, as `rounding:` is.
    assert_eq!(
        rules(
            "function f(w: Heaviness) -> Heaviness {\n    return Heaviness(decimal.round(w, rounding: Rounding.down))\n}\n"
        ),
        ["type.argument_count"]
    );
}

#[test]
fn rounding_has_four_modes() {
    for mode in ["half_even", "half_up", "up", "down"] {
        assert_eq!(
            rules(&format!(
                "function f(a: Heaviness) -> Heaps {{\n    return Heaps(integer.from_decimal(a, rounding: Rounding.{mode}))\n}}\n"
            )),
            NONE,
            "{mode}"
        );
    }
    assert_eq!(
        rules(
            "function f(a: Heaviness) -> Heaps {\n    return Heaps(integer.from_decimal(a, rounding: Rounding.floor))\n}\n"
        ),
        ["type.unknown_enum_variant"]
    );
}

#[test]
fn text_format_shows_a_number_with_no_time_options() {
    let found = diagnostics(
        "function f(g: Heaps) -> PresentationText {\n    return text.format(g, locale: Locale.en_gb, zone: Zone.utc)\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.temporal_option_unused");
    assert_eq!(found[0].message, "`zone:` means nothing for a `Heaps`");
    // A `Bool` still has no format for people.
    assert_eq!(
        rules(
            "type Flag = Bool {}\n\nfunction f(b: Flag) -> PresentationText {\n    return text.format(b, locale: Locale.en_gb)\n}\n"
        ),
        ["type.mismatch"]
    );
}

//! Area 19, "The standard operator table" (chunk 5.5j2): `20%` is a
//! `Percentage` of 0.20, remainder keeps its spaced `%` and `20%3` is
//! `syntax.percent_ambiguous`, naming both readings; a bare number where a
//! percentage goes is refused, while `Percentage(ratio)` builds one from a
//! `Decimal` ratio; the table's `Percentage` rows: `Money * Percentage`,
//! `Money + Percentage` refused as ambiguous, a number or a quantity times
//! a percentage keeping its kind, and two percentages adding and
//! multiplying to a percentage; `Ratio` and `Fraction` as standard objects.

use crate::support;

use jadpo_db::{Database, Layout};
use jadpo_diag::Diagnostic;

const NAMES: &str = "type ItemPrice = Money {\n    currency: Currency.gbp\n}\n\ntype Rate = Percentage {}\n\ntype Reduction = Percentage {\n    min: 0%\n    max: 100%\n}\n\ntype Board = Length {}\n\ntype Share = Decimal {\n    scale: 4\n}\n\ntype Count = Int {}\n\ntype Answer = Bool {}\n\ntype Typed = Text {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    );
    jadpo_check::check(&db, project)
}

/// A diagnostic's next step, as text.
fn next_of(found: &Diagnostic) -> String {
    found
        .next
        .as_ref()
        .map(|next| next.text.clone())
        .unwrap_or_default()
}

fn rules(source: &str) -> Vec<String> {
    diagnostics(source)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect()
}

const NONE: &[&str] = &[];

#[test]
fn a_percent_literal_is_a_percentage() {
    let source = "function vat() -> Rate {\n    return Rate(20%)\n}\n\nfunction small() -> Rate {\n    return Rate(12.5%)\n}\n\nfunction cut() -> Rate {\n    return Rate(-5%)\n}\n\nfunction most() -> Reduction {\n    return Reduction(100%)\n}\n\nfunction plain() -> Rate {\n    var half: Percentage = 50%\n    return Rate(half)\n}\n";
    assert_eq!(rules(source), NONE);
    assert_eq!(
        rules("function f() -> Reduction {\n    return Reduction(120%)\n}\n"),
        ["type.invalid_literal"],
        "a literal is checked against the type's bounds"
    );
    assert_eq!(
        rules("type Bad = Percentage {\n    max: 1\n}\n"),
        ["type.setting_value"],
        "a percentage's bounds are written with `%`"
    );
}

#[test]
fn a_bare_number_where_a_percentage_goes_is_refused() {
    let found = diagnostics("function f(r: Rate) -> Answer {\n    return Answer(r == 0.25)\n}\n");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.incomparable");
    let found = diagnostics(
        "function apply(r: Rate) -> Rate {\n    return r\n}\n\nfunction f() -> Rate {\n    return apply(0.25)\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.mismatch");
    assert!(
        next_of(&found[0]).contains("`25%`"),
        "{}",
        next_of(&found[0])
    );
}

#[test]
fn a_ratio_becomes_a_percentage_by_construction() {
    let source = "function quarter() -> Rate {\n    return Rate(Percentage(0.25))\n}\n\nfunction share_of(ratio: Share) -> Rate {\n    return Rate(ratio)\n}\n\nfunction literal() -> Rate {\n    return Rate(0.125)\n}\n";
    assert_eq!(rules(source), NONE);
    let found = diagnostics("function f(n: Count) -> Rate {\n    return Rate(n)\n}\n");
    assert_eq!(
        found.iter().map(|d| d.rule.id).collect::<Vec<_>>(),
        ["type.constructor_input"],
        "a whole number is no ratio"
    );
    let found = diagnostics("function f() -> Rate {\n    return Rate(20)\n}\n");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.constructor_input");
    assert!(
        next_of(&found[0]).contains("`20%`"),
        "{}",
        next_of(&found[0])
    );
}

#[test]
fn the_table_s_percentage_rows() {
    let source = "function charged(p: ItemPrice, r: Rate) -> ItemPrice {\n    return ItemPrice(p * r)\n}\n\nfunction vat(p: ItemPrice) -> ItemPrice {\n    return ItemPrice(p * 20%)\n}\n\nfunction gross(p: ItemPrice) -> ItemPrice {\n    return ItemPrice(p * 120%)\n}\n\nfunction trimmed() -> Board {\n    var total: Length = 2metre + 45cm + 3inch\n    var trim: Length = total * 15%\n    return Board(trim)\n}\n\nfunction scaled(s: Share, r: Rate) -> Share {\n    return Share(s * r)\n}\n\nfunction counted(n: Count, r: Rate) -> Share {\n    return Share(n * r)\n}\n\nfunction both(a: Rate, b: Rate) -> Rate {\n    return Rate(a * b)\n}\n\nfunction added(a: Rate, b: Rate) -> Rate {\n    return Rate(a + b - 5%)\n}\n\nfunction twice(a: Rate) -> Rate {\n    return Rate(a * 2)\n}\n\nfunction bigger(a: Rate, b: Rate) -> Answer {\n    return Answer(a > b and a >= 5%)\n}\n";
    assert_eq!(rules(source), NONE);
}

#[test]
fn money_plus_a_percentage_is_ambiguous() {
    let found =
        diagnostics("function f(p: ItemPrice) -> ItemPrice {\n    return ItemPrice(p + 20%)\n}\n");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.arithmetic_operand");
    let next = next_of(&found[0]);
    assert!(
        next.contains("`p * 120%`") && next.contains("`p + p * 20%`"),
        "{next}"
    );
    let found =
        diagnostics("function f(p: ItemPrice) -> ItemPrice {\n    return ItemPrice(p - 20%)\n}\n");
    let next = next_of(&found[0]);
    assert!(
        next.contains("`p * 80%`") && next.contains("`p - p * 20%`"),
        "{next}"
    );
    assert_eq!(
        rules("function f(p: ItemPrice, r: Rate) -> ItemPrice {\n    return ItemPrice(p + r)\n}\n"),
        ["type.arithmetic_operand"]
    );
}

#[test]
fn what_the_table_has_no_row_for_is_refused() {
    for (source, why) in [
        (
            "function f(p: ItemPrice, r: Rate) -> ItemPrice {\n    return ItemPrice(r * p)\n}\n",
            "the amount is written first",
        ),
        (
            "function f(a: Board, r: Rate) -> Board {\n    return Board(r * a)\n}\n",
            "the quantity is written first",
        ),
        (
            "function f(a: Board, r: Rate) -> Board {\n    return Board(a + r)\n}\n",
            "a length and a percentage don't add",
        ),
        (
            "function f(r: Rate) -> Rate {\n    return Rate(r + 0.5)\n}\n",
            "a bare number isn't a percentage",
        ),
        (
            "function f(s: Share, r: Rate) -> Share {\n    return Share(s + r)\n}\n",
            "a number and a percentage don't add",
        ),
        (
            "function f(r: Rate) -> Rate {\n    return Rate(r / 2)\n}\n",
            "no row divides a percentage",
        ),
    ] {
        assert_eq!(rules(source), ["type.arithmetic_operand"], "{why}");
    }
    assert_eq!(
        rules("function f(r: Rate) -> Rate {\n    return Rate(-r)\n}\n"),
        ["type.unary_operand"]
    );
}

#[test]
fn remainder_keeps_its_spaced_percent_and_20_percent_3_names_both() {
    assert_eq!(
        rules("function f(n: Count) -> Count {\n    return Count(n % 3)\n}\n"),
        NONE
    );
    let found = diagnostics("function f() -> Count {\n    return Count(20%3)\n}\n");
    assert!(!found.is_empty());
    assert_eq!(found[0].rule.id, "syntax.percent_ambiguous");
    assert!(
        found[0].message.contains("`20% 3`") && found[0].message.contains("`20 % 3`"),
        "{}",
        found[0].message
    );
}

#[test]
fn a_percentage_is_read_written_and_shown() {
    let source = "function shown(r: Rate, reader: Locale) -> PresentationText {\n    return text.format(r, locale: reader)\n}\n\nfunction written(r: Rate) -> Typed {\n    return Typed(text.from(r))\n}\n\nfunction read(raw: Typed) fails Unreadable -> Rate {\n    var rate: Rate = attempt text.parse(raw, as: Rate) {\n        malformed: Unreadable\n    }\n    return rate\n}\n";
    assert_eq!(rules(source), NONE);
}

#[test]
fn ratios_and_fractions_are_standard_objects() {
    let source = "function screen() -> Ratio {\n    return Ratio {\n        numerator: 16.0\n        denominator: 9.0\n    }\n}\n\nfunction three_quarters() -> Fraction {\n    return Fraction {\n        numerator: 3\n        denominator: 4\n    }\n}\n\nfunction top(f: Fraction) -> Count {\n    return Count(f.numerator)\n}\n\nfunction written(f: Fraction) -> Typed {\n    return Typed(text.from(f))\n}\n";
    assert_eq!(rules(source), NONE);
    assert_eq!(
        rules(
            "function f() -> Fraction {\n    return Fraction {\n        numerator: 3\n        denominator: 0\n    }\n}\n"
        ),
        ["type.invalid_literal"],
        "a fraction's denominator is at least 1"
    );
    assert_eq!(
        rules("type Ratio = Int {}\n"),
        ["type.standard_clash"],
        "`Ratio` is a standard type"
    );
}

/// A percentage written out has the one value it is, so an amount that is
/// never negative times `90%` is never negative either, and building a
/// type with `min: 0` from it takes no `attempt`, as it doesn't from a
/// percentage type with bounds; one that may be negative still does
/// (chunk 5.7c, example 09's whole cake).
#[test]
fn a_percent_literal_s_value_bounds_a_product() {
    let source = "type Sterling = Money {\n    currency: Currency.gbp\n    min: 0\n}\n\nfunction cake(full: Sterling) -> Sterling {\n    return Sterling(full * 90%)\n}\n\nfunction bounded(full: Sterling, cut: Reduction) -> Sterling {\n    return Sterling(full * cut)\n}\n";
    assert_eq!(rules(source), NONE);
    assert_eq!(
        rules(
            "type Sterling = Money {\n    currency: Currency.gbp\n    min: 0\n}\n\nfunction refund(full: Sterling) -> Sterling {\n    return Sterling(full * -10%)\n}\n"
        ),
        ["failure.attempt_required"],
        "a negative percentage can take the amount below its `min`"
    );
}

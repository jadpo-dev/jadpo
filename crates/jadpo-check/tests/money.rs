//! Area 19, "Money and payments" and "The standard operator table"
//! (chunk 5.5h): a `Money` type's `currency:`, `min:`, `max:` and
//! `rounding:`; `+`, `-` and `<` in one currency, a compile error where
//! both types fix different ones and `currency_mismatch` under `attempt`
//! where only the running program knows them, needing no arm inside an
//! `if` that compares the currencies; `*` by a number and `/` by a
//! `Decimal` rate; `/` by an `Int` refused as `type.money_split`; the
//! `money` operations; and `text.format`'s money options.

use crate::support;

use jadpo_db::{Database, Layout};
use jadpo_diag::Diagnostic;

const NAMES: &str = "type CakePrice = Money {\n    currency: Currency.gbp\n    min: 0\n    rounding: Rounding.half_up\n}\n\ntype EuroPrice = Money {\n    currency: Currency.eur\n    min: 0\n}\n\ntype Charge = Money {\n    min: 0\n}\n\ntype Fee = Money {\n    currency: Currency.gbp\n    min: 1\n}\n\ntype Guests = Int {\n    min: 1\n    max: 20\n}\n\ntype Share = Int {\n    min: 0\n}\n\ntype Rate = Decimal {\n    scale: 2\n    min: 0.01\n}\n\ntype Pounds = Decimal {\n    scale: 2\n}\n\ntype Answer = Bool {}\n\ntype Ratio = Decimal {\n    scale: 4\n}\n\ntype Amount = Money {}\n\ntype Pence = Int {}\n\ntype Mismatch = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype WrongCurrency = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype TooPrecise = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

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
fn a_money_type_fixes_its_currency_bounds_and_rounding() {
    assert_eq!(rules(""), NONE);
    assert_eq!(
        rules("type Count = Int {\n    currency: Currency.gbp\n}\n"),
        ["type.constraint_base"]
    );
    assert_eq!(
        rules("type Cost = Money {\n    currency: gbp\n}\n"),
        ["type.setting_value"]
    );
    assert_eq!(
        rules("type Cost = Money {\n    rounding: half_up\n}\n"),
        ["type.setting_value"]
    );
    assert_eq!(
        rules("type Cost = Money {\n    min: 0.00\n}\n"),
        ["type.setting_value"],
        "minor units are whole"
    );
    assert_eq!(
        rules("type Tally = Int {\n    rounding: Rounding.up\n}\n"),
        ["type.constraint_base"]
    );
}

#[test]
fn amounts_in_one_currency_add_compare_and_scale() {
    let source = "function total(a: CakePrice, b: CakePrice) -> CakePrice {\n    return CakePrice(a + b)\n}\n\nfunction cheaper(a: CakePrice, b: CakePrice) -> Answer {\n    return Answer(a < b)\n}\n\nfunction doubled(a: CakePrice) -> CakePrice {\n    return CakePrice(a * 2)\n}\n\nfunction tax(a: CakePrice) -> CakePrice {\n    return CakePrice(a * 0.2)\n}\n\nfunction net(a: CakePrice) -> CakePrice {\n    return CakePrice(a / 1.20)\n}\n\nfunction scaled(a: CakePrice, rate: Rate) -> CakePrice {\n    return CakePrice(a / rate)\n}\n\nfunction times(a: Fee, fee: Fee) -> Ratio {\n    return Ratio(a / fee)\n}\n\nfunction same(a: CakePrice, b: CakePrice) -> Answer {\n    return Answer(a == b)\n}\n\nfunction plain(a: CakePrice) -> Answer {\n    return Answer(a == money.from_minor(500, Currency.gbp))\n}\n";
    assert_eq!(rules(source), NONE);
}

#[test]
fn a_currency_both_types_fix_differently_fails_to_compile() {
    let found = diagnostics(
        "function f(a: CakePrice, b: EuroPrice) -> CakePrice {\n    return CakePrice(a + b)\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.arithmetic_operand");
    assert_eq!(
        found[0].message,
        "`CakePrice` and `EuroPrice` can't be added: they're in different currencies"
    );
    assert_eq!(
        rules("function f(a: CakePrice, b: EuroPrice) -> Answer {\n    return Answer(a < b)\n}\n"),
        ["type.ordering_operand"]
    );
    assert_eq!(
        rules(
            "function f(a: CakePrice) -> CakePrice {\n    return CakePrice(a - money.from_minor(5, Currency.eur))\n}\n"
        ),
        ["type.arithmetic_operand"],
        "a currency written out is fixed too"
    );
}

#[test]
fn a_currency_known_only_while_running_is_mapped_or_proved() {
    assert_eq!(
        rules("function f(a: Charge, b: Charge) -> Charge {\n    return Charge(a + b)\n}\n"),
        ["failure.attempt_required"]
    );
    assert_eq!(
        rules(
            "function f(a: Charge, b: Charge) fails Mismatch -> Amount {\n    var sum: Money = attempt a + b {\n        currency_mismatch: Mismatch\n    }\n    return Amount(sum)\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function f(a: Charge, b: Charge) fails Mismatch -> Answer {\n    var less: Bool = attempt a < b {\n        currency_mismatch: Mismatch\n    }\n    return Answer(less)\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function f(a: Charge, b: Charge) -> Charge {\n    if (money.currency(a) == money.currency(b)) {\n        return Charge(a + b)\n    }\n    return a\n}\n"
        ),
        NONE,
        "the condition proves one currency"
    );
    assert_eq!(
        rules(
            "function f(a: Charge, b: CakePrice) -> CakePrice {\n    if (money.currency(a) == Currency.gbp) {\n        return CakePrice(a)\n    }\n    return b\n}\n"
        ),
        NONE,
        "a currency compared with one written out is that one"
    );
    assert_eq!(
        rules(
            "function f(a: Charge, b: Charge) fails Mismatch -> Amount {\n    if (money.currency(a) == money.currency(b)) {\n        var sum: Money = attempt a + b {\n            currency_mismatch: Mismatch\n        }\n        return Amount(sum)\n    }\n    return Amount(a)\n}\n"
        ),
        ["failure.attempt_infallible"]
    );
    assert_eq!(
        rules(
            "function f(a: CakePrice, b: CakePrice) fails Mismatch -> Amount {\n    var sum: Money = attempt a + b {\n        currency_mismatch: Mismatch\n    }\n    return Amount(sum)\n}\n"
        ),
        ["failure.attempt_infallible"]
    );
}

#[test]
fn dividing_money_by_a_count_is_refused_for_split() {
    let found = diagnostics(
        "function f(total: CakePrice) -> CakePrice {\n    return CakePrice(total / 3)\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.money_split");
    assert_eq!(
        rules(
            "function f(total: CakePrice, guests: Guests) -> CakePrice {\n    return CakePrice(total / guests)\n}\n"
        ),
        ["type.money_split"]
    );
}

#[test]
fn only_the_operator_tables_rows_take_money() {
    for (source, rule) in [
        (
            "function f(a: CakePrice) -> CakePrice {\n    return CakePrice(2 * a)\n}\n",
            "type.arithmetic_operand",
        ),
        (
            "function f(a: CakePrice) -> CakePrice {\n    return CakePrice(a + 1)\n}\n",
            "type.arithmetic_operand",
        ),
        (
            "function f(a: CakePrice) -> CakePrice {\n    return CakePrice(a % 2)\n}\n",
            "type.arithmetic_operand",
        ),
        (
            "function f(a: CakePrice) -> CakePrice {\n    return CakePrice(-a)\n}\n",
            "type.unary_operand",
        ),
        (
            "function f(a: CakePrice, b: CakePrice) -> CakePrice {\n    return CakePrice(a * b)\n}\n",
            "type.arithmetic_operand",
        ),
        (
            "function f(a: CakePrice, b: CakePrice) -> Ratio {\n    return Ratio(a / b)\n}\n",
            "type.divisor_zero",
        ),
    ] {
        assert_eq!(rules(source), [rule], "{source}");
    }
}

#[test]
fn the_money_operations_check() {
    let source = "function list_price() -> CakePrice {\n    return CakePrice(money.from_minor(1050, Currency.gbp))\n}\n\nfunction entered(amount: Pounds) fails TooPrecise -> Amount {\n    var made: Money = attempt money.of(amount, Currency.jpy) {\n        too_many_places: TooPrecise\n    }\n    return Amount(made)\n}\n\nfunction pounds(amount: Pounds) -> Amount {\n    return Amount(money.of(amount, Currency.gbp))\n}\n\nfunction thirds(total: CakePrice) -> Answer {\n    var shares: List<Money> = money.split(total, 3)\n    return Answer(list.count(shares) == 3)\n}\n\nfunction shares(total: CakePrice) -> Answer {\n    var shares: List<Money> = money.allocate(total, [1, 2, 0])\n    return Answer(list.count(shares) == 3)\n}\n\nfunction pence(price: CakePrice) -> Pence {\n    return Pence(money.minor(price))\n}\n\nfunction basket(lines: List<CakePrice>) -> CakePrice {\n    return CakePrice(money.sum(lines, currency: Currency.gbp))\n}\n\nfunction mixed(lines: List<Charge>) fails Mismatch -> Amount {\n    var sum: Money = attempt money.sum(lines, currency: Currency.gbp) {\n        currency_mismatch: Mismatch\n    }\n    return Amount(sum)\n}\n";
    assert_eq!(rules(source), NONE);
    assert_eq!(
        rules(
            "function f(amount: Ratio) -> Amount {\n    return Amount(money.of(amount, Currency.gbp))\n}\n"
        ),
        ["failure.attempt_required"],
        "a decimal with any places can have too many"
    );
    assert_eq!(
        rules(
            "function f(lines: List<Charge>) -> Amount {\n    return Amount(money.sum(lines, currency: Currency.gbp))\n}\n"
        ),
        ["failure.attempt_required"]
    );
    assert_eq!(
        rules(
            "function f(total: CakePrice) -> Answer {\n    var shares: List<Money> = money.split(total, 0)\n    return Answer(list.count(shares) > 0)\n}\n"
        ),
        ["type.divisor_zero"]
    );
    assert_eq!(
        rules(
            "function f(total: CakePrice, parts: Share) -> Answer {\n    var shares: List<Money> = money.split(total, parts)\n    return Answer(list.count(shares) > 0)\n}\n"
        ),
        ["type.divisor_zero"],
        "a count that may be 0"
    );
    assert_eq!(
        rules(
            "function f(total: CakePrice, parts: Guests) -> Answer {\n    var shares: List<Money> = money.split(total, parts)\n    return Answer(list.count(shares) > 0)\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function f(total: CakePrice) -> Answer {\n    var shares: List<Money> = money.allocate(total, [1, -1])\n    return Answer(list.count(shares) > 0)\n}\n"
        ),
        ["type.argument_negative"]
    );
    assert_eq!(
        rules(
            "function f(total: CakePrice) -> Answer {\n    var shares: List<Money> = money.allocate(total, [])\n    return Answer(list.count(shares) > 0)\n}\n"
        ),
        ["type.argument_empty"]
    );
    assert_eq!(
        rules(
            "function f(total: CakePrice, ratios: List<Share>) -> Answer {\n    var shares: List<Money> = money.allocate(total, ratios)\n    return Answer(list.count(shares) > 0)\n}\n"
        ),
        ["type.argument_empty"],
        "a list that may be empty"
    );
}

#[test]
fn building_a_money_type_checks_its_currency() {
    assert_eq!(
        rules("function f(amount: Charge) -> CakePrice {\n    return CakePrice(amount)\n}\n"),
        ["failure.attempt_required"]
    );
    assert_eq!(
        rules(
            "function f(amount: Charge) fails WrongCurrency -> CakePrice {\n    var price: CakePrice = attempt CakePrice(amount) {\n        currency: WrongCurrency\n    }\n    return price\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules("function f(amount: Fee) -> CakePrice {\n    return CakePrice(amount)\n}\n"),
        NONE,
        "same currency, and at least 1 is at least 0"
    );
}

#[test]
fn money_is_formatted_with_its_options() {
    assert_eq!(
        rules(
            "function f(price: CakePrice) -> PresentationText {\n    return text.format(price, locale: Locale.en_gb, precision: MoneyPrecision.whole, display: MoneyDisplay.code, sign: MoneySign.accounting)\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function f(count: Guests) -> PresentationText {\n    return text.format(count, locale: Locale.en_gb, precision: MoneyPrecision.whole)\n}\n"
        ),
        ["type.temporal_option_unused"]
    );
    assert_eq!(
        rules(
            "function f(price: CakePrice) -> PresentationText {\n    return text.format(price, locale: Locale.en_gb, zone: Zone.utc)\n}\n"
        ),
        ["type.temporal_option_unused"]
    );
}

#[test]
fn an_object_of_an_amount_and_a_currency_is_money() {
    let found = diagnostics("type Cost = Object {\n    pence: Int\n    currency_code: Text\n}\n");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.standard_exists");
    assert_eq!(
        found[0].message,
        "`Cost` is the standard `Money` under another name"
    );
    assert_eq!(
        rules(
            "type Coin = Enum {\n    gbp\n    eur\n}\n\ntype Cost = Object {\n    amount: Pounds\n    currency: Coin\n}\n"
        ),
        ["type.standard_exists"],
        "a currency held as an enum"
    );
    assert_eq!(
        rules("type Money = Object {\n    amount: Int\n}\n"),
        ["type.standard_clash"]
    );
}

#[test]
fn zz_debug() {
    let Ok(source) = std::env::var("MONEY_DEBUG") else {
        return;
    };
    for d in diagnostics(&source) {
        eprintln!(
            "{}: {} | {:?} | {:?}",
            d.rule.id,
            d.message,
            d.primary.map(|l| l.text),
            d.reason
        );
    }
}

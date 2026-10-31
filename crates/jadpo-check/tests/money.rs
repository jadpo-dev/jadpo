//! Area 19, "Money and payments" and "The standard operator table"
//! (chunk 5.5h): a `Money` type's `currency:`, `min:`, `max:` and
//! `rounding:`; `+`, `-` and `<` in one currency, a compile error where
//! both types fix different ones and `currency_mismatch` under `attempt`
//! where only the running program knows them, needing no arm inside an
//! `if` that compares the currencies; `*` by a number and `/` by a
//! `Decimal` rate; `/` by an `Int` refused as `type.money_split`; the
//! `money` operations; and `text.format`'s money options. Chunk 5.5h2:
//! `money.convert` with an `ExchangeRate`, an inverted rate a compile error
//! where both currencies are known and `currency_mismatch` otherwise.
//! Chunk 5.5h4: a type built on `ExchangeRate` fixes its currencies with
//! `from:` and `to:`, so an inverted rate is a compile error across
//! function boundaries and the right one needs no `attempt`.

use crate::support;

use jadpo_db::{Database, Layout};
use jadpo_diag::Diagnostic;

const NAMES: &str = "type CakePrice = Money {\n    currency: Currency.gbp\n    min: 0\n    rounding: Rounding.half_up\n}\n\ntype EuroPrice = Money {\n    currency: Currency.eur\n    min: 0\n}\n\ntype Charge = Money {\n    min: 0\n}\n\ntype Fee = Money {\n    currency: Currency.gbp\n    min: 1\n}\n\ntype Guests = Int {\n    min: 1\n    max: 20\n}\n\ntype Share = Int {\n    min: 0\n}\n\ntype Rate = Decimal {\n    scale: 2\n    min: 0.01\n}\n\ntype Tender = Decimal {\n    scale: 2\n}\n\ntype Answer = Bool {}\n\ntype Factor = Decimal {\n    scale: 4\n}\n\ntype Amount = Money {}\n\ntype Euros = Money {\n    currency: Currency.eur\n}\n\ntype Coins = Int {}\n\ntype Mismatch = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype WrongCurrency = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype TooPrecise = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype Dollars = Money {\n    currency: Currency.usd\n}\n\ntype PoundsToEuros = ExchangeRate {\n    from: Currency.gbp\n    to: Currency.eur\n}\n\ntype EurosToPounds = ExchangeRate {\n    from: Currency.eur\n    to: Currency.gbp\n}\n\ntype FromPounds = ExchangeRate {\n    from: Currency.gbp\n}\n\ntype WrongRate = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

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
    let source = "function total(a: CakePrice, b: CakePrice) -> CakePrice {\n    return CakePrice(a + b)\n}\n\nfunction cheaper(a: CakePrice, b: CakePrice) -> Answer {\n    return Answer(a < b)\n}\n\nfunction doubled(a: CakePrice) -> CakePrice {\n    return CakePrice(a * 2)\n}\n\nfunction tax(a: CakePrice) -> CakePrice {\n    return CakePrice(a * 0.2)\n}\n\nfunction net(a: CakePrice) -> CakePrice {\n    return CakePrice(a / 1.20)\n}\n\nfunction scaled(a: CakePrice, rate: Rate) -> CakePrice {\n    return CakePrice(a / rate)\n}\n\nfunction times(a: Fee, fee: Fee) -> Factor {\n    return Factor(a / fee)\n}\n\nfunction same(a: CakePrice, b: CakePrice) -> Answer {\n    return Answer(a == b)\n}\n\nfunction plain(a: CakePrice) -> Answer {\n    return Answer(a == money.from_minor(500, Currency.gbp))\n}\n";
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
            "function f(a: CakePrice, b: CakePrice) -> Factor {\n    return Factor(a / b)\n}\n",
            "type.divisor_zero",
        ),
    ] {
        assert_eq!(rules(source), [rule], "{source}");
    }
}

#[test]
fn the_money_operations_check() {
    let source = "function list_price() -> CakePrice {\n    return CakePrice(money.from_minor(1050, Currency.gbp))\n}\n\nfunction entered(amount: Tender) fails TooPrecise -> Amount {\n    var made: Money = attempt money.of(amount, Currency.jpy) {\n        too_many_places: TooPrecise\n    }\n    return Amount(made)\n}\n\nfunction pounds(amount: Tender) -> Amount {\n    return Amount(money.of(amount, Currency.gbp))\n}\n\nfunction thirds(total: CakePrice) -> Answer {\n    var shares: List<Money> = money.split(total, 3)\n    return Answer(list.count(shares) == 3)\n}\n\nfunction shares(total: CakePrice) -> Answer {\n    var shares: List<Money> = money.allocate(total, [1, 2, 0])\n    return Answer(list.count(shares) == 3)\n}\n\nfunction pence(price: CakePrice) -> Coins {\n    return Coins(money.minor(price))\n}\n\nfunction basket(lines: List<CakePrice>) -> CakePrice {\n    return CakePrice(money.sum(lines, currency: Currency.gbp))\n}\n\nfunction mixed(lines: List<Charge>) fails Mismatch -> Amount {\n    var sum: Money = attempt money.sum(lines, currency: Currency.gbp) {\n        currency_mismatch: Mismatch\n    }\n    return Amount(sum)\n}\n";
    assert_eq!(rules(source), NONE);
    assert_eq!(
        rules(
            "function f(amount: Factor) -> Amount {\n    return Amount(money.of(amount, Currency.gbp))\n}\n"
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
            "type Coin = Enum {\n    gbp\n    eur\n}\n\ntype Cost = Object {\n    amount: Tender\n    currency: Coin\n}\n"
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
fn a_rate_converts_an_amount_in_its_from_currency() {
    assert_eq!(
        rules(
            "function f(cake: CakePrice, rate: ExchangeRate) -> Amount {\n    return Amount(money.convert(cake, rate))\n}\n"
        ),
        ["failure.attempt_required"],
        "a rate the app was given may be from any currency"
    );
    assert_eq!(
        rules(
            "function f(cake: CakePrice, rate: ExchangeRate) fails Mismatch -> Amount {\n    var converted: Money = attempt money.convert(cake, rate) {\n        currency_mismatch: Mismatch\n    }\n    return Amount(converted)\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function f(cake: CakePrice, rate: ExchangeRate) -> Amount {\n    if (rate.from == Currency.gbp) {\n        return Amount(money.convert(cake, rate))\n    }\n    return Amount(cake)\n}\n"
        ),
        NONE,
        "a condition proves the rate's currency"
    );
    assert_eq!(
        rules(
            "function f(price: Charge, rate: ExchangeRate) -> Amount {\n    if (rate.from == money.currency(price)) {\n        return Amount(money.convert(price, rate))\n    }\n    return Amount(price)\n}\n"
        ),
        NONE,
        "or that it is the amount's"
    );
    assert_eq!(
        rules(
            "function f(cake: CakePrice, rate: ExchangeRate) -> Euros? {\n    if (rate.from == Currency.gbp and rate.to == Currency.eur) {\n        return Euros(money.convert(cake, rate))\n    }\n    return none\n}\n"
        ),
        NONE,
        "the result is in the rate's `to`"
    );
    assert_eq!(
        rules(
            "function f(cake: CakePrice, rate: ExchangeRate) -> Euros? {\n    if (rate.from == Currency.gbp) {\n        return Euros(money.convert(cake, rate))\n    }\n    return none\n}\n"
        ),
        ["failure.attempt_required"],
        "a `to` not known may be another currency"
    );
    assert_eq!(
        rules(
            "function f(cake: CakePrice) -> Euros {\n    return Euros(money.convert(cake, ExchangeRate {\n        from: Currency.gbp\n        to: Currency.eur\n        rate: 1.17\n        at: \"2026-10-18T09:00:00Z\"\n    }))\n}\n"
        ),
        NONE,
        "a rate written out fixes both"
    );
    assert_eq!(
        rules(
            "function f(cake: CakePrice, rate: ExchangeRate) fails Mismatch -> Amount {\n    if (rate.from == Currency.gbp) {\n        var converted: Money = attempt money.convert(cake, rate) {\n            currency_mismatch: Mismatch\n        }\n        return Amount(converted)\n    }\n    return Amount(cake)\n}\n"
        ),
        ["failure.attempt_infallible"]
    );
}

#[test]
fn an_inverted_rate_is_a_compile_error() {
    let found = diagnostics(
        "function f(cake: CakePrice, rate: ExchangeRate) -> Amount {\n    if (rate.from == Currency.eur) {\n        return Amount(money.convert(cake, rate))\n    }\n    return Amount(cake)\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.mismatch");
    assert_eq!(
        found[0].message,
        "`rate` converts from EUR, and `cake` is in GBP"
    );
    assert_eq!(
        rules(
            "function f(cake: CakePrice) -> Amount {\n    return Amount(money.convert(cake, ExchangeRate {\n        from: Currency.eur\n        to: Currency.gbp\n        rate: 0.85\n        at: \"2026-10-18T09:00:00Z\"\n    }))\n}\n"
        ),
        ["type.mismatch"],
        "a rate written out the wrong way round"
    );
    assert_eq!(
        rules(
            "function f(price: EuroPrice, rate: ExchangeRate) -> Amount {\n    if (rate.from == Currency.gbp) {\n        return Amount(money.convert(price, rate))\n    }\n    return Amount(price)\n}\n"
        ),
        ["type.mismatch"]
    );
}

#[test]
fn an_exchange_rate_is_an_object_of_its_fields() {
    assert_eq!(
        rules(
            "type Pair = Object {\n    from: Currency\n    to: Currency\n    rate: Factor\n    at: Instant\n}\n"
        ),
        ["type.standard_exists"],
        "its shape is the standard `ExchangeRate`"
    );
    assert_eq!(
        rules("function f(rate: ExchangeRate) -> Factor {\n    return Factor(rate.rate)\n}\n"),
        NONE,
        "its fields are read as an object's"
    );
}

#[test]
fn a_rate_type_fixes_its_currencies() {
    assert_eq!(rules(""), NONE);
    assert_eq!(
        rules("type Odd = Money {\n    from: Currency.gbp\n}\n"),
        ["type.constraint_base"],
        "only a rate has a `from`"
    );
    assert_eq!(
        rules("type Odd = ExchangeRate {\n    currency: Currency.gbp\n}\n"),
        ["type.constraint_base"],
        "a rate has two currencies, not one"
    );
    assert_eq!(
        rules("type Odd = ExchangeRate {\n    to: gbp\n}\n"),
        ["type.setting_value"]
    );
}

#[test]
fn a_type_built_on_a_fixed_currency_keeps_it() {
    let found = diagnostics("type Odd = CakePrice {\n    currency: Currency.eur\n}\n");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.constraint_contradiction");
    assert_eq!(
        found[0].message,
        "`currency: Currency.eur` is different from `CakePrice`'s `currency: Currency.gbp`"
    );
    let edits = &found[0].next.as_ref().expect("a next step").edits;
    assert_eq!(edits.len(), 1, "removes the setting");
    assert_eq!(edits[0].replacement, "");
    assert_eq!(
        rules("type Same = CakePrice {\n    currency: Currency.gbp\n}\n"),
        NONE,
        "repeating it is allowed"
    );
    assert_eq!(
        rules("type Odd = PoundsToEuros {\n    from: Currency.eur\n}\n"),
        ["type.constraint_contradiction"]
    );
    assert_eq!(
        rules("type Odd = PoundsToEuros {\n    to: Currency.usd\n}\n"),
        ["type.constraint_contradiction"]
    );
    assert_eq!(
        rules("type ToEuros = FromPounds {\n    to: Currency.eur\n}\n"),
        NONE,
        "a currency the parent leaves open may be fixed"
    );
}

#[test]
fn a_typed_rate_converts_with_no_attempt() {
    assert_eq!(
        rules(
            "function f(cake: CakePrice, rate: PoundsToEuros) -> Euros {\n    return Euros(money.convert(cake, rate))\n}\n"
        ),
        NONE,
        "the amount is in the rate's `from`, and the result in its `to`"
    );
    assert_eq!(
        rules(
            "function f(cake: CakePrice, rate: FromPounds) -> Amount {\n    return Amount(money.convert(cake, rate))\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function f(cake: CakePrice, rate: FromPounds) -> Euros {\n    return Euros(money.convert(cake, rate))\n}\n"
        ),
        ["failure.attempt_required"],
        "a `to` it doesn't fix may be another currency"
    );
    assert_eq!(
        rules(
            "function f(price: Charge, rate: PoundsToEuros) -> Amount {\n    return Amount(money.convert(price, rate))\n}\n"
        ),
        ["failure.attempt_required"],
        "an amount in any currency may be in another"
    );
}

#[test]
fn an_inverted_typed_rate_fails_across_a_function_boundary() {
    let found = diagnostics(
        "function f(cake: CakePrice, rate: EurosToPounds) -> Amount {\n    return Amount(money.convert(cake, rate))\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.mismatch");
    assert_eq!(
        found[0].message,
        "`rate` converts from EUR, and `cake` is in GBP"
    );
    assert_eq!(
        found[0].primary.as_ref().map(|label| label.text.as_str()),
        Some("a rate from EUR to GBP")
    );
    let found = diagnostics(
        "function f(price: Dollars, rate: PoundsToEuros) -> Amount {\n    return Amount(money.convert(price, rate))\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        found[0].message, "`rate` converts from GBP, and `price` is in USD",
        "a rate for a third currency"
    );
    assert_eq!(
        rules(
            "function f(cake: CakePrice, rate: PoundsToEuros) -> Euros {\n    return Euros(money.convert(cake, rate))\n}\n\nfunction g(cake: CakePrice, rate: EurosToPounds) -> Euros {\n    return f(cake, rate)\n}\n"
        ),
        ["type.mismatch"],
        "one rate type never stands in for the other"
    );
}

#[test]
fn building_a_rate_type_checks_its_currencies() {
    assert_eq!(
        rules(
            "function f(rate: ExchangeRate) -> PoundsToEuros {\n    return PoundsToEuros(rate)\n}\n"
        ),
        ["failure.attempt_required"],
        "a rate the app was given may be for any currencies"
    );
    assert_eq!(
        rules(
            "function f(rate: ExchangeRate) fails WrongRate -> PoundsToEuros {\n    var typed: PoundsToEuros = attempt PoundsToEuros(rate) {\n        from: WrongRate\n        to: WrongRate\n    }\n    return typed\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function f(rate: ExchangeRate) -> PoundsToEuros? {\n    if (rate.from == Currency.gbp and rate.to == Currency.eur) {\n        return PoundsToEuros(rate)\n    }\n    return none\n}\n"
        ),
        NONE,
        "a condition proves both"
    );
    assert_eq!(
        rules(
            "function f() -> PoundsToEuros {\n    return PoundsToEuros(ExchangeRate {\n        from: Currency.gbp\n        to: Currency.eur\n        rate: 1.17\n        at: \"2026-10-18T09:00:00Z\"\n    })\n}\n"
        ),
        NONE,
        "a rate written out fixes both"
    );
    assert_eq!(
        rules("function f(rate: PoundsToEuros) -> FromPounds {\n    return FromPounds(rate)\n}\n"),
        NONE,
        "its own type fixes them"
    );
    assert_eq!(
        rules(
            "function f(rate: ExchangeRate) fails WrongRate -> FromPounds {\n    var typed: FromPounds = attempt FromPounds(rate) {\n        from: WrongRate\n        to: WrongRate\n    }\n    return typed\n}\n"
        ),
        ["failure.unknown_mapping"],
        "a currency the type leaves open is never broken"
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

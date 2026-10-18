//! Area 19, "Money and payments" and "The standard operator table" (chunk
//! 5.5h), run: amounts add and compare in one currency, `*` and `/` round
//! by the type's `rounding:`, a currency known only while running fails
//! `currency_mismatch` where its `attempt` maps it, `split` and `allocate`
//! hand every minor unit out, and an amount is shown, read and written as
//! the contract says. Chunk 5.5h2: `money.convert` applies an
//! `ExchangeRate` from the amount's currency, and fails `currency_mismatch`
//! for one from another. Chunk 5.5h4: a rate type's `from:` and `to:`,
//! checked as a rate is built from one the app was given and as one is read
//! from JSON or text. The runtime's test (crates/jadpo-runtime/tests/
//! money.rs) holds the operations to the same values over many totals.

use crate::support;

use jadpo_interp::Value;
use support::{decimal, failure, int, list, main, text, value};

const SOURCE: &str = "type CakePrice = Money {\n    currency: Currency.gbp\n    min: 0\n}\n\ntype TaxedPrice = Money {\n    currency: Currency.gbp\n    rounding: Rounding.half_up\n}\n\ntype Charge = Money {}\n\ntype Pence = Int {}\n\ntype Pounds = Decimal {\n    scale: 3\n}\n\ntype Answer = Bool {}\n\ntype Shown = Text {}\n\ntype Mismatch = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype TooPrecise = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype WrongCurrency = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction total(a: CakePrice, b: CakePrice) -> CakePrice {\n    return CakePrice(a + b)\n}\n\nfunction cheaper(a: CakePrice, b: CakePrice) -> Answer {\n    return Answer(a < b)\n}\n\nfunction even_tax(price: CakePrice) -> Charge {\n    return Charge(price * 0.125)\n}\n\nfunction up_tax(price: TaxedPrice) -> Charge {\n    return Charge(price * 0.125)\n}\n\nfunction net(gross: TaxedPrice) -> Charge {\n    return Charge(gross / 1.20)\n}\n\nfunction thirds(total: CakePrice) -> List<Pence> {\n    return list.map(money.split(total, 3), share -> Pence(money.minor(share)))\n}\n\nfunction shares(total: CakePrice) -> List<Pence> {\n    return list.map(money.allocate(total, [1, 2, 0]), share -> Pence(money.minor(share)))\n}\n\nfunction basket(lines: List<CakePrice>) -> CakePrice {\n    return CakePrice(money.sum(lines, currency: Currency.gbp))\n}\n\nfunction entered(amount: Pounds, currency: Currency) fails TooPrecise -> Charge {\n    var made: Money = attempt money.of(amount, currency) {\n        too_many_places: TooPrecise\n    }\n    return Charge(made)\n}\n\nfunction combined(a: Charge, b: Charge) fails Mismatch -> Charge {\n    var sum: Money = attempt a + b {\n        currency_mismatch: Mismatch\n    }\n    return Charge(sum)\n}\n\nfunction checked(a: Charge, b: Charge) -> Charge {\n    if (money.currency(a) == money.currency(b)) {\n        return Charge(a + b)\n    }\n    return a\n}\n\nfunction priced(amount: Charge) fails WrongCurrency -> TaxedPrice {\n    var price: TaxedPrice = attempt TaxedPrice(amount) {\n        currency: WrongCurrency\n    }\n    return price\n}\n\nfunction shown(price: Charge, reader: Locale) -> PresentationText {\n    return text.format(price, locale: reader)\n}\n\nfunction whole(price: Charge) -> PresentationText {\n    return text.format(price, locale: Locale.en_gb, precision: MoneyPrecision.whole)\n}\n\nfunction canonical(price: Charge) -> Shown {\n    return Shown(text.from(price))\n}\n\nfunction converted(price: CakePrice, rate: ExchangeRate) fails Mismatch -> Charge {\n    var abroad: Money = attempt money.convert(price, rate) {\n        currency_mismatch: Mismatch\n    }\n    return Charge(abroad)\n}\n\nfunction converted_if_from_pounds(price: CakePrice, rate: ExchangeRate) -> Charge {\n    if (rate.from == Currency.gbp) {\n        return Charge(money.convert(price, rate))\n    }\n    return Charge(price)\n}\n";

fn currency(code: &str) -> Value {
    jadpo_value::money::currency_value(jadpo_std::money::ordinal_of_code(code).unwrap()).unwrap()
}

fn amount(code: &str, minor: i64) -> Value {
    jadpo_value::money::value(jadpo_std::money::ordinal_of_code(code).unwrap(), minor)
}

fn locale(name: &str) -> Value {
    let ordinal = jadpo_std::locale_ordinal(name).unwrap();
    Value::variant(jadpo_std::LOCALE, ordinal, name, Vec::new())
}

#[test]
fn amounts_in_one_currency_add_compare_and_round() {
    let program = main(SOURCE).with_proof_checks();
    let call = |name: &str, args: Vec<Value>| value(&program, name, args);
    assert_eq!(
        call("total", vec![amount("GBP", 1050), amount("GBP", 250)]),
        amount("GBP", 1300)
    );
    assert_eq!(
        call("cheaper", vec![amount("GBP", 1050), amount("GBP", 250)]),
        Value::Bool(false)
    );
    // 12.5p: half to even by default, half up where the type says.
    assert_eq!(
        call("even_tax", vec![amount("GBP", 100)]),
        amount("GBP", 12)
    );
    assert_eq!(call("up_tax", vec![amount("GBP", 100)]), amount("GBP", 13));
    // VAT taken off at 20%.
    assert_eq!(call("net", vec![amount("GBP", 1200)]), amount("GBP", 1000));
    assert_eq!(
        call("thirds", vec![amount("GBP", 1000)]),
        list(vec![int(334), int(333), int(333)])
    );
    assert_eq!(
        call("shares", vec![amount("GBP", 5)]),
        list(vec![int(2), int(3), int(0)])
    );
    assert_eq!(
        call(
            "basket",
            vec![list(vec![amount("GBP", 350), amount("GBP", 120)])]
        ),
        amount("GBP", 470)
    );
    assert_eq!(call("basket", vec![list(Vec::new())]), amount("GBP", 0));
}

#[test]
fn a_currency_known_only_while_running_is_checked_as_it_runs() {
    let program = main(SOURCE).with_proof_checks();
    assert_eq!(
        value(
            &program,
            "combined",
            vec![amount("EUR", 500), amount("EUR", 250)]
        ),
        amount("EUR", 750)
    );
    let refused = failure(
        &program,
        "combined",
        vec![amount("EUR", 500), amount("GBP", 250)],
    );
    assert_eq!(refused.name, "Mismatch");
    assert_eq!(
        value(
            &program,
            "checked",
            vec![amount("JPY", 500), amount("JPY", 20)]
        ),
        amount("JPY", 520)
    );
    assert_eq!(
        value(
            &program,
            "checked",
            vec![amount("JPY", 500), amount("USD", 20)]
        ),
        amount("JPY", 500)
    );
    assert_eq!(
        failure(&program, "priced", vec![amount("EUR", 500)]).name,
        "WrongCurrency"
    );
    assert_eq!(
        value(&program, "priced", vec![amount("GBP", 500)]),
        amount("GBP", 500)
    );
}

#[test]
fn an_amount_is_made_from_major_units_with_its_currencys_places() {
    let program = main(SOURCE);
    assert_eq!(
        value(
            &program,
            "entered",
            vec![decimal("10.500"), currency("GBP")]
        ),
        amount("GBP", 1050)
    );
    assert_eq!(
        value(&program, "entered", vec![decimal("1.250"), currency("BHD")]),
        amount("BHD", 1250)
    );
    assert_eq!(
        failure(
            &program,
            "entered",
            vec![decimal("10.505"), currency("GBP")]
        )
        .name,
        "TooPrecise"
    );
    assert_eq!(
        failure(
            &program,
            "entered",
            vec![decimal("10.500"), currency("JPY")]
        )
        .name,
        "TooPrecise"
    );
}

#[test]
fn an_amount_is_shown_and_written_as_the_contract_says() {
    let program = main(SOURCE);
    assert_eq!(
        value(
            &program,
            "shown",
            vec![amount("GBP", 123_450), locale("en_gb")]
        ),
        text("£1,234.50")
    );
    assert_eq!(
        value(
            &program,
            "shown",
            vec![amount("EUR", 123_450), locale("fr_fr")]
        ),
        text("1\u{202f}234,50\u{a0}€")
    );
    assert_eq!(
        value(&program, "whole", vec![amount("GBP", 123_450)]),
        text("£1,235")
    );
    assert_eq!(
        value(&program, "canonical", vec![amount("JPY", 500)]),
        text("JPY 500")
    );
}

fn rate(text: &str) -> Value {
    jadpo_value::standard::parse(text, "ExchangeRate").unwrap()
}

#[test]
fn a_rate_converts_an_amount_in_its_from_currency() {
    let program = main(SOURCE).with_proof_checks();
    let to_euros = rate("GBP/EUR 1.17 2026-10-18T09:00:00Z");
    let to_pounds = rate("EUR/GBP 0.85 2026-10-18T09:00:00Z");
    // €12.285, half to even.
    assert_eq!(
        value(
            &program,
            "converted",
            vec![amount("GBP", 1050), to_euros.clone()]
        ),
        amount("EUR", 1228)
    );
    assert_eq!(
        failure(
            &program,
            "converted",
            vec![amount("GBP", 1050), to_pounds.clone()]
        )
        .name,
        "Mismatch",
        "a rate the wrong way round is never applied"
    );
    assert_eq!(
        value(
            &program,
            "converted_if_from_pounds",
            vec![amount("GBP", 1050), to_euros]
        ),
        amount("EUR", 1228)
    );
    assert_eq!(
        value(
            &program,
            "converted_if_from_pounds",
            vec![amount("GBP", 1050), to_pounds]
        ),
        amount("GBP", 1050)
    );
}

const TYPED: &str = "type CakePrice = Money {\n    currency: Currency.gbp\n}\n\ntype Charge = Money {}\n\ntype PoundsToEuros = ExchangeRate {\n    from: Currency.gbp\n    to: Currency.eur\n}\n\ntype Sent = Text {}\n\ntype WrongRate = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype NotARate = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction priced(price: CakePrice, rate: PoundsToEuros) -> Charge {\n    return Charge(money.convert(price, rate))\n}\n\nfunction typed(rate: ExchangeRate) fails WrongRate -> PoundsToEuros {\n    var checked: PoundsToEuros = attempt PoundsToEuros(rate) {\n        from: WrongRate\n        to: WrongRate\n    }\n    return checked\n}\n\nfunction typed_if_checked(rate: ExchangeRate) -> PoundsToEuros? {\n    if (rate.from == Currency.gbp and rate.to == Currency.eur) {\n        return PoundsToEuros(rate)\n    }\n    return none\n}\n\nfunction from_json(sent: Sent) fails NotARate -> PoundsToEuros {\n    return attempt json.read(sent, as: PoundsToEuros) {\n        malformed: NotARate\n        mismatch: NotARate\n    }\n}\n\nfunction from_text(sent: Sent) fails NotARate, WrongRate -> PoundsToEuros {\n    return attempt text.parse(sent, as: PoundsToEuros) {\n        malformed: NotARate\n        from: WrongRate\n        to: WrongRate\n    }\n}\n";

#[test]
fn a_rate_type_fixes_its_currencies_as_it_runs() {
    let program = main(TYPED).with_proof_checks();
    let to_euros = rate("GBP/EUR 1.17 2026-10-18T09:00:00Z");
    let to_pounds = rate("EUR/GBP 0.85 2026-10-18T09:00:00Z");
    let to_dollars = rate("GBP/USD 1.30 2026-10-18T09:00:00Z");
    assert_eq!(
        value(
            &program,
            "priced",
            vec![amount("GBP", 1050), to_euros.clone()]
        ),
        amount("EUR", 1228)
    );
    assert_eq!(value(&program, "typed", vec![to_euros.clone()]), to_euros);
    assert_eq!(
        failure(&program, "typed", vec![to_pounds.clone()]).name,
        "WrongRate"
    );
    assert_eq!(
        failure(&program, "typed", vec![to_dollars.clone()]).name,
        "WrongRate"
    );
    assert_eq!(
        value(&program, "typed_if_checked", vec![to_euros.clone()]),
        to_euros
    );
    assert_eq!(
        value(&program, "typed_if_checked", vec![to_dollars]),
        Value::None
    );
}

#[test]
fn a_rate_read_at_the_boundary_is_checked_against_its_type() {
    let program = main(TYPED);
    let to_euros = rate("GBP/EUR 1.17 2026-10-18T09:00:00Z");
    let json = |from: &str, to: &str| {
        text(&format!(
            "{{\"from\":\"{from}\",\"to\":\"{to}\",\"rate\":\"1.17\",\"at\":\"2026-10-18T09:00:00Z\"}}"
        ))
    };
    assert_eq!(
        value(&program, "from_json", vec![json("GBP", "EUR")]),
        to_euros
    );
    assert_eq!(
        failure(&program, "from_json", vec![json("EUR", "GBP")]).name,
        "NotARate",
        "the wrong way round"
    );
    assert_eq!(
        failure(&program, "from_json", vec![json("GBP", "USD")]).name,
        "NotARate"
    );
    assert_eq!(
        value(
            &program,
            "from_text",
            vec![text("GBP/EUR 1.17 2026-10-18T09:00:00Z")]
        ),
        to_euros
    );
    assert_eq!(
        failure(
            &program,
            "from_text",
            vec![text("EUR/GBP 0.85 2026-10-18T09:00:00Z")]
        )
        .name,
        "WrongRate"
    );
    assert_eq!(
        failure(&program, "from_text", vec![text("GBP to EUR")]).name,
        "NotARate"
    );
}

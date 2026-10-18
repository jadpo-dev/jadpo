//! Area 19, "The standard operator table" (chunk 5.5j2), run: `20%` is a
//! `Percentage` of 0.20; an amount times a percentage rounds by its type's
//! `rounding:`; a number or a quantity times a percentage keeps its kind,
//! the quantity rounded half to even to its kind's scale; two percentages
//! add and multiply to a percentage; `Percentage(ratio)` makes 0.25 into
//! 25%; and a percentage is read, written and shown for people through
//! the locale. `Ratio` and `Fraction` are standard objects with their
//! canonical text. The runtime's test (crates/jadpo-runtime/tests/
//! percentages.rs) holds the operators to the same values.

use crate::support;

use jadpo_interp::Value;
use jadpo_std::QuantityKind;
use support::{decimal, failure, main, text, value};

const SOURCE: &str = "type CakePrice = Money {\n    currency: Currency.gbp\n}\n\ntype TaxedPrice = Money {\n    currency: Currency.gbp\n    rounding: Rounding.half_up\n}\n\ntype Rate = Percentage {}\n\ntype Discount = Percentage {\n    min: 0%\n    max: 100%\n}\n\ntype Board = Length {}\n\ntype Dose = Mass {}\n\ntype Share = Decimal {\n    scale: 4\n}\n\ntype Count = Int {}\n\ntype Answer = Bool {}\n\ntype Typed = Text {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype TooMuch = Failure {\n    kind: FailureKind.invalid_value\n}\n\n/// The standard rate of VAT.\nconstant standard_vat: Rate = 20%\n\nfunction vat(price: CakePrice) -> CakePrice {\n    return CakePrice(price * standard_vat)\n}\n\nfunction showcase() -> Board {\n    var total: Length = 2metre + 45cm + 3inch\n    var trim: Length = total * 15%\n    return Board(trim)\n}\n\nfunction showcase_label(reader: Locale) -> PresentationText {\n    var total: Length = 2metre + 45cm + 3inch\n    var trim: Length = total * 15%\n    return text.format(trim, locale: reader, unit: LengthUnit.centimetre)\n}\n\nfunction even_vat(price: CakePrice) -> CakePrice {\n    return CakePrice(price * 12.5%)\n}\n\nfunction up_vat(price: TaxedPrice) -> TaxedPrice {\n    return TaxedPrice(price * 12.5%)\n}\n\nfunction charged(price: CakePrice, rate: Rate) -> CakePrice {\n    return CakePrice(price * rate)\n}\n\nfunction scaled(s: Share, rate: Rate) -> Share {\n    return Share(s * rate)\n}\n\nfunction counted(n: Count, rate: Rate) -> Share {\n    return Share(n * rate)\n}\n\nfunction thin(a: Board) -> Board {\n    return Board(a * 0.00015%)\n}\n\nfunction both(a: Rate, b: Rate) -> Rate {\n    return Rate(a * b)\n}\n\nfunction added(a: Rate, b: Rate) -> Rate {\n    return Rate(a + b - 5%)\n}\n\nfunction twice(a: Rate) -> Rate {\n    return Rate(a * 2)\n}\n\nfunction bigger(a: Rate, b: Rate) -> Answer {\n    return Answer(a > b)\n}\n\nfunction same() -> Answer {\n    return Answer(20% == Percentage(0.2))\n}\n\nfunction quarter() -> Rate {\n    return Rate(Percentage(0.25))\n}\n\nfunction share_of(ratio: Share) -> Rate {\n    return Rate(ratio)\n}\n\nfunction capped(rate: Rate) fails TooMuch -> Discount {\n    var off: Discount = attempt Discount(rate) {\n        min: TooMuch\n        max: TooMuch\n    }\n    return off\n}\n\nfunction shown(rate: Rate, reader: Locale) -> PresentationText {\n    return text.format(rate, locale: reader)\n}\n\nfunction written(rate: Rate) -> Typed {\n    return Typed(text.from(rate))\n}\n\nfunction read(raw: Typed) fails Unreadable -> Rate {\n    var rate: Rate = attempt text.parse(raw, as: Rate) {\n        malformed: Unreadable\n    }\n    return rate\n}\n\nfunction screen() -> Typed {\n    var wide: Ratio = Ratio {\n        numerator: 16.0\n        denominator: 9.0\n    }\n    return Typed(text.from(wide))\n}\n\nfunction three_quarters() -> Typed {\n    var part: Fraction = Fraction {\n        numerator: 3\n        denominator: 4\n    }\n    return Typed(text.from(part))\n}\n\nfunction top(raw: Typed) fails Unreadable -> Count {\n    var part: Fraction = attempt text.parse(raw, as: Fraction) {\n        malformed: Unreadable\n    }\n    return Count(part.numerator)\n}\n\nfunction dose() -> Dose {\n    return Dose(1mcg)\n}\n\nfunction dose_label(given: Dose, reader: Locale) -> PresentationText {\n    return text.format(given, locale: reader, unit: MassUnit.microgram)\n}\n\nfunction dose_text(given: Dose) -> Typed {\n    return Typed(text.from(given))\n}\n";

fn percent(fraction: &str) -> Value {
    Value::Percentage(jadpo_value::Decimal::parse(fraction).unwrap())
}

fn amount(minor: i64) -> Value {
    jadpo_value::money::value(jadpo_std::money::ordinal_of_code("GBP").unwrap(), minor)
}

fn quantity(literal: &str) -> Value {
    let (unit, amount) = jadpo_std::quantity::parse_literal(literal).unwrap();
    jadpo_value::units::value(unit.kind, amount)
}

fn locale(name: &str) -> Value {
    let ordinal = jadpo_std::locale_ordinal(name).unwrap();
    Value::variant(jadpo_std::LOCALE, ordinal, name, Vec::new())
}

#[test]
fn the_showcase_gives_the_exact_metres_in_two_locales() {
    let program = main(SOURCE).with_proof_checks();
    let call = |name: &str, args: Vec<Value>| value(&program, name, args);
    // 2.5262 m times 15% is 0.37893 m, exact at a length's six places.
    assert_eq!(
        call("showcase", vec![]),
        jadpo_value::units::value(QuantityKind::Length, 378_930)
    );
    assert_eq!(
        call("showcase_label", vec![locale("en_gb")]),
        text("37.893 cm")
    );
    assert_eq!(
        call("showcase_label", vec![locale("de_de")]),
        text("37,893 cm")
    );
}

#[test]
fn the_table_s_percentage_rows_run() {
    let program = main(SOURCE).with_proof_checks();
    let call = |name: &str, args: Vec<Value>| value(&program, name, args);
    // 12.5% of £1.00 is 12.5p: half to even by default, half up where the
    // type says.
    assert_eq!(call("even_vat", vec![amount(100)]), amount(12));
    assert_eq!(call("up_vat", vec![amount(100)]), amount(13));
    assert_eq!(
        call("charged", vec![amount(1050), percent("0.20")]),
        amount(210)
    );
    assert_eq!(
        call("scaled", vec![decimal("80.00"), percent("0.15")]),
        decimal("12")
    );
    assert_eq!(
        call("counted", vec![Value::Int(3), percent("0.20")]),
        decimal("0.6")
    );
    // 1.5 micrometres, rounded half to even to a length's six places.
    assert_eq!(
        call("thin", vec![quantity("1metre")]),
        jadpo_value::units::value(QuantityKind::Length, 2)
    );
    assert_eq!(
        call("both", vec![percent("0.50"), percent("0.20")]),
        percent("0.10")
    );
    assert_eq!(
        call("added", vec![percent("0.20"), percent("0.10")]),
        percent("0.25")
    );
    assert_eq!(call("twice", vec![percent("0.20")]), percent("0.40"));
    assert_eq!(
        call("bigger", vec![percent("0.20"), percent("0.125")]),
        Value::Bool(true)
    );
    assert_eq!(call("same", vec![]), Value::Bool(true));
    assert_eq!(call("vat", vec![amount(1050)]), amount(210));
}

#[test]
fn a_ratio_becomes_a_percentage_by_construction() {
    let program = main(SOURCE).with_proof_checks();
    let call = |name: &str, args: Vec<Value>| value(&program, name, args);
    assert_eq!(call("quarter", vec![]), percent("0.25"));
    assert_eq!(call("share_of", vec![decimal("0.125")]), percent("0.125"));
    assert_eq!(call("capped", vec![percent("0.30")]), percent("0.30"));
    let failed = failure(&program, "capped", vec![percent("1.20")]);
    assert_eq!(failed.name, "TooMuch");
}

#[test]
fn a_percentage_is_read_written_and_shown() {
    let program = main(SOURCE).with_proof_checks();
    let call = |name: &str, args: Vec<Value>| value(&program, name, args);
    assert_eq!(
        call("shown", vec![percent("0.125"), locale("en_gb")]),
        text("12.5%")
    );
    assert_eq!(
        call("shown", vec![percent("0.20"), locale("de_de")]),
        text("20\u{a0}%")
    );
    assert_eq!(call("written", vec![percent("0.125")]), text("12.5%"));
    assert_eq!(call("written", vec![percent("0.20")]), text("20%"));
    assert_eq!(call("read", vec![text("20%")]), percent("0.2"));
    assert_eq!(call("read", vec![text("-2.5%")]), percent("-0.025"));
    for raw in ["20", "0.2", "%", "twenty%"] {
        let failed = failure(&program, "read", vec![text(raw)]);
        assert_eq!(failed.name, "Unreadable", "{raw}");
    }
}

#[test]
fn ratios_and_fractions_are_written_as_people_write_them() {
    let program = main(SOURCE).with_proof_checks();
    let call = |name: &str, args: Vec<Value>| value(&program, name, args);
    assert_eq!(call("screen", vec![]), text("16.0:9.0"));
    assert_eq!(call("three_quarters", vec![]), text("3/4"));
    assert_eq!(call("top", vec![text("-1/2")]), Value::Int(-1));
    for raw in ["3/0", "3", "3/4/5", "0.5/1", "3 / 4"] {
        let failed = failure(&program, "top", vec![text(raw)]);
        assert_eq!(failed.name, "Unreadable", "{raw}");
    }
}

#[test]
fn a_microgram_round_trips_exactly() {
    let program = main(SOURCE).with_proof_checks();
    let call = |name: &str, args: Vec<Value>| value(&program, name, args);
    let dose = call("dose", vec![]);
    assert_eq!(dose, jadpo_value::units::value(QuantityKind::Mass, 1));
    assert_eq!(
        call("dose_label", vec![dose.clone(), locale("en_gb")]),
        text("1 μg")
    );
    assert_eq!(call("dose_text", vec![dose]), text("0.000000001 kg"));
}

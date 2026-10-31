//! Area 19, "Physical quantities" (chunk 5.5j), run: a quantity is held in
//! its kind's base unit, so values made in different units add and
//! compare (`1km == 1000metre`); `*` and `/` by a number keep the kind and
//! two kinds give what the product table says; the `units` operations
//! make, read and add quantities; and a quantity is read from text, shown
//! in a chosen unit through the locale and written as its canonical text.
//! The runtime's test (crates/jadpo-runtime/tests/quantities.rs) holds the
//! conversions to the same values.

use crate::support;

use jadpo_interp::Value;
use jadpo_std::QuantityKind;
use support::{decimal, failure, list, main, text, value};

const SOURCE: &str = "type Shelf = Length {\n    min: 10cm\n    max: 3metre\n}\n\ntype Board = Length {}\n\ntype Floor = Area {}\n\ntype Pace = Speed {}\n\ntype Walk = Duration {\n    min: 1s\n}\n\ntype Room = Temperature {}\n\ntype Warming = TemperatureDifference {}\n\ntype Upload = DataSize {}\n\ntype Line = Bitrate {}\n\ntype Factor = Decimal {\n    scale: 4\n}\n\ntype Tally = Decimal {\n    scale: 2\n}\n\ntype Answer = Bool {}\n\ntype Typed = Text {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype WrongUnit = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype OutOfRange = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction showcase() -> Board {\n    var total: Length = 2metre + 45cm + 3inch\n    return Board(total)\n}\n\nfunction same() -> Answer {\n    return Answer(1km == 1000metre)\n}\n\nfunction longer(a: Board, b: Board) -> Answer {\n    return Answer(a > b)\n}\n\nfunction twice(a: Board) -> Board {\n    return Board(a * 2)\n}\n\nfunction share(a: Board) -> Board {\n    return Board(a / 4)\n}\n\nfunction ratio(a: Shelf, b: Shelf) -> Factor {\n    return Factor(a / b)\n}\n\nfunction floor(a: Board, b: Board) -> Floor {\n    return Floor(a * b)\n}\n\nfunction pace(d: Board, t: Walk) -> Pace {\n    return Pace(d / t)\n}\n\nfunction warmer(a: Room, b: Room) -> Warming {\n    return Warming(a - b)\n}\n\nfunction heated(a: Room, by: Warming) -> Room {\n    return Room(a + by)\n}\n\nfunction line(size: Upload, t: Walk) -> Line {\n    return Line(size / t)\n}\n\nfunction made(n: Tally) -> Board {\n    return Board(units.of(n, LengthUnit.mile))\n}\n\nfunction miles(a: Board) -> Tally {\n    return Tally(units.in(a, LengthUnit.mile, scale: 2, rounding: Rounding.half_even))\n}\n\nfunction total(items: List<Board>) -> Board {\n    return Board(units.sum(items))\n}\n\nfunction written_total() -> Board {\n    return Board(units.sum([1km, 500metre]))\n}\n\nfunction label(a: Board, reader: Locale) -> PresentationText {\n    return text.format(a, locale: reader, unit: LengthUnit.centimetre)\n}\n\nfunction plain(a: Board, reader: Locale) -> PresentationText {\n    return text.format(a, locale: reader)\n}\n\nfunction canonical(a: Board) -> Typed {\n    return Typed(text.from(a))\n}\n\nfunction entered(raw: Typed) fails Unreadable, WrongUnit, OutOfRange -> Shelf {\n    var read: Shelf = attempt text.parse(raw, as: Shelf) {\n        malformed: Unreadable\n        unknown_unit: WrongUnit\n        min: OutOfRange\n        max: OutOfRange\n    }\n    return read\n}\n\nfunction built(a: Board) fails OutOfRange -> Shelf {\n    var shelf: Shelf = attempt Shelf(a) {\n        min: OutOfRange\n        max: OutOfRange\n    }\n    return shelf\n}\n";

fn quantity(literal: &str) -> Value {
    let (unit, amount) = jadpo_std::quantity::parse_literal(literal).unwrap();
    jadpo_value::units::value(unit.kind, amount)
}

fn locale(name: &str) -> Value {
    let ordinal = jadpo_std::locale_ordinal(name).unwrap();
    Value::variant(jadpo_std::LOCALE, ordinal, name, Vec::new())
}

fn millis(n: i64) -> Value {
    Value::Duration(n)
}

#[test]
fn quantities_add_and_compare_across_units() {
    let program = main(SOURCE).with_proof_checks();
    let call = |name: &str, args: Vec<Value>| value(&program, name, args);
    assert_eq!(call("showcase", vec![]), quantity("2.5262metre"));
    assert_eq!(
        call("showcase", vec![]),
        jadpo_value::units::value(QuantityKind::Length, 2_526_200)
    );
    assert_eq!(call("same", vec![]), Value::Bool(true));
    assert_eq!(
        call("longer", vec![quantity("1km"), quantity("999metre")]),
        Value::Bool(true)
    );
    assert_eq!(call("twice", vec![quantity("45cm")]), quantity("90cm"));
    assert_eq!(call("share", vec![quantity("1metre")]), quantity("25cm"));
    assert_eq!(
        call("ratio", vec![quantity("1metre"), quantity("25cm")]),
        decimal("4")
    );
}

#[test]
fn kinds_multiply_and_divide_as_the_table_says() {
    let program = main(SOURCE).with_proof_checks();
    let call = |name: &str, args: Vec<Value>| value(&program, name, args);
    assert_eq!(
        call("floor", vec![quantity("3metre"), quantity("2metre")]),
        quantity("6sqm")
    );
    assert_eq!(
        call("pace", vec![quantity("100metre"), millis(8_000)]),
        quantity("12.5mps")
    );
    assert_eq!(
        call("warmer", vec![quantity("25degC"), quantity("20degC")]),
        jadpo_value::units::value(QuantityKind::TemperatureDifference, 5_000)
    );
    assert_eq!(
        call(
            "heated",
            vec![
                quantity("20degC"),
                jadpo_value::units::value(QuantityKind::TemperatureDifference, 5_000)
            ]
        ),
        quantity("25degC")
    );
    assert_eq!(
        call("line", vec![quantity("1MB"), millis(1_000)]),
        quantity("8Mbps")
    );
}

#[test]
fn the_units_operations_make_read_and_add_quantities() {
    let program = main(SOURCE).with_proof_checks();
    let call = |name: &str, args: Vec<Value>| value(&program, name, args);
    assert_eq!(call("made", vec![decimal("1.00")]), quantity("1mi"));
    assert_eq!(call("miles", vec![quantity("5km")]), decimal("3.11"));
    assert_eq!(
        call(
            "total",
            vec![list(vec![quantity("1km"), quantity("500metre")])]
        ),
        quantity("1.5km")
    );
    assert_eq!(call("total", vec![list(vec![])]), quantity("0metre"));
    assert_eq!(call("written_total", vec![]), quantity("1.5km"));
}

#[test]
fn a_quantity_is_read_shown_and_written_one_way() {
    let program = main(SOURCE).with_proof_checks();
    let call = |name: &str, args: Vec<Value>| value(&program, name, args);
    let total = quantity("2.5262metre");
    assert_eq!(
        call("label", vec![total.clone(), locale("en_gb")]),
        text("252.62 cm")
    );
    assert_eq!(
        call("label", vec![total.clone(), locale("de_de")]),
        text("252,62 cm")
    );
    assert_eq!(
        call("plain", vec![total.clone(), locale("en_gb")]),
        text("2.5262 m")
    );
    assert_eq!(call("canonical", vec![total]), text("2.5262 metre"));
    assert_eq!(call("entered", vec![text("90 cm")]), quantity("90cm"));
    assert_eq!(call("entered", vec![text("3 ft")]), quantity("3ft"));
    for (raw, code) in [
        ("90", "Unreadable"),
        ("90 kg", "WrongUnit"),
        ("5 metre", "OutOfRange"),
        ("1 cm", "OutOfRange"),
    ] {
        let failed = failure(&program, "entered", vec![text(raw)]);
        assert_eq!(failed.name, code, "{raw}");
    }
    assert_eq!(call("built", vec![quantity("1metre")]), quantity("1metre"));
    let failed = failure(&program, "built", vec![quantity("4metre")]);
    assert_eq!(failed.name, "OutOfRange");
}

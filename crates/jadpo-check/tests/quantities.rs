//! Area 19, "Physical quantities" (chunk 5.5j): a quantity is made with a
//! unit (a suffixed literal, `units.of`, `text.parse`, or another of its
//! kind by construction), a bare number where one is expected is
//! `type.unit_required`, never declinable, and a suffix that names no
//! unit, or misses one only by case, is `type.unit_unknown`; `+`, `-`
//! and `<` within one kind, `*` and `/` by a plain number, and the
//! products and quotients of the table, any other mix a compile error
//! naming the two kinds; `min:` and `max:` written with a unit; the
//! `units` operations, each taking units of its value's kind; and
//! `text.format`'s `unit:`.

use crate::support;

use jadpo_db::{Database, Layout};
use jadpo_diag::Diagnostic;

const NAMES: &str = "type Shelf = Length {\n    min: 0metre\n    max: 3metre\n}\n\ntype Board = Length {}\n\ntype Gap = Length {\n    min: 1mm\n}\n\ntype Floor = Area {}\n\ntype Tank = Volume {}\n\ntype Load = Mass {}\n\ntype Pace = Speed {}\n\ntype Trip = Duration {\n    min: 1s\n}\n\ntype Room = Temperature {}\n\ntype Warming = TemperatureDifference {}\n\ntype Noise = SoundLevel {}\n\ntype Upload = DataSize {}\n\ntype Line = Bitrate {}\n\ntype Answer = Bool {}\n\ntype Factor = Decimal {\n    scale: 4\n}\n\ntype Count = Int {}\n\ntype Places = Decimal {\n    scale: 2\n}\n\ntype Typed = Text {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype WrongUnit = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype TooLong = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

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
fn a_quantity_type_takes_bounds_written_with_a_unit() {
    assert_eq!(rules(""), NONE);
    assert_eq!(
        rules("type Width = Length {\n    min: 0\n}\n"),
        ["type.unit_required"]
    );
    assert_eq!(
        rules("type Width = Length {\n    max: 2kg\n}\n"),
        ["type.setting_value"]
    );
    assert_eq!(
        rules("type Width = Length {\n    min: 1m\n}\n"),
        ["type.setting_value"],
        "`m` is minutes"
    );
}

#[test]
fn quantities_are_made_with_a_unit() {
    let source = "function shelf() -> Shelf {\n    return Shelf(90cm)\n}\n\nfunction board() -> Board {\n    return Board(units.of(5, LengthUnit.kilometre))\n}\n\nfunction plank(given: Shelf) -> Board {\n    return Board(given)\n}\n\nfunction cold() -> Room {\n    return Room(-40degC)\n}\n\nfunction small() -> Upload {\n    return Upload(4KiB)\n}\n";
    assert_eq!(rules(source), NONE);
}

#[test]
fn a_bare_number_where_a_quantity_goes_is_unit_required() {
    let found = diagnostics("function shelf() -> Shelf {\n    return Shelf(90)\n}\n");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.unit_required");
    assert_eq!(
        found[0].message,
        "`Shelf` needs a unit, and `90` is a bare number"
    );
    assert!(!found[0].rule.declinable, "never declinable");
    let found = diagnostics("function board(n: Count) -> Board {\n    return Board(n)\n}\n");
    assert_eq!(
        found.iter().map(|d| d.rule.id).collect::<Vec<_>>(),
        ["type.unit_required"]
    );
    assert_eq!(
        rules("function sized(given: Board) -> Answer {\n    return Answer(given < 3)\n}\n"),
        ["type.unit_required"]
    );
}

#[test]
fn a_suffix_that_names_no_unit_is_reported_with_its_near_misses() {
    let found = diagnostics(
        "type Heater = Power {}\n\nfunction heater() -> Heater {\n    return Heater(5mw)\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.unit_unknown");
    assert!(
        found[0].message.contains("`5mW`") && found[0].message.contains("`5MW`"),
        "{}",
        found[0].message
    );
    assert_eq!(
        rules("function shelf() -> Shelf {\n    return Shelf(5zz)\n}\n"),
        ["type.unit_unknown"]
    );
    assert_eq!(
        rules("function shelf() -> Shelf {\n    return Shelf(5in)\n}\n"),
        ["type.unit_unknown"],
        "`in` is a keyword, so inches are `inch`"
    );
}

#[test]
fn one_kind_adds_compares_and_scales_across_units() {
    let source = "function total(a: Shelf) -> Board {\n    var sum: Length = 2metre + 45cm + 3inch\n    return Board(a + sum)\n}\n\nfunction same() -> Answer {\n    return Answer(1km == 1000metre)\n}\n\nfunction longer(a: Board, b: Board) -> Answer {\n    return Answer(a < b)\n}\n\nfunction twice(a: Board) -> Board {\n    return Board(a * 2)\n}\n\nfunction part(a: Board) -> Board {\n    return Board(a / 2.5)\n}\n\nfunction share(a: Gap, b: Gap) -> Factor {\n    return Factor(a / b)\n}\n\nfunction short(a: Board) -> Answer {\n    return Answer(a < 3metre)\n}\n";
    assert_eq!(rules(source), NONE);
}

#[test]
fn the_table_multiplies_and_divides_kinds() {
    let source = "function floor(a: Board, b: Board) -> Floor {\n    return Floor(a * b)\n}\n\nfunction tank(f: Floor, h: Board) -> Tank {\n    return Tank(f * h)\n}\n\nfunction pace(d: Board, t: Trip) -> Pace {\n    return Pace(d / t)\n}\n\nfunction paced(d: Board) -> Pace {\n    return Pace(d / 2h)\n}\n\nfunction warmer(a: Room, b: Room) -> Warming {\n    return Warming(a - b)\n}\n\nfunction heated(a: Room, by: Warming) -> Room {\n    return Room(a + by)\n}\n\nfunction line(size: Upload, t: Trip) -> Line {\n    return Line(size / t)\n}\n";
    assert_eq!(rules(source), NONE);
}

#[test]
fn a_mix_of_kinds_is_a_compile_error_naming_both() {
    let found =
        diagnostics("function f(a: Board, b: Load) -> Board {\n    return Board(a + b)\n}\n");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.arithmetic_operand");
    assert_eq!(
        found[0].message,
        "`Board` and `Load` can't be added: a `Length` and a `Mass` are different kinds"
    );
    let found =
        diagnostics("function f(a: Board, b: Load) -> Board {\n    return Board(a * b)\n}\n");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.arithmetic_operand");
    assert_eq!(
        found[0].message,
        "`Board` and `Load` can't be multiplied: a `Length` times a `Mass` is no quantity"
    );
    assert_eq!(
        rules("function f(a: Room, b: Room) -> Room {\n    return Room(a + b)\n}\n"),
        ["type.arithmetic_operand"],
        "a temperature adds only a difference"
    );
    assert_eq!(
        rules("function f(a: Noise, b: Noise) -> Noise {\n    return Noise(a + b)\n}\n"),
        ["type.arithmetic_operand"],
        "a sound level never adds"
    );
    assert_eq!(
        rules("function f(a: Board, b: Load) -> Answer {\n    return Answer(a < b)\n}\n"),
        ["type.ordering_operand"]
    );
    assert_eq!(
        rules("function f(a: Board) -> Board {\n    return Board(2 * a)\n}\n"),
        ["type.arithmetic_operand"],
        "the quantity is written first"
    );
    assert_eq!(
        rules("function f(a: Board) -> Answer {\n    return Answer(a == 2kg)\n}\n"),
        ["type.incomparable"]
    );
}

#[test]
fn the_units_operations_take_units_of_their_value_s_kind() {
    let source = "function miles(a: Board) -> Places {\n    return Places(units.in(a, LengthUnit.mile, scale: 2, rounding: Rounding.half_even))\n}\n\nfunction total(items: List<Board>) -> Board {\n    return Board(units.sum(items))\n}\n\nfunction label(a: Board, reader: Locale) -> PresentationText {\n    return text.format(a, locale: reader, unit: LengthUnit.centimetre)\n}\n\nfunction plain(a: Board, reader: Locale) -> PresentationText {\n    return text.format(a, locale: reader)\n}\n";
    assert_eq!(rules(source), NONE);
    assert_eq!(
        rules(
            "function f(a: Board) -> Places {\n    return Places(units.in(a, MassUnit.kilogram, scale: 2, rounding: Rounding.half_even))\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(
            "function f(a: Board, reader: Locale) -> PresentationText {\n    return text.format(a, locale: reader, unit: MassUnit.kilogram)\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(
            "function f(n: Count, reader: Locale) -> PresentationText {\n    return text.format(n, locale: reader, unit: LengthUnit.metre)\n}\n"
        ),
        ["type.temporal_option_unused"]
    );
    assert_eq!(
        rules("function f(items: List<Room>) -> Room {\n    return Room(units.sum(items))\n}\n"),
        ["type.arithmetic_operand"],
        "temperatures don't add up"
    );
}

#[test]
fn text_is_read_as_a_quantity_with_its_kind_s_units() {
    let source = "function shelf(raw: Typed) fails Unreadable, WrongUnit, TooLong -> Shelf {\n    var read: Shelf = attempt text.parse(raw, as: Shelf) {\n        malformed: Unreadable\n        unknown_unit: WrongUnit\n        max: TooLong\n        min: TooLong\n    }\n    return read\n}\n";
    assert_eq!(rules(source), NONE);
    assert_eq!(
        rules(
            "function shelf(raw: Typed) fails Unreadable, TooLong -> Shelf {\n    var read: Shelf = attempt text.parse(raw, as: Shelf) {\n        malformed: Unreadable\n        max: TooLong\n        min: TooLong\n    }\n    return read\n}\n"
        ),
        ["failure.rule_unmapped"]
    );
}

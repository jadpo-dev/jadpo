//! The Phase 1 review's reproducers (roadmap chunk 1.9): each mistake the
//! review found the checker accepting is now reported, once, with its rule.

mod support;

use jadpo_db::Layout;
use support::rules;

fn check(source: &str) -> Vec<String> {
    rules(Layout::Loose, &[("main.jadpo", source)])
}

const CLEAN: &[String] = &[];

const COUNTS: &str = "type Count = Int {\n    min: 0\n}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype DeliveryState = Enum {\n    pending\n    shipped\n}\n\ntype Refused = Failure {\n    kind: FailureKind.rejected\n}\n\ntype TooBig = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn with_counts(source: &str) -> Vec<String> {
    check(&format!("{COUNTS}{source}"))
}

// --- finding 2: `reject` and `invalid:` name failures ----------------------

#[test]
fn reject_names_only_a_failure() {
    assert_eq!(
        with_counts("function f(n: Count) -> Count {\n    reject FailureKind.rejected\n}\n"),
        ["semantic.wrong_name_kind"]
    );
    assert_eq!(
        with_counts(
            "function f(n: Count) -> Count {\n    reject DeliveryState.pending {\n        n: n\n    }\n}\n"
        ),
        ["semantic.wrong_name_kind"]
    );
}

#[test]
fn a_mapping_names_only_a_failure() {
    assert_eq!(
        with_counts(
            "function f(n: Count) -> Quantity {\n    var q: Quantity = attempt Quantity(n * 2) invalid: DeliveryState.pending\n    return q\n}\n"
        ),
        ["semantic.wrong_name_kind"]
    );
}

// --- finding 6: `reject` starts a statement or an arm ----------------------

#[test]
fn reject_is_never_a_value() {
    assert_eq!(
        with_counts(
            "action f() fails Refused -> Count {\n    var x: Count = reject Refused\n    return x\n}\n"
        ),
        ["failure.reject_placement"]
    );
    assert_eq!(
        with_counts(
            "function g(n: Count) -> Count {\n    return n\n}\n\nfunction f() fails Refused -> Count {\n    return g(reject Refused)\n}\n"
        ),
        ["failure.reject_placement"]
    );
    assert_eq!(
        with_counts(
            "function f(n: Count) fails Refused -> Count {\n    if (reject Refused) {\n        return n\n    }\n    return n\n}\n"
        ),
        ["failure.reject_placement"]
    );
    assert_eq!(
        with_counts(
            "function f(ns: List<Count>) fails Refused -> List<Count> {\n    return collection.map(ns, n -> reject Refused)\n}\n"
        ),
        ["failure.reject_placement"]
    );
}

#[test]
fn reject_still_stands_alone_and_ends_an_arm() {
    assert_eq!(
        with_counts(
            "function f(n: Count) fails Refused -> Count {\n    if (n > 3) {\n        reject Refused\n    }\n    return match n {\n        0 -> reject Refused\n        _ -> n\n    }\n}\n"
        ),
        CLEAN
    );
}

// --- finding 7: the standard library's divisions -------------------------

#[test]
fn a_standard_division_needs_a_divisor_that_excludes_zero() {
    assert_eq!(
        with_counts(
            "function f(a: Count, b: Count) -> Count {\n    return Count(integer.divide(a, b, rounding: Rounding.up))\n}\n"
        ),
        ["type.divisor_zero"]
    );
    assert_eq!(
        with_counts(
            "function f(a: Count) -> Count {\n    return Count(integer.divide(a, 0, rounding: Rounding.up))\n}\n"
        ),
        ["type.divisor_zero"]
    );
    assert_eq!(
        with_counts(
            "function f(a: Count, b: Quantity) fails TooBig -> Count {\n    return attempt Count(integer.divide(a, b, rounding: Rounding.up)) invalid: TooBig\n}\n"
        ),
        CLEAN
    );
}

// --- finding 4: functions never read the clock ---------------------------

const AT: &str = "type ReminderCutoff = Instant {}\n\n";

#[test]
fn only_actions_read_the_clock() {
    assert_eq!(
        check(&format!(
            "{AT}function cutoff() -> ReminderCutoff {{\n    return ReminderCutoff(clock.now)\n}}\n"
        )),
        ["type.clock_context"]
    );
    assert_eq!(
        check(&format!(
            "{AT}library dates {{\n    function cutoff() -> ReminderCutoff {{\n        return ReminderCutoff(clock.now)\n    }}\n}}\n"
        )),
        ["type.clock_context"]
    );
    assert_eq!(
        check(&format!(
            "{AT}type Flag = Bool {{}}\n\nfunction late(cutoffs: List<ReminderCutoff>) -> List<ReminderCutoff> {{\n    return collection.filter(cutoffs, c -> c < clock.now)\n}}\n"
        )),
        ["type.clock_context"]
    );
    assert_eq!(
        check(&format!(
            "{AT}action cutoff() -> ReminderCutoff {{\n    return ReminderCutoff(clock.now)\n}}\n"
        )),
        CLEAN
    );
}

#[test]
fn a_generated_namespace_member_not_built_yet_is_reported() {
    assert_eq!(
        check(&format!(
            "{AT}action cutoff() -> ReminderCutoff {{\n    return ReminderCutoff(clock.whenever)\n}}\n"
        )),
        ["toolchain.not_built_yet"]
    );
    assert_eq!(
        check(
            "type Count = Int {}\n\naction limit() -> Count {\n    var x: Count = config.no_such_setting\n    return x\n}\n"
        ),
        ["toolchain.not_built_yet"]
    );
    assert_eq!(
        check(&format!(
            "{AT}action cutoff() -> ReminderCutoff {{\n    return ReminderCutoff(clock.later())\n}}\n"
        )),
        ["toolchain.not_built_yet"]
    );
}

// --- finding 5: literals never fail at runtime ----------------------------

fn literal_into(ty: &str, base: &str, literal: &str) -> Vec<String> {
    check(&format!(
        "type {ty} = {base} {{}}\n\nfunction f() -> {ty} {{\n    return {literal}\n}}\n"
    ))
}

#[test]
fn time_literals_are_iso_8601_and_durations_have_units() {
    let invalid = ["type.invalid_literal"];
    assert_eq!(literal_into("At", "Instant", "\"not a time\""), invalid);
    assert_eq!(
        literal_into("At", "Instant", "\"2026-10-25T08:30:00.123456Z\""),
        invalid
    );
    assert_eq!(
        literal_into("At", "Instant", "\"2026-10-25T08:30:00Z\""),
        CLEAN
    );
    assert_eq!(
        literal_into("On", "CalendarDate", "\"2026-13-45\""),
        invalid
    );
    assert_eq!(literal_into("On", "CalendarDate", "\"2026-10-25\""), CLEAN);
    assert_eq!(
        literal_into("Wait", "Duration", "\"banana\""),
        ["type.mismatch"]
    );
    assert_eq!(literal_into("Wait", "Duration", "15m"), CLEAN);
}

#[test]
fn duration_bounds_are_checked() {
    assert_eq!(
        check(
            "type Delay = Duration {\n    max: 1h\n}\n\nfunction f() -> Delay {\n    return 2h\n}\n"
        ),
        ["type.invalid_literal"]
    );
    assert_eq!(
        check(
            "type Delay = Duration {\n    max: 1h\n}\n\nfunction f() -> Delay {\n    return 30m\n}\n"
        ),
        CLEAN
    );
}

#[test]
fn zones_weekdays_and_locales_are_enums() {
    assert_eq!(
        check("function f() -> Zone {\n    return Zone.mars_olympus\n}\n"),
        ["type.unknown_enum_variant"]
    );
    assert_eq!(
        check("function f() -> Zone {\n    return 5\n}\n"),
        ["type.mismatch"]
    );
    assert_eq!(
        check("function f() -> Weekday {\n    return \"monday\"\n}\n"),
        ["type.mismatch"]
    );
    assert_eq!(
        check(
            "function f(z: Zone) -> Zone {\n    var other: Zone = z.anything\n    return other\n}\n"
        ),
        ["type.field_on_non_record"]
    );
    assert_eq!(
        check(
            "function f() -> Zone {\n    return Zone.europe_london\n}\n\nfunction g() -> Locale {\n    return Locale.en_gb\n}\n\nfunction h() -> Weekday {\n    return Weekday.monday\n}\n"
        ),
        CLEAN
    );
}

#[test]
fn a_number_too_long_to_hold_is_invalid() {
    assert_eq!(
        check(&format!(
            "type Count = Int {{\n    max: 10\n}}\n\nfunction f() -> Count {{\n    return {}\n}}\n",
            "9".repeat(42)
        )),
        ["type.invalid_literal"]
    );
}

// --- finding 8: area 2's field rules hold on entities and events ----------

#[test]
fn entity_and_event_fields_follow_area_two() {
    let names = "type Name = Text {}\n\ntype Count = Int {}\n\n";
    let entity = |field: &str| {
        check(&format!(
            "{names}entity Thing {{\n    id: Uuid\n{field}}}\n"
        ))
    };
    assert_eq!(
        entity("    name: Name {\n        optional: true\n    }\n"),
        ["type.optional_outside_input"]
    );
    assert_eq!(
        entity("    n: Count {\n        max_length: 3\n    }\n"),
        ["type.constraint_base"]
    );
    assert_eq!(
        entity("    name: Name {\n        bogus: 1\n    }\n"),
        ["type.unknown_setting"]
    );
    assert_eq!(entity("    include: Name\n"), ["type.reserved_field_name"]);
    assert_eq!(entity("    n: Count {\n        min: 5\n    }\n"), CLEAN);
    assert_eq!(
        check(&format!(
            "{names}type Happened = Event {{\n    name: Name {{\n        default: \"x\"\n    }}\n}}\n"
        )),
        ["type.optional_outside_input"]
    );
}

// --- finding 9: rules contradict along the chain ----------------------------

#[test]
fn a_refinement_cannot_contradict_what_it_builds_on() {
    let contradiction = ["type.constraint_contradiction"];
    assert_eq!(
        check("type P = Int {\n    min: 10\n}\n\ntype T = P {\n    max: 3\n}\n"),
        contradiction
    );
    assert_eq!(
        check(
            "type Price = Decimal {\n    scale: 2\n}\n\ntype FinePrice = Price {\n    scale: 4\n}\n"
        ),
        contradiction
    );
    assert_eq!(
        check("type Delay = Duration {\n    min: 2h\n    max: 1h\n}\n"),
        contradiction
    );
    assert_eq!(
        check(
            "type P = Int {\n    min: 10\n}\n\ntype Box = Object {\n    p: P {\n        max: 5\n    }\n}\n"
        ),
        contradiction
    );
    assert_eq!(
        check(
            "type P = Int {\n    min: 10\n}\n\ntype T = P {\n    max: 30\n}\n\ntype Price = Decimal {\n    scale: 2\n}\n\ntype Small = Price {\n    max: 9.99\n}\n"
        ),
        CLEAN
    );
}

// --- finding 10: sets and maps as decided ----------------------------------

#[test]
fn a_boundary_map_is_keyed_by_text() {
    let base = "type Weight = Int {}\n\ntype At = Instant {}\n\ntype Tag = Text {}\n\n";
    assert_eq!(
        check(&format!(
            "{base}type A = Input {{\n    w: Map<At, Weight>\n}}\n"
        )),
        ["type.map_key_boundary"]
    );
    assert_eq!(
        check(&format!(
            "{base}type B = Output {{\n    w: List<Map<Weight, Weight>>\n}}\n"
        )),
        ["type.map_key_boundary"]
    );
    assert_eq!(
        check(&format!(
            "{base}type C = Output {{\n    w: Map<Tag, Weight>\n    v: Map<Uuid, Weight>\n}}\n\ntype D = Object {{\n    w: Map<Weight, Weight>\n}}\n"
        )),
        CLEAN
    );
}

#[test]
fn a_set_literal_holds_each_value_once() {
    let base = "type Tag = Text {}\n\n";
    assert_eq!(
        check(&format!(
            "{base}function f() -> Set<Tag> {{\n    return [\"a\", \"a\"]\n}}\n"
        )),
        ["type.invalid_literal"]
    );
    assert_eq!(
        check(&format!(
            "{base}function f() -> Set<Tag> {{\n    return [\"a\", \"b\"]\n}}\n"
        )),
        CLEAN
    );
}

// --- finding 25: `include` never loops ---------------------------------------

#[test]
fn include_never_leads_back_to_itself() {
    let base = "type Name = Text {}\n\n";
    assert_eq!(
        check(&format!(
            "{base}type A = Output {{\n    include: [A]\n    n: Name\n}}\n"
        )),
        ["type.copy_cycle"]
    );
    assert_eq!(
        check(&format!(
            "{base}type B = Output {{\n    include: [C]\n}}\n\ntype C = Output {{\n    include: [B]\n    x: Name\n}}\n"
        )),
        ["type.copy_cycle"]
    );
}

// --- finding 14: a test is known by its subject and sentence -----------------

#[test]
fn a_sentence_names_one_test_in_the_project() {
    assert_eq!(
        check(
            "test \"adds up\" {\n    assert 1 == 1\n}\n\ntest \"adds up\" {\n    assert 2 == 2\n}\n"
        ),
        ["semantic.duplicate_declaration"]
    );
    assert_eq!(
        rules(
            Layout::Loose,
            &[
                ("a.jadpo", "test \"adds up\" {\n    assert 1 == 1\n}\n"),
                ("b.jadpo", "test \"adds up\" {\n    assert 2 == 2\n}\n"),
            ]
        ),
        ["semantic.duplicate_declaration"]
    );
}

#[test]
fn moving_a_test_to_another_domain_keeps_its_key() {
    use jadpo_check::graph::graph;
    use jadpo_db::Database;
    use jadpo_graph::{Key, Kind};
    let key_of = |path: &str| {
        let db = Database::default();
        let project = support::project(
            &db,
            Layout::Project,
            &[
                support::APP,
                (
                    "domains/planning/types/count.jadpo",
                    "type Count = Int {}\n",
                ),
                (path, "test \"adds up\" {\n    assert 1 == 1\n}\n"),
            ],
        );
        graph(&db, project)
            .nodes()
            .iter()
            .filter(|node| node.kind == Kind::Test)
            .map(|node| node.key.clone())
            .collect::<Vec<Key>>()
    };
    let before = key_of("domains/planning/types/count.test.jadpo");
    let after = key_of("domains/billing/types/count.test.jadpo");
    assert_eq!(before, [Key::from_text("test count \"adds up\"")]);
    assert_eq!(before, after);
}

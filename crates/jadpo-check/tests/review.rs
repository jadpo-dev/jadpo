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

// --- finding 2: `reject` and mappings name failures ----------------------

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
            "function f(n: Count) -> Quantity {\n    var q: Quantity = attempt Quantity(n * 2) {\n        invalid: DeliveryState.pending\n    }\n    return q\n}\n"
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
            "function f(a: Count, b: Quantity) -> Count {\n    return Count(integer.divide(a, b, rounding: Rounding.up))\n}\n"
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

// --- finding 23: what may be public, and own-domain calls are bare ----------

#[test]
fn only_a_domains_interface_can_be_public() {
    let in_project = |path: &str, text: &str| {
        rules(
            Layout::Project,
            &[
                support::APP,
                (
                    "domains/planning/types/count.jadpo",
                    "type Count = Int {}\n",
                ),
                (
                    "domains/planning/actions/keep.jadpo",
                    "action keep(n: Count) -> Count {\n    return n\n}\n",
                ),
                (path, text),
            ],
        )
    };
    assert_eq!(
        in_project(
            "domains/planning/types/count.test.jadpo",
            "public test \"adds up\" {\n    assert 1 == 1\n}\n"
        ),
        ["visibility.public_not_allowed"]
    );
    assert_eq!(
        in_project(
            "domains/planning/actions/tick.jadpo",
            "action tick(n: Count) -> Count {\n    return helper(n)\n}\n\npublic function helper(n: Count) -> Count {\n    return n\n}\n"
        ),
        ["visibility.public_not_allowed"]
    );
    assert_eq!(
        in_project(
            "domains/planning/actions/tick.jadpo",
            "public action tick() -> Unit {}\n"
        ),
        CLEAN
    );
}

#[test]
fn an_own_domain_call_is_bare() {
    assert_eq!(
        rules(
            Layout::Project,
            &[
                support::APP,
                (
                    "domains/planning/actions/snooze_todo.jadpo",
                    "action snooze_todo() -> Unit {}\n"
                ),
                (
                    "domains/planning/actions/tick.jadpo",
                    "action tick() -> Unit {\n    planning.snooze_todo()\n}\n"
                ),
            ]
        ),
        ["semantic.own_domain_qualification"]
    );
}

// --- finding 25: an entity's operations --------------------------------------

#[test]
fn an_entity_has_only_its_operations() {
    let entity = "entity Order {\n    id: Uuid\n    deleted_at: Instant?\n\n    lifecycle: {\n        transitions: {\n            archive: {\n                set: {\n                    deleted_at: clock.now\n                }\n            }\n        }\n    }\n}\n\n";
    assert_eq!(
        check(&format!(
            "{entity}action f(id: Order.id) -> Order.id {{\n    var order: Order = attempt Order.fetch(id)\n    return order.id\n}}\n"
        )),
        ["semantic.unknown_callee"]
    );
    assert_eq!(
        check(&format!(
            "{entity}action f(id: Order.id) -> Order.id {{\n    Order.archive(id)\n    return Order.id(id)\n}}\n"
        )),
        CLEAN
    );
}

// --- finding 25: operators read literals and text alike -----------------------

#[test]
fn a_literal_takes_the_other_sides_type_in_arithmetic_as_in_comparison() {
    let price = "type Price = Decimal {\n    scale: 2\n}\n\n";
    assert_eq!(
        check(&format!(
            "{price}function f(a: Price) -> Price {{\n    return Price(a + 3)\n}}\n"
        )),
        ["type.arithmetic_operand"]
    );
    assert_eq!(
        check(&format!(
            "{price}type Flag = Bool {{}}\n\nfunction f(a: Price) -> Flag {{\n    return Flag(a == 3)\n}}\n"
        )),
        ["type.incomparable"]
    );
    assert_eq!(
        check(&format!(
            "{price}function f(a: Price) -> Price {{\n    return Price(a + 3.00)\n}}\n"
        )),
        CLEAN
    );
}

#[test]
fn joining_text_needs_the_same_type_as_comparing_it() {
    let names = "type First = Text {}\n\ntype Last = Text {}\n\ntype Full = Text {}\n\ntype Flag = Bool {}\n\n";
    assert_eq!(
        check(&format!(
            "{names}function f(a: First, b: Last) -> Full {{\n    return Full(a + b)\n}}\n"
        )),
        ["type.arithmetic_operand"]
    );
    assert_eq!(
        check(&format!(
            "{names}function f(a: First, b: Last) -> Flag {{\n    return Flag(a == b)\n}}\n"
        )),
        ["type.incomparable"]
    );
    assert_eq!(
        check(&format!(
            "{names}function f(a: First, b: First) -> Full {{\n    return Full(a + \" \" + b)\n}}\n"
        )),
        CLEAN
    );
}

#[test]
fn brackets_around_a_matched_call_only_group_it() {
    let risky = "type Count = Int {}\n\ntype Nope = Failure {\n    kind: FailureKind.rejected\n}\n\nfunction risky(n: Count) fails Nope -> Count {\n    if (n > 3) {\n        reject Nope\n    }\n    return n\n}\n\n";
    assert_eq!(
        check(&format!(
            "{risky}function f(n: Count) -> Count {{\n    return match (risky(n)) {{\n        success(v) -> v\n        Nope -> n\n    }}\n}}\n"
        )),
        CLEAN
    );
}

// --- finding 25: reserved words -----------------------------------------------

#[test]
fn declaration_words_never_name_anything() {
    let name = "type Name = Text {}\n\n";
    for source in [
        "function test(n: Name) -> Name {\n    return n\n}\n",
        "function f(package: Name) -> Name {\n    return package\n}\n",
        "function f(n: Name) -> Name {\n    var seed: Name = n\n    return seed\n}\n",
        "type Thing = Object {\n    resource: Name\n}\n",
        "type Thing = Object {\n    type: Name\n}\n",
    ] {
        assert_eq!(
            check(&format!("{name}{source}")),
            ["syntax.expected_name"],
            "{source}"
        );
    }
    // A setting the language names keeps its key.
    assert_eq!(
        check(&format!(
            "{name}type Thing = Object {{\n    input: Name\n    value: Name\n}}\n"
        )),
        CLEAN
    );
}

#[test]
fn a_move_suggestion_never_names_where_the_file_already_is() {
    let db = jadpo_db::Database::default();
    let project = support::project(
        &db,
        Layout::Project,
        &[
            support::APP,
            (
                "shared/actions/x.jadpo",
                "type Count = Int {}\n\naction x(n: Count) -> Count {\n    return n\n}\n",
            ),
        ],
    );
    let problems = jadpo_check::check(&db, project);
    let location = problems
        .iter()
        .find(|d| d.rule.id == "layout.unknown_location")
        .expect("unknown location");
    let next = location
        .next
        .as_ref()
        .map(|n| n.text.clone())
        .unwrap_or_default();
    assert_eq!(next, "Move the file to `domains/<domain>/actions/x.jadpo`");
}

// --- area 1: a type may fold to the callable in its own file ------------------

#[test]
fn a_type_may_share_its_folded_name_with_the_callable_in_its_file() {
    let input = "type Title = Text {}\n\ntype CreateTodo = Input {\n    title: Title\n}\n\naction create_todo(request: CreateTodo) -> Title {\n    return request.title\n}\n";
    assert_eq!(
        rules(
            Layout::Project,
            &[
                support::APP,
                ("domains/planning/actions/create_todo.jadpo", input)
            ]
        ),
        CLEAN
    );
    assert_eq!(
        rules(
            Layout::Loose,
            &[
                ("due.jadpo", "type DueDate = CalendarDate {}\n"),
                (
                    "main.jadpo",
                    "function due_date(due: DueDate) -> DueDate {\n    return due\n}\n"
                ),
            ]
        ),
        ["naming.folded_collision"]
    );
}

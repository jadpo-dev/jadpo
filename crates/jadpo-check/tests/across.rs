//! Area 2, "Rules across fields" (chunk 5.5t), in the checker: an object
//! type names rules across its fields; an object literal that may break one
//! is mapped by the rule's name, and one the declared ranges, the literals
//! written or a condition around it prove takes no arm; a rule is a `Bool`
//! over its type's own fields, written only in an `Object`, `Input` or
//! `Output`, under a name of its own; and each rule is a node of the graph.

use crate::support;

use jadpo_db::{Database, Layout};
use jadpo_graph::{Detail, Kind};

const NAMES: &str = "type Moment = Instant {}\n\ntype Day = CalendarDate {}\n\ntype Backwards = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype TooLong = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype Nights = Int {\n    min: 1\n    max: 30\n}\n\ntype Count = Int {}\n\ntype Booking = Object {\n    starts: Moment\n    ends: Moment\n    rule ends_after_start: ends > starts\n}\n\n";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn a_literal_that_may_break_a_rule_is_mapped_by_its_name() {
    assert_eq!(
        rules(
            "function book(starts: Moment, ends: Moment) fails Backwards -> Booking {\n    var booking: Booking = attempt Booking {\n        starts: starts\n        ends: ends\n    } {\n        ends_after_start: Backwards\n    }\n    return booking\n}\n"
        ),
        NONE
    );
    // `invalid:` covers it as it covers any rule.
    assert_eq!(
        rules(
            "function book(starts: Moment, ends: Moment) fails Backwards -> Booking {\n    return attempt Booking {\n        starts: starts\n        ends: ends\n    } {\n        invalid: Backwards\n    }\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function book(starts: Moment, ends: Moment) -> Booking {\n    return Booking {\n        starts: starts\n        ends: ends\n    }\n}\n"
        ),
        ["failure.attempt_required"]
    );
    assert_eq!(
        rules(
            "function book(starts: Moment, ends: Moment) fails Backwards -> Booking {\n    return attempt Booking {\n        starts: starts\n        ends: ends\n    } {\n        ends_after: Backwards\n    }\n}\n"
        ),
        ["failure.unknown_mapping"],
        "a misspelt rule"
    );
    assert_eq!(
        rules(
            "function book(starts: Moment, ends: Moment) fails Backwards -> Booking {\n    return attempt Booking {\n        starts: starts\n        ends: ends\n    } {\n    }\n}\n"
        ),
        ["failure.rule_unmapped"]
    );
}

#[test]
fn a_condition_around_the_literal_proves_its_rule() {
    // After a check that leaves.
    assert_eq!(
        rules(
            "function book(starts: Moment, ends: Moment) fails Backwards -> Booking {\n    if (ends <= starts) {\n        reject Backwards\n    }\n    return Booking {\n        starts: starts\n        ends: ends\n    }\n}\n"
        ),
        NONE
    );
    // Written the other way round, inside the branch where it holds.
    assert_eq!(
        rules(
            "function book(starts: Moment, ends: Moment) fails Backwards -> Booking {\n    if (starts < ends) {\n        return Booking {\n            starts: starts\n            ends: ends\n        }\n    }\n    reject Backwards\n}\n"
        ),
        NONE
    );
    // A proved rule takes no `attempt`, and its arm can't run.
    assert_eq!(
        rules(
            "function book(starts: Moment, ends: Moment) fails Backwards -> Booking {\n    if (ends <= starts) {\n        reject Backwards\n    }\n    return attempt Booking {\n        starts: starts\n        ends: ends\n    } {\n        ends_after_start: Backwards\n    }\n}\n"
        ),
        ["failure.attempt_infallible"]
    );
    // A weaker check proves nothing: `ends` may equal `starts`.
    assert_eq!(
        rules(
            "function book(starts: Moment, ends: Moment) fails Backwards -> Booking {\n    if (ends < starts) {\n        reject Backwards\n    }\n    return Booking {\n        starts: starts\n        ends: ends\n    }\n}\n"
        ),
        ["failure.attempt_required"]
    );
    // Pointing a variable at a new value ends what was known of it.
    assert_eq!(
        rules(
            "function book(starts: Moment, ends: Moment, later: Moment) fails Backwards -> Booking {\n    var mut finish: Moment = ends\n    if (finish <= starts) {\n        reject Backwards\n    }\n    finish = later\n    return Booking {\n        starts: starts\n        ends: finish\n    }\n}\n"
        ),
        ["failure.attempt_required"]
    );
}

#[test]
fn each_rule_is_proved_or_mapped_on_its_own() {
    let stay = "type Stay = Object {\n    arrives: Day\n    leaves: Day\n    nights: Nights\n    rule leaves_after_arrival: leaves > arrives\n    rule short: nights <= 14\n}\n\n";
    assert_eq!(
        rules(&format!(
            "{stay}function stay(arrives: Day, leaves: Day, nights: Nights) fails Backwards, TooLong -> Stay {{\n    if (nights > 14) {{\n        reject TooLong\n    }}\n    return attempt Stay {{\n        arrives: arrives\n        leaves: leaves\n        nights: nights\n    }} {{\n        leaves_after_arrival: Backwards\n    }}\n}}\n"
        )),
        NONE
    );
    assert_eq!(
        rules(&format!(
            "{stay}function stay(arrives: Day, leaves: Day, nights: Nights) fails Backwards, TooLong -> Stay {{\n    if (nights > 14) {{\n        reject TooLong\n    }}\n    return attempt Stay {{\n        arrives: arrives\n        leaves: leaves\n        nights: nights\n    }} {{\n        leaves_after_arrival: Backwards\n        short: TooLong\n    }}\n}}\n"
        )),
        ["failure.rule_cannot_break"]
    );
    assert_eq!(
        rules(&format!(
            "{stay}function stay(arrives: Day, leaves: Day, nights: Nights) fails Backwards -> Stay {{\n    return attempt Stay {{\n        arrives: arrives\n        leaves: leaves\n        nights: nights\n    }} {{\n        leaves_after_arrival: Backwards\n    }}\n}}\n"
        )),
        ["failure.rule_unmapped"],
        "`short` can break too"
    );
    // `and` needs both sides proved, `or` either.
    let both = "type Band = Object {\n    low: Nights\n    high: Nights\n    rule ordered: low <= high and high <= 30\n    rule some: low >= 1 or high > 40\n}\n\n";
    assert_eq!(
        rules(&format!(
            "{both}function band(low: Nights, high: Nights) fails Backwards -> Band {{\n    if (low > high) {{\n        reject Backwards\n    }}\n    return Band {{\n        low: low\n        high: high\n    }}\n}}\n"
        )),
        NONE
    );
}

#[test]
fn declared_ranges_and_literals_prove_a_rule() {
    let party = "type Adults = Int {\n    min: 1\n    max: 12\n}\n\ntype Children = Int {\n    min: 0\n    max: 6\n}\n\ntype Party = Object {\n    adults: Adults\n    children: Children\n    rule not_crowded: children <= adults * 6\n    rule even: children % 2 == 0\n}\n\n";
    assert_eq!(
        rules(&format!(
            "{party}function party(adults: Adults, children: Children) fails Backwards -> Party {{\n    return attempt Party {{\n        adults: adults\n        children: children\n    }} {{\n        even: Backwards\n    }}\n}}\n"
        )),
        NONE,
        "`not_crowded` always holds"
    );
    let band = "type Band = Object {\n    low: Nights\n    high: Nights\n    rule ordered: low <= high\n}\n\n";
    assert_eq!(
        rules(&format!(
            "{band}function band() -> Band {{\n    return Band {{\n        low: 3\n        high: 5\n    }}\n}}\n"
        )),
        NONE
    );
    assert_eq!(
        rules(&format!(
            "{band}function band() -> Band {{\n    return Band {{\n        low: 5\n        high: 3\n    }}\n}}\n"
        )),
        ["type.invalid_literal"],
        "5 is above 3"
    );
    assert_eq!(
        rules(
            "function backwards() -> Booking {\n    return Booking {\n        starts: \"2026-03-02T10:00:00Z\"\n        ends: \"2026-03-02T09:00:00Z\"\n    }\n}\n"
        ),
        ["type.invalid_literal"]
    );
    assert_eq!(
        rules(
            "function forwards() -> Booking {\n    return Booking {\n        starts: \"2026-03-02T09:00:00Z\"\n        ends: \"2026-03-02T10:00:00Z\"\n    }\n}\n"
        ),
        NONE
    );
}

#[test]
fn a_rule_is_a_condition_over_its_type_s_own_fields() {
    let declared = |rule: &str| {
        rules(&format!(
            "type Band = Object {{\n    low: Nights\n    high: Nights\n    rule {rule}\n}}\n\n"
        ))
    };
    assert_eq!(declared("ordered: low <= high"), NONE);
    assert_eq!(declared("spread: high - low"), ["type.rule_not_condition"]);
    assert_eq!(declared("ordered: low <= top"), ["type.unknown_value"]);
    assert_eq!(
        declared("short: integer.max(low, high) <= 14"),
        NONE,
        "a standard operation that gives its result from its arguments"
    );
    assert_eq!(
        rules(
            "type Opens = Bool {}\n\nfunction opens(at: Nights) -> Opens {\n    return Opens(true)\n}\n\ntype Band = Object {\n    low: Nights\n    high: Nights\n    rule open: opens(low)\n}\n"
        ),
        ["type.rule_outside_fields"]
    );
    assert_eq!(
        rules(
            "constant longest: Nights = 14\n\ntype Band = Object {\n    low: Nights\n    high: Nights\n    rule short: high <= longest\n}\n"
        ),
        ["type.rule_outside_fields"]
    );
    // A field that may be `none` is matched.
    assert_eq!(
        rules(
            "type Search = Input {\n    from: Day\n    until: Day {\n        optional: true\n    }\n    rule ordered: match until {\n        some(day) -> day >= from\n        none -> true\n    }\n}\n"
        ),
        NONE
    );
    // `rule` is still a field's name.
    assert_eq!(
        rules("type Note = Text {}\n\ntype Policy = Object {\n    rule: Note\n}\n"),
        NONE
    );
}

#[test]
fn a_rule_has_a_name_of_its_own_in_an_object_type() {
    let declared = |rule: &str| {
        rules(&format!(
            "type Band = Object {{\n    low: Nights\n    high: Nights\n    {rule}\n}}\n\n"
        ))
    };
    assert_eq!(declared("rule low: low <= high"), ["type.rule_name_taken"]);
    assert_eq!(
        declared("rule invalid: low <= high"),
        ["type.rule_name_taken"]
    );
    assert_eq!(
        declared("rule ordered: low <= high\n    rule ordered: low < high"),
        ["type.rule_name_taken"]
    );
    for declaration in [
        "type Short = Int {\n    max: 14\n    rule small: Short <= 14\n}\n",
        "type Shade = Enum {\n    red\n    rule small: true\n}\n",
        "entity Room {\n    id: Uuid\n    beds: Nights\n    rule small: beds <= 4\n}\n",
        "type Shown = Event {\n    nights: Nights\n    rule small: nights <= 4\n}\n",
    ] {
        assert_eq!(
            rules(declaration),
            ["type.rule_outside_object"],
            "{declaration}"
        );
    }
    // An `Input` and an `Output` name rules too.
    assert_eq!(
        rules(
            "type Ask = Input {\n    low: Nights\n    high: Nights\n    rule ordered: low <= high\n}\n\ntype Told = Output {\n    low: Nights\n    high: Nights\n    rule ordered: low <= high\n}\n"
        ),
        NONE
    );
}

#[test]
fn each_rule_is_a_node_of_its_type() {
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", NAMES)]);
    let graph = jadpo_check::graph::graph(&db, project);
    let rule = graph
        .nodes()
        .iter()
        .find(|node| node.kind == Kind::Rule)
        .expect("a rule node");
    assert_eq!(rule.name, "Booking.ends_after_start");
    assert_eq!(
        rule.detail,
        Detail::Rule {
            order: 0,
            condition: "ends > starts".to_string(),
        }
    );
    assert_eq!(rule.parent.as_ref().map(|key| key.name()), Some("Booking"));
}

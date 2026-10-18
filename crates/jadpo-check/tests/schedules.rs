//! Area 19, "Time and calendar additions" (chunk 5.5k2), in the checker:
//! `temporal.occurrences` takes a `count` proved from 1 to 1000, so an
//! unbounded rule is never expanded without a bound, and a
//! `RecurrenceRule` or `OpeningHours` written out is checked while
//! compiling, as any type held as text is.

use crate::support;

use jadpo_db::Layout;

fn check(source: &str) -> Vec<String> {
    let types = "type ClassSchedule = RecurrenceRule {}\n\ntype ClassTime = ZonedTime {}\n\ntype SearchFrom = Instant {}\n\ntype BarHours = OpeningHours {}\n\ntype IsOpen = Bool {}\n\ntype Wanted = Int {\n    min: 1\n    max: 10\n}\n\ntype Many = Int {\n    min: 1\n}\n\ntype Plain = Text {}\n\n";
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{types}{source}"))],
    )
}

fn occurrences(rule: &str, count: &str) -> String {
    format!(
        "function f(timetable: ClassSchedule, from: SearchFrom, wanted: Wanted, many: Many, plain: Plain) -> List<ClassTime> {{\n    return list.map(temporal.occurrences({rule}, from: from, count: {count}), at -> ClassTime(at))\n}}\n"
    )
}

#[test]
fn a_count_is_proved_from_1_to_1000() {
    assert_eq!(
        check(&occurrences("timetable", "wanted")),
        Vec::<String>::new()
    );
    assert_eq!(
        check(&occurrences("timetable", "1000")),
        Vec::<String>::new()
    );
    for count in ["0", "1001", "-1"] {
        assert_eq!(
            check(&occurrences("timetable", count)),
            ["type.mismatch"],
            "`count: {count}`"
        );
    }
    // A type with no `max`, and a plain number, can be any size.
    assert_eq!(check(&occurrences("timetable", "many")), ["type.mismatch"]);
    assert_eq!(
        check(&occurrences("timetable", "list.count([1, 2])")),
        ["type.mismatch"]
    );
}

#[test]
fn a_rule_written_out_is_checked_while_compiling() {
    assert_eq!(
        check(&occurrences(
            "\"DTSTART;TZID=Europe/London:20261026T093000\\nRRULE:FREQ=WEEKLY;BYDAY=MO\"",
            "3"
        )),
        Vec::<String>::new()
    );
    // 26 October 2026 is a Monday, which the rule doesn't give.
    assert_eq!(
        check(&occurrences(
            "\"DTSTART;TZID=Europe/London:20261026T093000\\nRRULE:FREQ=WEEKLY;BYDAY=TU\"",
            "3"
        )),
        ["type.invalid_literal"]
    );
    // Plain text isn't a rule.
    assert_eq!(check(&occurrences("plain", "3")), ["type.mismatch"]);
}

#[test]
fn opening_hours_written_out_are_checked_while_compiling() {
    let open = |hours: &str| {
        check(&format!(
            "function f(at: SearchFrom, plain: Plain) -> IsOpen {{\n    return IsOpen(temporal.open_at({hours}, at))\n}}\n"
        ))
    };
    assert_eq!(
        open("\"Mo-Fr 09:00-17:30; Sa 10:00-16:00 [Europe/London]\""),
        Vec::<String>::new()
    );
    assert_eq!(open("\"Mo-Fr 09:00-17:30\""), ["type.invalid_literal"]);
    assert_eq!(open("plain"), ["type.mismatch"]);
}

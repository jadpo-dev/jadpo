//! Area 19, "Time and calendar additions" (chunk 5.5k), in the checker:
//! `CalendarPeriod`'s operator rows and what else is refused, a time
//! type's fields checked as an object's field rules are, the formats a
//! local time can be shown in, and `DateRange` taken by the `range`
//! operations.

use crate::support;

use jadpo_db::Layout;

fn check(source: &str) -> Vec<String> {
    let types = "type Day = CalendarDate {}\n\ntype Seen = ZonedTime {}\n\ntype Hour = Int {}\n\ntype Flag = Bool {}\n\ntype Shown = PresentationText {}\n\n";
    support::rules(Layout::Loose, &[("main.jadpo", &format!("{types}{source}"))])
}

#[test]
fn a_period_s_operator_rows_and_nothing_else() {
    let cases: &[(&str, &[&str])] = &[
        // A date moved by a period keeps the date's type.
        (
            "function f(day: Day, term: CalendarPeriod) -> Day {\n    return day + term\n}\n",
            &[],
        ),
        (
            "function f(day: Day, term: CalendarPeriod) -> Day {\n    return day - term\n}\n",
            &[],
        ),
        (
            "function f(a: CalendarPeriod, b: CalendarPeriod) -> CalendarPeriod {\n    return a + b - b * 2\n}\n",
            &[],
        ),
        (
            "function f(a: CalendarPeriod, b: CalendarPeriod) -> Flag {\n    return Flag(a == b)\n}\n",
            &[],
        ),
        // The date comes first.
        (
            "function f(day: Day, term: CalendarPeriod) -> Day {\n    return term + day\n}\n",
            &["type.arithmetic_operand"],
        ),
        // A local time needs `dst:`, so it takes no period by operator.
        (
            "function f(at: Seen, term: CalendarPeriod) -> Seen {\n    return at + term\n}\n",
            &["type.arithmetic_operand"],
        ),
        (
            "function f(term: CalendarPeriod) -> CalendarPeriod {\n    return 2 * term\n}\n",
            &["type.arithmetic_operand"],
        ),
        (
            "function f(term: CalendarPeriod) -> CalendarPeriod {\n    return term * 1.5\n}\n",
            &["type.arithmetic_operand"],
        ),
        // No order: a month and 30 days don't compare.
        (
            "function f(a: CalendarPeriod, b: CalendarPeriod) -> Flag {\n    return Flag(a < b)\n}\n",
            &["type.ordering_operand"],
        ),
        // A duration still doesn't move a date.
        (
            "function f(day: Day) -> Day {\n    return day + 5d\n}\n",
            &["type.arithmetic_operand"],
        ),
        (
            "function f(a: Day, b: Day) -> CalendarPeriod {\n    return temporal.period_between(a, b)\n}\n",
            &[],
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(&check(source), expected, "{source}");
    }
}

#[test]
fn a_time_type_s_fields_are_proved_like_an_object_s() {
    let cases: &[(&str, &[&str])] = &[
        (
            "function f() -> LocalTime {\n    return LocalTime {\n        hour: 23\n        minute: 59\n        second: 59\n    }\n}\n",
            &[],
        ),
        (
            "function f() -> LocalTime {\n    return LocalTime {\n        hour: 24\n        minute: 0\n        second: 0\n    }\n}\n",
            &["type.invalid_literal"],
        ),
        (
            "function f(h: Hour) -> LocalTime {\n    return LocalTime {\n        hour: h + 0\n        minute: 0\n        second: 0\n    }\n}\n",
            &["type.field_unproved"],
        ),
        (
            "function f(at: LocalDateTime) -> Hour {\n    return Hour(at.time.hour)\n}\n",
            &[],
        ),
        (
            "function f(at: LocalTime) -> Hour {\n    return Hour(at.hours)\n}\n",
            &["type.unknown_field"],
        ),
        (
            "function f(a: YearMonth, b: YearMonth) -> Flag {\n    return Flag(a < b)\n}\n",
            &[],
        ),
        (
            "function f(a: UtcOffset, b: UtcOffset) -> Flag {\n    return Flag(a < b)\n}\n",
            &["type.ordering_operand"],
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(&check(source), expected, "{source}");
    }
}

#[test]
fn a_local_time_is_shown_only_by_the_parts_it_has() {
    let shown = |ty: &str, format: &str| {
        check(&format!(
            "function f(at: {ty}) -> Shown {{\n    return Shown(text.format(at, locale: Locale.en_gb, format: {format}))\n}}\n"
        ))
    };
    let declared = "type ClockOnly = TimeFormat {\n    hour: ComponentStyle.two_digit\n    minute: ComponentStyle.two_digit\n}\n\ntype WithZone = TimeFormat {\n    day: ComponentStyle.numeric\n    hour: ComponentStyle.two_digit\n    zone_name: ComponentStyle.short\n}\n\n";
    let none: &[&str] = &[];
    let missing: &[&str] = &["type.temporal_format_part_missing"];
    assert_eq!(shown("LocalTime", "TimeFormat.time_short"), none);
    assert_eq!(shown("LocalTime", "TimeFormat.time_full"), none);
    assert_eq!(shown("LocalTime", "TimeFormat.date_medium"), missing);
    assert_eq!(shown("LocalTime", "TimeFormat.date_time_short"), missing);
    assert_eq!(shown("LocalDateTime", "TimeFormat.date_time_full"), none);
    let with = |ty: &str, format: &str| {
        check(&format!(
            "{declared}function f(at: {ty}) -> Shown {{\n    return Shown(text.format(at, locale: Locale.en_gb, format: {format}))\n}}\n"
        ))
    };
    assert_eq!(with("LocalTime", "ClockOnly"), none);
    assert_eq!(with("LocalTime", "WithZone"), missing);
    assert_eq!(with("LocalDateTime", "WithZone"), missing);
    // A date range is dates: a clock time is refused as for a date.
    assert_eq!(
        shown("DateRange", "TimeFormat.time_short"),
        ["type.temporal_format_requires_time"]
    );
    assert_eq!(shown("DateRange", "TimeFormat.date_long"), none);
}

#[test]
fn the_range_operations_take_a_date_range() {
    let cases: &[(&str, &[&str])] = &[
        (
            "function f(days: DateRange, day: Day) -> Flag {\n    return Flag(range.contains(days, day))\n}\n",
            &[],
        ),
        (
            "function f(a: DateRange, b: DateRange) -> DateRange? {\n    return range.intersection(a, b)\n}\n",
            &[],
        ),
        // Each range keeps its own end rule, so they don't mix.
        (
            "function f(a: DateRange, b: Range<CalendarDate>) -> Flag {\n    return Flag(range.overlaps(a, b))\n}\n",
            &["type.mismatch"],
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(&check(source), expected, "{source}");
    }
}

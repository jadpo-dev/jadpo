//! Area 10, time: what arithmetic takes time values, and what the
//! `temporal` operations check while compiling: options that mean nothing,
//! clock times written out, and the conditions an `attempt` maps.

use crate::support;

use jadpo_db::Layout;
use support::rules;

const TYPES: &str = "type DueAt = Instant {}\n\ntype Notice = Duration {\n    max: 7d\n}\n\ntype Timeout = Duration {}\n\ntype ShopDay = CalendarDate {}\n\ntype Opening = ZonedTime {}\n\ntype Unclear = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn check(source: &str) -> Vec<String> {
    rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{TYPES}{source}"))],
    )
}

#[test]
fn instants_move_by_durations_and_keep_their_type() {
    assert_eq!(
        check(
            "function earlier(due: DueAt, notice: Notice) -> DueAt {\n    return due - notice\n}\n\nfunction later(due: DueAt, notice: Notice) -> DueAt {\n    return notice + due\n}\n\nfunction gap(a: DueAt, b: DueAt) -> Timeout {\n    return Timeout(b - a + 5m)\n}\n"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn time_values_take_only_the_arithmetic_that_means_something() {
    for body in [
        // A date moves by calendar days, never by a duration.
        "function f(day: ShopDay) -> ShopDay {\n    return day + 1h\n}\n",
        // A local time moves on its calendar or through its instant.
        "function f(at: Opening) -> Opening {\n    return at + 1h\n}\n",
        // Two durations of different types are different units.
        "function f(a: Notice, b: Timeout) -> Timeout {\n    return Timeout(a + b)\n}\n",
        // Durations aren't multiplied.
        "function f(a: Notice) -> Timeout {\n    return Timeout(a * 2)\n}\n",
        // Two instants don't add up.
        "function f(a: DueAt, b: DueAt) -> DueAt {\n    return a + b\n}\n",
    ] {
        assert_eq!(check(body), ["type.arithmetic_operand"], "{body}");
    }
}

#[test]
fn a_local_time_has_no_order_of_its_own() {
    assert_eq!(
        check(
            "type Before = Bool {}\n\nfunction f(a: Opening, b: Opening) -> Before {\n    return Before(a < b)\n}\n"
        ),
        ["type.ordering_operand"]
    );
    assert_eq!(
        check(
            "type Before = Bool {}\n\nfunction f(a: Opening, b: Opening) -> Before {\n    return Before(temporal.instant(a) < temporal.instant(b))\n}\n"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn an_unclear_local_time_is_mapped_by_its_option() {
    // Without `dst:`, the call can fail and needs an `attempt`.
    assert_eq!(
        check(
            "function f(day: ShopDay) -> Opening {\n    return Opening(temporal.resolve(day, at: \"09:30\", zone: Zone.utc))\n}\n"
        ),
        ["failure.attempt_required"]
    );
    // `invalid:` covers it as it covers a construction's rules.
    assert_eq!(
        check(
            "function f(day: ShopDay) fails Unclear -> Opening {\n    var at: ZonedTime = attempt temporal.resolve(day, at: \"09:30\", zone: Zone.utc) {\n        invalid: Unclear\n    }\n    return Opening(at)\n}\n"
        ),
        Vec::<String>::new()
    );
    // A `Dst` passed as a value might be `reject`, so it settles nothing.
    assert_eq!(
        check(
            "function f(day: ShopDay, dst: Dst) -> Opening {\n    return Opening(temporal.resolve(day, at: \"09:30\", zone: Zone.utc, dst: dst))\n}\n"
        ),
        ["failure.attempt_required"]
    );
    // Written out, it settles the condition, and an arm for it never runs.
    assert_eq!(
        check(
            "function f(day: ShopDay) fails Unclear -> Opening {\n    var at: ZonedTime = attempt temporal.resolve(day, at: \"09:30\", zone: Zone.utc, dst: Dst.later) {\n        dst: Unclear\n    }\n    return Opening(at)\n}\n"
        ),
        ["failure.attempt_infallible"]
    );
}

#[test]
fn a_date_moves_by_months_with_a_day_the_month_has() {
    assert_eq!(
        check(
            "function f(day: ShopDay) -> ShopDay {\n    return temporal.add_months(day, 1, invalid_day: InvalidDay.last_valid_day)\n}\n"
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        check(
            "function f(day: ShopDay) fails Unclear -> ShopDay {\n    return attempt temporal.add_months(day, 1) {\n        invalid_day: Unclear\n    }\n}\n"
        ),
        Vec::<String>::new()
    );
    // A date has no clock time, so `dst:` neither applies nor needs an arm.
    assert_eq!(
        check(
            "function f(day: ShopDay) -> ShopDay {\n    return temporal.add_days(day, 1, dst: Dst.later)\n}\n"
        ),
        ["type.temporal_option_unused"]
    );
    assert_eq!(
        check(
            "function f(day: ShopDay) fails Unclear -> ShopDay {\n    return attempt temporal.add_months(day, 1) {\n        invalid_day: Unclear\n        dst: Unclear\n    }\n}\n"
        ),
        ["failure.rule_cannot_break"]
    );
}

#[test]
fn a_clock_time_is_written_out() {
    for at in ["\"9:30\"", "\"24:00\"", "\"09:30:15.1234\"", "time"] {
        let source = format!(
            "function f(day: ShopDay, time: Text) -> Opening {{\n    return Opening(temporal.resolve(day, at: {at}, zone: Zone.utc, dst: Dst.later))\n}}\n"
        );
        let found = check(&source);
        assert!(
            found.contains(&"type.temporal_clock_invalid".to_string()),
            "{at}: {found:?}"
        );
    }
}

#[test]
fn only_a_test_moves_the_clock() {
    let files = [
        support::APP,
        (
            "domains/shop/fixtures/later.jadpo",
            "type Stamp = Instant {}\n\nfixture later() -> Stamp {\n    clock.advance(1h)\n    return Stamp(clock.now)\n}\n",
        ),
    ];
    assert!(rules(Layout::Project, &files).contains(&"type.test_clock_context".to_string()));
    assert_eq!(
        check(
            "type Stamp = Instant {}\n\naction now() -> Stamp {\n    return Stamp(clock.now)\n}\n\ntest \"the clock moves\" {\n    clock: \"2026-01-15T12:00:00Z\"\n\n    clock.advance(1h)\n    assert now() == Stamp(\"2026-01-15T13:00:00Z\")\n}\n"
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        check("test \"a clock\" {\n    clock: \"yesterday\"\n\n    assert true\n}\n"),
        ["type.setting_value"]
    );
}

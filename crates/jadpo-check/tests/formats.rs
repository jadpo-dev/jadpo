//! Area 10, "Showing dates and times to people": declared and built-in
//! time formats, `PresentationText`, and what `temporal.format` and
//! `temporal.format_relative` check while compiling. The text they give
//! comes with locales (chunk 5.3), so a call that checks is refused as
//! `toolchain.not_built_yet` until then.

use crate::support;

use jadpo_db::Layout;
use support::rules;

const TYPES: &str = "type PlacedAt = Instant {}\n\ntype ShopDay = CalendarDate {}\n\ntype Opening = ZonedTime {}\n\ntype ShortDate = TimeFormat {\n    day: ComponentStyle.numeric\n    month: ComponentStyle.short\n    year: ComponentStyle.numeric\n}\n\ntype ShortTime = TimeFormat {\n    hour: ComponentStyle.two_digit\n    minute: ComponentStyle.two_digit\n}\n\n";

fn check(source: &str) -> Vec<String> {
    rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{TYPES}{source}"))],
    )
}

/// A function that formats `value` of `ty` with the arguments `args`.
fn formats(ty: &str, args: &str) -> Vec<String> {
    check(&format!(
        "function label(value: {ty}, locale: Locale) -> PresentationText {{\n    return temporal.format(value, {args})\n}}\n"
    ))
}

const NOT_BUILT: [&str; 1] = ["toolchain.not_built_yet"];

#[test]
fn a_declared_format_names_components_and_their_styles() {
    assert_eq!(check(""), Vec::<String>::new());
    assert_eq!(
        check("type Empty = TimeFormat {}\n"),
        ["type.temporal_format_components_empty"]
    );
    for style in ["ComponentStyle.long", "numeric", "2", "Zone.utc"] {
        assert_eq!(
            check(&format!("type Odd = TimeFormat {{\n    day: {style}\n}}\n")),
            ["type.temporal_format_component_invalid"],
            "{style}"
        );
    }
    assert_eq!(
        check("type Odd = TimeFormat {\n    days: ComponentStyle.numeric\n}\n"),
        ["type.unknown_setting"]
    );
    assert_eq!(
        check(
            "type Odd = TimeFormat {\n    day: ComponentStyle.numeric\n    day: ComponentStyle.two_digit\n}\n"
        ),
        ["type.temporal_format_component_duplicate"]
    );
    // Every component with every style it takes.
    assert_eq!(
        check(
            "type Everything = TimeFormat {\n    weekday: ComponentStyle.narrow\n    day: ComponentStyle.two_digit\n    month: ComponentStyle.long\n    year: ComponentStyle.two_digit\n    hour: ComponentStyle.numeric\n    minute: ComponentStyle.numeric\n    second: ComponentStyle.two_digit\n    zone_name: ComponentStyle.long\n}\n"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_format_is_named_only_in_format() {
    // `TimeFormat` is a base, written only after `=`.
    assert_eq!(
        check("type Settings = Object {\n    shown: TimeFormat\n}\n"),
        ["type.base_outside_declaration"]
    );
    // A declared format holds no value, so nothing is typed with it.
    assert_eq!(
        check("type Settings = Object {\n    shown: ShortDate\n}\n"),
        ["type.temporal_format_invalid"]
    );
    assert_eq!(
        check("function f(shown: ShortDate) -> PresentationText {\n    return \"x\"\n}\n"),
        ["type.temporal_format_invalid"]
    );
}

#[test]
fn an_instant_is_formatted_in_a_zone() {
    for format in ["TimeFormat.date_time_long", "ShortDate", "ShortTime"] {
        assert_eq!(
            formats(
                "PlacedAt",
                &format!("zone: Zone.europe_london, locale: locale, format: {format}")
            ),
            NOT_BUILT,
            "{format}"
        );
    }
    assert_eq!(
        formats("PlacedAt", "locale: locale, format: TimeFormat.date_long"),
        ["type.temporal_zone_required"]
    );
    // Given in order, without names, it is the same call.
    assert_eq!(
        formats("PlacedAt", "Zone.utc, Locale.en_gb, TimeFormat.date_long"),
        NOT_BUILT
    );
}

#[test]
fn a_local_time_or_a_date_carries_no_zone() {
    assert_eq!(
        formats("Opening", "locale: locale, format: TimeFormat.time_short"),
        NOT_BUILT
    );
    assert_eq!(
        formats("ShopDay", "locale: locale, format: ShortDate"),
        NOT_BUILT
    );
    for ty in ["Opening", "ShopDay"] {
        assert_eq!(
            formats(
                ty,
                "zone: Zone.utc, locale: locale, format: TimeFormat.date_long"
            ),
            ["type.temporal_option_unused"],
            "{ty}"
        );
    }
}

#[test]
fn a_date_has_no_clock_time_to_show() {
    for format in [
        "TimeFormat.time_short",
        "TimeFormat.date_time_full",
        "ShortTime",
    ] {
        assert_eq!(
            formats("ShopDay", &format!("locale: locale, format: {format}")),
            ["type.temporal_format_requires_time"],
            "{format}"
        );
    }
    assert_eq!(
        check(
            "type Zoned = TimeFormat {\n    day: ComponentStyle.numeric\n    zone_name: ComponentStyle.short\n}\n\nfunction label(day: ShopDay) -> PresentationText {\n    return temporal.format(day, locale: Locale.en_gb, format: Zoned)\n}\n"
        ),
        ["type.temporal_format_requires_time"]
    );
}

#[test]
fn the_format_is_written_out() {
    for format in [
        "TimeFormat.date_huge",
        "\"date_long\"",
        "shown",
        "PlacedAt",
        "Zone.utc",
    ] {
        let found = check(&format!(
            "function label(value: PlacedAt, shown: Locale) -> PresentationText {{\n    return temporal.format(value, zone: Zone.utc, locale: shown, format: {format})\n}}\n"
        ));
        assert_eq!(found, ["type.temporal_format_invalid"], "{format}");
    }
    assert_eq!(
        formats("PlacedAt", "zone: Zone.utc, format: TimeFormat.date_long"),
        ["type.argument_count"]
    );
    assert_eq!(
        formats("PlacedAt", "zone: Zone.utc, locale: locale"),
        ["type.argument_count"]
    );
}

#[test]
fn relative_text_takes_an_instant_or_a_local_time() {
    let relative = |ty: &str, args: &str| {
        check(&format!(
            "action seen(value: {ty}, locale: Locale) -> PresentationText {{\n    return temporal.format_relative(value, {args})\n}}\n"
        ))
    };
    assert_eq!(
        relative(
            "PlacedAt",
            "now: clock.now, zone: Zone.europe_london, locale: locale"
        ),
        NOT_BUILT
    );
    assert_eq!(
        relative("Opening", "now: clock.now, locale: locale"),
        NOT_BUILT
    );
    assert_eq!(
        relative("PlacedAt", "now: clock.now, locale: locale"),
        ["type.temporal_zone_required"]
    );
    assert_eq!(
        relative("Opening", "now: clock.now, zone: Zone.utc, locale: locale"),
        ["type.temporal_option_unused"]
    );
    assert_eq!(
        relative("ShopDay", "now: clock.now, locale: locale"),
        ["type.mismatch"]
    );
}

#[test]
fn presentation_text_is_shown_never_used_as_data() {
    // It goes into an output, and a function may return it.
    assert_eq!(
        check(
            "type OrderView = Output {\n    placed: PresentationText\n}\n\nfunction view(placed: PresentationText) -> OrderView {\n    return OrderView {\n        placed: placed\n    }\n}\n"
        ),
        Vec::<String>::new()
    );
    // Text written in the source may stand for one, and two compare as
    // text does.
    assert_eq!(
        check(
            "type Same = Bool {}\n\nfunction same(a: PresentationText) -> Same {\n    return Same(a == \"25 Oct 2026\")\n}\n"
        ),
        Vec::<String>::new()
    );
    // It is never rebuilt into text, ordered or added to.
    for body in [
        "type Note = Text {}\n\nfunction f(a: PresentationText) -> Note {\n    return Note(a)\n}\n",
        "type Before = Bool {}\n\nfunction f(a: PresentationText, b: PresentationText) -> Before {\n    return Before(a < b)\n}\n",
        "function f(a: PresentationText) -> PresentationText {\n    return a + \" ago\"\n}\n",
    ] {
        assert_eq!(check(body).len(), 1, "{body}");
    }
}

#[test]
fn presentation_text_is_never_stored() {
    for field in [
        "PresentationText",
        "PresentationText?",
        "List<PresentationText>",
        "Label",
        "Shown",
    ] {
        let found = check(&format!(
            "type Label = PresentationText {{}}\n\ntype Shown = Object {{\n    label: Label\n}}\n\nentity Receipt {{\n    id: Uuid\n    issued_label: {field}\n}}\n"
        ));
        assert_eq!(found, ["type.presentation_text_persistence"], "{field}");
    }
}

#[test]
fn presentation_text_is_not_a_setting() {
    assert_eq!(
        rules(
            Layout::Project,
            &[
                support::APP,
                (
                    "config.jadpo",
                    "config {\n    greeting: PresentationText\n}\n"
                ),
            ]
        ),
        ["configuration.type_unsupported"]
    );
}

//! Area 10, "Showing dates and times to people": declared and built-in
//! time formats, `PresentationText`, and what the time types'
//! `text.format` (area 19) and `temporal.format_relative` check while
//! compiling. The text they give is
//! `jadpo-value`'s; the locales they accept are `locales.rs`'s.

use crate::support;

use jadpo_db::Layout;
use support::rules;

const TYPES: &str = "type OrderedAt = Instant {}\n\ntype ShopDay = CalendarDate {}\n\ntype Opening = ZonedTime {}\n\ntype ShortDate = TimeFormat {\n    day: ComponentStyle.numeric\n    month: ComponentStyle.short\n    year: ComponentStyle.numeric\n}\n\ntype ShortTime = TimeFormat {\n    hour: ComponentStyle.two_digit\n    minute: ComponentStyle.two_digit\n}\n\n";

fn check(source: &str) -> Vec<String> {
    rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{TYPES}{source}"))],
    )
}

/// A function that formats `value` of `ty` with the arguments `args`.
fn formats(ty: &str, args: &str) -> Vec<String> {
    check(&format!(
        "function label(value: {ty}, reader_locale: Locale) -> PresentationText {{\n    return text.format(value, {args})\n}}\n"
    ))
}

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
                "OrderedAt",
                &format!("zone: Zone.europe_london, locale: reader_locale, format: {format}")
            ),
            Vec::<String>::new(),
            "{format}"
        );
    }
    assert_eq!(
        formats(
            "OrderedAt",
            "locale: reader_locale, format: TimeFormat.date_long"
        ),
        ["type.temporal_zone_required"]
    );
    // Given in order, without names, it is the same call.
    assert_eq!(
        formats("OrderedAt", "Locale.en_gb, Zone.utc, TimeFormat.date_long"),
        Vec::<String>::new()
    );
}

#[test]
fn a_local_time_or_a_date_carries_no_zone() {
    assert_eq!(
        formats(
            "Opening",
            "locale: reader_locale, format: TimeFormat.time_short"
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        formats("ShopDay", "locale: reader_locale, format: ShortDate"),
        Vec::<String>::new()
    );
    for ty in ["Opening", "ShopDay"] {
        assert_eq!(
            formats(
                ty,
                "zone: Zone.utc, locale: reader_locale, format: TimeFormat.date_long"
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
            formats(
                "ShopDay",
                &format!("locale: reader_locale, format: {format}")
            ),
            ["type.temporal_format_requires_time"],
            "{format}"
        );
    }
    assert_eq!(
        check(
            "type Zoned = TimeFormat {\n    day: ComponentStyle.numeric\n    zone_name: ComponentStyle.short\n}\n\nfunction label(day: ShopDay) -> PresentationText {\n    return text.format(day, locale: Locale.en_gb, format: Zoned)\n}\n"
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
        "OrderedAt",
        "Zone.utc",
    ] {
        let found = check(&format!(
            "function label(value: OrderedAt, shown: Locale) -> PresentationText {{\n    return text.format(value, zone: Zone.utc, locale: shown, format: {format})\n}}\n"
        ));
        assert_eq!(found, ["type.temporal_format_invalid"], "{format}");
    }
    assert_eq!(
        formats("OrderedAt", "zone: Zone.utc, format: TimeFormat.date_long"),
        ["type.argument_count"]
    );
    // A time needs its format, as `text.format`'s data entry for it says.
    assert_eq!(
        formats("OrderedAt", "zone: Zone.utc, locale: reader_locale"),
        ["type.temporal_format_invalid"]
    );
}

#[test]
fn relative_text_takes_an_instant_or_a_local_time() {
    let relative = |ty: &str, args: &str| {
        check(&format!(
            "action seen(value: {ty}, reader_locale: Locale) -> PresentationText {{\n    return temporal.format_relative(value, {args})\n}}\n"
        ))
    };
    assert_eq!(
        relative(
            "OrderedAt",
            "now: clock.now, zone: Zone.europe_london, locale: reader_locale"
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        relative("Opening", "now: clock.now, locale: reader_locale"),
        Vec::<String>::new()
    );
    assert_eq!(
        relative("OrderedAt", "now: clock.now, locale: reader_locale"),
        ["type.temporal_zone_required"]
    );
    assert_eq!(
        relative(
            "Opening",
            "now: clock.now, zone: Zone.utc, locale: reader_locale"
        ),
        ["type.temporal_option_unused"]
    );
    assert_eq!(
        relative("ShopDay", "now: clock.now, locale: reader_locale"),
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

#[test]
fn a_format_is_given_to_its_operation_as_text() {
    use jadpo_graph::body::{Callee, Expr, Literal, Node};
    let db = jadpo_db::Database::default();
    let source = format!(
        "{TYPES}function label(value: Opening, reader_locale: Locale) -> PresentationText {{\n    return text.format(value, locale: reader_locale, format: ShortDate)\n}}\n"
    );
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", source.as_str())]);
    let file = project.files(&db)[0];
    let callable = jadpo_check::types::callables_of(&db, file)[0];
    let body = &jadpo_check::types::typed_body(&db, project, callable).body;
    let args = body
        .sites
        .iter()
        .find_map(|site| match &site.node {
            Node::Expr(Expr::Call {
                callee: Callee::Std(jadpo_std::Op::TextFormat),
                args,
                ..
            }) => Some(args.clone()),
            _ => None,
        })
        .expect("the call");
    // `value`, `locale`, no `zone:` and the format, in the table's order.
    assert_eq!(args.len(), 4);
    assert!(args[2].is_none());
    let format = args[3].expect("a format");
    assert_eq!(
        body.site(format).node,
        Node::Expr(Expr::Literal(Literal::Text(
            "day:numeric month:short year:numeric".to_string()
        )))
    );
}

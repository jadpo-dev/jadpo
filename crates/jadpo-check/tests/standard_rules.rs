//! Area 19, the standard object types' rules across their parts (chunk
//! 5.5t2), in the checker: each is a named rule, as an application's object
//! type names its own (area 2, "Rules across fields"). A literal that
//! breaks one is `type.invalid_literal` naming it; a value built in code
//! from values is proved, for a rule ordering two parts, by a condition
//! around it or the declared ranges, or attempted and mapped by the rule's
//! name; and `text.parse` reading one can break them too.

use crate::support;

use jadpo_db::Layout;

const NAMES: &str = "type Words = Text {}

type Grams = Int {
    min: 0
}

type Refused = Failure {
    kind: FailureKind.invalid_value
}

";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

fn problems(source: &str) -> Vec<String> {
    support::problems(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

const NONE: &[&str] = &[];

/// A literal that breaks `rule`: one `type.invalid_literal`, naming it.
fn broken_literal(source: &str, ty: &str, rule: &str) {
    assert_eq!(
        problems(source),
        [format!(
            "main.jadpo type.invalid_literal: This `{ty}` breaks its rule `{rule}`"
        )],
        "{source}"
    );
}

/// `make`, a function building a `ty` in code from `parts`, needs an
/// `attempt` mapping `named`, or `invalid:`, and refuses an arm for a rule
/// the type doesn't have.
fn attempted(signature: &str, ty: &str, parts: &str, named: &[&str]) {
    // A range is written `Range { ... }` whatever it holds.
    let head = ty.split('<').next().unwrap_or(ty);
    let bare =
        format!("function make({signature}) -> {ty} {{\n    return {head} {{\n{parts}    }}\n}}\n");
    assert_eq!(rules(&bare), ["failure.attempt_required"], "{bare}");
    let arms: String = named
        .iter()
        .map(|rule| format!("        {rule}: Refused\n"))
        .collect();
    let mapped = format!(
        "function make({signature}) fails Refused -> {ty} {{\n    return attempt {head} {{\n{parts}    }} {{\n{arms}    }}\n}}\n"
    );
    assert_eq!(rules(&mapped), NONE, "{mapped}");
    let any = format!(
        "function make({signature}) fails Refused -> {ty} {{\n    return attempt {head} {{\n{parts}    }} {{\n        invalid: Refused\n    }}\n}}\n"
    );
    assert_eq!(rules(&any), NONE, "{any}");
    let misspelt = format!(
        "function make({signature}) fails Refused -> {ty} {{\n    return attempt {head} {{\n{parts}    }} {{\n{arms}        no_such_rule: Refused\n    }}\n}}\n"
    );
    assert_eq!(rules(&misspelt), ["failure.unknown_mapping"], "{misspelt}");
}

#[test]
fn a_range_s_start_is_never_after_its_end() {
    broken_literal(
        "function slot() -> InstantRange {\n    return InstantRange {\n        start: \"2026-10-25T10:00:00Z\"\n        end: \"2026-10-25T09:00:00Z\"\n    }\n}\n",
        "InstantRange",
        "start_not_after_end",
    );
    broken_literal(
        "function stay() -> DateRange {\n    return DateRange {\n        start: \"2026-10-25\"\n        end: \"2026-10-24\"\n    }\n}\n",
        "DateRange",
        "start_not_after_end",
    );
    broken_literal(
        "function band() -> Range<Grams> {\n    return Range {\n        start: 800\n        end: 400\n    }\n}\n",
        "Range",
        "start_not_after_end",
    );
    // An empty range, a one-day stay and an open end keep it.
    assert_eq!(
        rules(
            "function slot() -> InstantRange {\n    return InstantRange {\n        start: \"2026-10-25T10:00:00Z\"\n        end: \"2026-10-25T10:00:00Z\"\n    }\n}\n\nfunction stay() -> DateRange {\n    return DateRange {\n        start: \"2026-10-25\"\n        end: \"2026-10-25\"\n    }\n}\n\nfunction band() -> Range<Grams> {\n    return Range {\n        start: 800\n        end: none\n    }\n}\n"
        ),
        NONE
    );
    attempted(
        "window: InstantRange",
        "InstantRange",
        "        start: window.end\n        end: window.start\n",
        &["start_not_after_end"],
    );
    attempted(
        "stay: DateRange, other: DateRange",
        "DateRange",
        "        start: stay.start\n        end: other.end\n",
        &["start_not_after_end"],
    );
    attempted(
        "start: Grams, end: Grams",
        "Range<Grams>",
        "        start: start\n        end: end\n",
        &["start_not_after_end"],
    );
}

#[test]
fn a_condition_or_a_range_proves_a_rule_in_order() {
    assert_eq!(
        rules(
            "function band(start: Grams, end: Grams) fails Refused -> Range<Grams> {\n    if (end < start) {\n        reject Refused\n    }\n    return Range {\n        start: start\n        end: end\n    }\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function band(start: Grams, end: Grams) fails Refused -> Range<Grams> {\n    if (end < start) {\n        reject Refused\n    }\n    return attempt Range {\n        start: start\n        end: end\n    } {\n        start_not_after_end: Refused\n    }\n}\n"
        ),
        ["failure.attempt_infallible"]
    );
    assert_eq!(
        rules(
            "function joined(stay: DateRange, other: DateRange) fails Refused -> DateRange {\n    if (other.end < stay.start) {\n        reject Refused\n    }\n    return DateRange {\n        start: stay.start\n        end: other.end\n    }\n}\n"
        ),
        NONE,
        "a part read from another value, compared before"
    );
    // A range with an open end and a value is open whatever the value.
    assert_eq!(
        rules(
            "function from(start: Grams) -> Range<Grams> {\n    return Range {\n        start: start\n        end: none\n    }\n}\n"
        ),
        NONE
    );
    // The declared ranges prove it: a policy's fewest characters, 8, is
    // never more than its most, at least 64.
    assert_eq!(
        rules(
            "function policy(most: PasswordPolicy.max_length) -> PasswordPolicy {\n    return PasswordPolicy {\n        min_length: 8\n        max_length: most\n        check_breached: true\n    }\n}\n"
        ),
        NONE
    );
}

#[test]
fn an_iso_week_53_is_only_in_a_year_with_one() {
    broken_literal(
        "function week() -> IsoWeek {\n    return IsoWeek {\n        year: 2027\n        week: 53\n    }\n}\n",
        "IsoWeek",
        "week_in_year",
    );
    assert_eq!(
        rules(
            "function week() -> IsoWeek {\n    return IsoWeek {\n        year: 2026\n        week: 53\n    }\n}\n\nfunction early(year: IsoWeek.year) -> IsoWeek {\n    return IsoWeek {\n        year: year\n        week: 52\n    }\n}\n"
        ),
        NONE,
        "2026 has 53 weeks, and every year a 52nd"
    );
    attempted(
        "year: IsoWeek.year, week: IsoWeek.week",
        "IsoWeek",
        "        year: year\n        week: week\n",
        &["week_in_year"],
    );
    // Read from text, a week 53 its year doesn't have breaks the rule.
    assert_eq!(
        rules(
            "function read(raw: Words) fails Refused -> IsoWeek {\n    return attempt text.parse(raw, as: IsoWeek) {\n        malformed: Refused\n    }\n}\n"
        ),
        ["failure.rule_unmapped"]
    );
    assert_eq!(
        rules(
            "function read(raw: Words) fails Refused -> IsoWeek {\n    return attempt text.parse(raw, as: IsoWeek) {\n        malformed: Refused\n        week_in_year: Refused\n    }\n}\n"
        ),
        NONE
    );
}

const EVENT_TAIL: &str =
    "        location: none\n        description: none\n        recurrence: none\n";

#[test]
fn a_calendar_event_s_parts_go_together() {
    broken_literal(
        &format!(
            "function fair() -> CalendarEvent {{\n    return CalendarEvent {{\n        summary: \"Fair\"\n        first_day: \"2026-10-24\"\n        last_day: \"2026-10-24\"\n        start: none\n        end: none\n        zone: Zone.europe_london\n{EVENT_TAIL}    }}\n}}\n"
        ),
        "CalendarEvent",
        "days_or_times",
    );
    broken_literal(
        &format!(
            "function fair() -> CalendarEvent {{\n    return CalendarEvent {{\n        summary: \"Fair\"\n        first_day: \"2026-10-24\"\n        last_day: \"2026-10-23\"\n        start: none\n        end: none\n        zone: none\n{EVENT_TAIL}    }}\n}}\n"
        ),
        "CalendarEvent",
        "last_day_not_before_first",
    );
    attempted(
        "stay: DateRange, other: DateRange",
        "CalendarEvent",
        &format!(
            "        summary: \"Fair\"\n        first_day: stay.start\n        last_day: other.end\n        start: none\n        end: none\n        zone: none\n{EVENT_TAIL}"
        ),
        &["last_day_not_before_first"],
    );
    assert_eq!(
        rules(&format!(
            "function fair(stay: DateRange, other: DateRange) fails Refused -> CalendarEvent {{\n    if (other.end < stay.start) {{\n        reject Refused\n    }}\n    return CalendarEvent {{\n        summary: \"Fair\"\n        first_day: stay.start\n        last_day: other.end\n        start: none\n        end: none\n        zone: none\n{EVENT_TAIL}    }}\n}}\n"
        )),
        NONE,
        "a condition proves the days' order, and the rest is written out"
    );
    // A part that may be `none` leaves which kind of event it is to the
    // running program.
    attempted(
        "other: CalendarEvent",
        "CalendarEvent",
        &format!(
            "        summary: \"Fair\"\n        first_day: other.first_day\n        last_day: other.last_day\n        start: none\n        end: none\n        zone: none\n{EVENT_TAIL}"
        ),
        &["days_or_times", "last_day_not_before_first"],
    );
}

#[test]
fn an_address_s_postal_code_is_its_country_s() {
    broken_literal(
        "function office() -> PostalAddress {\n    return PostalAddress {\n        lines: [\"1 High Street\"]\n        locality: none\n        region: none\n        postal_code: \"75004\"\n        country: Country.gb\n    }\n}\n",
        "PostalAddress",
        "postal_code_of_country",
    );
    attempted(
        "code: PostalCode",
        "PostalAddress",
        "        lines: [\"1 High Street\"]\n        locality: none\n        region: none\n        postal_code: code\n        country: Country.gb\n",
        &["postal_code_of_country"],
    );
    // No postal code keeps it.
    assert_eq!(
        rules(
            "function office(home: Country) -> PostalAddress {\n    return PostalAddress {\n        lines: [\"1 High Street\"]\n        locality: none\n        region: none\n        postal_code: none\n        country: home\n    }\n}\n"
        ),
        NONE
    );
}

#[test]
fn a_document_s_number_is_its_kind_s_and_country_s() {
    broken_literal(
        "function held() -> IdentityDocument {\n    return IdentityDocument {\n        kind: IdentityDocumentKind.passport\n        country: Country.gb\n        number: \"AB12\"\n        expires: none\n    }\n}\n",
        "IdentityDocument",
        "number_of_kind_and_country",
    );
    attempted(
        "kind: IdentityDocumentKind",
        "IdentityDocument",
        "        kind: kind\n        country: Country.gb\n        number: \"123456789\"\n        expires: none\n",
        &["number_of_kind_and_country"],
    );
    broken_literal(
        "function held() -> PassportNumber {\n    return PassportNumber {\n        country: Country.gb\n        number: \"AB12\"\n    }\n}\n",
        "PassportNumber",
        "number_of_country",
    );
    attempted(
        "issuer: Country",
        "PassportNumber",
        "        country: issuer\n        number: \"123456789\"\n",
        &["number_of_country"],
    );
}

#[test]
fn a_content_rating_is_one_of_its_body_s() {
    broken_literal(
        "function film() -> ContentRating {\n    return ContentRating {\n        scheme: ContentRatingScheme.pegi\n        rating: \"12A\"\n    }\n}\n",
        "ContentRating",
        "rating_of_scheme",
    );
    attempted(
        "body: ContentRatingScheme",
        "ContentRating",
        "        scheme: body\n        rating: \"12A\"\n",
        &["rating_of_scheme"],
    );
}

#[test]
fn a_timecode_and_a_cue_keep_their_rules() {
    let timecode = |frames: &str, rate: &str, drop: &str| {
        format!(
            "function frame() -> Timecode {{\n    return Timecode {{\n        hours: 0\n        minutes: 1\n        seconds: 0\n        frames: {frames}\n        rate: \"{rate}\"\n        drop_frame: {drop}\n    }}\n}}\n"
        )
    };
    broken_literal(
        &timecode("25", "25/1", "false"),
        "Timecode",
        "frames_within_rate",
    );
    broken_literal(
        &timecode("0", "25/1", "true"),
        "Timecode",
        "drop_frame_rate",
    );
    broken_literal(
        &timecode("1", "30000/1001", "true"),
        "Timecode",
        "frame_not_dropped",
    );
    assert_eq!(rules(&timecode("2", "30000/1001", "true")), NONE);
    attempted(
        "other: Timecode",
        "Timecode",
        "        hours: 0\n        minutes: 1\n        seconds: 0\n        frames: 0\n        rate: \"30000/1001\"\n        drop_frame: other.drop_frame\n",
        &["frame_not_dropped"],
    );
    let cue = |start: &str, end: &str, text: &str| {
        format!(
            "function caption() -> SubtitleCue {{\n    return SubtitleCue {{\n        start: {start}\n        end: {end}\n        text: \"{text}\"\n    }}\n}}\n"
        )
    };
    broken_literal(
        &cue("-1s", "4s", "Hello"),
        "SubtitleCue",
        "start_not_negative",
    );
    broken_literal(&cue("4s", "1s", "Hello"), "SubtitleCue", "end_after_start");
    broken_literal(
        &cue("1s", "4s", "a --> b"),
        "SubtitleCue",
        "text_without_arrow",
    );
    attempted(
        "other: SubtitleCue",
        "SubtitleCue",
        "        start: other.end\n        end: other.start\n        text: \"Hello\"\n",
        &["start_not_negative", "end_after_start"],
    );
}

#[test]
fn ratings_and_blood_pressures_keep_their_order() {
    broken_literal(
        "function stars() -> Rating {\n    return Rating {\n        value: 4.5\n        best: 1.0\n        worst: 5.0\n    }\n}\n",
        "Rating",
        "best_above_worst",
    );
    broken_literal(
        "function stars() -> Rating {\n    return Rating {\n        value: 6.0\n        best: 5.0\n        worst: 1.0\n    }\n}\n",
        "Rating",
        "value_on_scale",
    );
    attempted(
        "value: Rating.value",
        "Rating",
        "        value: value\n        best: 5.0\n        worst: 1.0\n",
        &["value_on_scale"],
    );
    broken_literal(
        "function reading() -> BloodPressure {\n    return BloodPressure {\n        systolic: 80mmHg\n        diastolic: 120mmHg\n    }\n}\n",
        "BloodPressure",
        "diastolic_below_systolic",
    );
    attempted(
        "systolic: BloodPressure.systolic, diastolic: BloodPressure.diastolic",
        "BloodPressure",
        "        systolic: systolic\n        diastolic: diastolic\n",
        &["diastolic_below_systolic"],
    );
    assert_eq!(
        rules(
            "function usage(input: TokenCount, cached: TokenCount) fails Refused -> TokenUsage {\n    if (cached > input) {\n        reject Refused\n    }\n    return TokenUsage {\n        input: input\n        output: 5\n        cached: cached\n    }\n}\n"
        ),
        NONE
    );
}

#[test]
fn tiles_tokens_policies_and_boxes_keep_their_rules() {
    broken_literal(
        "function tile() -> MapTile {\n    return MapTile {\n        zoom: 1\n        x: 2\n        y: 0\n    }\n}\n",
        "MapTile",
        "within_zoom",
    );
    attempted(
        "x: MapTile.x",
        "MapTile",
        "        zoom: 1\n        x: x\n        y: 0\n",
        &["within_zoom"],
    );
    broken_literal(
        "function usage() -> TokenUsage {\n    return TokenUsage {\n        input: 10\n        output: 5\n        cached: 11\n    }\n}\n",
        "TokenUsage",
        "cached_within_input",
    );
    attempted(
        "input: TokenCount, cached: TokenCount",
        "TokenUsage",
        "        input: input\n        output: 5\n        cached: cached\n",
        &["cached_within_input"],
    );
    // No cached count keeps it.
    assert_eq!(
        rules(
            "function usage(input: TokenCount) -> TokenUsage {\n    return TokenUsage {\n        input: input\n        output: 5\n        cached: none\n    }\n}\n"
        ),
        NONE
    );
    broken_literal(
        "function policy() -> PasswordPolicy {\n    return PasswordPolicy {\n        min_length: 100\n        max_length: 64\n        check_breached: true\n    }\n}\n",
        "PasswordPolicy",
        "min_not_above_max",
    );
    attempted(
        "fewest: PasswordPolicy.min_length",
        "PasswordPolicy",
        "        min_length: fewest\n        max_length: 64\n        check_breached: true\n",
        &["min_not_above_max"],
    );
    broken_literal(
        "function area() -> BoundingBox {\n    return BoundingBox {\n        south: 52.0\n        west: -1.0\n        north: 51.0\n        east: 1.0\n    }\n}\n",
        "BoundingBox",
        "south_not_above_north",
    );
}

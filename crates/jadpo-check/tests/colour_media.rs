//! Area 19, "Colour" and "Media" (chunk 5.5p), in the checker: `colour`'s
//! and `media`'s operations typed by their stubs, a percentage and an angle
//! taken as written or as a named type built on one but never as a bare
//! number; a colour's channels, a rating's category, a timecode's frames
//! and drop-frame label, and a cue's times and text checked while compiling
//! where a literal writes them; and an application's own colour refused as
//! the standard `Colour` under another name, its alpha optional.

use crate::support;

use jadpo_db::Layout;

const NAMES: &str = "type Wire = Text {}\n\ntype Measured = Decimal {\n    scale: 2\n}\n\ntype Hue = Angle {}\n\ntype Share = Percentage {\n    min: 0%\n    max: 100%\n}\n\n";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn colour_s_operations_take_colours_percentages_and_angles() {
    assert_eq!(
        rules(
            "function swatch(hue: Hue, share: Share) -> Colour {\n    return colour.from_hsl(hue, share, 50%)\n}\n\nfunction literal() -> Colour {\n    return colour.from_oklch(62.8%, 0.2577, 29.23deg)\n}\n\nfunction mixed(a: Colour, b: Colour, share: Share) -> Colour {\n    return colour.with_lightness(colour.mix(a, b, share), 75%)\n}\n\nfunction ratio(a: Colour, b: Colour) -> Measured {\n    return Measured(colour.contrast(a, b))\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function swatch(hue: Hue) -> Colour {\n    return colour.from_hsl(hue, 0.5, 50%)\n}\n"
        ),
        ["type.mismatch"],
        "a bare number isn't a percentage"
    );
    assert_eq!(
        rules("function swatch() -> Colour {\n    return colour.from_hsl(120, 100%, 50%)\n}\n"),
        ["type.unit_required"],
        "a bare number isn't an angle"
    );
    assert_eq!(
        rules(
            "function swatch(a: Colour, b: Wire) -> Colour {\n    return colour.mix(a, b, 50%)\n}\n"
        ),
        ["type.mismatch"],
        "text is read as a colour first"
    );
}

#[test]
fn fit_takes_and_gives_dimensions() {
    assert_eq!(
        rules(
            "function thumbnail(still: ImageDimensions) -> ImageDimensions {\n    var tile: ImageDimensions = ImageDimensions {\n        width: 320\n        height: 320\n    }\n    return media.fit(still, within: tile)\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function thumbnail(still: ImageDimensions, frame: VideoResolution) -> ImageDimensions {\n    return media.fit(still, within: frame)\n}\n"
        ),
        ["type.mismatch"],
        "a video's resolution isn't an image's dimensions"
    );
}

#[test]
fn literals_are_checked_by_their_standards() {
    let colour = |red: &str| {
        rules(&format!(
            "function brand() -> Colour {{\n    return Colour {{\n        red: {red}\n        green: 111\n        blue: 217\n        alpha: 255\n    }}\n}}\n"
        ))
    };
    assert_eq!(colour("30"), NONE);
    assert_eq!(
        colour("256"),
        ["type.invalid_literal"],
        "a channel is 0 to 255"
    );
    let rating = |rating: &str| {
        rules(&format!(
            "function rated() -> ContentRating {{\n    return ContentRating {{\n        scheme: ContentRatingScheme.bbfc\n        rating: \"{rating}\"\n    }}\n}}\n"
        ))
    };
    assert_eq!(rating("12A"), NONE);
    assert_eq!(
        rating("PG-13"),
        ["type.invalid_literal"],
        "the MPA's, not the BBFC's"
    );
    let timecode = |minutes: i64, frames: i64, numerator: i64, drop: bool| {
        rules(&format!(
            "function cut() -> Timecode {{\n    return Timecode {{\n        hours: 1\n        minutes: {minutes}\n        seconds: 0\n        frames: {frames}\n        rate: FrameRate {{\n            numerator: {numerator}\n            denominator: 1001\n        }}\n        drop_frame: {drop}\n    }}\n}}\n"
        ))
    };
    assert_eq!(timecode(10, 0, 30_000, true), NONE);
    assert_eq!(timecode(11, 2, 30_000, true), NONE);
    assert_eq!(
        timecode(11, 1, 30_000, true),
        ["type.invalid_literal"],
        "drop-frame skips frames 0 and 1"
    );
    assert_eq!(timecode(11, 1, 30_000, false), NONE);
    assert_eq!(
        timecode(0, 0, 25_025, true),
        ["type.invalid_literal"],
        "25 frames a second has no drop-frame"
    );
    assert_eq!(
        timecode(0, 30, 30_000, false),
        ["type.invalid_literal"],
        "frames below the rate's"
    );
    let cue = |start: &str, end: &str, text: &str| {
        rules(&format!(
            "function caption() -> SubtitleCue {{\n    return SubtitleCue {{\n        start: {start}\n        end: {end}\n        text: \"{text}\"\n    }}\n}}\n"
        ))
    };
    assert_eq!(cue("1s", "4s", "Hello"), NONE);
    assert_eq!(cue("4s", "1s", "Hello"), ["type.invalid_literal"]);
    assert_eq!(cue("1s", "4s", "a --> b"), ["type.invalid_literal"]);
}

#[test]
fn an_application_s_own_colour_is_the_standard_one() {
    let declared = |fields: &str| {
        rules(&format!(
            "type Channel = Int {{\n    min: 0\n    max: 255\n}}\n\ntype BrandColour = Object {{\n{fields}}}\n"
        ))
    };
    assert_eq!(
        declared("    red: Channel\n    green: Channel\n    blue: Channel\n    alpha: Channel\n"),
        ["type.standard_exists"]
    );
    assert_eq!(
        declared("    r: Channel\n    g: Channel\n    b: Channel\n"),
        ["type.standard_exists"],
        "the alpha is optional, the initials common names"
    );
    assert_eq!(
        declared("    red: Channel\n    green: Channel\n"),
        NONE,
        "two channels aren't a colour"
    );
    assert_eq!(
        declared("    red: Channel\n    green: Channel\n    blue: Channel\n    name: Wire\n"),
        NONE,
        "another field makes it something else"
    );
}

#[test]
fn the_new_names_are_standard() {
    for name in ["Colour", "Rect", "Timecode", "PaperSize"] {
        assert_eq!(
            rules(&format!("type {name} = Text {{}}\n")),
            ["type.standard_clash"],
            "{name}"
        );
    }
}

//! The media types (area 19, "Media", chunk 5.5p): sizes and shapes on a
//! screen or a page (`ImageDimensions`, `AspectRatio`, `Rect`, `Insets`,
//! `PaperSize`), an image's `ExifOrientation` and `AltText`, video and audio
//! (`FrameRate`, `VideoResolution`, `VideoCodec`, `AudioCodec`, `Timecode`),
//! a `SubtitleCue` and a `ContentRating`. The governing standards are cited
//! at each: ISO 216 and ANSI/ASME Y14.1 for paper, EXIF 2.32 (TIFF 6.0's
//! orientation) for orientation, the W3C WebCodecs Codec Registry for codec
//! strings (RFC 6381's form), SMPTE ST 12-1 for timecode with its
//! drop-frame counting, WebVTT for cues, and each classification body's
//! own published categories for ratings. As everywhere, only the canonical
//! text is read.
//!
//! `media.fit`'s arithmetic is here too ([`fit`]), in whole numbers.

use super::across::{Check, Known, Order, StandardRule, Verdict, fits, in_order};
use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, Part, StandardType,
    Storage, TextForm,
};
use crate::Builtin;
use crate::read::SqlType;

/// `ImageDimensions`.
pub const IMAGE_DIMENSIONS: &str = "ImageDimensions";
/// `AspectRatio`.
pub const ASPECT_RATIO: &str = "AspectRatio";
/// `Rect`.
pub const RECT: &str = "Rect";
/// `Insets`.
pub const INSETS: &str = "Insets";
/// `PaperSize`.
pub const PAPER_SIZE: &str = "PaperSize";
/// `ExifOrientation`.
pub const EXIF_ORIENTATION: &str = "ExifOrientation";
/// `AltText`.
pub const ALT_TEXT: &str = "AltText";
/// `FrameRate`.
pub const FRAME_RATE: &str = "FrameRate";
/// `VideoResolution`.
pub const VIDEO_RESOLUTION: &str = "VideoResolution";
/// `VideoCodec`.
pub const VIDEO_CODEC: &str = "VideoCodec";
/// `AudioCodec`.
pub const AUDIO_CODEC: &str = "AudioCodec";
/// `Timecode`.
pub const TIMECODE: &str = "Timecode";
/// `SubtitleCue`.
pub const SUBTITLE_CUE: &str = "SubtitleCue";
/// `ContentRating`.
pub const CONTENT_RATING: &str = "ContentRating";
/// `ContentRatingScheme`: the body a `ContentRating` is by.
pub const CONTENT_RATING_SCHEME: &str = "ContentRatingScheme";

/// The most pixels a side has: PNG's limit, 2^31 − 1, the largest of the
/// common image formats' (JPEG's is 65,535), so a side fits an `integer`
/// column.
pub const MOST_PIXELS: i64 = 2_147_483_647;

/// A side in whole pixels, at least 1.
const PIXELS: Part = Part::Whole(1, MOST_PIXELS);

/// A length on a screen or a page that is never below 0.
const NOT_NEGATIVE: Part = Part::Bounded {
    of: Builtin::Decimal,
    min: Some(crate::Num {
        mantissa: 0,
        scale: 0,
    }),
    max: None,
};

const fn examples(
    valid: &'static [(&'static str, &'static str)],
    malformed: &'static [&'static str],
) -> Examples {
    Examples {
        valid,
        format: None,
        malformed,
        of: None,
        settings: "",
    }
}

const CANONICAL: Format = Format {
    key: FormatKey::Canonical,
    options: NO_OPTIONS,
};

// ---------------------------------------------------------------------
// Sizes and shapes
// ---------------------------------------------------------------------

/// `ImageDimensions`' and `VideoResolution`'s parts, in order.
pub const DIMENSIONS_FIELDS: [(&str, Part); 2] = [("width", PIXELS), ("height", PIXELS)];

/// Width and height in whole pixels, written `1920x1080`, width first as
/// CSS, ffmpeg and xrandr write it, and sent as named fields so nothing
/// reads them the other way round.
const fn dimensions(
    name: &'static str,
    valid: &'static [(&'static str, &'static str)],
) -> StandardType {
    StandardType {
        name,
        holds: Holds::Object(&DIMENSIONS_FIELDS),
        text: Some(TextForm::Wholes("x")),
        json: Json::Fields(&["width", "height"]),
        storage: Storage::Columns {
            parts: &[("width", "integer"), ("height", "integer")],
            fixed: &[],
        },
        format: CANONICAL,
        ordered: false,
        validator: None,
        shape: None,
        examples: examples(
            valid,
            &[
                "0x1080",
                "1920x0",
                "1920X1080",
                "1920 x 1080",
                "1920×1080",
                "01920x1080",
                "-1920x1080",
                "1920x",
                "2147483648x1",
                "1920",
                "",
            ],
        ),
        release: FIRST_RELEASE,
        signals: crate::standard::NO_SIGNALS,
    }
}

/// An image's size in pixels.
pub const IMAGE_DIMENSIONS_ENTRY: StandardType = dimensions(
    IMAGE_DIMENSIONS,
    &[
        ("1x1", "1x1"),
        ("4000x3000", "4000x3000"),
        ("320x240", "320x240"),
        ("2147483647x1", "2147483647x1"),
    ],
);

/// A video's frame size in pixels: ITU-R BT.709's HD and BT.2020's UHD
/// among them, by their pixels rather than a name like "4K", which names
/// both 3840 and 4096 pixels across.
pub const VIDEO_RESOLUTION_ENTRY: StandardType = dimensions(
    VIDEO_RESOLUTION,
    &[
        ("1280x720", "1280x720"),
        ("1920x1080", "1920x1080"),
        ("3840x2160", "3840x2160"),
        ("4096x2160", "4096x2160"),
    ],
);

/// `AspectRatio`'s parts, in order.
pub const ASPECT_RATIO_FIELDS: [(&str, Part); 2] = [("width", PIXELS), ("height", PIXELS)];

/// Width to height in whole numbers, written `16:9`, held as written as
/// `Ratio` is: `16:9` and `32:18` are two values.
pub const ASPECT_RATIO_ENTRY: StandardType = StandardType {
    name: ASPECT_RATIO,
    holds: Holds::Object(&ASPECT_RATIO_FIELDS),
    text: Some(TextForm::Wholes(":")),
    json: Json::Text,
    storage: Storage::Columns {
        parts: &[("width", "integer"), ("height", "integer")],
        fixed: &[],
    },
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("16:9", "16:9"),
            ("4:3", "4:3"),
            ("1:1", "1:1"),
            ("32:18", "32:18"),
            ("239:100", "239:100"),
        ],
        &[
            "16/9", "1.85:1", "0:1", "16:0", "16 : 9", "16:", ":9", "-16:9", "1.78", "",
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `Rect`'s parts, in order: its top-left corner and its size.
pub const RECT_FIELDS: [(&str, Part); 4] = [
    ("x", Part::Of(Builtin::Decimal)),
    ("y", Part::Of(Builtin::Decimal)),
    ("width", NOT_NEGATIVE),
    ("height", NOT_NEGATIVE),
];

/// A rectangle by its top-left corner and its size, never negative,
/// written as SVG writes a `viewBox`, `0 0 300 200`, each number with the
/// places it was given and one space between, and sent as named fields.
pub const RECT_ENTRY: StandardType = StandardType {
    name: RECT,
    holds: Holds::Object(&RECT_FIELDS),
    text: Some(TextForm::Decimals(" ")),
    json: Json::Fields(&["x", "y", "width", "height"]),
    storage: Storage::Columns {
        parts: &[
            ("x", "numeric"),
            ("y", "numeric"),
            ("width", "numeric"),
            ("height", "numeric"),
        ],
        fixed: &[],
    },
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("0 0 300 200", "0 0 300 200"),
            ("-10.5 20 300.25 0", "-10.5 20 300.25 0"),
            ("12 8 0.5 1.50", "12 8 0.5 1.50"),
        ],
        &[
            "0,0,300,200",
            "0 0 -1 200",
            "0 0 300 -0.5",
            "0 0 300",
            "0 0 300 200 1",
            "0  0 300 200",
            " 0 0 300 200",
            "0 0 300px 200px",
            "0 0 .5 1",
            "",
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `Insets`' parts, in CSS's order.
pub const INSETS_FIELDS: [(&str, Part); 4] = [
    ("top", NOT_NEGATIVE),
    ("right", NOT_NEGATIVE),
    ("bottom", NOT_NEGATIVE),
    ("left", NOT_NEGATIVE),
];

/// Space inside an edge on each side, never negative, written in the CSS
/// box model's order, `8 16 8 16`, always all four, and sent as named
/// fields.
pub const INSETS_ENTRY: StandardType = StandardType {
    name: INSETS,
    holds: Holds::Object(&INSETS_FIELDS),
    text: Some(TextForm::Decimals(" ")),
    json: Json::Fields(&["top", "right", "bottom", "left"]),
    storage: Storage::Columns {
        parts: &[
            ("top", "numeric"),
            ("right", "numeric"),
            ("bottom", "numeric"),
            ("left", "numeric"),
        ],
        fixed: &[],
    },
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("8 16 8 16", "8 16 8 16"),
            ("0 0 0 0", "0 0 0 0"),
            ("0.5 1 1.5 2", "0.5 1 1.5 2"),
        ],
        &[
            "8 16",
            "8",
            "-1 0 0 0",
            "8px 16px 8px 16px",
            "8,16,8,16",
            "8 16 8 16 ",
            "",
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// The paper sizes, each its own code: ISO 216's A and B series and the
/// three US sizes of ANSI/ASME Y14.1 an office prints on.
pub const PAPER_SIZES: &[(&str, &str)] = &[
    ("a0", "A0"),
    ("a1", "A1"),
    ("a2", "A2"),
    ("a3", "A3"),
    ("a4", "A4"),
    ("a5", "A5"),
    ("a6", "A6"),
    ("a7", "A7"),
    ("a8", "A8"),
    ("a9", "A9"),
    ("a10", "A10"),
    ("b0", "B0"),
    ("b1", "B1"),
    ("b2", "B2"),
    ("b3", "B3"),
    ("b4", "B4"),
    ("b5", "B5"),
    ("b6", "B6"),
    ("b7", "B7"),
    ("b8", "B8"),
    ("b9", "B9"),
    ("b10", "B10"),
    ("letter", "Letter"),
    ("legal", "Legal"),
    ("tabloid", "Tabloid"),
];

/// A paper size, by its name.
pub const PAPER_SIZE_ENTRY: StandardType = StandardType {
    name: PAPER_SIZE,
    holds: Holds::Enum(PAPER_SIZES),
    text: Some(TextForm::Code),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[("A4", "A4"), ("B5", "B5"), ("Letter", "Letter")],
        &["A11", "C4", "Executive", "210x297mm", "A 4", ""],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

// ---------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------

/// EXIF 2.32's orientations (its tag 274, TIFF 6.0's `Orientation`), each
/// named for where the stored image's first row and first column are seen,
/// as libtiff names them, and coded as its value.
pub const EXIF_ORIENTATIONS: &[(&str, &str)] = &[
    ("top_left", "1"),
    ("top_right", "2"),
    ("bottom_right", "3"),
    ("bottom_left", "4"),
    ("left_top", "5"),
    ("right_top", "6"),
    ("right_bottom", "7"),
    ("left_bottom", "8"),
];

/// How a stored image is turned to be seen upright.
pub const EXIF_ORIENTATION_ENTRY: StandardType = StandardType {
    name: EXIF_ORIENTATION,
    holds: Holds::Enum(EXIF_ORIENTATIONS),
    text: Some(TextForm::Code),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[("1", "1"), ("6", "6"), ("8", "8")],
        &["0", "9", "06", "top_left", ""],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// What an image shows, for someone who can't see it (WCAG 2.2's text
/// alternative): one line of 1 to 1,000 characters, no space at either
/// end. An image with nothing to say has `none`, not empty text.
pub const ALT_TEXT_ENTRY: StandardType = StandardType {
    name: ALT_TEXT,
    holds: Holds::Text {
        pattern: r"[^\s\p{Cc}](?:[^\p{Cc}]{0,998}[^\s\p{Cc}])?",
        personal: None,
    },
    text: Some(TextForm::Checked),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                "A red bicycle leaning against a brick wall",
                "A red bicycle leaning against a brick wall",
            ),
            ("Logo", "Logo"),
        ],
        &["", " Logo", "Logo ", "Two\nlines", "Tab\tinside"],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

// ---------------------------------------------------------------------
// Video and audio
// ---------------------------------------------------------------------

/// `FrameRate`'s parts, in order.
pub const FRAME_RATE_FIELDS: [(&str, Part); 2] = [
    ("numerator", Part::Whole(1, MOST_PIXELS)),
    ("denominator", Part::Whole(1, MOST_PIXELS)),
];

/// Frames a second as an exact fraction, written as ffmpeg and SMPTE
/// write one, `30000/1001` for NTSC's 29.97, `25/1` for PAL's 25; held as
/// written, as a `Fraction` is.
pub const FRAME_RATE_ENTRY: StandardType = StandardType {
    name: FRAME_RATE,
    holds: Holds::Object(&FRAME_RATE_FIELDS),
    text: Some(TextForm::Wholes("/")),
    json: Json::Text,
    storage: Storage::Columns {
        parts: &[("numerator", "integer"), ("denominator", "integer")],
        fixed: &[],
    },
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("24/1", "24/1"),
            ("25/1", "25/1"),
            ("24000/1001", "24000/1001"),
            ("30000/1001", "30000/1001"),
            ("60000/1001", "60000/1001"),
        ],
        &[
            "29.97",
            "30",
            "0/1",
            "30/0",
            "30000 / 1001",
            "-24/1",
            "030/1",
            "",
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// A video codec string the W3C WebCodecs Codec Registry lists, in RFC
/// 6381's form: AV1, H.264 (`avc1`, `avc3`), HEVC (`hev1`, `hvc1`) and VP9
/// with their profile and level, and VP8.
pub const VIDEO_CODEC_PATTERN: &str = r"(?:av01|avc1|avc3|hev1|hvc1|vp09)(?:\.[0-9A-Za-z]+)+|vp8";

/// An audio codec string the W3C WebCodecs Codec Registry lists: AAC and
/// the other MPEG-4 audio (`mp4a` with its object type), FLAC, MP3, Opus,
/// Vorbis, G.711's μ-law and A-law, and linear PCM.
pub const AUDIO_CODEC_PATTERN: &str =
    r"mp4a(?:\.[0-9A-Za-z]+)+|flac|mp3|opus|vorbis|ulaw|alaw|pcm-(?:u8|s16|s24|s32|f32)";

/// A video codec, as a media type's `codecs` parameter names it.
pub const VIDEO_CODEC_ENTRY: StandardType = StandardType {
    name: VIDEO_CODEC,
    holds: Holds::Text {
        pattern: VIDEO_CODEC_PATTERN,
        personal: None,
    },
    text: Some(TextForm::Checked),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("avc1.42001E", "avc1.42001E"),
            ("hvc1.1.6.L93.B0", "hvc1.1.6.L93.B0"),
            ("av01.0.04M.08", "av01.0.04M.08"),
            ("vp09.00.10.08", "vp09.00.10.08"),
            ("vp8", "vp8"),
        ],
        &[
            "h264",
            "avc1",
            "AVC1.42001E",
            "vp9",
            "mp4a.40.2",
            "avc1.",
            "",
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// An audio codec, as a media type's `codecs` parameter names it.
pub const AUDIO_CODEC_ENTRY: StandardType = StandardType {
    name: AUDIO_CODEC,
    holds: Holds::Text {
        pattern: AUDIO_CODEC_PATTERN,
        personal: None,
    },
    text: Some(TextForm::Checked),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("mp4a.40.2", "mp4a.40.2"),
            ("opus", "opus"),
            ("flac", "flac"),
            ("pcm-s16", "pcm-s16"),
        ],
        &["aac", "mp4a", "Opus", "pcm-s8", "avc1.42001E", ""],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `Timecode`'s parts, in order: SMPTE ST 12-1's address, the rate it
/// counts at, and whether it counts drop-frame.
pub const TIMECODE_FIELDS: [(&str, Part); 6] = [
    ("hours", Part::Whole(0, 23)),
    ("minutes", Part::Whole(0, 59)),
    ("seconds", Part::Whole(0, 59)),
    ("frames", Part::Whole(0, 119)),
    ("rate", Part::Standard(FRAME_RATE)),
    ("drop_frame", Part::Of(Builtin::Bool)),
];

/// A frame's SMPTE ST 12-1 address at a rate: hours, minutes, seconds and
/// frames, counted drop-frame or not. Its notation, `01:00:00;00`, doesn't
/// say its rate, so it has no single text form: it is sent as its parts
/// and shown in that notation.
pub const TIMECODE_ENTRY: StandardType = StandardType {
    name: TIMECODE,
    holds: Holds::Object(&TIMECODE_FIELDS),
    text: None,
    json: Json::Fields(&[
        "hours",
        "minutes",
        "seconds",
        "frames",
        "rate",
        "drop_frame",
    ]),
    storage: Storage::Columns {
        parts: &[
            ("hours", "smallint"),
            ("minutes", "smallint"),
            ("seconds", "smallint"),
            ("frames", "smallint"),
            ("rate_numerator", "integer"),
            ("rate_denominator", "integer"),
            ("drop_frame", "boolean"),
        ],
        fixed: &[],
    },
    format: Format {
        key: FormatKey::Timecode,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"hours":1,"minutes":0,"seconds":0,"frames":0,"rate":"25/1","drop_frame":false}"#,
                "01:00:00:00",
            ),
            (
                r#"{"hours":0,"minutes":10,"seconds":0,"frames":0,"rate":"30000/1001","drop_frame":true}"#,
                "00:10:00;00",
            ),
            (
                r#"{"hours":0,"minutes":1,"seconds":0,"frames":2,"rate":"30000/1001","drop_frame":true}"#,
                "00:01:00;02",
            ),
            (
                r#"{"hours":23,"minutes":59,"seconds":59,"frames":59,"rate":"60000/1001","drop_frame":false}"#,
                "23:59:59:59",
            ),
        ],
        &[
            // A frame past the rate, a label drop-frame skips, drop-frame
            // at a rate that has none, an hour past 23, and the notation.
            r#"{"hours":0,"minutes":0,"seconds":0,"frames":25,"rate":"25/1","drop_frame":false}"#,
            r#"{"hours":0,"minutes":1,"seconds":0,"frames":1,"rate":"30000/1001","drop_frame":true}"#,
            r#"{"hours":0,"minutes":1,"seconds":0,"frames":3,"rate":"60000/1001","drop_frame":true}"#,
            r#"{"hours":0,"minutes":0,"seconds":0,"frames":0,"rate":"25/1","drop_frame":true}"#,
            r#"{"hours":24,"minutes":0,"seconds":0,"frames":0,"rate":"25/1","drop_frame":false}"#,
            r#""01:00:00:00""#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `SubtitleCue`'s parts, in order.
pub const SUBTITLE_CUE_FIELDS: [(&str, Part); 3] = [
    ("start", Part::Of(Builtin::Duration)),
    ("end", Part::Of(Builtin::Duration)),
    ("text", Part::Matching(CUE_TEXT_PATTERN)),
];

/// A cue's text: lines of text with no control character, one line feed
/// between them and none at either end, since a blank line ends a cue in
/// WebVTT.
pub const CUE_TEXT_PATTERN: &str = r"[^\p{Cc}]+(?:\n[^\p{Cc}]+)*";

/// The arrow between a WebVTT cue's times, which its text never holds.
pub const CUE_ARROW: &str = "-->";

/// A timed caption as WebVTT has it: when it starts and ends, from the
/// start of the media, to the millisecond, and its text. Written as the
/// cue's timings line and its text, `00:00:01.000 --> 00:00:04.000` and a
/// line feed, with no identifier or settings.
pub const SUBTITLE_CUE_ENTRY: StandardType = StandardType {
    name: SUBTITLE_CUE,
    holds: Holds::Object(&SUBTITLE_CUE_FIELDS),
    text: Some(TextForm::Cue),
    json: Json::Fields(&["start", "end", "text"]),
    storage: Storage::Columns {
        parts: &[("start", "bigint"), ("end", "bigint"), ("text", "text")],
        fixed: &[],
    },
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                "00:00:01.000 --> 00:00:04.000\nNever drink liquid nitrogen.",
                "00:00:01.000 --> 00:00:04.000\nNever drink liquid nitrogen.",
            ),
            (
                "00:01:05.250 --> 00:01:07.500\n— It will perforate your stomach.\n— You could die.",
                "00:01:05.250 --> 00:01:07.500\n— It will perforate your stomach.\n— You could die.",
            ),
            (
                "100:00:00.000 --> 100:00:00.001\nEnd",
                "100:00:00.000 --> 100:00:00.001\nEnd",
            ),
        ],
        &[
            // An end at or before its start, the arrow in its text, a
            // blank line, no text, hours left out, a comma before the
            // milliseconds (SRT's), settings, and an hour of one digit.
            "00:00:04.000 --> 00:00:01.000\nBackwards",
            "00:00:01.000 --> 00:00:01.000\nNo time at all",
            "00:00:01.000 --> 00:00:04.000\nAn --> arrow",
            "00:00:01.000 --> 00:00:04.000\nOne\n\nTwo",
            "00:00:01.000 --> 00:00:04.000\n",
            "00:00:01.000 --> 00:00:04.000",
            "00:01.000 --> 00:04.000\nShort",
            "00:00:01,000 --> 00:00:04,000\nSRT",
            "00:00:01.000 --> 00:00:04.000 align:start\nSettings",
            "0:00:01.000 --> 0:00:04.000\nOne digit",
            "00:00:60.000 --> 00:01:04.000\nSixty seconds",
            "",
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

// ---------------------------------------------------------------------
// Content ratings
// ---------------------------------------------------------------------

/// The classification bodies a `ContentRating` is by, each coded as it
/// writes its own name: the UK's BBFC for films, the US's Motion Picture
/// Association (the MPAA until 2019) for films, Europe's PEGI and North
/// America's ESRB for games.
pub const CONTENT_RATING_SCHEMES: &[(&str, &str)] = &[
    ("bbfc", "BBFC"),
    ("mpa", "MPA"),
    ("pegi", "PEGI"),
    ("esrb", "ESRB"),
];

/// Each body's categories, as it writes them, in `CONTENT_RATING_SCHEMES`'
/// order: BBFC's seven, the MPA's five, PEGI's five ages and ESRB's six
/// with Rating Pending.
pub const CONTENT_RATINGS: &[&[&str]] = &[
    &["U", "PG", "12A", "12", "15", "18", "R18"],
    &["G", "PG", "PG-13", "R", "NC-17"],
    &["3", "7", "12", "16", "18"],
    &["E", "E10+", "T", "M", "AO", "RP"],
];

/// A rating's text: one of a body's categories.
pub const RATING_PATTERN: &str = r"[0-9A-Z+\-]{1,5}";

/// A classification body.
pub const CONTENT_RATING_SCHEME_ENTRY: StandardType = StandardType {
    name: CONTENT_RATING_SCHEME,
    holds: Holds::Enum(CONTENT_RATING_SCHEMES),
    text: Some(TextForm::Code),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[("BBFC", "BBFC"), ("MPA", "MPA"), ("ESRB", "ESRB")],
        &["MPAA", "USK", "BBFC ", ""],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// Whether `rating` is one of the categories of the body coded `scheme`.
pub fn is_rating_of(rating: &str, scheme: &str) -> bool {
    CONTENT_RATING_SCHEMES
        .iter()
        .position(|(_, code)| *code == scheme)
        .and_then(|at| CONTENT_RATINGS.get(at))
        .is_some_and(|ratings| ratings.contains(&rating))
}

/// `ContentRating`'s rule across its parts (chunk 5.5t2): its rating is
/// one of its body's categories.
pub const CONTENT_RATING_RULES: &[StandardRule] = &[StandardRule {
    name: "rating_of_scheme",
    written: "rating is one of its scheme's categories",
    label: "not one of the rating body's categories",
    reason: "A `ContentRating`'s `rating` is one of the categories its `scheme`'s body publishes, so `BBFC 12A` is one and `PEGI 12A` isn't.",
    next: "Write one of the body's categories, or change `scheme`",
    check: Check::By(|parts| {
        fits(parts, "rating", &["scheme"], |rating, by| {
            by.first()
                .is_some_and(|scheme| is_rating_of(rating, scheme))
        })
    }),
}];

/// An age rating by the body that gave it, written as people write one,
/// the body's code, a space and its category: `BBFC 12A`, `PEGI 16`.
pub const CONTENT_RATING_ENTRY: StandardType = StandardType {
    name: CONTENT_RATING,
    holds: Holds::Object(&[
        ("scheme", Part::Standard(CONTENT_RATING_SCHEME)),
        ("rating", Part::Matching(RATING_PATTERN)),
    ]),
    text: Some(TextForm::CodeThenText),
    json: Json::Text,
    storage: Storage::Columns {
        parts: &[("scheme", "text"), ("rating", "text")],
        fixed: &[],
    },
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("BBFC 12A", "BBFC 12A"),
            ("MPA PG-13", "MPA PG-13"),
            ("PEGI 16", "PEGI 16"),
            ("ESRB E10+", "ESRB E10+"),
        ],
        &[
            "BBFC PG-13",
            "PEGI 15",
            "MPAA R",
            "BBFC12A",
            "BBFC  12A",
            "BBFC 12a",
            "12A",
            "",
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

// ---------------------------------------------------------------------
// Rules across a timecode's and a cue's parts
// ---------------------------------------------------------------------

/// `Timecode`'s rules across its parts (chunk 5.5t2), in order: its frames
/// below its rate, drop-frame only at 29.97 and 59.94, and never on a
/// label drop-frame counting skips.
pub const TIMECODE_RULES: &[StandardRule] = &[
    StandardRule {
        name: "frames_within_rate",
        written: "frames is below the rate's whole frames a second",
        label: "a frame past its rate",
        reason: "A `Timecode`'s `frames` count from 0 to one below its rate's whole frames a second, rounded up: 0 to 24 at 25/1, 0 to 29 at 30000/1001 (SMPTE ST 12-1).",
        next: "Write a frame below the rate's frames a second, or the rate the timecode counts at",
        check: Check::By(|parts| {
            match (
                parts.part("frames").value().and_then(|f| f.whole()),
                parts.part("rate").value().and_then(|r| r.rate()),
            ) {
                (Some(frames), Some((n, d))) if frames < nominal_frames(n, d) => Verdict::Holds,
                (Some(_), Some(_)) => Verdict::Breaks("frames"),
                _ => Verdict::Unknown,
            }
        }),
    },
    StandardRule {
        name: "drop_frame_rate",
        written: "drop_frame only at 30000/1001 or 60000/1001",
        label: "drop-frame at a rate with none",
        reason: "SMPTE ST 12-1 counts drop-frame only at 30000/1001 and 60000/1001 frames a second, where it keeps the timecode in step with the clock; every other rate counts each frame.",
        next: "Write `drop_frame: false`, or the rate the drop-frame timecode counts at",
        check: Check::By(|parts| {
            let drop = parts.part("drop_frame").value().and_then(|d| d.flag());
            let rate = parts.part("rate").value().and_then(|r| r.rate());
            match (drop, rate.map(|(n, d)| dropped_frames(n, d))) {
                (Some(false), _) | (_, Some(Some(_))) => Verdict::Holds,
                (Some(true), Some(None)) => Verdict::Breaks("drop_frame"),
                _ => Verdict::Unknown,
            }
        }),
    },
    StandardRule {
        name: "frame_not_dropped",
        written: "a drop-frame timecode is never a label drop-frame counting skips",
        label: "a label drop-frame skips",
        reason: "Drop-frame counting skips frames 0 and 1 (0 to 3 at 60000/1001) at the start of each minute except minutes 0, 10, 20, 30, 40 and 50, so that address is never a frame (SMPTE ST 12-1).",
        next: "Write the frame after the skipped ones, or count without drop-frame",
        check: Check::By(|parts| {
            let whole = |name: &str| parts.part(name).value().and_then(|n| n.whole());
            let drop = parts.part("drop_frame").value().and_then(|d| d.flag());
            let dropped = parts
                .part("rate")
                .value()
                .and_then(|r| r.rate())
                .map(|(n, d)| dropped_frames(n, d));
            // Counted without drop-frame, or at a rate with none, which
            // `drop_frame_rate` refuses, nothing is skipped.
            if drop == Some(false) || dropped == Some(None) {
                return Verdict::Holds;
            }
            let (minutes, seconds, frames) = (whole("minutes"), whole("seconds"), whole("frames"));
            // Off the minute's first labels, nothing is skipped either.
            if minutes.is_some_and(|m| m % 10 == 0)
                || seconds.is_some_and(|s| s != 0)
                || frames.is_some_and(|f| f >= 4)
            {
                return Verdict::Holds;
            }
            match (drop, dropped, frames) {
                (Some(true), Some(Some(skipped)), Some(frames))
                    if minutes.is_some() && seconds.is_some() =>
                {
                    if frames < skipped {
                        Verdict::Breaks("frames")
                    } else {
                        Verdict::Holds
                    }
                }
                _ => Verdict::Unknown,
            }
        }),
    },
];

/// `SubtitleCue`'s rules across its parts (chunk 5.5t2), in order: it
/// starts at or after the start of the media, ends after it starts, and its
/// text never holds the arrow.
pub const SUBTITLE_CUE_RULES: &[StandardRule] = &[
    StandardRule {
        name: "start_not_negative",
        written: "start >= 0ms",
        label: "before the media starts",
        reason: "A `SubtitleCue`'s times are from the start of the media, so it starts at 0 or after (WebVTT).",
        next: "Write a start of 0 or more",
        check: Check::By(
            |parts| match parts.part("start").value().and_then(|s| s.millis()) {
                Some(start) if start >= 0 => Verdict::Holds,
                Some(_) => Verdict::Breaks("start"),
                None => Verdict::Unknown,
            },
        ),
    },
    StandardRule {
        name: "end_after_start",
        written: "end > start",
        label: "not after the start",
        reason: "WebVTT requires a cue's end to be after its start.",
        next: "Write an end after the start",
        check: in_order(&[Order::below("start", "end")], "end"),
    },
    StandardRule {
        name: "text_without_arrow",
        written: "text holds no `-->`",
        label: "an arrow in the text",
        reason: "WebVTT reads `-->` as the line of a cue's times, so its text never holds one.",
        next: "Write the text without `-->`",
        check: Check::By(|parts| match parts.part("text") {
            Known::Given(Some(given)) => match given.text() {
                Some(text) if text.contains(CUE_ARROW) => Verdict::Breaks("text"),
                Some(_) => Verdict::Holds,
                None => Verdict::Unknown,
            },
            _ => Verdict::Unknown,
        }),
    },
];

/// Frames a second at `rate`, a fraction, rounded up: a timecode's frames
/// count up to one below it.
pub fn nominal_frames(numerator: i64, denominator: i64) -> i64 {
    if denominator <= 0 {
        return 0;
    }
    let (n, d) = (i128::from(numerator), i128::from(denominator));
    i64::try_from((n + d - 1) / d).unwrap_or(i64::MAX)
}

/// The frames drop-frame counting skips at the start of a minute at
/// `rate`: 2 at 30000/1001, 4 at 60000/1001, by value; `None` at a rate
/// with no drop-frame counting.
pub fn dropped_frames(numerator: i64, denominator: i64) -> Option<i64> {
    let (n, d) = (i128::from(numerator), i128::from(denominator));
    if n * 1001 == 30_000 * d {
        Some(2)
    } else if n * 1001 == 60_000 * d {
        Some(4)
    } else {
        None
    }
}

// ---------------------------------------------------------------------
// Text forms
// ---------------------------------------------------------------------

/// A whole number written with no sign and no leading zero, within
/// `min..=max`.
pub fn whole_in(text: &str, min: i64, max: i64) -> Option<i64> {
    let plain = !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    let n: i64 = plain.then(|| text.parse().ok()).flatten()?;
    (min..=max).contains(&n).then_some(n)
}

/// A decimal number as a canonical text writes one: an optional `-`, digits
/// with no leading zero but a lone one, and optional places, each a digit:
/// `-10.5`, `0`, `1.50`.
pub fn plain_decimal(text: &str) -> Option<crate::Num> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let (whole, places) = match digits.split_once('.') {
        Some((whole, places)) => (whole, Some(places)),
        None => (digits, None),
    };
    let all_digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(whole)
        || (whole.len() > 1 && whole.starts_with('0'))
        || places.is_some_and(|places| !all_digits(places))
    {
        return None;
    }
    crate::Num::parse(text)
}

/// A WebVTT timestamp to the millisecond, `hh:mm:ss.ttt` with at least two
/// digits of hours, written as a canonical text writes one.
pub fn cue_time(millis: i64) -> Option<String> {
    if millis < 0 {
        return None;
    }
    let (seconds, ms) = (millis / 1000, millis % 1000);
    let (minutes, second) = (seconds / 60, seconds % 60);
    let (hours, minute) = (minutes / 60, minutes % 60);
    Some(format!("{hours:02}:{minute:02}:{second:02}.{ms:03}"))
}

/// A WebVTT timestamp read in that form: hours of two digits or more (no
/// leading zero past two), minutes and seconds below 60, three digits of
/// milliseconds.
pub fn read_cue_time(text: &str) -> Option<i64> {
    let (clock, ms) = text.split_once('.')?;
    let mut parts = clock.split(':');
    let (hours, minutes, seconds) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let digits = |part: &str, len: Option<usize>| {
        !part.is_empty()
            && part.bytes().all(|b| b.is_ascii_digit())
            && len.is_none_or(|len| part.len() == len)
    };
    if !digits(hours, None)
        || hours.len() < 2
        || (hours.len() > 2 && hours.starts_with('0'))
        || !digits(minutes, Some(2))
        || !digits(seconds, Some(2))
        || !digits(ms, Some(3))
    {
        return None;
    }
    let (hours, minutes, seconds, ms): (i64, i64, i64, i64) = (
        hours.parse().ok()?,
        minutes.parse().ok()?,
        seconds.parse().ok()?,
        ms.parse().ok()?,
    );
    if minutes > 59 || seconds > 59 {
        return None;
    }
    hours
        .checked_mul(3_600_000)?
        .checked_add(minutes * 60_000 + seconds * 1000 + ms)
}

/// A cue's canonical text: its timings line and its text.
pub fn cue_text(start: i64, end: i64, text: &str) -> Option<String> {
    Some(format!(
        "{} {CUE_ARROW} {}\n{text}",
        cue_time(start)?,
        cue_time(end)?
    ))
}

/// A cue read from its canonical text: its start, its end, and its text,
/// its text's own pattern held. The rules across them are its rules
/// (`SUBTITLE_CUE_RULES`), which the cue is checked by once it is read.
pub fn read_cue(raw: &str) -> Option<(i64, i64, &str)> {
    let (timings, text) = raw.split_once('\n')?;
    let (start, end) = timings.split_once(&format!(" {CUE_ARROW} "))?;
    let (start, end) = (read_cue_time(start)?, read_cue_time(end)?);
    super::matches_whole(CUE_TEXT_PATTERN, text).then_some((start, end, text))
}

/// A timecode in SMPTE ST 12-1's notation: `hh:mm:ss:ff`, with `;` before
/// the frames when it counts drop-frame.
pub fn timecode_notation(
    hours: i64,
    minutes: i64,
    seconds: i64,
    frames: i64,
    drop: bool,
) -> String {
    let mark = if drop { ';' } else { ':' };
    format!("{hours:02}:{minutes:02}:{seconds:02}{mark}{frames:02}")
}

// ---------------------------------------------------------------------
// `media.fit`
// ---------------------------------------------------------------------

/// `media.fit`: a `width` by `height` image scaled to fit inside `within_w`
/// by `within_h`, keeping its aspect ratio and never made larger, each side
/// rounded down to a whole pixel and never below 1 (CSS's `object-fit:
/// scale-down`). Worked out in whole numbers: the side that limits is the
/// one whose box is narrower for the image's shape, and it is exactly the
/// box's.
pub fn fit(width: i64, height: i64, within_w: i64, within_h: i64) -> (i64, i64) {
    if width <= within_w && height <= within_h {
        return (width, height);
    }
    let (w, h, bw, bh) = (
        i128::from(width),
        i128::from(height),
        i128::from(within_w),
        i128::from(within_h),
    );
    // Scaled by bw / w when that is the smaller factor: bw * h <= bh * w.
    let (fw, fh) = if bw * h <= bh * w {
        (bw, (h * bw) / w)
    } else {
        ((w * bh) / h, bh)
    };
    let side = |n: i128| i64::try_from(n.max(1)).unwrap_or(i64::MAX);
    (side(fw), side(fh))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_the_aspect_ratio_rounding_down() {
        // CSS `object-fit: scale-down`, each side rounded down.
        assert_eq!(fit(4000, 3000, 320, 320), (320, 240));
        assert_eq!(fit(3000, 4000, 320, 320), (240, 320));
        assert_eq!(fit(1000, 333, 320, 320), (320, 106));
        assert_eq!(fit(1920, 1080, 1280, 1280), (1280, 720));
        // Never larger than the original.
        assert_eq!(fit(200, 100, 320, 320), (200, 100));
        assert_eq!(fit(320, 320, 320, 320), (320, 320));
        // A sliver keeps a pixel.
        assert_eq!(fit(100_000, 1, 100, 100), (100, 1));
        // The widest sides don't overflow.
        assert_eq!(fit(MOST_PIXELS, MOST_PIXELS, 1, 1), (1, 1));
        assert_eq!(
            fit(MOST_PIXELS, 2, MOST_PIXELS - 1, 2),
            (MOST_PIXELS - 1, 1)
        );
    }

    /// The first of a timecode's rules its known parts break, by name.
    fn timecode_broken(
        minutes: Option<i64>,
        seconds: Option<i64>,
        frames: Option<i64>,
        rate: Option<(i64, i64)>,
        drop_frame: Option<bool>,
    ) -> Option<(&'static str, Verdict)> {
        use super::super::across::{Given, Listed};
        let whole = |n: Option<i64>| match n {
            Some(n) => Known::Given(Some(Given::Number(crate::Num::whole(i128::from(n))))),
            None => Known::Unknown,
        };
        let parts = Listed {
            parts: vec![
                ("minutes", whole(minutes)),
                ("seconds", whole(seconds)),
                ("frames", whole(frames)),
                (
                    "rate",
                    rate.map_or(Known::Unknown, |(n, d)| {
                        Known::Given(Some(Given::Rate(n, d)))
                    }),
                ),
                (
                    "drop_frame",
                    drop_frame.map_or(Known::Unknown, |flag| Known::Given(Some(Given::Bool(flag)))),
                ),
            ],
            ..Listed::default()
        };
        TIMECODE_RULES
            .iter()
            .find_map(|rule| match rule.verdict(&parts) {
                Verdict::Holds | Verdict::Unknown => None,
                broken => Some((rule.name, broken)),
            })
    }

    #[test]
    fn drop_frame_skips_two_labels_a_minute_but_every_tenth() {
        let ntsc = Some((30_000, 1001));
        let skipped = |m, s, f| timecode_broken(Some(m), Some(s), Some(f), ntsc, Some(true));
        let dropped = Some(("frame_not_dropped", Verdict::Breaks("frames")));
        assert_eq!(skipped(1, 0, 0), dropped);
        assert_eq!(skipped(1, 0, 1), dropped);
        assert_eq!(skipped(1, 0, 2), None);
        assert_eq!(skipped(10, 0, 0), None);
        assert_eq!(skipped(1, 1, 0), None);
        let fifty_nine = Some((60_000, 1001));
        assert!(timecode_broken(Some(1), Some(0), Some(3), fifty_nine, Some(true)).is_some());
        assert_eq!(
            timecode_broken(Some(1), Some(0), Some(4), fifty_nine, Some(true)),
            None
        );
        // By value, however the rate is written.
        assert_eq!(dropped_frames(2997, 100), None);
        assert_eq!(dropped_frames(60_000, 2002), Some(2));
        assert_eq!(
            timecode_broken(None, None, Some(0), Some((25, 1)), Some(true)),
            Some(("drop_frame_rate", Verdict::Breaks("drop_frame")))
        );
        assert_eq!(nominal_frames(30_000, 1001), 30);
        assert_eq!(nominal_frames(25, 1), 25);
    }

    #[test]
    fn cue_times_are_webvtt_timestamps() {
        assert_eq!(cue_time(3_723_004).as_deref(), Some("01:02:03.004"));
        assert_eq!(read_cue_time("01:02:03.004"), Some(3_723_004));
        assert_eq!(read_cue_time("1:02:03.004"), None);
        assert_eq!(read_cue_time("02:03.004"), None);
        assert_eq!(read_cue_time("001:02:03.004"), None);
        assert_eq!(read_cue_time("100:00:00.000"), Some(360_000_000));
        assert!(is_rating_of("12A", "BBFC"));
        assert!(!is_rating_of("12A", "PEGI"));
    }
}

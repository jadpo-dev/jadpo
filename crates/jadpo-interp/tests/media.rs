//! Area 19, "Media" (chunk 5.5p), run: the page's example
//! (`project=media`), a video library's thumbnails fitted as CSS's
//! `object-fit: scale-down` fits them and rounded down, a film's age rating
//! read as one of its body's categories, an edit's start shown in SMPTE's
//! notation and a caption written as a WebVTT cue; and the other media
//! types read from text by their standards, refusing what isn't one.

use crate::support;

use jadpo_interp::Value;
use support::{failure, main, text, value};

const SOURCE: &str = "type FormText = Text {}

type NotARating = Failure {
    kind: FailureKind.invalid_value
}

/// A still's thumbnail: scaled to fit a 320-pixel square, never enlarged.
function thumbnail(video_still: ImageDimensions) -> ImageDimensions {
    var tile: ImageDimensions = ImageDimensions {
        width: 320
        height: 320
    }
    return media.fit(video_still, within: tile)
}

/// The age rating a film's form names.
function film_rating(raw: FormText) fails NotARating -> ContentRating {
    var typed_rating: ContentRating = attempt text.parse(raw, as: ContentRating) {
        malformed: NotARating
        rating_of_scheme: NotARating
    }
    return typed_rating
}

/// Where an edit starts, as the edit list shows it.
function edit_start(edit_point: Timecode) -> PresentationText {
    return text.format(edit_point, locale: Locale.en_gb)
}

/// A caption as a WebVTT file writes it.
function caption_text(opening_caption: SubtitleCue) -> FormText {
    return FormText(text.from(opening_caption))
}
";

fn size(width: i64, height: i64) -> Value {
    Value::record(
        "ImageDimensions",
        vec![
            ("width".into(), Value::Int(width)),
            ("height".into(), Value::Int(height)),
        ],
    )
}

#[test]
fn a_thumbnail_fits_its_tile_and_is_never_enlarged() {
    let program = main(SOURCE).with_proof_checks();
    let thumbnail = |w: i64, h: i64| value(&program, "thumbnail", vec![size(w, h)]);
    assert_eq!(thumbnail(4000, 3000), size(320, 240));
    assert_eq!(thumbnail(3000, 4000), size(240, 320));
    assert_eq!(thumbnail(1000, 333), size(320, 106));
    assert_eq!(thumbnail(1920, 1080), size(320, 180));
    assert_eq!(thumbnail(200, 100), size(200, 100));
    assert_eq!(thumbnail(5000, 1), size(320, 1));
}

#[test]
fn a_rating_is_one_of_its_body_s_categories() {
    let program = main(SOURCE).with_proof_checks();
    for raw in ["BBFC 12A", "MPA PG-13", "PEGI 16", "ESRB E10+"] {
        let Value::Record(rating) = value(&program, "film_rating", vec![text(raw)]) else {
            panic!("a rating");
        };
        assert!(rating.get("scheme").is_some(), "{raw}");
    }
    for raw in ["BBFC PG-13", "PEGI 15", "MPAA R", "12A", "BBFC  12A"] {
        assert_eq!(
            failure(&program, "film_rating", vec![text(raw)]).name,
            "NotARating",
            "{raw}"
        );
    }
}

const READ: &str = "type FormText = Text {}

type Unreadable = Failure {
    kind: FailureKind.invalid_value
}

function dimensions(raw: FormText) fails Unreadable -> ImageDimensions {
    return attempt text.parse(raw, as: ImageDimensions) {
        malformed: Unreadable
    }
}

function frame_rate(raw: FormText) fails Unreadable -> FrameRate {
    return attempt text.parse(raw, as: FrameRate) {
        malformed: Unreadable
    }
}

function aspect(raw: FormText) fails Unreadable -> AspectRatio {
    return attempt text.parse(raw, as: AspectRatio) {
        malformed: Unreadable
    }
}

function view_box(raw: FormText) fails Unreadable -> Rect {
    return attempt text.parse(raw, as: Rect) {
        malformed: Unreadable
    }
}

function padding(raw: FormText) fails Unreadable -> Insets {
    return attempt text.parse(raw, as: Insets) {
        malformed: Unreadable
    }
}

function video_codec(raw: FormText) fails Unreadable -> VideoCodec {
    return attempt text.parse(raw, as: VideoCodec) {
        malformed: Unreadable
    }
}

function audio_codec(raw: FormText) fails Unreadable -> AudioCodec {
    return attempt text.parse(raw, as: AudioCodec) {
        malformed: Unreadable
    }
}

function caption(raw: FormText) fails Unreadable -> SubtitleCue {
    return attempt text.parse(raw, as: SubtitleCue) {
        malformed: Unreadable
        invalid: Unreadable
    }
}

function written(cue: SubtitleCue) -> FormText {
    return FormText(text.from(cue))
}
";

#[test]
fn media_types_are_read_by_their_standards() {
    let program = main(READ).with_proof_checks();
    let cases: &[(&str, &str, &str)] = &[
        ("dimensions", "1920x1080", "1920 x 1080"),
        ("frame_rate", "30000/1001", "29.97"),
        ("aspect", "16:9", "1.78"),
        ("view_box", "0 0 300 200", "0,0,300,200"),
        ("padding", "8 16 8 16", "8 16"),
        ("video_codec", "avc1.42001E", "h264"),
        ("audio_codec", "mp4a.40.2", "aac"),
        (
            "caption",
            "00:00:01.000 --> 00:00:04.000\nHello",
            "00:00:01,000 --> 00:00:04,000\nHello",
        ),
    ];
    for (function, valid, invalid) in cases {
        assert!(
            !matches!(value(&program, function, vec![text(valid)]), Value::None),
            "{function}: `{valid}`"
        );
        assert_eq!(
            failure(&program, function, vec![text(invalid)]).name,
            "Unreadable",
            "{function}: `{invalid}`"
        );
    }
    let cue = value(
        &program,
        "caption",
        vec![text("00:01:05.250 --> 00:01:07.500\nOne\nTwo")],
    );
    assert_eq!(
        value(&program, "written", vec![cue]),
        text("00:01:05.250 --> 00:01:07.500\nOne\nTwo")
    );
    assert_eq!(
        failure(
            &program,
            "caption",
            vec![text("00:00:04.000 --> 00:00:01.000\nBackwards")]
        )
        .name,
        "Unreadable"
    );
}

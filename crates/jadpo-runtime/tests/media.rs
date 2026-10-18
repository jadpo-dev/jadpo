//! Area 19, "Media" (chunk 5.5p), in the runtime: `media.fit` as compiled
//! code calls it (`jadpo_value::run`), held to CSS's `object-fit:
//! scale-down` with each side rounded down to a whole pixel; and the rules
//! across a timecode's and a cue's parts as a value read from JSON meets
//! them. The page's test (`project=media`) and the interpreter's `media`
//! tests assert the same values. The types' text, JSON and columns are the
//! one harness's (`standard_types`).

use jadpo_std::Op;
use jadpo_std::standard::media::{IMAGE_DIMENSIONS, SUBTITLE_CUE, TIMECODE};
use jadpo_value::{Value, run, standard};

fn size(width: i64, height: i64) -> Value {
    Value::record(
        IMAGE_DIMENSIONS,
        vec![
            ("width".into(), Value::Int(width)),
            ("height".into(), Value::Int(height)),
        ],
    )
}

fn fit(image: (i64, i64), within: (i64, i64)) -> (i64, i64) {
    let fitted = run(
        Op::MediaFit,
        &[size(image.0, image.1), size(within.0, within.1)],
        None,
        &mut |_| None,
    )
    .expect("fits");
    let Value::Record(record) = fitted else {
        panic!("dimensions");
    };
    match (record.get("width"), record.get("height")) {
        (Some(Value::Int(w)), Some(Value::Int(h))) => (*w, *h),
        other => panic!("{other:?}"),
    }
}

#[test]
fn fit_is_object_fit_scale_down_rounded_down() {
    // The page's thumbnails.
    assert_eq!(fit((4000, 3000), (320, 320)), (320, 240));
    assert_eq!(fit((1000, 333), (320, 320)), (320, 106));
    assert_eq!(fit((200, 100), (320, 320)), (200, 100));
    // `contain`'s scale, min(320 / 3000, 240 / 4000), for a tall photo in
    // a wide box: the height limits.
    assert_eq!(fit((3000, 4000), (320, 240)), (180, 240));
    // 1920x1080 into 1280x1280 is exactly 720p; into 1000x1000 the height
    // is 562.5, rounded down.
    assert_eq!(fit((1920, 1080), (1280, 1280)), (1280, 720));
    assert_eq!(fit((1920, 1080), (1000, 1000)), (1000, 562));
    // An image that fits one side and not the other still shrinks.
    assert_eq!(fit((500, 2000), (1000, 1000)), (250, 1000));
    // Never below a pixel.
    assert_eq!(fit((10_000, 3), (100, 100)), (100, 1));
}

fn timecode(minutes: i64, seconds: i64, frames: i64, rate: (i64, i64), drop: bool) -> Value {
    let rate = Value::record(
        "FrameRate",
        vec![
            ("numerator".into(), Value::Int(rate.0)),
            ("denominator".into(), Value::Int(rate.1)),
        ],
    );
    Value::record(
        TIMECODE,
        vec![
            ("hours".into(), Value::Int(0)),
            ("minutes".into(), Value::Int(minutes)),
            ("seconds".into(), Value::Int(seconds)),
            ("frames".into(), Value::Int(frames)),
            ("rate".into(), rate),
            ("drop_frame".into(), Value::Bool(drop)),
        ],
    )
}

fn broken(value: &Value) -> Option<&'static str> {
    match value {
        Value::Record(record) => standard::broken_part(record),
        _ => panic!("a record"),
    }
}

#[test]
fn drop_frame_counts_as_smpte_st_12_1_has_it() {
    let ntsc = (30_000, 1001);
    // Frames 0 and 1 are skipped at each minute but every tenth.
    assert_eq!(broken(&timecode(1, 0, 0, ntsc, true)), Some("frames"));
    assert_eq!(broken(&timecode(1, 0, 1, ntsc, true)), Some("frames"));
    assert_eq!(broken(&timecode(1, 0, 2, ntsc, true)), None);
    assert_eq!(broken(&timecode(10, 0, 0, ntsc, true)), None);
    assert_eq!(broken(&timecode(1, 0, 0, ntsc, false)), None);
    // 0 to 3 at 59.94.
    assert_eq!(
        broken(&timecode(1, 0, 3, (60_000, 1001), true)),
        Some("frames")
    );
    assert_eq!(broken(&timecode(1, 0, 4, (60_000, 1001), true)), None);
    // Drop-frame only at those rates; frames below the rate's.
    assert_eq!(
        broken(&timecode(0, 0, 0, (25, 1), true)),
        Some("drop_frame")
    );
    assert_eq!(broken(&timecode(0, 0, 29, ntsc, false)), None);
    assert_eq!(broken(&timecode(0, 0, 30, ntsc, false)), Some("frames"));
    assert_eq!(broken(&timecode(0, 0, 24, (25, 1), false)), None);
}

#[test]
fn a_cue_ends_after_it_starts_and_holds_no_arrow() {
    let cue = |start: i64, end: i64, text: &str| {
        Value::record(
            SUBTITLE_CUE,
            vec![
                ("start".into(), Value::Duration(start)),
                ("end".into(), Value::Duration(end)),
                ("text".into(), Value::text(text)),
            ],
        )
    };
    assert_eq!(broken(&cue(1000, 4000, "Hello")), None);
    assert_eq!(broken(&cue(4000, 1000, "Hello")), Some("end"));
    assert_eq!(broken(&cue(1000, 1000, "Hello")), Some("end"));
    assert_eq!(broken(&cue(-1, 1000, "Hello")), Some("start"));
    assert_eq!(broken(&cue(1000, 4000, "a --> b")), Some("text"));
    // Its canonical text, as the page's test writes it.
    assert_eq!(
        standard::from(&cue(1000, 4000, "Never drink liquid nitrogen.")),
        Ok(Value::text(
            "00:00:01.000 --> 00:00:04.000\nNever drink liquid nitrogen."
        ))
    );
}

#[test]
fn a_rating_is_one_of_its_body_s_categories() {
    let read = |text: &str| standard::parse(text, "ContentRating");
    assert!(read("BBFC 12A").is_ok());
    assert!(read("bbfc 12A").is_ok(), "a body's code ignores case");
    assert!(read("MPA NC-17").is_ok());
    assert!(read("ESRB AO").is_ok());
    for refused in ["BBFC PG-13", "MPA 12A", "PEGI 15", "ESRB 18", "BBFC 12a"] {
        assert!(read(refused).is_err(), "{refused}");
    }
    assert_eq!(
        read("bbfc 12A").and_then(|rating| standard::from(&rating)),
        Ok(Value::text("BBFC 12A"))
    );
}

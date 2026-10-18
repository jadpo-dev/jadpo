//! Area 19, the geospatial types (chunk 5.5s), in the runtime: a point
//! built by its names, written as GeoJSON by the codec compiled code uses
//! with its longitude first, and read back with neither axis swapped; a
//! ring wound the other way written by the right-hand rule; and a latitude
//! and a bearing shown as compiled code calls `text.format`, with the
//! standard type the checker names. The interpreter's `geospatial` tests
//! assert the same values; the types' text, JSON and columns are the one
//! harness's (`standard_types`).

use jadpo_std::Op;
use jadpo_std::http::{Codec, CodecId};
use jadpo_value::json::{Codecs, Path};
use jadpo_value::{Decimal, Value, run};

/// The codec table of one geometry type, as the plan makes it.
fn table(ty: &str) -> Vec<Codec> {
    vec![Codec::Standard { ty: ty.to_string() }]
}

fn written(ty: &str, value: &Value) -> String {
    let codecs = table(ty);
    let codecs = Codecs {
        codecs: &codecs,
        validators: &[],
        shapes: &[],
    };
    let mut out = String::new();
    codecs.write(CodecId(0), value, &mut out);
    out
}

fn read(ty: &str, json: &str) -> Option<Value> {
    let codecs = table(ty);
    let codecs = Codecs {
        codecs: &codecs,
        validators: &[],
        shapes: &[],
    };
    let mut problems = Vec::new();
    codecs.read(
        CodecId(0),
        &serde_json::from_str(json).expect("JSON"),
        &Path::default(),
        &mut problems,
    )
}

fn decimal(text: &str) -> Decimal {
    Decimal::parse(text).expect("a decimal")
}

#[test]
fn a_point_round_trips_through_geojson_with_its_axes_unswapped() {
    let sydney = jadpo_value::geo::point(decimal("-33.8688"), decimal("151.2093"), Value::None);
    let json = written("Point", &sydney);
    assert_eq!(
        json,
        r#"{"type":"Point","coordinates":[151.2093,-33.8688]}"#
    );
    let Some(Value::Record(back)) = read("Point", &json) else {
        panic!("{json} reads back");
    };
    assert_eq!(
        back.get("latitude"),
        Some(&Value::Decimal(decimal("-33.8688")))
    );
    assert_eq!(
        back.get("longitude"),
        Some(&Value::Decimal(decimal("151.2093")))
    );
}

#[test]
fn a_ring_is_written_by_the_right_hand_rule() {
    let corner = |longitude: &str, latitude: &str| {
        jadpo_value::geo::point(decimal(latitude), decimal(longitude), Value::None)
    };
    // Built clockwise, as code may build it.
    let clockwise = vec![
        corner("0", "0"),
        corner("0", "1"),
        corner("1", "1"),
        corner("1", "0"),
        corner("0", "0"),
    ];
    let polygon = Value::record(
        "Polygon",
        vec![
            ("outer".into(), Value::list(clockwise)),
            ("holes".into(), Value::list(Vec::new())),
        ],
    );
    assert_eq!(
        written("Polygon", &polygon),
        r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}"#
    );
    assert_eq!(
        read(
            "Polygon",
            r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1]]]}"#
        ),
        None,
        "a ring left open"
    );
}

/// `text.format` as compiled code calls it, with the standard type the
/// checker names.
fn shown(value: Value, standard: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::LOCALE, "en_gb").expect("a locale");
    let locale = Value::variant(jadpo_std::LOCALE, ordinal, "en_gb", Vec::new());
    run(
        Op::TextFormat,
        &[
            value,
            locale,
            Value::None,
            Value::None,
            Value::None,
            Value::None,
            Value::None,
            Value::None,
            Value::text(standard),
        ],
        None,
        &mut |_| None,
    )
    .expect("shown")
}

#[test]
fn coordinates_and_bearings_are_shown_with_their_hemispheres_and_degrees() {
    assert_eq!(
        shown(Value::Decimal(decimal("-33.8688")), "Latitude"),
        Value::text("33.8688° S")
    );
    assert_eq!(
        shown(Value::Decimal(decimal("151.2093")), "Longitude"),
        Value::text("151.2093° E")
    );
    assert_eq!(
        shown(Value::Decimal(decimal("247.5")), "Bearing"),
        Value::text("247.5°")
    );
    let depot = jadpo_value::geo::point(decimal("51.5072"), decimal("-0.1276"), Value::None);
    assert_eq!(shown(depot, "Point"), Value::text("51.5072° N, 0.1276° W"));
}

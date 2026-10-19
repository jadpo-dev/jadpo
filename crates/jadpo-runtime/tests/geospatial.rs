//! Area 19, the geospatial types (chunk 5.5s), in the runtime: a point
//! built by its names, written as GeoJSON by the codec compiled code uses
//! with its longitude first, and read back with neither axis swapped; a
//! ring wound the other way written by the right-hand rule; and a latitude
//! and a bearing shown as compiled code calls `text.format`, with the
//! standard type the checker names. The interpreter's `geospatial` tests
//! assert the same values; the types' text, JSON and columns are the one
//! harness's (`jadpo-interp`'s `standard_types`).

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

/// `geo.from_wkt` and `geo.to_wkt` as compiled code calls them (area 20,
/// "Interop"; chunk 5.5u6): the same values and conditions the
/// interpreter's `geo_interop` tests assert.
#[test]
fn a_patch_round_trips_through_wkt_as_compiled_code_runs_it() {
    let patch = "POLYGON((-0.2 51.49,-0.05 51.49,-0.05 51.54,-0.2 51.54,-0.2 51.49),(-0.15 51.5,-0.15 51.52,-0.1 51.52,-0.1 51.5,-0.15 51.5))";
    let read = run(Op::GeoFromWkt, &[Value::text(patch)], None, &mut |_| None).expect("a patch");
    let Value::Variant(variant) = &read else {
        panic!("a geometry");
    };
    assert_eq!(&*variant.name, "polygon");
    assert_eq!(
        run(Op::GeoToWkt, std::slice::from_ref(&read), None, &mut |_| {
            None
        }),
        Ok(Value::text(patch))
    );
    assert_eq!(
        run(
            Op::GeoFromWkt,
            &[Value::text("SRID=4258;POINT(1 2)")],
            None,
            &mut |_| None
        ),
        Err(jadpo_value::Fault::Broke(jadpo_std::RuleKey::OutOfRange))
    );
    assert_eq!(
        run(
            Op::GeoFromWkt,
            &[Value::text("POINT EMPTY")],
            None,
            &mut |_| None
        ),
        Err(jadpo_value::Fault::Broke(jadpo_std::RuleKey::Malformed))
    );
}

/// A GeoJSON feature collection read and written by the codec compiled
/// code uses (area 20, "Interop"; chunk 5.5u6): its features' geometries
/// as GeoJSON, their properties as their own type's JSON, and back
/// unchanged; the interpreter's `geo_interop` tests run the same through
/// `json.read` and `json.write`.
#[test]
fn a_feature_collection_round_trips_through_json_as_compiled_code_runs_it() {
    use jadpo_std::types::{FieldPlan, ShapeId, ShapePlan, ValidatorId};
    let properties = ShapePlan {
        ty: "PinProperties".to_string(),
        variant: None,
        fields: vec![FieldPlan {
            key: "field PinProperties.label".to_string(),
            name: "label".to_string(),
            validator: ValidatorId(0),
            absent: None,
            omittable: false,
            none_left_out: false,
            section: None,
        }],
    };
    let codecs = vec![
        Codec::FeatureCollection {
            feature: CodecId(1),
        },
        Codec::Feature {
            geometry: CodecId(2),
            properties: CodecId(3),
        },
        Codec::Standard {
            ty: "Point".to_string(),
        },
        Codec::Object {
            shape: ShapeId(0),
            fields: vec![CodecId(4)],
            validator: None,
        },
        Codec::Scalar {
            builtin: jadpo_std::Builtin::Text,
            validator: ValidatorId(0),
            int_as_number: false,
        },
    ];
    let validators = vec![jadpo_value::Validator::new(
        Some(jadpo_std::Builtin::Text),
        Vec::new(),
    )];
    let shapes = vec![jadpo_value::shape::Shape::new(&properties)];
    let codecs = Codecs {
        codecs: &codecs,
        validators: &validators,
        shapes: &shapes,
    };
    let layer = r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[-0.1276,51.5072]},"properties":{"label":"Crumbs"}}]}"#;
    let mut problems = Vec::new();
    let read = codecs
        .read(
            CodecId(0),
            &serde_json::from_str(layer).expect("JSON"),
            &Path::default(),
            &mut problems,
        )
        .unwrap_or_else(|| panic!("{problems:?}"));
    let mut written = String::new();
    codecs.write(CodecId(0), &read, &mut written);
    assert_eq!(written, layer);
    // An `id` is refused at its place.
    let mut problems = Vec::new();
    let with_id = layer.replace(r#"{"type":"Feature","#, r#"{"type":"Feature","id":1,"#);
    assert!(
        codecs
            .read(
                CodecId(0),
                &serde_json::from_str(&with_id).expect("JSON"),
                &Path::default(),
                &mut problems
            )
            .is_none()
    );
    assert_eq!(
        problems,
        [jadpo_value::json::Problem {
            field: "features.0.id".to_string(),
            rule: "unknown_field".to_string(),
        }]
    );
}

/// `geo.from_geojson`, `geo.from_lng_lat` and `geo.from_lat_lng` as
/// compiled code calls them (area 19, "Shared vocabulary types"; chunk
/// 5.11): the same values and conditions the interpreter's `geo_interop`
/// tests assert.
#[test]
fn the_constructions_at_an_axis_order_s_edges_as_compiled_code_runs_them() {
    let call = |op: Op, arg: Value| run(op, &[arg], None, &mut |_| None);
    let depot = jadpo_value::geo::point(
        Decimal::parse("51.5072").expect("a latitude"),
        Decimal::parse("-0.1276").expect("a longitude"),
        Value::None,
    );
    let pair = |items: &[&str]| {
        Value::list(
            items
                .iter()
                .map(|item| Value::decimal(item).expect("a number"))
                .collect(),
        )
    };
    assert_eq!(
        call(Op::GeoFromLngLat, pair(&["-0.1276", "51.5072"])),
        Ok(depot.clone())
    );
    assert_eq!(
        call(Op::GeoFromLatLng, pair(&["51.5072", "-0.1276"])),
        Ok(depot.clone())
    );
    assert_eq!(
        call(
            Op::GeoFromLatLng,
            Value::list(vec![Value::Int(51), Value::Int(0)])
        ),
        Ok(jadpo_value::geo::point(
            Decimal::new(51, 0),
            Decimal::new(0, 0),
            Value::None
        ))
    );
    let read = call(
        Op::GeoFromGeojson,
        Value::text(r#"{"type":"Point","coordinates":[-0.1276,51.5072]}"#),
    )
    .expect("a point");
    let Value::Variant(variant) = &read else {
        panic!("a geometry");
    };
    assert_eq!(&*variant.name, "point");
    assert_eq!(variant.fields.first().map(|(_, held)| held), Some(&depot));
    assert_eq!(
        call(Op::GeoToWkt, read),
        Ok(Value::text("POINT(-0.1276 51.5072)"))
    );
    for (op, arg, condition) in [
        (
            Op::GeoFromGeojson,
            Value::text(r#"{"type":"Point","coordinates":[0,91]}"#),
            jadpo_std::RuleKey::OutOfRange,
        ),
        (
            Op::GeoFromGeojson,
            Value::text("POINT(0 0)"),
            jadpo_std::RuleKey::Malformed,
        ),
        (
            Op::GeoFromLatLng,
            pair(&["91", "0"]),
            jadpo_std::RuleKey::OutOfRange,
        ),
        (
            Op::GeoFromLngLat,
            pair(&["0"]),
            jadpo_std::RuleKey::Malformed,
        ),
    ] {
        assert_eq!(
            call(op, arg),
            Err(jadpo_value::Fault::Broke(condition)),
            "{op:?}"
        );
    }
}

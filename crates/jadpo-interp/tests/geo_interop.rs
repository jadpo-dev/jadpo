//! Area 20's interop (chunk 5.5u6), run: a delivery patch read from a
//! planner's WKT export, longitude first, and written back as PostGIS's
//! `ST_AsText` writes it, each axis still the one it was named; WKT that
//! isn't a geometry fails `malformed` and a coordinate out of its range or
//! another reference system `out_of_range`, each mapped by its `attempt`.
//! PostGIS's own reading and writing of the same text is
//! `jadpo-cli`'s `geography` test.

use crate::support;

use jadpo_interp::Value;
use support::{decimal, failure, main, text, value};

const WKT: &str = "type Exported = Text {}

type Unreadable = Failure {
    kind: FailureKind.invalid_value
}

type OutOfReach = Failure {
    kind: FailureKind.invalid_value
}

/// A shape from the planner's export, as WKT.
function shape_from_export(cell: Exported) fails Unreadable, OutOfReach -> Geometry {
    return attempt geo.from_wkt(cell) {
        malformed: Unreadable
        out_of_range: OutOfReach
    }
}

/// A shape for the planner's export.
function exported(shape: Geometry) -> Exported {
    return Exported(geo.to_wkt(shape))
}

/// A patch for the planner's export.
function patch_exported(patch: Polygon) -> Exported {
    return Exported(geo.to_wkt(patch))
}

/// A patch from the export, if the shape is one.
function patch_of(shape: Geometry) -> Polygon? {
    return match shape {
        Geometry.point -> none
        Geometry.line_string -> none
        Geometry.polygon(drawn) -> drawn.polygon
        Geometry.multi_point -> none
        Geometry.multi_line_string -> none
        Geometry.multi_polygon -> none
    }
}

/// The depot, written in code.
function depot() -> Point {
    return Point {
        latitude: 51.5072
        longitude: -0.1276
        altitude: 35metre
    }
}

/// The depot for the planner's export.
function depot_exported() -> Exported {
    return Exported(geo.to_wkt(depot()))
}
";

const PATCH: &str = "POLYGON((-0.2 51.49,-0.05 51.49,-0.05 51.54,-0.2 51.54,-0.2 51.49),(-0.15 51.5,-0.15 51.52,-0.1 51.52,-0.1 51.5,-0.15 51.5))";

fn patch(program: &jadpo_interp::Program, wkt: &str) -> Value {
    let shape = value(program, "shape_from_export", vec![text(wkt)]);
    match value(program, "patch_of", vec![shape]) {
        Value::None => panic!("{wkt} is a polygon"),
        some => some,
    }
}

#[test]
fn a_patch_round_trips_through_wkt_with_its_axes_unswapped() {
    let program = main(WKT).with_proof_checks();
    let read = patch(&program, PATCH);
    assert_eq!(
        value(&program, "patch_exported", vec![read.clone()]),
        text(PATCH)
    );
    // The first corner is 51.49° N, so the longitude came first.
    let Value::Record(polygon) = &read else {
        panic!("a polygon");
    };
    let Some(Value::List(outer)) = polygon.get("outer") else {
        panic!("its outer ring");
    };
    let Some(Value::Record(corner)) = outer.first() else {
        panic!("a corner");
    };
    assert_eq!(corner.get("latitude"), Some(&decimal("51.49")));
    assert_eq!(corner.get("longitude"), Some(&decimal("-0.2")));
    // The same text in another case, with spaces and EWKT's system, reads
    // as the same patch.
    let ewkt = format!("SRID=4326; {}", PATCH.to_lowercase().replace(',', ", "));
    assert_eq!(patch(&program, &ewkt), read);
    // Every kind goes back out as the geometry it came in as.
    for wkt in [
        "POINT(-0.1276 51.5072)",
        "POINT Z (-0.1276 51.5072 35)",
        "LINESTRING(0 0,1 1)",
        "MULTIPOINT((1 2),(3 4))",
        "MULTILINESTRING((0 0,1 1),(2 2,3 3))",
        "MULTIPOLYGON(((0 0,1 0,1 1,0 0)),((2 2,3 2,3 3,2 2)))",
    ] {
        let shape = value(&program, "shape_from_export", vec![text(wkt)]);
        assert_eq!(value(&program, "exported", vec![shape]), text(wkt));
    }
    assert_eq!(
        value(&program, "depot_exported", vec![]),
        text("POINT Z (-0.1276 51.5072 35)")
    );
}

#[test]
fn wkt_that_isn_t_a_geometry_fails_with_its_condition() {
    let program = main(WKT).with_proof_checks();
    for (cell, failed) in [
        ("POINT(0 91)", "OutOfReach"),
        ("SRID=4258;POINT(1 2)", "OutOfReach"),
        ("POINT EMPTY", "Unreadable"),
        ("GEOMETRYCOLLECTION(POINT(0 0))", "Unreadable"),
        ("POLYGON((0 0,1 0,1 1,0 1))", "Unreadable"),
        ("POLYGON((0 0,10 10,0 10,10 0,0 0))", "Unreadable"),
        ("LINESTRING(0 0,1 1 5)", "Unreadable"),
        ("{\"type\":\"Point\",\"coordinates\":[0,0]}", "Unreadable"),
    ] {
        assert_eq!(
            failure(&program, "shape_from_export", vec![text(cell)]).name,
            failed,
            "{cell}"
        );
    }
}

/// A geometry mixing positions with and without an altitude is never
/// made (area 20, "Altitudes are all or nothing", chunk 5.5u7): built from
/// values it breaks `altitudes_agree`, which its `attempt` maps, so
/// `geo.to_wkt` only ever meets one it can write; WKT that mixes them is
/// `malformed`.
#[test]
fn a_shape_mixing_altitudes_never_reaches_wkt() {
    let program = main(
        &(WKT.to_string()
            + "
/// A line between two corners, for the planner's export.
function stepped(from: Point, to: Point) fails Unreadable -> Exported {
    var line: LineString = attempt LineString {
        points: [from, to]
    } {
        altitudes_agree: Unreadable
    }
    return Exported(geo.to_wkt(line))
}
"),
    );
    let corner = |latitude: &str, altitude: Value| {
        Value::record(
            "Point",
            vec![
                ("latitude".into(), decimal(latitude)),
                ("longitude".into(), decimal(latitude)),
                ("altitude".into(), altitude),
            ],
        )
    };
    let height = || {
        Value::Quantity(jadpo_value::Quantity {
            kind: jadpo_std::QuantityKind::Length,
            amount: 5_000_000,
        })
    };
    assert_eq!(
        failure(
            &program,
            "stepped",
            vec![corner("0", Value::None), corner("1", height())]
        )
        .name,
        "Unreadable"
    );
    assert_eq!(
        value(
            &program,
            "stepped",
            vec![corner("0", height()), corner("1", height())]
        ),
        text("LINESTRING Z (0 0 5,1 1 5)")
    );
    assert_eq!(
        failure(
            &program,
            "shape_from_export",
            vec![text("LINESTRING(0 0,1 1 5)")]
        )
        .name,
        "Unreadable"
    );
}

const LAYER: &str = "type Sent = Text {}

type Unreadable = Failure {
    kind: FailureKind.invalid_value
}

/// What a delivery patch is called.
type PatchName = Text {
    max_length: 80
}

/// How many vans serve a patch.
type VanCount = Int {
    min: 0
    max: 50
}

/// What the map shows beside a patch.
type PatchProperties = Object {
    bakery_id: Uuid
    name: PatchName
    vans: VanCount
    note: PatchName?
}

/// The map's delivery layer, as a mapping library reads it.
function patch_layer(outlines: List<Polygon>, names: List<PatchName>) -> FeatureCollection<Polygon, PatchProperties> {
    return FeatureCollection {
        features: list.map(outlines, outline -> Feature {
            geometry: outline
            properties: PatchProperties {
                bakery_id: \"7b0c1f8e-3a52-4d7e-9a41-2f1d5c6e8b90\"
                name: PatchName(\"Soho\")
                vans: 3
                note: none
            }
        })
    }
}

/// The layer as the map is sent it.
function sent(layer: FeatureCollection<Polygon, PatchProperties>) -> Sent {
    return Sent(json.write(layer))
}

/// A layer the planner drew, as its map library sends it.
function drawn(given: Sent) fails Unreadable -> FeatureCollection<Polygon, PatchProperties> {
    return attempt json.read(given, as: FeatureCollection<Polygon, PatchProperties>) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

/// Each patch's name, as the map labels it.
function labels(layer: FeatureCollection<Polygon, PatchProperties>) -> List<PatchName> {
    return list.map(layer.features, feature -> feature.properties.name)
}

/// Each patch's outline.
function outlines(layer: FeatureCollection<Polygon, PatchProperties>) -> List<Polygon> {
    return list.map(layer.features, feature -> feature.geometry)
}
";

const DRAWN: &str = r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[-0.2,51.49],[-0.05,51.49],[-0.05,51.54],[-0.2,51.54],[-0.2,51.49]]]},"properties":{"bakery_id":"7b0c1f8e-3a52-4d7e-9a41-2f1d5c6e8b90","name":"Soho","vans":3,"note":null}},{"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]},"properties":{"bakery_id":"0f8e2d4c-6b1a-4e3f-8c5d-9a7b1e2f3c4d","name":"Null Island","vans":0,"note":"Boat only"}}]}"#;

#[test]
fn a_feature_collection_round_trips_through_json_unchanged() {
    let program = main(LAYER).with_proof_checks();
    // Read, typed: each feature's properties by their own types, `null`
    // as `none`, and written back byte for byte.
    let layer = value(&program, "drawn", vec![text(DRAWN)]);
    assert_eq!(value(&program, "sent", vec![layer.clone()]), text(DRAWN));
    assert_eq!(
        value(&program, "labels", vec![layer.clone()]),
        support::list(vec![text("Soho"), text("Null Island")])
    );
    let again = value(
        &program,
        "drawn",
        vec![value(&program, "sent", vec![layer.clone()])],
    );
    assert_eq!(again, layer);
    // Built in code, sent, and read back as the same value.
    let outlines = value(&program, "outlines", vec![layer]);
    let built = value(
        &program,
        "patch_layer",
        vec![outlines, support::list(vec![text("Soho")])],
    );
    let back = value(
        &program,
        "drawn",
        vec![value(&program, "sent", vec![built.clone()])],
    );
    assert_eq!(back, built);
}

#[test]
fn what_isn_t_the_layer_is_refused() {
    let program = main(LAYER).with_proof_checks();
    let feature = |members: &str| {
        format!(r#"{{"type":"FeatureCollection","features":[{{"type":"Feature",{members}}}]}}"#)
    };
    let point = r#""geometry":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}"#;
    let properties = r#""properties":{"bakery_id":"7b0c1f8e-3a52-4d7e-9a41-2f1d5c6e8b90","name":"Soho","vans":3,"note":null}"#;
    for given in [
        // An `id`, which properties carry.
        feature(&format!(r#""id":1,{point},{properties}"#)),
        // No geometry, or one of another kind.
        feature(&format!(r#""geometry":null,{properties}"#)),
        feature(&format!(
            r#""geometry":{{"type":"Point","coordinates":[0,0]}},{properties}"#
        )),
        // Properties missing, `null`, untyped or past their rules.
        feature(point),
        feature(&format!(r#"{point},"properties":null"#)),
        feature(&format!(r#"{point},"properties":{{"name":"Soho"}}"#)),
        feature(&format!(
            r#"{point},"properties":{{"bakery_id":"7b0c1f8e-3a52-4d7e-9a41-2f1d5c6e8b90","name":"Soho","vans":51,"note":null}}"#
        )),
        // A foreign member, and another GeoJSON object.
        feature(&format!(r#"{point},{properties},"title":"A patch""#)),
        r#"{"type":"Feature","geometry":null,"properties":{}}"#.to_string(),
        r#"{"type":"FeatureCollection"}"#.to_string(),
        r#"{"features":[]}"#.to_string(),
    ] {
        assert_eq!(
            failure(&program, "drawn", vec![text(&given)]).name,
            "Unreadable",
            "{given}"
        );
    }
    // A box, which any GeoJSON object may carry, is checked and ignored.
    let boxed = r#"{"type":"FeatureCollection","bbox":[0,0,1,1],"features":[]}"#;
    let layer = value(&program, "drawn", vec![text(boxed)]);
    assert_eq!(
        value(&program, "sent", vec![layer]),
        text(r#"{"type":"FeatureCollection","features":[]}"#)
    );
}

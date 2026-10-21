//! Area 20's interop (chunk 5.5u6), run: a delivery patch read from a
//! planner's WKT export, longitude first, and written back as PostGIS's
//! `ST_AsText` writes it, each axis still the one it was named; WKT that
//! isn't a geometry fails `malformed` and a coordinate out of its range or
//! another reference system `out_of_range`, each mapped by its `attempt`.
//! PostGIS's own reading and writing of the same text is
//! `jadpo-cli`'s `geography` test.

use crate::support;

use jadpo_interp::Value;
use support::{decimal, failure, int, main, text, value};

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

const PINS: &str = "/// A pin's label.
type PinLabel = Text {
    max_length: 80
}

/// What the map shows beside a pin.
type PinProperties = Object {
    label: PinLabel
}

/// The map's pins.
action map_pins() -> FeatureCollection<Point, PinProperties> {
    return FeatureCollection {
        features: [
            Feature {
                geometry: Point {
                    latitude: 51.5072
                    longitude: -0.1276
                    altitude: none
                }
                properties: PinProperties {
                    label: PinLabel(\"Crumbs\")
                }
            }
        ]
    }
}

/// The map's layers.
routes Pins {
    prefix: /pins

    /// Every pin, as the map draws them.
    POST / {
        authentication: RouteAuthentication.none
        output: FeatureCollection<Point, PinProperties>
        run: map_pins()
    }
}

policy {
    actions: {
        map_pins: [Access.public]
    }
}
";

const PINS_TESTS: &str = "test \"the map fetches its pins as GeoJSON\" {
    var response: Response = request {
        method: HttpMethod.post
        path: \"/pins\"
    }
    assert response.status == 200
    assert response.code == none
    assert response.output == map_pins()
}
";

/// The pins as RFC 7946 GeoJSON, as a mapping library fetches them.
const PINS_SENT: &str = r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[-0.1276,51.5072]},"properties":{"label":"Crumbs"}}]}"#;

/// A route sends a layer bare (area 7, "A map layer as output", chunk
/// 5.5u7): its body is the `FeatureCollection` itself, which a mapping
/// library fetches as it is, and a route test reads it back as the layer
/// the action returned.
#[test]
fn a_route_s_layer_is_fetched_as_plain_geojson() {
    let program = support::program(&[("main.jadpo", PINS), ("main.test.jadpo", PINS_TESTS)]);
    let host = jadpo_runtime::MemoryHost::new();
    let mut ctx = jadpo_runtime::Ctx::of(&program, &host, jadpo_runtime::Env::default());
    let request = jadpo_runtime::http::Request {
        method: "POST".to_string(),
        path: "/pins".to_string(),
        headers: vec![
            ("content-type".to_string(), "application/json".to_string()),
            (
                "idempotency-key".to_string(),
                "6f1d2c3b-4a59-4e7f-8d10-2b3c4d5e6f70".to_string(),
            ),
        ],
        ..Default::default()
    };
    let response = jadpo_runtime::block_on(jadpo_runtime::http::handle(
        &program.app(),
        &program,
        &mut ctx,
        &request,
        "req_1",
    ))
    .expect("nothing waits");
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    assert_eq!(String::from_utf8_lossy(&response.body), PINS_SENT);
    assert_eq!(
        support::run(
            &program,
            "test main \"the map fetches its pins as GeoJSON\"",
            vec![]
        ),
        jadpo_interp::Outcome::Value(Value::Unit)
    );
}

/// The constructions at an axis order's named edges (area 19, "Shared
/// vocabulary types"; chunk 5.11): GeoJSON text, longitude first, and a
/// pair of numbers in the order the operation's name says.
const PAIRS: &str = "type Exported = Text {}

type Reading = Decimal {
    scale: 15
}

type WholeReading = Int {}

type Unreadable = Failure {
    kind: FailureKind.invalid_value
}

type OutOfReach = Failure {
    kind: FailureKind.invalid_value
}

/// A shape a mapping tool sent, as GeoJSON.
function shape_from_geojson(cell: Exported) fails Unreadable, OutOfReach -> Geometry {
    return attempt geo.from_geojson(cell) {
        malformed: Unreadable
        out_of_range: OutOfReach
    }
}

/// A shape as GeoJSON again.
function geojson_of(shape: Geometry) -> Exported {
    return Exported(json.write(shape))
}

/// A shape as WKT.
function wkt_of(shape: Geometry) -> Exported {
    return Exported(geo.to_wkt(shape))
}

/// A position a tracker sends longitude first.
function spot_lng_lat(pair: List<Reading>) fails Unreadable, OutOfReach -> Point {
    return attempt geo.from_lng_lat(pair) {
        malformed: Unreadable
        out_of_range: OutOfReach
    }
}

/// A position a tracker sends latitude first.
function spot_lat_lng(pair: List<Reading>) fails Unreadable, OutOfReach -> Point {
    return attempt geo.from_lat_lng(pair) {
        malformed: Unreadable
        out_of_range: OutOfReach
    }
}

/// A position in whole degrees, latitude first.
function spot_whole(pair: List<WholeReading>) fails Unreadable, OutOfReach -> Point {
    return attempt geo.from_lat_lng(pair) {
        malformed: Unreadable
        out_of_range: OutOfReach
    }
}

/// A position as WKT.
function spot_wkt(spot: Point) -> Exported {
    return Exported(geo.to_wkt(spot))
}

/// A position as GeoJSON.
function spot_geojson(spot: Point) -> Exported {
    return Exported(json.write(spot))
}
";

fn numbers(items: &[&str]) -> Value {
    support::list(items.iter().map(|item| decimal(item)).collect())
}

fn spot(latitude: &str, longitude: &str, altitude: Value) -> Value {
    jadpo_value::geo::point(
        jadpo_value::Decimal::parse(latitude).expect("a latitude"),
        jadpo_value::Decimal::parse(longitude).expect("a longitude"),
        altitude,
    )
}

/// The shape a `Geometry` holds.
fn held(shape: &Value) -> Value {
    let Value::Variant(variant) = shape else {
        panic!("a geometry: {shape:?}");
    };
    variant.fields.first().expect("its shape").1.clone()
}

#[test]
fn geojson_text_round_trips_as_its_geometry_with_its_axes_unswapped() {
    let program = main(PAIRS).with_proof_checks();
    for geojson in [
        r#"{"type":"Point","coordinates":[-0.1276,51.5072]}"#,
        r#"{"type":"LineString","coordinates":[[0,0],[1,1]]}"#,
        r#"{"type":"Polygon","coordinates":[[[-0.2,51.49],[-0.05,51.49],[-0.05,51.54],[-0.2,51.54],[-0.2,51.49]],[[-0.15,51.5],[-0.15,51.52],[-0.1,51.52],[-0.1,51.5],[-0.15,51.5]]]}"#,
        r#"{"type":"MultiPoint","coordinates":[[1,2],[3,4]]}"#,
        r#"{"type":"MultiLineString","coordinates":[[[0,0],[1,1]],[[2,2],[3,3]]]}"#,
        r#"{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,0]]],[[[2,2],[3,2],[3,3],[2,2]]]]}"#,
    ] {
        let shape = value(&program, "shape_from_geojson", vec![text(geojson)]);
        assert_eq!(
            value(&program, "geojson_of", vec![shape]),
            text(geojson),
            "{geojson}"
        );
    }
    // Longitude first, so the depot is at 51.5072° N.
    let depot = value(
        &program,
        "shape_from_geojson",
        vec![text(
            r#"{"type":"Point","coordinates":[-0.1276,51.5072,35]}"#,
        )],
    );
    assert_eq!(
        value(&program, "wkt_of", vec![depot.clone()]),
        text("POINT Z (-0.1276 51.5072 35)")
    );
    assert_eq!(
        held(&depot),
        value(
            &program,
            "spot_lat_lng",
            vec![numbers(&["51.5072", "-0.1276", "35"])]
        )
    );
    // A patch read as GeoJSON is the patch read as WKT.
    let patch = value(
        &program,
        "shape_from_geojson",
        vec![text(
            r#"{"type":"Polygon","coordinates":[[[-0.2,51.49],[-0.05,51.49],[-0.05,51.54],[-0.2,51.54],[-0.2,51.49]]]}"#,
        )],
    );
    assert_eq!(
        value(&program, "wkt_of", vec![patch]),
        text("POLYGON((-0.2 51.49,-0.05 51.49,-0.05 51.54,-0.2 51.54,-0.2 51.49))")
    );
    // A ring wound the other way is rewound, as RFC 7946 asks of a reader.
    let clockwise = value(
        &program,
        "shape_from_geojson",
        vec![text(
            r#"{"type":"Polygon","coordinates":[[[0,0],[0,1],[1,1],[1,0],[0,0]]]}"#,
        )],
    );
    assert_eq!(
        value(&program, "geojson_of", vec![clockwise]),
        text(r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}"#)
    );
}

#[test]
fn geojson_text_that_isn_t_a_geometry_fails_with_its_condition() {
    let program = main(PAIRS).with_proof_checks();
    for (cell, failed) in [
        // Not one JSON document, or not one of the six geometries.
        ("", "Unreadable"),
        ("POINT(-0.1276 51.5072)", "Unreadable"),
        (r#"{"type":"Point","coordinates":[0,0]} {}"#, "Unreadable"),
        ("[-0.1276,51.5072]", "Unreadable"),
        (r#"{"type":"Point"}"#, "Unreadable"),
        (r#"{"type":"Circle","coordinates":[0,0]}"#, "Unreadable"),
        (
            r#"{"type":"GeometryCollection","geometries":[{"type":"Point","coordinates":[0,0]}]}"#,
            "Unreadable",
        ),
        (
            r#"{"type":"Feature","geometry":{"type":"Point","coordinates":[0,0]},"properties":{}}"#,
            "Unreadable",
        ),
        // A member RFC 7946 doesn't have, a reference system included.
        (
            r#"{"type":"Point","coordinates":[0,0],"crs":{"type":"name"}}"#,
            "Unreadable",
        ),
        // Positions that aren't two or three numbers, of one size.
        (r#"{"type":"Point","coordinates":[0]}"#, "Unreadable"),
        (r#"{"type":"Point","coordinates":["0","0"]}"#, "Unreadable"),
        (r#"{"type":"Point","coordinates":[0,0,0,0]}"#, "Unreadable"),
        (
            r#"{"type":"LineString","coordinates":[[0,0]]}"#,
            "Unreadable",
        ),
        (
            r#"{"type":"LineString","coordinates":[[0,0],[1,1,5]]}"#,
            "Unreadable",
        ),
        // Rings not closed, or not valid.
        (
            r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1]]]}"#,
            "Unreadable",
        ),
        (
            r#"{"type":"Polygon","coordinates":[[[0,0],[10,10],[0,10],[10,0],[0,0]]]}"#,
            "Unreadable",
        ),
        // A coordinate past its range.
        (r#"{"type":"Point","coordinates":[0,91]}"#, "OutOfReach"),
        (r#"{"type":"Point","coordinates":[0,-90.5]}"#, "OutOfReach"),
        (r#"{"type":"Point","coordinates":[181,0]}"#, "OutOfReach"),
        (
            r#"{"type":"LineString","coordinates":[[0,0],[-180.25,1]]}"#,
            "OutOfReach",
        ),
    ] {
        assert_eq!(
            failure(&program, "shape_from_geojson", vec![text(cell)]).name,
            failed,
            "{cell}"
        );
    }
    // The poles and the antimeridian are in range.
    for cell in [
        r#"{"type":"Point","coordinates":[180,90]}"#,
        r#"{"type":"Point","coordinates":[-180,-90]}"#,
    ] {
        value(&program, "shape_from_geojson", vec![text(cell)]);
    }
}

#[test]
fn a_pair_s_order_is_the_one_its_operation_names() {
    let program = main(PAIRS).with_proof_checks();
    let depot = spot("51.5072", "-0.1276", Value::None);
    assert_eq!(
        value(
            &program,
            "spot_lng_lat",
            vec![numbers(&["-0.1276", "51.5072"])]
        ),
        depot
    );
    assert_eq!(
        value(
            &program,
            "spot_lat_lng",
            vec![numbers(&["51.5072", "-0.1276"])]
        ),
        depot
    );
    // Out to WKT and GeoJSON, longitude first, and back.
    assert_eq!(
        value(&program, "spot_wkt", vec![depot.clone()]),
        text("POINT(-0.1276 51.5072)")
    );
    let sent = value(&program, "spot_geojson", vec![depot.clone()]);
    assert_eq!(
        sent,
        text(r#"{"type":"Point","coordinates":[-0.1276,51.5072]}"#)
    );
    assert_eq!(
        held(&value(&program, "shape_from_geojson", vec![sent])),
        depot
    );
    // The same two numbers are another place the other way round.
    assert_eq!(
        value(
            &program,
            "spot_lng_lat",
            vec![numbers(&["51.5072", "-0.1276"])]
        ),
        spot("-0.1276", "51.5072", Value::None)
    );
    // A third number is the altitude, in metres.
    let high = value(
        &program,
        "spot_lng_lat",
        vec![numbers(&["-0.1276", "51.5072", "35"])],
    );
    assert_eq!(
        value(&program, "spot_wkt", vec![high.clone()]),
        text("POINT Z (-0.1276 51.5072 35)")
    );
    assert_eq!(
        value(
            &program,
            "spot_lat_lng",
            vec![numbers(&["51.5072", "-0.1276", "35"])]
        ),
        high
    );
    // Whole degrees, and the edges of each range.
    assert_eq!(
        value(
            &program,
            "spot_whole",
            vec![support::list(vec![int(51), int(0)])]
        ),
        spot("51", "0", Value::None)
    );
    assert_eq!(
        value(&program, "spot_lat_lng", vec![numbers(&["90", "180"])]),
        spot("90", "180", Value::None)
    );
    assert_eq!(
        value(&program, "spot_lng_lat", vec![numbers(&["-180", "-90"])]),
        spot("-90", "-180", Value::None)
    );
    // Each coordinate held as the shortest decimal of the nearest double,
    // as a `Point` built in code is (area 20, question 2).
    assert_eq!(
        value(
            &program,
            "spot_lat_lng",
            vec![numbers(&["51.50720000001234567", "-0.1276"])]
        ),
        spot("51.50720000001235", "-0.1276", Value::None)
    );
}

#[test]
fn a_pair_that_isn_t_a_position_fails_with_its_condition() {
    let program = main(PAIRS).with_proof_checks();
    for (name, pair, failed) in [
        // Two numbers or three.
        ("spot_lng_lat", &[][..], "Unreadable"),
        ("spot_lat_lng", &["51.5072"][..], "Unreadable"),
        (
            "spot_lat_lng",
            &["51.5072", "-0.1276", "35", "1"][..],
            "Unreadable",
        ),
        // 91 is a longitude, never a latitude.
        ("spot_lat_lng", &["91", "0"][..], "OutOfReach"),
        ("spot_lat_lng", &["-90.5", "0"][..], "OutOfReach"),
        ("spot_lat_lng", &["0", "180.5"][..], "OutOfReach"),
        ("spot_lng_lat", &["0", "91"][..], "OutOfReach"),
        ("spot_lng_lat", &["-181", "0"][..], "OutOfReach"),
    ] {
        assert_eq!(
            failure(&program, name, vec![numbers(pair)]).name,
            failed,
            "{name} {pair:?}"
        );
    }
    assert_eq!(
        value(&program, "spot_lng_lat", vec![numbers(&["91", "0"])]),
        spot("0", "91", Value::None)
    );
}

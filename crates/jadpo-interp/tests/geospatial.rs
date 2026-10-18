//! Area 19, the geospatial types (chunk 5.5s), run: a point built by its
//! names and written as GeoJSON with its longitude first, read back with
//! neither axis swapped; a polygon wound the other way read and written by
//! the right-hand rule; each geometry read as `Geometry`; coordinates and
//! bearings shown with their hemispheres and degrees; and what isn't a
//! geometry refused, an axis out of range or a ring left open included.

use crate::support;

use jadpo_interp::Value;
use support::{decimal, failure, main, text, value};

const GEO: &str = "type Sent = Text {}

type Unreadable = Failure {
    kind: FailureKind.invalid_value
}

/// The depot every round starts from.
function depot() -> Point {
    return Point {
        latitude: 51.5072
        longitude: -0.1276
        altitude: none
    }
}

/// A position as the tracker's feed sends it.
function sent(at: Point) -> Sent {
    return Sent(json.write(at))
}

/// A van's position from the tracker's feed.
function tracked(feed: Sent) fails Unreadable -> Point {
    return attempt json.read(feed, as: Point) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

/// How far north a position is.
function north_of_equator(at: Point) -> Latitude {
    return at.latitude
}

/// How far east a position is.
function east_of_greenwich(at: Point) -> Longitude {
    return at.longitude
}

/// A delivery zone from the planner's map.
function zone(drawn: Sent) fails Unreadable -> Polygon {
    return attempt json.read(drawn, as: Polygon) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

/// A zone as the map is sent it.
function zone_sent(area: Polygon) -> Sent {
    return Sent(json.write(area))
}

/// Any shape the planner draws.
function drawn(shape: Sent) fails Unreadable -> Geometry {
    return attempt json.read(shape, as: Geometry) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

/// A shape as the map is sent it.
function drawn_sent(shape: Geometry) -> Sent {
    return Sent(json.write(shape))
}

/// A position for the driver.
function shown(at: Point, reader: Locale) -> PresentationText {
    return text.format(at, locale: reader)
}

/// A latitude for the driver.
function latitude_shown(at: Point, reader: Locale) -> PresentationText {
    return text.format(at.latitude, locale: reader)
}

/// A heading for the driver.
function heading(towards: Bearing, reader: Locale) -> PresentationText {
    return text.format(towards, locale: reader)
}

/// The area a search covers.
function search_area() -> BoundingBox {
    return BoundingBox {
        south: 51.28
        west: -0.51
        north: 51.69
        east: 0.33
    }
}

/// An area for the driver.
function area_shown(area: BoundingBox, reader: Locale) -> PresentationText {
    return text.format(area, locale: reader)
}

/// A latitude typed into the planner.
function typed_latitude(raw: Sent) fails Unreadable -> Latitude {
    return attempt text.parse(raw, as: Latitude) {
        malformed: Unreadable
        min: Unreadable
        max: Unreadable
    }
}
";

fn locale(name: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::LOCALE, name).expect("a locale");
    Value::variant(jadpo_std::LOCALE, ordinal, name, Vec::new())
}

const LONDON: &str = r#"{"type":"Point","coordinates":[-0.1276,51.5072]}"#;

#[test]
fn a_point_round_trips_through_geojson_with_its_axes_unswapped() {
    let program = main(GEO).with_proof_checks();
    let depot = value(&program, "depot", vec![]);
    // Written longitude first, as RFC 7946 orders a position.
    assert_eq!(value(&program, "sent", vec![depot.clone()]), text(LONDON));
    // Read back, each axis is still the one it was named.
    let back = value(&program, "tracked", vec![text(LONDON)]);
    assert_eq!(back, depot);
    assert_eq!(
        value(&program, "north_of_equator", vec![back.clone()]),
        decimal("51.5072")
    );
    assert_eq!(
        value(&program, "east_of_greenwich", vec![back]),
        decimal("-0.1276")
    );
    // Sydney, south and east, so a swap would show.
    let sydney = value(
        &program,
        "tracked",
        vec![text(
            r#"{"type":"Point","coordinates":[151.2093,-33.8688]}"#,
        )],
    );
    assert_eq!(
        value(&program, "north_of_equator", vec![sydney.clone()]),
        decimal("-33.8688")
    );
    assert_eq!(
        value(&program, "east_of_greenwich", vec![sydney]),
        decimal("151.2093")
    );
}

#[test]
fn what_isn_t_a_point_is_refused() {
    let program = main(GEO).with_proof_checks();
    for feed in [
        r#"{"type":"Point","coordinates":[0,91]}"#,
        r#"{"type":"Point","coordinates":[-181,0]}"#,
        r#"{"type":"Point","coordinates":[0]}"#,
        r#"{"type":"Point","coordinates":[0,0],"crs":{"type":"name"}}"#,
        r#"{"latitude":"51.5072","longitude":"-0.1276"}"#,
        r#"[-0.1276,51.5072]"#,
        "not JSON",
    ] {
        assert_eq!(
            failure(&program, "tracked", vec![text(feed)]).name,
            "Unreadable",
            "{feed}"
        );
    }
}

#[test]
fn a_zone_is_read_in_either_winding_and_sent_by_the_right_hand_rule() {
    let program = main(GEO).with_proof_checks();
    let counterclockwise = r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}"#;
    let clockwise = r#"{"type":"Polygon","coordinates":[[[0,0],[0,1],[1,1],[1,0],[0,0]]]}"#;
    for drawn in [counterclockwise, clockwise] {
        let area = value(&program, "zone", vec![text(drawn)]);
        assert_eq!(
            value(&program, "zone_sent", vec![area]),
            text(counterclockwise),
            "{drawn}"
        );
    }
    // A hole wound counterclockwise is sent clockwise.
    let holed = r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]],[[0.25,0.25],[0.75,0.25],[0.75,0.75],[0.25,0.75],[0.25,0.25]]]}"#;
    let area = value(&program, "zone", vec![text(holed)]);
    assert_eq!(
        value(&program, "zone_sent", vec![area]),
        text(
            r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]],[[0.25,0.25],[0.25,0.75],[0.75,0.75],[0.75,0.25],[0.25,0.25]]]}"#
        )
    );
    for open in [
        r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1]]]}"#,
        r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[0,0]]]}"#,
        r#"{"type":"Polygon","coordinates":[]}"#,
    ] {
        assert_eq!(
            failure(&program, "zone", vec![text(open)]).name,
            "Unreadable",
            "{open}"
        );
    }
}

#[test]
fn any_geometry_is_read_by_its_geojson_type() {
    let program = main(GEO).with_proof_checks();
    for shape in [
        LONDON,
        r#"{"type":"LineString","coordinates":[[0,0],[1,1]]}"#,
        r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}"#,
        r#"{"type":"MultiPoint","coordinates":[[0,0],[1,1]]}"#,
        r#"{"type":"MultiLineString","coordinates":[[[0,0],[1,1]]]}"#,
        r#"{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,1],[0,0]]]]}"#,
    ] {
        let read = value(&program, "drawn", vec![text(shape)]);
        assert_eq!(value(&program, "drawn_sent", vec![read]), text(shape));
    }
    let Value::Variant(point) = value(&program, "drawn", vec![text(LONDON)]) else {
        panic!("a geometry");
    };
    assert_eq!(&*point.name, "point");
    for shape in [
        r#"{"type":"GeometryCollection","geometries":[]}"#,
        r#"{"type":"Feature","geometry":null,"properties":null}"#,
        r#"{"tag":"point","point":{"type":"Point","coordinates":[0,0]}}"#,
    ] {
        assert_eq!(
            failure(&program, "drawn", vec![text(shape)]).name,
            "Unreadable",
            "{shape}"
        );
    }
}

#[test]
fn positions_are_shown_with_their_hemispheres() {
    let program = main(GEO).with_proof_checks();
    let depot = value(&program, "depot", vec![]);
    assert_eq!(
        value(&program, "shown", vec![depot.clone(), locale("en_gb")]),
        text("51.5072° N, 0.1276° W")
    );
    assert_eq!(
        value(&program, "latitude_shown", vec![depot, locale("en_gb")]),
        text("51.5072° N")
    );
    assert_eq!(
        value(&program, "heading", vec![decimal("247.5"), locale("en_gb")]),
        text("247.5°")
    );
    let area = value(&program, "search_area", vec![]);
    assert_eq!(
        value(&program, "area_shown", vec![area, locale("en_gb")]),
        text("51.28° N, 0.51° W – 51.69° N, 0.33° E")
    );
}

#[test]
fn a_typed_latitude_keeps_its_range() {
    let program = main(GEO).with_proof_checks();
    assert_eq!(
        value(&program, "typed_latitude", vec![text("-33.8688")]),
        decimal("-33.8688")
    );
    for raw in ["91", "-90.5", "51.5 N", ""] {
        assert_eq!(
            failure(&program, "typed_latitude", vec![text(raw)]).name,
            "Unreadable",
            "{raw}"
        );
    }
}

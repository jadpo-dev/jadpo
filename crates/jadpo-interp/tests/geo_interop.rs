//! Area 20's interop (chunk 5.5u6), run: a delivery patch read from a
//! planner's WKT export, longitude first, and written back as PostGIS's
//! `ST_AsText` writes it, each axis still the one it was named; WKT that
//! isn't a geometry fails `malformed` and a coordinate out of its range or
//! another reference system `out_of_range`, each mapped by its `attempt`.
//! PostGIS's own reading and writing of the same text is
//! `jadpo-cli`'s `geography` test.

use crate::support;

use jadpo_interp::Value;
use support::{decimal, failure, fault, main, text, value};

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

#[test]
fn a_shape_mixing_altitudes_has_no_wkt() {
    let program = main(
        &(WKT.to_string()
            + "
/// A line with one corner at a height.
function stepped() -> Exported {
    return Exported(geo.to_wkt(LineString {
        points: [
            Point {
                latitude: 0.0
                longitude: 0.0
                altitude: none
            },
            Point {
                latitude: 1.0
                longitude: 1.0
                altitude: 5metre
            }
        ]
    }))
}
"),
    );
    let found = fault(&program, "stepped", vec![]);
    assert!(
        format!("{:?}", found.kind).contains("mix with and without an altitude"),
        "{found:?}"
    );
}

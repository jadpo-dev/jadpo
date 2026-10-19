//! Area 20's predicates and shapes (chunk 5.5u3) check: `geo.intersects`,
//! `contains`, `within`, `centroid` and `bounding_box` take any geometry,
//! `contains` a `BoundingBox` or a `Circle` too; `within(distance:)` and a
//! `Circle` measure between points only (question 10); a distance is never
//! negative. A polygon literal breaking `rings_valid` is refused while
//! compiling, in each way the page lists, and one built from values maps
//! the rule. A geometry literal mixing positions with and without an
//! altitude breaks `altitudes_agree` while compiling, and one built from
//! values maps it (chunk 5.5u7).

use crate::support;

use jadpo_db::{Database, Layout};

const TYPES: &str = "type Inside = Bool {}\n\ntype Reach = Length {\n    min: 0metre\n}\n\ntype AnyReach = Length {}\n\ntype Name = Text {}\n\ntype NotAPatch = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

/// Each problem as its rule, and its message and reason when `reasons`
/// asks.
fn check(source: &str, reasons: bool) -> Vec<String> {
    let db = Database::default();
    let text = format!("{TYPES}{source}");
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &text)]);
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| match (&d.reason, reasons) {
            (Some(reason), true) => format!("{}: {} {reason}", d.rule.id, d.message),
            _ => d.rule.id.to_string(),
        })
        .collect()
}

const NONE: &[&str] = &[];

#[test]
fn every_predicate_and_shape_checks() {
    let source = "function delivers(patch: Polygon, customer: Point) -> Inside {\n    return Inside(geo.contains(patch, customer))\n}\n\nfunction served(customer: Point, patches: MultiPolygon) -> Inside {\n    return Inside(geo.within(customer, patches))\n}\n\nfunction crosses(round: LineString, patch: Geometry) -> Inside {\n    return Inside(geo.intersects(round, patch))\n}\n\nfunction in_view(pin: Point, view: BoundingBox) -> Inside {\n    return Inside(geo.within(pin, view))\n}\n\nfunction round_in_view(view: BoundingBox, round: LineString) -> Inside {\n    return Inside(geo.contains(view, round))\n}\n\nfunction in_reach(pin: Point, around: Circle) -> Inside {\n    return Inside(geo.within(pin, around))\n}\n\nfunction walkable(a: Point, b: Point, reach: Reach) -> Inside {\n    return Inside(geo.within(a, b, distance: reach))\n}\n\nfunction nearby(a: Point, b: Point) -> Inside {\n    return Inside(geo.within(a, b, distance: 5km))\n}\n\nfunction label(patch: Polygon) -> Point {\n    return geo.centroid(patch)\n}\n\nfunction frame(shape: Geometry) -> BoundingBox {\n    return geo.bounding_box(shape)\n}\n";
    assert_eq!(check(source, false), NONE);
}

#[test]
fn the_predicates_take_geometries() {
    let problems = check(
        "function named(a: Name, b: Point) -> Inside {\n    return Inside(geo.contains(a, b))\n}\n\nfunction boxed(a: Point, b: BoundingBox) -> Inside {\n    return Inside(geo.intersects(a, b))\n}\n\nfunction framed(view: BoundingBox) -> Point {\n    return geo.centroid(view)\n}\n",
        true,
    );
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(
        problems[0].starts_with(
            "type.mismatch: `Name` can't be used where a geometry, a `BoundingBox` or a `Circle` is required"
        ),
        "{problems:?}"
    );
    assert!(
        problems[1]
            .starts_with("type.mismatch: `BoundingBox` can't be used where a geometry is required"),
        "{problems:?}"
    );
    assert!(problems[2].starts_with("type.mismatch"), "{problems:?}");
}

#[test]
fn a_distance_and_a_circle_measure_between_points_only() {
    let problems = check(
        "function near_patch(a: Point, patch: Polygon) -> Inside {\n    return Inside(geo.within(a, patch, distance: 5km))\n}\n\nfunction round_in_reach(around: Circle, round: LineString) -> Inside {\n    return Inside(geo.contains(around, round))\n}\n\nfunction patch_in_reach(patch: Polygon, around: Circle) -> Inside {\n    return Inside(geo.within(patch, around))\n}\n\nfunction box_near(a: Point, view: BoundingBox) -> Inside {\n    return Inside(geo.within(a, view, distance: 5km))\n}\n",
        true,
    );
    assert_eq!(problems.len(), 4, "{problems:?}");
    for problem in &problems {
        assert!(
            problem.starts_with("type.mismatch")
                && problem.contains("can't be used where a `Point` is required")
                && problem.contains("area 20, question 10"),
            "{problem}"
        );
    }
}

#[test]
fn a_distance_is_never_negative() {
    let problems = check(
        "function backwards(a: Point, b: Point) -> Inside {\n    return Inside(geo.within(a, b, distance: -5km))\n}\n\nfunction unbounded(a: Point, b: Point, reach: AnyReach) -> Inside {\n    return Inside(geo.within(a, b, distance: reach))\n}\n",
        false,
    );
    assert_eq!(
        problems,
        ["type.argument_negative", "type.argument_negative"]
    );
}

/// A coordinate as a decimal literal.
fn coordinate(text: &str) -> String {
    if text.contains('.') {
        text.to_string()
    } else {
        format!("{text}.0")
    }
}

/// A ring written out, from `latitude longitude` pairs, as given (its
/// closing point included or not).
fn ring(corners: &[&str]) -> String {
    let points: Vec<String> = corners
        .iter()
        .map(|corner| {
            let (latitude, longitude) = corner.split_once(' ').expect("a pair");
            format!(
                "Point {{\n                latitude: {}\n                longitude: {}\n                altitude: none\n            }}",
                coordinate(latitude),
                coordinate(longitude)
            )
        })
        .collect();
    format!(
        "[\n            {}\n        ]",
        points.join(",\n            ")
    )
}

/// A function returning a polygon literal of these rings.
fn literal(outer: &[&str], holes: &[&[&str]]) -> String {
    let holes: Vec<String> = holes.iter().map(|hole| ring(hole)).collect();
    format!(
        "function patch() -> Polygon {{\n    return Polygon {{\n        outer: {}\n        holes: [{}]\n    }}\n}}\n",
        ring(outer),
        holes.join(", ")
    )
}

/// Closed: the first corner again at the end.
fn closed(corners: &[&'static str]) -> Vec<&'static str> {
    let mut out = corners.to_vec();
    out.push(corners[0]);
    out
}

const SQUARE: &[&str] = &["0 0", "0 10", "10 10", "10 0"];

/// A way to break `rings_valid`: what it is, the outer ring and the holes.
type Case = (&'static str, Vec<&'static str>, Vec<Vec<&'static str>>);

#[test]
fn a_valid_polygon_literal_checks() {
    assert_eq!(check(&literal(&closed(SQUARE), &[]), false), NONE);
    let hole = closed(&["2 2", "4 2", "4 4", "2 4"]);
    assert_eq!(check(&literal(&closed(SQUARE), &[&hole]), false), NONE);
    // Across the antimeridian and round a pole.
    let across = closed(&["-10 170", "-10 -170", "10 -170", "10 170"]);
    assert_eq!(check(&literal(&across, &[]), false), NONE);
    let cap = closed(&["80 0", "80 90", "80 180", "80 -90"]);
    assert_eq!(check(&literal(&cap, &[]), false), NONE);
}

#[test]
fn every_way_a_literal_breaks_rings_valid_is_refused_while_compiling() {
    let cases: Vec<Case> = vec![
        ("not closed", SQUARE.to_vec(), vec![]),
        ("three points", vec!["0 0", "0 1", "0 0"], vec![]),
        (
            "crossing itself",
            closed(&["0 0", "10 10", "10 0", "0 10"]),
            vec![],
        ),
        (
            "doubling back",
            closed(&["0 0", "0 10", "0 5", "10 5"]),
            vec![],
        ),
        (
            "touching itself",
            closed(&["0 0", "0 10", "5 5", "10 10", "10 0", "5 5"]),
            vec![],
        ),
        (
            "a point repeated",
            closed(&["0 0", "0 10", "0 10", "10 10"]),
            vec![],
        ),
        (
            "an edge of half the globe",
            closed(&["10 0", "-10 180", "0 90"]),
            vec![],
        ),
        (
            "half the globe",
            closed(&["0 0", "0 90", "0 180", "0 -90"]),
            vec![],
        ),
        (
            "a hole outside",
            closed(SQUARE),
            vec![closed(&["20 20", "21 20", "21 21"])],
        ),
        (
            "a hole crossing",
            closed(SQUARE),
            vec![closed(&["5 5", "5 15", "6 15"])],
        ),
        (
            "a hole touching",
            closed(SQUARE),
            vec![closed(&["0 0", "1 2", "2 1"])],
        ),
        (
            "a hole in a hole",
            closed(SQUARE),
            vec![
                closed(&["1 1", "1 9", "9 9", "9 1"]),
                closed(&["2 2", "3 2", "3 3"]),
            ],
        ),
    ];
    for (why, outer, holes) in cases {
        let holes: Vec<&[&str]> = holes.iter().map(Vec::as_slice).collect();
        let problems = check(&literal(&outer, &holes), true);
        assert_eq!(problems.len(), 1, "{why}: {problems:?}");
        assert!(
            problems[0]
                .starts_with("type.invalid_literal: This `Polygon` breaks its rule `rings_valid`"),
            "{why}: {problems:?}"
        );
    }
}

#[test]
fn a_polygon_built_from_values_maps_rings_valid() {
    let unhandled = check(
        "function triangle(a: Point, b: Point, c: Point) -> Polygon {\n    return Polygon {\n        outer: [a, b, c, a]\n        holes: []\n    }\n}\n",
        true,
    );
    assert_eq!(unhandled.len(), 1, "{unhandled:?}");
    assert!(
        unhandled[0].starts_with("failure.attempt_required")
            && unhandled[0].contains("rings_valid"),
        "{unhandled:?}"
    );
    assert_eq!(
        check(
            "function triangle(a: Point, b: Point, c: Point) fails NotAPatch -> Polygon {\n    return attempt Polygon {\n        outer: [a, b, c, a]\n        holes: []\n    } {\n        rings_valid: NotAPatch\n        altitudes_agree: NotAPatch\n    }\n}\n",
            false
        ),
        NONE
    );
}

/// A point literal at `latitude longitude`, with `altitude` written as
/// given (`none`, `35metre`, or a name).
fn at(corner: &str, altitude: &str) -> String {
    let (latitude, longitude) = corner.split_once(' ').expect("a pair");
    format!(
        "Point {{\n            latitude: {}\n            longitude: {}\n            altitude: {altitude}\n        }}",
        coordinate(latitude),
        coordinate(longitude)
    )
}

/// Every position with an altitude or none (area 20, "Altitudes are all
/// or nothing", chunk 5.5u7): a literal that mixes them is refused while
/// compiling, at the part holding the first position of the other size,
/// in each geometry with more than one position; a multi-geometry's parts
/// must agree with each other.
#[test]
fn a_literal_mixing_altitudes_breaks_altitudes_agree() {
    let ground = |corner: &str| at(corner, "none");
    let high = |corner: &str| at(corner, "35metre");
    let ring = |point: &dyn Fn(&str) -> String| {
        format!(
            "[{}]",
            ["0 0", "0 1", "1 1", "1 0", "0 0"]
                .iter()
                .map(|corner| point(corner))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let hole = |point: &dyn Fn(&str) -> String| {
        format!(
            "[{}]",
            [
                "0.25 0.25",
                "0.75 0.25",
                "0.75 0.75",
                "0.25 0.75",
                "0.25 0.25"
            ]
            .iter()
            .map(|corner| point(corner))
            .collect::<Vec<_>>()
            .join(", ")
        )
    };
    let line = |a: String, b: String| format!("LineString {{\n        points: [{a}, {b}]\n    }}");
    let polygon = |outer: String, holes: String| {
        format!("Polygon {{\n        outer: {outer}\n        holes: [{holes}]\n    }}")
    };
    let cases: Vec<(&str, &str, String)> = vec![
        ("LineString", "a line", line(ground("0 0"), high("1 1"))),
        (
            "MultiPoint",
            "points",
            format!(
                "MultiPoint {{\n        points: [{}, {}]\n    }}",
                high("0 0"),
                ground("1 1")
            ),
        ),
        ("Polygon", "a hole", polygon(ring(&ground), hole(&high))),
        (
            "MultiLineString",
            "lines",
            format!(
                "MultiLineString {{\n        lines: [{}, {}]\n    }}",
                line(ground("0 0"), ground("1 1")),
                line(high("2 2"), high("3 3"))
            ),
        ),
        (
            "MultiPolygon",
            "polygons",
            format!(
                "MultiPolygon {{\n        polygons: [{}, {}]\n    }}",
                polygon(ring(&ground), String::new()),
                polygon(ring(&high), String::new())
            ),
        ),
    ];
    for (ty, why, literal) in cases {
        let source = format!("function shape() -> {ty} {{\n    return {literal}\n}}\n");
        let problems = check(&source, true);
        assert_eq!(problems.len(), 1, "{why}: {problems:?}");
        assert!(
            problems[0].starts_with(&format!(
                "type.invalid_literal: This `{ty}` breaks its rule `altitudes_agree`"
            )) && problems[0].contains("Altitudes are all or nothing"),
            "{why}: {problems:?}"
        );
        // All on the ground, or all at a height, checks.
        let agreeing = source.replace("35metre", "none");
        assert_eq!(check(&agreeing, false), NONE, "{why}: {agreeing}");
        let high_up = source.replace("altitude: none", "altitude: 35metre");
        assert_eq!(check(&high_up, false), NONE, "{why}: {high_up}");
    }
}

/// A geometry built from values may mix them, so it is attempted and
/// `altitudes_agree` mapped, as `rings_valid` is; an altitude of a type
/// that can't be `none` (a distance) is one, so a literal with them is
/// decided.
#[test]
fn a_geometry_built_from_values_maps_altitudes_agree() {
    let unhandled = check(
        "function round(a: Point, b: Point) -> LineString {\n    return LineString {\n        points: [a, b]\n    }\n}\n",
        true,
    );
    assert_eq!(unhandled.len(), 1, "{unhandled:?}");
    assert!(
        unhandled[0].starts_with("failure.attempt_required")
            && unhandled[0].contains("altitudes_agree"),
        "{unhandled:?}"
    );
    assert_eq!(
        check(
            "function round(a: Point, b: Point) fails NotAPatch -> LineString {\n    return attempt LineString {\n        points: [a, b]\n    } {\n        altitudes_agree: NotAPatch\n    }\n}\n\nfunction lines(a: LineString, b: LineString) fails NotAPatch -> MultiLineString {\n    return attempt MultiLineString {\n        lines: [a, b]\n    } {\n        altitudes_agree: NotAPatch\n    }\n}\n",
            false
        ),
        NONE
    );
    let decided = format!(
        "function climb(from: Point, to: Point) -> LineString {{\n    return LineString {{\n        points: [{}, {}]\n    }}\n}}\n",
        at("0 0", "geo.distance(from, to)"),
        at("1 1", "35metre")
    );
    assert_eq!(check(&decided, true), NONE);
    let mixed = decided.replace("35metre", "none");
    let refused = check(&mixed, false);
    assert_eq!(refused, ["type.invalid_literal"], "{mixed}");
}

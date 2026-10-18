//! Area 20's predicates and shapes (chunk 5.5u3) check: `geo.intersects`,
//! `contains`, `within`, `centroid` and `bounding_box` take any geometry,
//! `contains` a `BoundingBox` or a `Circle` too; `within(distance:)` and a
//! `Circle` measure between points only (question 10); a distance is never
//! negative. A polygon literal breaking `rings_valid` is refused while
//! compiling, in each way the page lists, and one built from values maps
//! the rule.

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
            "function triangle(a: Point, b: Point, c: Point) fails NotAPatch -> Polygon {\n    return attempt Polygon {\n        outer: [a, b, c, a]\n        holes: []\n    } {\n        rings_valid: NotAPatch\n    }\n}\n",
            false
        ),
        NONE
    );
}

//! Area 20's measures (chunk 5.5u2) check: `geo.distance`, `length`,
//! `bearing` and `area` take the geometries they measure and nothing else,
//! a distance or a bearing between points only (question 10), naming the
//! page; `bearing` fails with `same_point` unless its `attempt` maps it;
//! and the rest of the page's operations are named but not built yet.

use crate::support;

use jadpo_db::{Database, Layout};

const TYPES: &str = "type Driven = Length {}\n\ntype Ground = Area {}\n\ntype Inside = Bool {}\n\ntype SameSpot = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

/// Each problem as its rule, and its reason when `reasons` asks.
fn check(source: &str, reasons: bool) -> Vec<String> {
    let db = Database::default();
    let text = format!("{TYPES}{source}");
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &text)]);
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| match (&d.reason, reasons) {
            (Some(reason), true) => format!("{}: {reason}", d.rule.id),
            _ => d.rule.id.to_string(),
        })
        .collect()
}

const NONE: &[&str] = &[];

#[test]
fn every_measure_checks() {
    let source = "function leg(a: Point, b: Point) -> Driven {\n    return Driven(geo.distance(a, b))\n}\n\nfunction heading(a: Point, b: Point) fails SameSpot -> Bearing {\n    return attempt geo.bearing(a, b) {\n        same_point: SameSpot\n    }\n}\n\nfunction round(path: LineString) -> Driven {\n    return Driven(geo.length(path))\n}\n\nfunction rounds(paths: MultiLineString) -> Driven {\n    return Driven(geo.length(paths))\n}\n\nfunction ground(patch: Polygon) -> Ground {\n    return Ground(geo.area(patch))\n}\n\nfunction grounds(patches: MultiPolygon) -> Ground {\n    return Ground(geo.area(patches))\n}\n";
    assert_eq!(check(source, false), NONE);
}

#[test]
fn a_bearing_maps_same_point() {
    let problems = check(
        "function heading(a: Point, b: Point) -> Bearing {\n    return geo.bearing(a, b)\n}\n",
        true,
    );
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].starts_with("failure.attempt_required: Two equal points have no direction"),
        "{problems:?}"
    );
}

#[test]
fn distance_and_bearing_are_between_points_only() {
    let problems = check(
        "function reach(a: Point, patch: Polygon) -> Driven {\n    return Driven(geo.distance(a, patch))\n}\n",
        true,
    );
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].starts_with("type.mismatch: `geo.distance`'s `to` takes a `Point`")
            && problems[0].contains("area 20, question 10"),
        "{problems:?}"
    );
}

#[test]
fn length_and_area_take_the_shapes_they_measure() {
    assert_eq!(
        check(
            "function round(a: Point) -> Driven {\n    return Driven(geo.length(a))\n}\n\nfunction ground(path: LineString) -> Ground {\n    return Ground(geo.area(path))\n}\n\nfunction any(shape: Geometry) -> Ground {\n    return Ground(geo.area(shape))\n}\n",
            false
        ),
        ["type.mismatch", "type.mismatch", "type.mismatch"]
    );
}

#[test]
fn the_rest_of_the_page_is_not_built_yet() {
    assert_eq!(
        check(
            "function inside(patch: Polygon, at: Point) -> Inside {\n    return Inside(geo.contains(patch, at))\n}\n",
            false
        ),
        ["toolchain.not_built_yet"]
    );
}

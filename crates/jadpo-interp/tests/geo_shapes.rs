//! Area 20's predicates and shapes (chunk 5.5u3), run: a bakery delivers
//! to a customer inside one of its patches, on its edge included; which
//! pins are in view, near enough, or within a circle; a patch's label and
//! box; and a polygon built in code or read from JSON refused when its
//! rings aren't valid, `rings_valid` mapped by its `attempt`, `format`
//! read at the ring. The runtime holds the same operations to PostGIS's
//! answers (crates/jadpo-runtime/tests/geodesics.rs).

use crate::support;

use jadpo_interp::Value;
use support::{decimal, failure, main, text, value};

const BAKERY: &str = "type Delivers = Bool {}

type InView = Bool {}

type NotAPatch = Failure {
    kind: FailureKind.invalid_value
}

type Unreadable = Failure {
    kind: FailureKind.invalid_value
}

type DrawnShape = Text {}

type WalkingReach = Length {
    min: 0metre
}

/// Whether a customer is inside one of the bakery's patches.
function delivers_to(patches: List<Polygon>, customer: Point) -> Delivers {
    return Delivers(list.any(patches, patch -> geo.contains(patch, customer)))
}

/// Whether a pin is inside the map a person is looking at.
function in_view(pin: Point, view: BoundingBox) -> InView {
    return InView(geo.within(pin, view))
}

/// Whether a van's round stays inside the map.
function round_in_view(round: LineString, view: BoundingBox) -> InView {
    return InView(geo.within(round, view))
}

/// Whether a pin is in a circle drawn on the map.
function in_circle(pin: Point, around: Circle) -> InView {
    return InView(geo.contains(around, pin))
}

/// Whether two shops are near enough to walk between.
function walkable(from: Point, to: Point, reach: WalkingReach) -> Delivers {
    return Delivers(geo.within(from, to, distance: reach))
}

/// Whether a round crosses a patch.
function crosses(round: LineString, patch: Polygon) -> Delivers {
    return Delivers(geo.intersects(round, patch))
}

/// Where a patch's name goes on the map.
function label(patch: Polygon) -> Point {
    return geo.centroid(patch)
}

/// The map's view of a patch.
function frame(patch: Polygon) -> BoundingBox {
    return geo.bounding_box(patch)
}

/// A patch from the corners a planner clicked, closed.
function patch_from(corners: List<Point>, first: Point) fails NotAPatch -> Polygon {
    return attempt Polygon {
        outer: list.append(corners, first)
        holes: []
    } {
        rings_valid: NotAPatch
    }
}

/// A patch as a planner's map sends it.
function drawn(shape: DrawnShape) fails Unreadable -> Polygon {
    return attempt json.read(shape, as: Polygon) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}
";

fn point(latitude: &str, longitude: &str) -> Value {
    Value::record(
        "Point",
        vec![
            ("latitude".into(), decimal(latitude)),
            ("longitude".into(), decimal(longitude)),
            ("altitude".into(), Value::None),
        ],
    )
}

fn points(corners: &[&str]) -> Value {
    Value::list(
        corners
            .iter()
            .map(|corner| {
                let (latitude, longitude) = corner.split_once(' ').expect("a pair");
                point(latitude, longitude)
            })
            .collect(),
    )
}

/// A polygon value from closed rings.
fn polygon(outer: &[&str], holes: &[&[&str]]) -> Value {
    Value::record(
        "Polygon",
        vec![
            ("outer".into(), points(outer)),
            (
                "holes".into(),
                Value::list(holes.iter().map(|hole| points(hole)).collect()),
            ),
        ],
    )
}

fn yes(value: Value) -> bool {
    value == Value::Bool(true)
}

const LONDON: &[&str] = &[
    "51.49 -0.2",
    "51.49 -0.05",
    "51.54 -0.05",
    "51.54 -0.2",
    "51.49 -0.2",
];

#[test]
fn a_customer_on_a_patch_s_edge_is_delivered_to() {
    let program = main(BAKERY);
    let patches = Value::list(vec![polygon(LONDON, &[])]);
    let at = |latitude: &str, longitude: &str| {
        yes(value(
            &program,
            "delivers_to",
            vec![patches.clone(), point(latitude, longitude)],
        ))
    };
    assert!(at("51.5007", "-0.1246"));
    assert!(!at("48.8566", "2.3522"));
    // A corner, and a point on the patch's western edge, a meridian.
    assert!(at("51.54", "-0.05"));
    assert!(at("51.5", "-0.2"));
    assert!(!at("51.5", "-0.2000001"));
}

#[test]
fn pins_in_view_in_reach_and_in_a_circle() {
    let program = main(BAKERY);
    let view = Value::record(
        "BoundingBox",
        vec![
            ("south".into(), decimal("51.48")),
            ("west".into(), decimal("-0.21")),
            ("north".into(), decimal("51.55")),
            ("east".into(), decimal("-0.04")),
        ],
    );
    assert!(yes(value(
        &program,
        "in_view",
        vec![point("51.5007", "-0.1246"), view.clone()]
    )));
    assert!(!yes(value(
        &program,
        "in_view",
        vec![point("51.4769", "0"), view.clone()]
    )));
    let round = Value::record(
        "LineString",
        vec![("points".into(), points(&["51.5007 -0.1246", "51.4769 0"]))],
    );
    assert!(!yes(value(
        &program,
        "round_in_view",
        vec![round.clone(), view]
    )));
    let metres = |n: i128| {
        Value::Quantity(jadpo_value::Quantity {
            kind: jadpo_std::QuantityKind::Length,
            amount: n * 1_000_000,
        })
    };
    let around = Value::record(
        "Circle",
        vec![
            ("centre".into(), point("51.5007", "-0.1246")),
            ("radius".into(), metres(1000)),
        ],
    );
    assert!(yes(value(
        &program,
        "in_circle",
        vec![point("51.5052", "-0.1246"), around.clone()]
    )));
    assert!(!yes(value(
        &program,
        "in_circle",
        vec![point("51.5142", "-0.1246"), around]
    )));
    let ben = point("51.5007", "-0.1246");
    let soho = point("51.5136", "-0.1365");
    assert!(!yes(value(
        &program,
        "walkable",
        vec![ben.clone(), soho.clone(), metres(1500)]
    )));
    assert!(yes(value(
        &program,
        "walkable",
        vec![ben, soho, metres(1800)]
    )));
    assert!(yes(value(
        &program,
        "crosses",
        vec![round, polygon(LONDON, &[])]
    )));
}

#[test]
fn a_patch_s_label_and_box() {
    let program = main(BAKERY);
    // PostGIS's ST_Centroid(x, false) for this patch is 51.515019338081686,
    // -0.12499999998910921 (tools/geo_cases.py), 2 × 10⁻¹² of a degree away.
    assert_eq!(
        value(&program, "label", vec![polygon(LONDON, &[])]).to_string(),
        "Point { latitude: 51.51501933808094, longitude: -0.12499999998684845, altitude: none }"
    );
    // Its northern edge bows north of 51.54 between its corners: PostGIS's
    // points every 100 m reach 51.5400239069116, and the box rounds up.
    assert_eq!(
        value(&program, "frame", vec![polygon(LONDON, &[])]).to_string(),
        "BoundingBox { south: 51.49, west: -0.2, north: 51.54002390691161, east: -0.05 }"
    );
}

#[test]
fn a_patch_built_from_clicks_maps_rings_valid() {
    let program = main(BAKERY);
    let built = value(
        &program,
        "patch_from",
        vec![
            points(&["51.49 -0.2", "51.49 -0.05", "51.54 -0.05"]),
            point("51.49", "-0.2"),
        ],
    );
    assert!(matches!(built, Value::Record(_)));
    // Clicked as a bow tie, and doubling back along an edge.
    for corners in [
        &["0 0", "10 10", "10 0", "0 10"][..],
        &["0 0", "0 10", "0 5", "10 5"][..],
    ] {
        assert_eq!(
            failure(
                &program,
                "patch_from",
                vec![points(corners), point("0", "0")]
            )
            .name,
            "NotAPatch"
        );
    }
}

#[test]
fn a_drawn_patch_that_isn_t_valid_is_refused() {
    let program = main(BAKERY);
    let drawn = |json: &str| text(json);
    assert!(matches!(
        value(
            &program,
            "drawn",
            vec![drawn(
                r#"{"type":"Polygon","coordinates":[[[0,0],[10,0],[10,10],[0,10],[0,0]]]}"#
            )]
        ),
        Value::Record(_)
    ));
    let bow_tie = r#"{"type":"Polygon","coordinates":[[[0,0],[10,10],[0,10],[10,0],[0,0]]]}"#;
    assert_eq!(
        failure(&program, "drawn", vec![drawn(bow_tie)]).name,
        "Unreadable"
    );
}

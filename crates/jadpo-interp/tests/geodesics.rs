//! Area 20's measures (chunk 5.5u2), run: the page's examples, a van's
//! first leg with `same_point` mapped and London to Paris on the WGS84
//! ellipsoid, then a line's length as its edges' distances summed and a
//! polygon's area with its hole taken away. The runtime holds the same
//! operations to a sample of GeographicLib's test set
//! (crates/jadpo-runtime/tests/geodesics.rs).

use crate::support;

use jadpo_interp::{Outcome, Value};
use support::{decimal, failure, program, run, value};

const BAKERY: &str = "type SameSpot = Failure {
    kind: FailureKind.invalid_value
}

type Driven = Length {}

type Ground = Area {}

type VanLeg = Object {
    trip_length: Length
    heading: Bearing
}

/// How far a bakery's van drives to a customer, and which way it sets off.
function first_leg(bakery: Point, customer: Point) fails SameSpot -> VanLeg {
    var setting_off: Bearing = attempt geo.bearing(bakery, customer) {
        same_point: SameSpot  // a pickup at the counter
    }
    return VanLeg {
        trip_length: geo.distance(bakery, customer)
        heading: setting_off
    }
}

/// A round's driving, stop to stop.
function round_length(round: LineString) -> Driven {
    return Driven(geo.length(round))
}

/// How much ground a patch covers, its park left out.
function covered(patch: Polygon) -> Ground {
    return Ground(geo.area(patch))
}

/// Every patch's ground together.
function all_covered(patches: MultiPolygon) -> Ground {
    return Ground(geo.area(patches))
}
";

const TESTS: &str = "test \"London to Paris is measured on the ellipsoid\" {
    var big_ben: Point = Point {
        latitude: 51.5072
        longitude: -0.1276
        altitude: none
    }
    var eiffel_tower: Point = Point {
        latitude: 48.8566
        longitude: 2.3522
        altitude: none
    }
    assert geo.distance(big_ben, eiffel_tower) == 343896.891267metre
    assert geo.distance(eiffel_tower, big_ben) == geo.distance(big_ben, eiffel_tower)
}
";

fn bakery() -> jadpo_interp::Program {
    program(&[("bakery.jadpo", BAKERY), ("bakery.test.jadpo", TESTS)])
}

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

fn points(corners: &[(&str, &str)]) -> Value {
    Value::list(
        corners
            .iter()
            .map(|(latitude, longitude)| point(latitude, longitude))
            .collect(),
    )
}

fn metres(text: &str) -> String {
    format!("{text}metre")
}

#[test]
fn the_page_s_test_passes() {
    let program = bakery();
    let outcome = run(
        &program,
        "test bakery \"London to Paris is measured on the ellipsoid\"",
        vec![],
    );
    assert!(
        matches!(outcome, Outcome::Value(_)),
        "{}",
        program
            .report(&outcome)
            .map(|r| r.plain())
            .unwrap_or_default()
    );
}

#[test]
fn a_van_s_first_leg_is_its_distance_and_heading() {
    let program = bakery();
    let leg = value(
        &program,
        "first_leg",
        vec![point("51.5072", "-0.1276"), point("48.8566", "2.3522")],
    );
    let Value::Record(leg) = leg else {
        panic!("a record");
    };
    assert_eq!(
        leg.get("trip_length").map(ToString::to_string),
        Some(metres("343896.891267"))
    );
    assert_eq!(
        leg.get("heading").map(ToString::to_string),
        Some("148.046085477".to_string())
    );
}

#[test]
fn a_pickup_at_the_counter_is_the_failure_same_point_maps_to() {
    let program = bakery();
    let counter = point("51.5072", "-0.1276");
    assert_eq!(
        failure(&program, "first_leg", vec![counter.clone(), counter]).name,
        "SameSpot"
    );
    // Every longitude meets at a pole, and -180 is 180.
    assert_eq!(
        failure(
            &program,
            "first_leg",
            vec![point("90", "0"), point("90", "45")]
        )
        .name,
        "SameSpot"
    );
    assert_eq!(
        failure(
            &program,
            "first_leg",
            vec![point("10", "-180"), point("10", "180")]
        )
        .name,
        "SameSpot"
    );
}

#[test]
fn a_round_is_its_legs_summed() {
    let program = bakery();
    let round = Value::record(
        "LineString",
        vec![(
            "points".into(),
            points(&[
                ("51.5072", "-0.1276"),
                ("48.8566", "2.3522"),
                ("51.5072", "-0.1276"),
            ]),
        )],
    );
    assert_eq!(
        value(&program, "round_length", vec![round]).to_string(),
        metres("687793.782534")
    );
}

#[test]
fn a_patch_s_ground_leaves_its_park_out() {
    let program = bakery();
    let square = [("0", "0"), ("0", "1"), ("1", "1"), ("1", "0"), ("0", "0")];
    let park = [
        ("0.25", "0.25"),
        ("0.75", "0.25"),
        ("0.75", "0.75"),
        ("0.25", "0.75"),
        ("0.25", "0.25"),
    ];
    let polygon = |holes: Vec<Value>| {
        Value::record(
            "Polygon",
            vec![
                ("outer".into(), points(&square)),
                ("holes".into(), Value::list(holes)),
            ],
        )
    };
    // PostGIS's ST_Area gives 12,308,778,361.47 m² for the square (area 20).
    assert_eq!(
        value(&program, "covered", vec![polygon(Vec::new())]).to_string(),
        "12308778361.469453sqm"
    );
    let whole = value(&program, "covered", vec![polygon(Vec::new())]);
    let with_park = value(&program, "covered", vec![polygon(vec![points(&park)])]);
    assert_ne!(whole, with_park);
    let both = Value::record(
        "MultiPolygon",
        vec![(
            "polygons".into(),
            Value::list(vec![polygon(Vec::new()), polygon(Vec::new())]),
        )],
    );
    // Rounded once, after summing: not twice the square's rounded area.
    assert_eq!(
        value(&program, "all_covered", vec![both]).to_string(),
        "24617556722.938907sqm"
    );
}

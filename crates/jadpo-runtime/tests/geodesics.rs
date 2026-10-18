//! Area 20's measures (chunk 5.5u2), natively: `geo.distance`, `bearing`
//! and `area` against a sample of GeographicLib's test set, the one the
//! gate also checks under WebAssembly (crates/jadpo-value/examples/
//! geodesics), through the one implementation both executors call, with
//! a digest of every answer that both targets must give; then
//! `same_point` and a line's length, as compiled code calls them.

#[path = "../../jadpo-value/examples/geodesics/check.rs"]
mod check;

use jadpo_std::RuleKey;
use jadpo_std::standard::geo::{LINE_STRING, MULTI_LINE_STRING};
use jadpo_value::{Decimal, Fault, Value, geo, run};

#[test]
fn every_measure_matches_geographiclib_s_test_set() {
    let (wrong, count) = check::mismatches();
    assert!(count >= 500, "{count} reference geodesics");
    assert!(
        wrong.is_empty(),
        "{} of {count}:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

fn point(latitude: &str, longitude: &str) -> Value {
    let number = |text: &str| Decimal::parse(text).expect("a decimal");
    geo::point(number(latitude), number(longitude), Value::None)
}

fn call(name: &str, args: &[Value]) -> Result<Value, Fault> {
    let op = jadpo_std::operation("geo", name).expect("an operation").op;
    run(op, args, None, &mut |_| None)
}

#[test]
fn two_equal_points_have_no_bearing() {
    let london = point("51.5072", "-0.1276");
    assert_eq!(
        call("bearing", &[london.clone(), london.clone()]),
        Err(Fault::Broke(RuleKey::SamePoint))
    );
    assert_eq!(
        call("bearing", &[point("-90", "10"), point("-90", "-170")]),
        Err(Fault::Broke(RuleKey::SamePoint))
    );
    // A distance between them is 0, not a failure.
    assert_eq!(
        call("distance", &[london.clone(), london]).map(|v| v.to_string()),
        Ok("0metre".to_string())
    );
    // Due east along the equator, due south along a meridian.
    assert_eq!(
        call("bearing", &[point("0", "0"), point("0", "1")]).map(|v| v.to_string()),
        Ok("90.000000000".to_string())
    );
    assert_eq!(
        call("bearing", &[point("10", "0"), point("-10", "0")]).map(|v| v.to_string()),
        Ok("180.000000000".to_string())
    );
}

#[test]
fn a_line_s_length_is_its_edges_distances_summed() {
    let path = [
        point("51.5072", "-0.1276"),
        point("48.8566", "2.3522"),
        point("45.764", "4.8357"),
    ];
    let edges: i128 = path
        .windows(2)
        .map(|pair| match call("distance", pair) {
            Ok(Value::Quantity(length)) => length.amount,
            other => panic!("{other:?}"),
        })
        .sum();
    let line = Value::record(
        LINE_STRING,
        vec![("points".into(), Value::list(path.to_vec()))],
    );
    let Ok(Value::Quantity(length)) = call("length", std::slice::from_ref(&line)) else {
        panic!("a length");
    };
    assert_eq!(length.amount, edges);
    let lines = Value::record(
        MULTI_LINE_STRING,
        vec![("lines".into(), Value::list(vec![line.clone(), line]))],
    );
    let Ok(Value::Quantity(both)) = call("length", &[lines]) else {
        panic!("a length");
    };
    assert_eq!(both.amount, 2 * edges);
}

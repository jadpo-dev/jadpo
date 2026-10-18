//! Area 19, "Web and software" (chunk 5.5n2), in the runtime: the values
//! the page's test (`project=software`) and the interpreter's `software`
//! tests assert, given to `semver.satisfies` as compiled code calls it
//! (`jadpo_value::run`), with node-semver's README examples and SemVer
//! 2.0.0's pre-release rules; and a version's order as `<` and sorting
//! see it. Its text, JSON and columns are the one harness's
//! (`jadpo-interp`'s `standard_types`).

use jadpo_std::Op;
use jadpo_value::{Value, ops, order, run, semver, standard};

fn version(text: &str) -> Value {
    semver::read_version(text).unwrap_or_else(|| panic!("`{text}` is a version"))
}

fn fits(text: &str, range: &str) -> bool {
    match run(
        Op::SemverSatisfies,
        &[version(text), Value::text(range)],
        None,
        &mut |_| None,
    ) {
        Ok(Value::Bool(inside)) => inside,
        other => panic!("{text} in {range}: {other:?}"),
    }
}

#[test]
fn satisfies_reads_ranges_as_node_semver_does() {
    // The page's test.
    assert!(fits("1.4.2", "^1.2.3"));
    assert!(!fits("1.4.2", "~1.2.3"));
    assert!(!fits("2.0.0-beta.1", "^1.2.3"));
    assert!(!fits("2.0.0-beta.1", ">=1.0.0"));
    assert!(fits("2.0.0-beta.1", ">=2.0.0-beta.0 <3.0.0"));
    // Carets keep the first number that isn't zero.
    assert!(fits("0.2.9", "^0.2.3"));
    assert!(!fits("0.3.0", "^0.2.3"));
    assert!(!fits("0.0.4", "^0.0.3"));
    // Tildes, partials, wildcards, hyphens and sets.
    assert!(fits("1.2.9", "~1.2.3"));
    assert!(fits("1.9.0", "~1"));
    assert!(fits("1.2.7", "1.2.x"));
    assert!(!fits("1.3.0", "1.2"));
    assert!(fits("2.3.9", "1.2.3 - 2.3"));
    assert!(!fits("2.4.0", "1.2.3 - 2.3"));
    assert!(fits("1.2.7", "1.2.7 || >=1.2.9 <2.0.0"));
    assert!(!fits("1.2.8", "1.2.7 || >=1.2.9 <2.0.0"));
}

#[test]
fn a_pre_release_is_inside_only_where_its_numbers_are_named() {
    // node-semver's README: `>1.2.3-alpha.3` takes `1.2.3-alpha.7` but
    // not `3.4.5-alpha.9`, though `3.4.5` is above it.
    assert!(fits("1.2.3-alpha.7", ">1.2.3-alpha.3"));
    assert!(!fits("3.4.5-alpha.9", ">1.2.3-alpha.3"));
    assert!(fits("3.4.5", ">1.2.3-alpha.3"));
    assert!(!fits("1.0.0-beta", "*"));
    assert!(fits("1.2.3-beta.4", "^1.2.3-beta.2"));
    assert!(!fits("1.2.4-beta.2", "^1.2.3-beta.2"));
}

#[test]
fn versions_order_by_precedence() {
    // SemVer 2.0.0 §11's order.
    let order_of = [
        "1.0.0-alpha",
        "1.0.0-alpha.1",
        "1.0.0-alpha.beta",
        "1.0.0-beta",
        "1.0.0-beta.2",
        "1.0.0-beta.11",
        "1.0.0-rc.1",
        "1.0.0",
        "1.9.0",
        "1.10.0",
    ];
    for pair in order_of.windows(2) {
        assert_eq!(
            ops::compare(&version(pair[0]), &version(pair[1])),
            Some(std::cmp::Ordering::Less),
            "{} < {}",
            pair[0],
            pair[1]
        );
    }
    // Build metadata: neither below the other, not equal, and sorted
    // after its precedence by its text.
    let (a, b) = (version("1.0.0+a"), version("1.0.0+b"));
    assert_eq!(ops::compare(&a, &b), Some(std::cmp::Ordering::Equal));
    assert!(!ops::equal(&a, &b));
    assert_eq!(order::compare(&a, &b), std::cmp::Ordering::Less);
    // Sorting sees `<`'s order, so 1.10.0 isn't before 1.9.0.
    let mut sorted: Vec<Value> = ["1.10.0", "1.9.0", "1.10.0-rc.1"]
        .into_iter()
        .map(version)
        .collect();
    sorted.sort_by(order::compare);
    let texts: Vec<Value> = sorted
        .iter()
        .map(|v| standard::from(v).expect("text"))
        .collect();
    assert_eq!(
        texts,
        [
            Value::text("1.9.0"),
            Value::text("1.10.0-rc.1"),
            Value::text("1.10.0")
        ]
    );
}

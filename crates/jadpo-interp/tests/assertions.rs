//! A failed `assert` keeps the value of every sub-expression of its
//! condition (area 13, "What a failed test reports": "every
//! sub-expression's value, as pytest and power-assert show"), each with
//! where it is written and where its value points: an operator for
//! arithmetic and comparisons, a field's name for a field read, otherwise
//! its start. Literals are left out, and so is anything inside a callee or
//! an inline function, which has no single value.

use crate::support;

use jadpo_interp::{AssertionDetail, Got, Outcome, Program};
use support::{program, run};

const SIZES: &str = "type Count = Int {\n    min: 0\n}\n\ntype Point = Object {\n    x: Count\n    y: Count\n}\n\ntype TooBig = Failure {\n    kind: FailureKind.rejected\n}\n\nfunction add(a: Count, b: Count) -> Count {\n    return Count(a + b)\n}\n\nfunction double(n: Count) -> Count {\n    var twice: Count = add(n, n)\n    return twice\n}\n\naction check_size(n: Count) fails TooBig -> Count {\n    if (n > 10) {\n        reject TooBig\n    }\n    return n\n}\n";

fn sizes(tests: &str) -> Program {
    program(&[("sizes.jadpo", SIZES), ("sizes.test.jadpo", tests)])
}

/// Each part of the assertion test `name` failed with: its source, the
/// source from its anchor on, and its value.
fn parts(program: &Program, name: &str) -> Vec<(String, String, String)> {
    let Outcome::Assertion(assertion) = run(program, &format!("test sizes \"{name}\""), vec![])
    else {
        panic!("{name} didn't fail an assertion");
    };
    let text = program.text("sizes.test.jadpo").expect("the test file");
    assertion
        .parts
        .iter()
        .map(|part| {
            (
                text[part.start as usize..part.end as usize].to_string(),
                text[part.anchor as usize..part.end as usize].to_string(),
                part.value.to_string(),
            )
        })
        .collect()
}

fn part(source: &str, anchored: &str, value: &str) -> (String, String, String) {
    (source.into(), anchored.into(), value.into())
}

#[test]
fn a_comparison_keeps_each_side_and_what_built_it() {
    let program = sizes("test \"adds wrongly\" {\n    assert add(2, 3) == add(1, 1) + 1\n}\n");
    assert_eq!(
        parts(&program, "adds wrongly"),
        [
            part("add(2, 3)", "add(2, 3)", "5"),
            part("add(1, 1)", "add(1, 1)", "2"),
            part("add(1, 1) + 1", "+ 1", "3"),
        ]
    );
}

#[test]
fn a_false_condition_keeps_its_own_value_and_its_parts() {
    let program = sizes(
        "test \"small or doubled\" {\n    var size: Count = 3\n    assert size > 10 or double(size) == 7\n}\n",
    );
    assert_eq!(
        parts(&program, "small or doubled"),
        [
            part("size", "size", "3"),
            part("size > 10", "> 10", "false"),
            part(
                "size > 10 or double(size) == 7",
                "or double(size) == 7",
                "false"
            ),
            part("double(size)", "double(size)", "6"),
            part("size", "size", "3"),
            part("double(size) == 7", "== 7", "false"),
        ]
    );
}

#[test]
fn a_field_read_points_at_the_field() {
    let program = sizes(
        "test \"the point is right\" {\n    var point: Point = Point {\n        x: 1\n        y: 2\n    }\n    assert point.x == 2\n}\n",
    );
    assert_eq!(
        parts(&program, "the point is right"),
        [
            part("point", "point", "Point { x: 1, y: 2 }"),
            part("point.x", "x", "1"),
        ]
    );
}

#[test]
fn an_expected_failure_keeps_the_call_and_its_arguments() {
    let program = sizes(
        "test \"a small size is rejected\" {\n    var size: Count = 3\n    assert check_size(size) fails TooBig\n}\n",
    );
    assert_eq!(
        parts(&program, "a small size is rejected"),
        [
            part("check_size(size)", "check_size(size)", "3"),
            part("size", "size", "3"),
        ]
    );
    let Outcome::Assertion(assertion) =
        run(&program, "test sizes \"a small size is rejected\"", vec![])
    else {
        panic!("no assertion");
    };
    assert!(matches!(
        &assertion.detail,
        AssertionDetail::Fails {
            got: Got::Value(_),
            ..
        }
    ));
}

#[test]
fn an_inline_function_has_no_single_value_to_show() {
    let program = sizes(
        "test \"two are large\" {\n    var sizes: List<Count> = [1, 20, 30]\n    assert list.count(list.filter(sizes, size -> size > 25)) == 2\n}\n",
    );
    assert_eq!(
        parts(&program, "two are large"),
        [
            part(
                "list.count(list.filter(sizes, size -> size > 25))",
                "list.count(list.filter(sizes, size -> size > 25))",
                "1"
            ),
            part(
                "list.filter(sizes, size -> size > 25)",
                "list.filter(sizes, size -> size > 25)",
                "[30]"
            ),
            part("sizes", "sizes", "[1, 20, 30]"),
        ]
    );
}

#[test]
fn a_passing_assertion_keeps_nothing() {
    let program = sizes("test \"adds\" {\n    assert add(2, 3) == 5\n}\n");
    assert_eq!(
        run(&program, "test sizes \"adds\"", vec![]),
        Outcome::Value(jadpo_interp::Value::Unit)
    );
}

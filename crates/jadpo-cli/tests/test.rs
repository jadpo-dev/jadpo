//! `jadpo test` (area 13, "Running tests" and "What a failed test
//! reports"; area 14): every screen's exact plain output, the rich form
//! of a pass and a failure, `--json`, and how a file, a line, a name, a
//! kind, the last run's failures and the order seed pick and order tests.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

/// Every kind of thing a test can meet: plain values, a record, a boolean
/// type, an action with two failures (one with an internal field), and
/// arithmetic that can overflow.
const SIZES: &str = "type Count = Int {\n    min: 0\n}\n\ntype Pence = Int {\n    min: 0\n}\n\ntype Point = Object {\n    x: Count\n    y: Count\n}\n\ntype Ready = Bool {}\n\ntype TooBig = Failure {\n    kind: FailureKind.rejected\n}\n\ntype TooSmall = Failure {\n    kind: FailureKind.rejected\n    internal: {\n        given: Count\n    }\n}\n\nfunction add(a: Count, b: Count) -> Count {\n    return Count(a + b)\n}\n\nfunction moved(point: Point) -> Point {\n    return Point {\n        x: Count(point.x + 1)\n        y: point.y\n    }\n}\n\nfunction ready(size: Count, limit: Count) -> Ready {\n    return Ready(size > limit)\n}\n\nfunction line_total(price: Pence, quantity: Count) -> Pence {\n    return Pence(price * quantity)\n}\n\naction check_size(n: Count) fails TooBig, TooSmall -> Count {\n    if (n > 10) {\n        reject TooBig\n    }\n    if (n < 2) {\n        reject TooSmall {\n            given: n\n        }\n    }\n    return n\n}\n";

const ADDS: &str = "test \"adds\" {\n    assert add(2, 3) == 5\n}\n";
const ADDS_WRONGLY: &str = "test \"adds wrongly\" {\n    assert add(2, 3) == add(1, 1) + 1\n}\n";
const MOVES: &str = "test \"the point moves right\" {\n    var start: Point = Point {\n        x: 1\n        y: 2\n    }\n    assert moved(start) == Point {\n        x: 3\n        y: 2\n    }\n}\n";
const READY: &str = "test \"a big size is ready\" {\n    var size: Count = 4\n    var limit: Count = 10\n    assert ready(size, limit)\n}\n";
const RETURNED: &str = "test \"a mid size is too big\" {\n    var size: Count = 3\n    assert check_size(size) fails TooBig\n}\n";
const INSTEAD: &str = "test \"one is too big\" {\n    assert check_size(1) fails TooBig\n}\n";
const KEPT: &str = "test \"a big size is kept\" {\n    var size: Count = attempt check_size(12)\n    assert size == 12\n}\n";
const PRICED: &str = "test \"a large order is priced\" {\n    var price: Pence = 4611686018427387904\n    assert line_total(price, 2) > 0\n}\n";
const ACTS: &str = "test \"a size is checked\" {\n    var size: Count = attempt check_size(5)\n    assert size == 5\n}\n";

/// A fresh folder for one test under cargo's temporary directory.
fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("test-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

/// Loose files: `sizes.jadpo` and these tests beside it.
fn loose(name: &str, tests: &[&str]) -> PathBuf {
    let root = folder(name);
    write(&root, "sizes.jadpo", SIZES);
    if !tests.is_empty() {
        write(&root, "sizes.test.jadpo", &tests.join("\n"));
    }
    root
}

fn jadpo_in(env: Environment, cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &env.with_cwd(cwd))
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    jadpo_in(Environment::empty(), cwd, args)
}

fn terminal() -> Environment {
    Environment::empty()
        .with_terminals(true, true, true)
        .with_var("TERM", "xterm-256color")
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_str(&output.stdout).expect("one JSON document")
}

#[test]
fn a_passing_run_is_one_line() {
    let root = loose("pass", &[ADDS, ACTS]);
    let output = jadpo(&root, &["test", "."]);
    assert_eq!(
        output.stdout,
        "tested: 2 passed, 1 pure test, 1 action test\n"
    );
    assert_eq!(output.stderr, "");
    assert_eq!(output.code, 0);
    let rich = jadpo_in(terminal(), &root, &["test", "."]);
    assert_eq!(
        strip_ansi(&rich.stdout),
        "✓ 2 passed  1 pure test · 1 action test\n"
    );
    let root = loose("pass-one-kind", &[ADDS]);
    assert_eq!(
        jadpo(&root, &["test", "."]).stdout,
        "tested: 1 passed, 1 pure test\n"
    );
}

#[test]
fn a_failed_assertion_shows_each_part_and_both_sides() {
    let root = loose("assertion", &[ADDS, ADDS_WRONGLY]);
    let output = jadpo(&root, &["test", ".", "--seed", "1"]);
    assert_eq!(output.code, 1);
    assert_eq!(output.stderr, "");
    assert_eq!(
        output.stdout,
        "\
FAIL  adds wrongly
 --> sizes.test.jadpo:6

  assert add(2, 3) == add(1, 1) + 1
         |            |         |
         5            2         3

  expected: 3
  actual: 5

  rule: test.assertion_failed
  rerun: jadpo test sizes.test.jadpo:6 --seed 1

test failed: 1 failed, 1 passed, 2 pure tests
"
    );
    let rich = jadpo_in(terminal(), &root, &["test", ".", "--seed", "1"]);
    assert_eq!(
        strip_ansi(&rich.stdout),
        "\
✗ failed  adds wrongly
  sizes.test.jadpo:6

  assert add(2, 3) == add(1, 1) + 1
         │            │         │
         5            2         3

  expected  3
  actual    5

  rule      test.assertion_failed
  rerun     jadpo test sizes.test.jadpo:6 --seed 1

✗ 1 failed  1 passed · 2 pure tests
"
    );
}

#[test]
fn a_record_comparison_lists_the_fields_that_differ() {
    let root = loose("record", &[MOVES]);
    assert_eq!(
        jadpo(&root, &["test", ".", "--seed", "1"]).stdout,
        "\
FAIL  the point moves right
 --> sizes.test.jadpo:6

  assert moved(start) == Point { ...
         |     |         |
         |     |         Point { x: 3, y: 2 }
         |     Point { x: 1, y: 2 }
         Point { x: 2, y: 2 }

  expected: Point { x: 3, y: 2 }
  actual: Point { x: 2, y: 2 }
  diff: ~ x: expected 3, actual 2

  rule: test.assertion_failed
  rerun: jadpo test sizes.test.jadpo:6 --seed 1

test failed: 1 failed, 1 pure test
"
    );
}

#[test]
fn a_false_condition_shows_its_own_value_and_its_parts() {
    let root = loose("condition", &[READY]);
    assert_eq!(
        jadpo(&root, &["test", ".", "--seed", "1"]).stdout,
        "\
FAIL  a big size is ready
 --> sizes.test.jadpo:4

  assert ready(size, limit)
         |     |     |
         false 4     10

  rule: test.assertion_failed
  rerun: jadpo test sizes.test.jadpo:4 --seed 1

test failed: 1 failed, 1 pure test
"
    );
}

#[test]
fn an_expected_failure_shows_what_the_call_did_instead() {
    let root = loose("returned", &[RETURNED]);
    assert_eq!(
        jadpo(&root, &["test", ".", "--seed", "1"]).stdout,
        "\
FAIL  a mid size is too big
 --> sizes.test.jadpo:3

  assert check_size(size) fails TooBig
         |          |
         3          3

  expected: failure `TooBig`
  actual: returned 3

  rule: test.assertion_failed
  rerun: jadpo test sizes.test.jadpo:3 --seed 1

test failed: 1 failed, 1 action test
"
    );
    let root = loose("instead", &[INSTEAD]);
    assert_eq!(
        jadpo(&root, &["test", ".", "--seed", "1"]).stdout,
        "\
FAIL  one is too big
 --> sizes.test.jadpo:2

  assert check_size(1) fails TooBig

  expected: failure `TooBig`
  actual: failure `TooSmall`
  at: sizes.jadpo:51:9
   |
51 |         reject TooSmall {
   |         ^^^^^^^^^^^^^^^^^ rejected here
  code: too_small
  kind: FailureKind.rejected
  internal: { given: 1 }
  path: test \"one is too big\" -> action check_size

  rule: test.assertion_failed
  rerun: jadpo test sizes.test.jadpo:2 --seed 1

test failed: 1 failed, 1 action test
"
    );
}

#[test]
fn an_uncaught_failure_shows_where_it_was_raised_and_its_path() {
    let root = loose("uncaught", &[KEPT]);
    assert_eq!(
        jadpo(&root, &["test", ".", "--seed", "1"]).stdout,
        "\
FAIL  a big size is kept
 --> sizes.test.jadpo:2

  failure: `TooBig` from action check_size
  at: sizes.jadpo:48:9
   |
48 |         reject TooBig
   |         ^^^^^^^^^^^^^ rejected here
  code: too_big
  kind: FailureKind.rejected
  path: test \"a big size is kept\" -> action check_size

  rule: test.unexpected_failure
  rerun: jadpo test sizes.test.jadpo:2 --seed 1

test failed: 1 failed, 1 action test
"
    );
}

#[test]
fn a_fault_shows_its_site_and_every_frame() {
    let root = loose("fault", &[PRICED]);
    let output = jadpo(&root, &["test", ".", "--seed", "1"]);
    assert_eq!(
        output.stdout,
        "\
FAIL  a large order is priced
 --> sizes.test.jadpo:3

  fault: int_overflow in function line_total
  at: sizes.jadpo:43:18
   |
43 |     return Pence(price * quantity)
   |                  ^^^^^^^^^^^^^^^^ out of range
  reason: `price * quantity` gives a whole number outside `Int`'s range, -9223372036854775808 to 9223372036854775807; Jadpo stops rather than wrap around.
  next: Bound the values that feed it with `max`, so the result always fits (agent)
  stack: sizes.jadpo:43      function line_total
         sizes.test.jadpo:3  test \"a large order is priced\"

  rule: test.fault
  rerun: jadpo test sizes.test.jadpo:3 --seed 1

test failed: 1 failed, 1 pure test
"
    );
    let rich = strip_ansi(&jadpo_in(terminal(), &root, &["test", ".", "--seed", "1"]).stdout);
    assert!(
        rich.contains(
            "  reason    `price * quantity` gives a whole number outside `Int`'s range,\n            -9223372036854775808 to 9223372036854775807; Jadpo stops rather than\n            wrap around.\n"
        ),
        "{rich}"
    );
}

#[test]
fn a_name_that_matches_nothing_suggests_the_nearest() {
    let root = loose("no-name", &[ADDS, ADDS_WRONGLY]);
    let output = jadpo(&root, &["test", ".", "--name", "add wrongly"]);
    assert_eq!(output.code, 1);
    assert_eq!(
        output.stdout,
        "\
error: No test's name contains `add wrongly`
  reason: None of the 2 tests' names contains this text. Did you mean `adds wrongly`?
  next: Use `--name \"adds wrongly\"` (agent)
  rule: test.no_tests
  help: docs/language/14-tooling.html#command-errors
"
    );
    let found = jadpo(&root, &["test", ".", "--name", "WRONG", "--seed", "1"]);
    assert!(
        found
            .stdout
            .ends_with("test failed: 1 failed, 1 of 2 tests\n"),
        "{}",
        found.stdout
    );
}

#[test]
fn nothing_to_run_is_test_no_tests() {
    let root = loose("none", &[]);
    let output = jadpo(&root, &["test", "."]);
    assert_eq!(output.code, 1);
    assert_eq!(
        output.stdout,
        "\
error: These files have no tests
  reason: No file holds a `test` declaration, so there is nothing to run.
  next: Add a test beside the code it tests, in a file named `<name>.test.jadpo` (agent)
  rule: test.no_tests
  help: docs/language/14-tooling.html#command-errors
"
    );
    let document = json(&jadpo(&root, &["test", ".", "--json"]));
    assert_eq!(document["ok"], false);
    assert_eq!(document["diagnostics"][0]["rule"], "test.no_tests");
}

#[test]
fn a_project_with_errors_is_refused_with_them() {
    let root = loose(
        "refused",
        &["test \"adds\" {\n    assert add(2, 3) == totl\n}\n"],
    );
    let output = jadpo(&root, &["test", "."]);
    assert_eq!(output.code, 1);
    assert!(
        output
            .stdout
            .starts_with("error: `totl` isn't defined in this scope\n --> sizes.test.jadpo:2:25\n"),
        "{}",
        output.stdout
    );
    assert!(
        output
            .stdout
            .ends_with("\ntests not run: 1 error, 2 files\n"),
        "{}",
        output.stdout
    );
    let document = json(&jadpo(&root, &["test", ".", "--json"]));
    assert_eq!(document["ok"], false);
    assert_eq!(document["data"]["ran"], false);
    assert_eq!(document["diagnostics"][0]["rule"], "type.unknown_value");
}

#[test]
fn route_and_schedule_tests_are_not_built_yet() {
    let root = loose("route", &[ADDS]);
    let output = jadpo(&root, &["test", ".", "--kind", "route"]);
    assert_eq!(output.code, 1);
    assert_eq!(
        output.stdout,
        "\
error: Route tests aren't built yet
  reason: A route test sends a request through a route (area 7), and this toolchain runs pure and action tests so far.
  next: Run `jadpo test --kind pure` or `jadpo test --kind action` (agent)
  rule: toolchain.not_built_yet
  help: docs/language/03-logic.html#toolchain-not-built-yet
"
    );
    let kinds = loose("kinds", &[ADDS, ACTS]);
    assert_eq!(
        jadpo(&kinds, &["test", ".", "--kind", "action"]).stdout,
        "tested: 1 passed, 1 of 2 tests\n"
    );
    let output = jadpo(&root, &["test", ".", "--kind", "action"]);
    assert!(
        output.stdout.starts_with("error: No action tests to run\n"),
        "{}",
        output.stdout
    );
}

#[test]
fn json_is_one_versioned_document_with_every_failure_as_data() {
    let root = loose("json", &[ADDS, MOVES, KEPT]);
    let output = jadpo(&root, &["test", ".", "--json", "--seed", "7"]);
    assert_eq!(output.code, 1);
    assert_eq!(output.stderr, "");
    let document = json(&output);
    assert_eq!(document["schemaVersion"], 1);
    assert_eq!(document["command"], "test");
    assert_eq!(document["ok"], false);
    assert_eq!(document["diagnostics"][0]["rule"], "test.failed");
    assert_eq!(document["diagnostics"][0]["message"], "2 of 3 tests failed");
    let data = &document["data"];
    assert_eq!(data["seed"], 7);
    assert_eq!(data["counts"]["ran"], 3);
    assert_eq!(data["counts"]["failed"], 2);
    assert_eq!(data["counts"]["ranByKind"]["action"], 1);
    let tests = data["tests"].as_array().expect("tests");
    let record = tests
        .iter()
        .find(|test| test["name"] == "the point moves right")
        .expect("the record test");
    let failure = &record["failure"];
    assert_eq!(failure["code"], "test.assertion_failed");
    assert_eq!(failure["location"]["line"], 10);
    assert_eq!(failure["rerun"], "jadpo test sizes.test.jadpo:10 --seed 7");
    let assertion = &failure["assertion"];
    assert_eq!(assertion["operator"], "==");
    assert_eq!(assertion["expected"], "Point { x: 3, y: 2 }");
    assert_eq!(assertion["actual"], "Point { x: 2, y: 2 }");
    assert_eq!(
        assertion["diff"],
        serde_json::json!([{ "change": "changed", "path": "x", "expected": "3", "actual": "2" }])
    );
    assert_eq!(assertion["parts"][0]["source"], "moved(start)");
    assert_eq!(assertion["parts"][0]["value"], "Point { x: 2, y: 2 }");
    assert!(
        assertion["source"]
            .as_str()
            .expect("source")
            .ends_with("y: 2\n    }")
    );
    let kept = tests
        .iter()
        .find(|test| test["name"] == "a big size is kept")
        .expect("the failure test");
    assert_eq!(kept["kind"], "action");
    assert_eq!(kept["failure"]["code"], "test.unexpected_failure");
    assert_eq!(kept["failure"]["failure"]["name"], "TooBig");
    assert_eq!(kept["failure"]["failure"]["code"], "too_big");
    assert_eq!(
        kept["failure"]["failure"]["path"],
        serde_json::json!(["test \"a big size is kept\"", "action check_size"])
    );
    let passed = tests
        .iter()
        .find(|test| test["name"] == "adds")
        .expect("the passing test");
    assert_eq!(passed["outcome"], "passed");
    assert!(passed.get("failure").is_none());
}

/// The order tests ran in, by name.
fn order(root: &Path, args: &[&str]) -> Vec<String> {
    let output = jadpo(root, args);
    json(&output)["data"]["tests"]
        .as_array()
        .expect("tests")
        .iter()
        .map(|test| test["name"].as_str().expect("name").to_string())
        .collect()
}

#[test]
fn the_order_seed_decides_the_order_and_reproduces_it() {
    let tests: Vec<String> = (1..=8)
        .map(|n| format!("test \"adds {n}\" {{\n    assert add({n}, 1) > 0\n}}\n"))
        .collect();
    let refs: Vec<&str> = tests.iter().map(String::as_str).collect();
    let root = loose("seed", &refs);
    let first = order(&root, &["test", ".", "--json", "--seed", "11"]);
    assert_eq!(
        first,
        order(&root, &["test", ".", "--json", "--seed", "11"])
    );
    assert_ne!(
        first,
        order(&root, &["test", ".", "--json", "--seed", "12"])
    );
    let mut sorted = first.clone();
    sorted.sort();
    assert_eq!(sorted.len(), 8);
    // A run without a seed picks one and says which.
    let seed = json(&jadpo(&root, &["test", ".", "--json"]))["data"]["seed"].clone();
    assert!(
        seed.as_u64()
            .is_some_and(|seed| (1..100_000).contains(&seed))
    );
}

#[test]
fn a_file_a_line_or_a_folder_picks_the_tests() {
    let root = loose("pick", &[ADDS, ADDS_WRONGLY, KEPT]);
    let line = jadpo(&root, &["test", "sizes.test.jadpo:5", "--seed", "1"]);
    assert!(
        line.stdout.starts_with("FAIL  adds wrongly\n"),
        "{}",
        line.stdout
    );
    assert!(
        line.stdout
            .ends_with("test failed: 1 failed, 1 of 3 tests\n")
    );
    let first = jadpo(&root, &["test", "sizes.test.jadpo:2"]);
    assert_eq!(first.stdout, "tested: 1 passed, 1 of 3 tests\n");
    let gap = jadpo(&root, &["test", "sizes.test.jadpo:4"]);
    assert_eq!(
        gap.stdout,
        "\
error: No test at `sizes.test.jadpo:4`
  reason: The tests in this file start at lines 1, 5 and 9.
  next: Give a line inside a test, such as `jadpo test sizes.test.jadpo:1` (agent)
  rule: test.no_tests
  help: docs/language/14-tooling.html#command-errors
"
    );
    let file = jadpo(&root, &["test", "sizes.jadpo"]);
    assert_eq!(
        file.stdout,
        "\
error: `sizes.jadpo` has no tests
  reason: The project's 3 tests are in `sizes.test.jadpo`.
  next: Give a file or folder that holds tests, or leave out the path to run every test (agent)
  rule: test.no_tests
  help: docs/language/14-tooling.html#command-errors
"
    );
    // Loose files given by folder rerun under that folder.
    let parent = root.parent().expect("parent");
    let name = root.file_name().and_then(|n| n.to_str()).expect("name");
    let from_parent = jadpo(parent, &["test", name, "--seed", "1"]);
    assert!(
        from_parent.stdout.contains(&format!(
            "rerun: jadpo test {name}/sizes.test.jadpo:6 --seed 1\n"
        )),
        "{}",
        from_parent.stdout
    );
}

const APP: &str = "application {}\n";
const COUNT: &str = "domains/stock/types/count.jadpo";
const TALLY: &str = "domains/stock/actions/tally.jadpo";
const TALLY_TEST: &str = "domains/stock/actions/tally.test.jadpo";

fn project(name: &str, tests: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", APP);
    write(&root, COUNT, "type Count = Int {\n    min: 0\n}\n");
    write(
        &root,
        TALLY,
        "action tally(count: Count) -> Count {\n    return count\n}\n",
    );
    write(&root, TALLY_TEST, tests);
    root
}

#[test]
fn failed_reruns_the_last_runs_failures_from_anywhere_in_the_project() {
    let root = project(
        "failed",
        "test \"tallies one\" {\n    assert tally(1) == 1\n}\n\ntest \"tallies two\" {\n    assert tally(2) == 3\n}\n",
    );
    let deep = root.join("domains/stock");
    let none = jadpo(&deep, &["test", "--failed"]);
    assert!(
        none.stdout
            .starts_with("error: There's no last run to rerun\n"),
        "{}",
        none.stdout
    );
    let first = jadpo(&deep, &["test", "--seed", "1"]);
    assert_eq!(first.code, 1);
    assert!(
        first
            .stdout
            .contains("rerun: jadpo test domains/stock/actions/tally.test.jadpo:6 --seed 1\n"),
        "{}",
        first.stdout
    );
    assert!(root.join("build/test/last-run.json").is_file());
    let again = jadpo(&deep, &["test", "--failed", "--seed", "1"]);
    assert!(
        again.stdout.starts_with("FAIL  tallies two\n"),
        "{}",
        again.stdout
    );
    assert!(
        again
            .stdout
            .ends_with("test failed: 1 failed, 1 of 2 tests\n")
    );
    // The rerun command works as printed, from a folder inside the project.
    let rerun = jadpo(
        &deep,
        &[
            "test",
            "domains/stock/actions/tally.test.jadpo:6",
            "--seed",
            "1",
        ],
    );
    assert_eq!(rerun.stdout, again.stdout);
    write(
        &root,
        TALLY_TEST,
        "test \"tallies one\" {\n    assert tally(1) == 1\n}\n\ntest \"tallies two\" {\n    assert tally(2) == 2\n}\n",
    );
    let fixed = jadpo(&deep, &["test", "--failed"]);
    assert_eq!(fixed.stdout, "tested: 1 passed, 1 of 2 tests\n");
    let nothing = jadpo(&deep, &["test", "--failed"]);
    assert_eq!(
        nothing.stdout,
        "nothing to rerun: no test failed last time\n"
    );
    assert_eq!(nothing.code, 0);
    assert_eq!(
        jadpo(&root, &["test"]).stdout,
        "tested: 2 passed, 2 action tests\n"
    );
}

#[test]
fn outside_a_project_it_asks_for_one() {
    let root = folder("nowhere");
    let output = jadpo(&root, &["test"]);
    assert_eq!(output.code, 1);
    assert!(
        output.stdout.starts_with("error: No Jadpo project here\n"),
        "{}",
        output.stdout
    );
}

#[test]
fn command_line_mistakes_name_the_fix() {
    let root = loose("mistakes", &[ADDS]);
    let kind = jadpo(&root, &["test", ".", "--kind", "pur"]);
    assert_eq!(kind.code, 2);
    assert_eq!(kind.stdout, "");
    assert_eq!(
        kind.stderr,
        "\
error: `pur` isn't a kind of test
  reason: A test is `pure`, `action`, `route` or `schedule`. Did you mean `pure`?
  next: Use `--kind pure` (agent)
  rule: cli.test_arguments
  help: docs/language/14-tooling.html#command-errors
"
    );
    let seed = jadpo(&root, &["test", ".", "--seed", "soon"]);
    assert_eq!(seed.code, 2);
    assert!(
        seed.stderr
            .starts_with("error: `soon` isn't an order seed\n"),
        "{}",
        seed.stderr
    );
    let name = jadpo(&root, &["test", ".", "--name"]);
    assert_eq!(name.code, 2);
    assert_eq!(
        name.stderr,
        "\
error: `--name` needs a text
  reason: `--name` takes a text.
  next: Give it after the flag: `jadpo test --name <text>` (agent)
  rule: cli.test_arguments
  help: docs/language/14-tooling.html#command-errors
"
    );
    let missing = jadpo(&root, &["test", "elsewhere.test.jadpo"]);
    assert_eq!(missing.code, 2);
    assert!(
        missing
            .stderr
            .starts_with("error: `elsewhere.test.jadpo` doesn't exist\n"),
        "{}",
        missing.stderr
    );
    let folder_line = jadpo(&root, &["test", ".:3"]);
    assert_eq!(folder_line.code, 2);
    assert!(
        folder_line
            .stderr
            .starts_with("error: `.:3` names a line in a folder\n"),
        "{}",
        folder_line.stderr
    );
}

#[test]
fn help_lists_the_flags_with_examples() {
    let output = jadpo(Path::new("."), &["help", "test"]);
    assert_eq!(output.code, 0);
    assert!(
        output.stdout.starts_with("jadpo test\n"),
        "{}",
        output.stdout
    );
    for line in [
        "  --name        Run tests whose names contain this text\n",
        "  --kind        Run one kind: pure, action, route, schedule\n",
        "  --failed      Rerun the tests that failed last time\n",
        "  --seed        Run in the order this seed gives\n",
        "    jadpo test --failed\n",
    ] {
        assert!(output.stdout.contains(line), "{line}\n{}", output.stdout);
    }
}

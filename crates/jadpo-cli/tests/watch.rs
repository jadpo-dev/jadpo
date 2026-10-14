//! `jadpo test --watch` (area 13, "Running tests"; area 14; docs/compiler-
//! runtime.md §2.4 and §2.7): a save reruns exactly the tests that reach
//! what changed, through the graph's digests and edges, on one long-lived
//! database that rechecks only what the save touched; a broken build shows
//! its errors and runs nothing, and the next good save runs what changed
//! since the last run. Every screen's plain and rich output and the
//! `--json` stream are compared exactly through `Watcher`, with no terminal
//! or clock; the save-to-results latency is measured through the real
//! loop on a generated project of 230 files and 312 tests.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jadpo_cli::watch::{Frame, WatchArgs, Watcher, serve_to};
use jadpo_cli::{Session, run};
use jadpo_db::Database;
use jadpo_term::{ColorChoice, Environment};
use jadpo_test::Options;
use jadpo_test::watch::{Edit, EditKind, Event, Watch};
use jadpo_test::{Ran, Run};

/// The generous ceiling on one save's results in the gate (§2.7): far
/// above the 1-second target, so a slow CI machine never flakes, and far
/// below what a full recheck and rerun would take on a large project.
const CEILING: Duration = Duration::from_secs(5);

const DOMAINS: [&str; 12] = [
    "north", "south", "east", "west", "harbour", "meadow", "quarry", "orchard", "valley", "summit",
    "river", "forest",
];

/// A fresh folder for one test under cargo's temporary directory.
mod support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("watch-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).expect("read")
}

/// Replaces `from` with `to` in a file, which must contain it.
fn edit(root: &Path, path: &str, from: &str, to: &str) {
    let text = read(root, path);
    assert!(text.contains(from), "{path} has no `{from}`");
    write(root, path, &text.replacen(from, to, 1));
}

/// A project of `domains` domains, each with an amount type, a library of
/// `steps` functions where each step calls the one before, `actions`
/// actions calling a step and taking the shared `Count`, and tests beside
/// every library and action: 2 + domains × (3 + 2 × actions) files and
/// domains × (steps + 2 × actions) tests.
fn generate(root: &Path, domains: usize, steps: usize, actions: usize) {
    write(root, "app.jadpo", "application {}\n");
    write(
        root,
        "shared/types/count.jadpo",
        "type Count = Int {\n    min: 0\n}\n",
    );
    for d in DOMAINS.iter().take(domains) {
        let ty = format!("{}{}Amount", d[..1].to_uppercase(), &d[1..]);
        write(
            root,
            &format!("domains/{d}/types/{d}_amount.jadpo"),
            &format!("type {ty} = Int {{\n    min: 0\n}}\n"),
        );
        let functions: Vec<String> = (0..steps)
            .map(|k| {
                let body = if k == 0 {
                    format!("return {ty}(a + b)")
                } else {
                    format!("return {ty}({d}_step_{}(a, b) + 1)", k - 1)
                };
                format!(
                    "    function {d}_step_{k}(a: {ty}, b: {ty}) -> {ty} {{\n        {body}\n    }}\n"
                )
            })
            .collect();
        write(
            root,
            &format!("domains/{d}/libraries/{d}_maths.jadpo"),
            &format!("library {d}_maths {{\n{}}}\n", functions.join("\n")),
        );
        let tests: Vec<String> = (0..steps)
            .map(|k| {
                format!(
                    "test \"{d} step {k} adds\" {{\n    assert {d}_maths.{d}_step_{k}(1, 2) == {}\n}}\n",
                    3 + k
                )
            })
            .collect();
        write(
            root,
            &format!("domains/{d}/libraries/{d}_maths.test.jadpo"),
            &tests.join("\n"),
        );
        for j in 0..actions {
            let k = if j == 0 { steps - 1 } else { j % steps };
            write(
                root,
                &format!("domains/{d}/actions/{d}_settle_{j}.jadpo"),
                &format!(
                    "action {d}_settle_{j}(x: {ty}, n: Count) -> {ty} {{\n    return {d}_maths.{d}_step_{k}(x, 1)\n}}\n"
                ),
            );
            write(
                root,
                &format!("domains/{d}/actions/{d}_settle_{j}.test.jadpo"),
                &format!(
                    "test \"{d} settles {j}\" {{\n    var got: {ty} = {d}_settle_{j}(1, 0)\n    assert got == {}\n}}\n\ntest \"{d} settles {j} from zero\" {{\n    var got: {ty} = {d}_settle_{j}(0, 0)\n    assert got == {}\n}}\n",
                    2 + k,
                    1 + k
                ),
            );
        }
    }
}

/// The large fixture: 12 domains, 230 files, 312 tests.
fn large(name: &str) -> PathBuf {
    let root = folder(name);
    generate(&root, 12, 10, 8);
    root
}

fn options(root: &Path) -> Options {
    Options {
        cwd: root.to_path_buf(),
        seed: Some(1),
        ..Options::default()
    }
}

fn run_of(event: &Event) -> &Run {
    match &event.ran {
        Some(Ran::Done(run)) => run,
        other => panic!("expected a run, got {other:?}"),
    }
}

/// The names of the tests a step ran, sorted.
fn ran(event: &Event) -> Vec<String> {
    let mut names: Vec<String> = run_of(event)
        .results
        .iter()
        .map(|result| result.test.name.clone())
        .collect();
    names.sort();
    names
}

fn saved(path: &str) -> Vec<Edit> {
    vec![Edit::new(path, EditKind::Saved)]
}

/// How many callables' bodies were checked again.
fn bodies(executions: &[String]) -> usize {
    executions
        .iter()
        .filter(|query| query.starts_with("typed_body("))
        .count()
}

/// How many times the per-file name resolution ran.
fn resolved_files(executions: &[String]) -> usize {
    executions
        .iter()
        .filter(|query| query.starts_with("resolve_file("))
        .count()
}

const LIBRARY: &str = "domains/north/libraries/north_maths.jadpo";
const LIBRARY_TESTS: &str = "domains/north/libraries/north_maths.test.jadpo";
const ACTION: &str = "domains/north/actions/north_settle_3.jadpo";

/// The tests in `domain` that reach its first step: every step's test and
/// both tests of every action.
fn whole_domain(domain: &str) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = (0..10).map(|k| format!("{domain} step {k} adds")).collect();
    for j in 0..8 {
        names.insert(format!("{domain} settles {j}"));
        names.insert(format!("{domain} settles {j} from zero"));
    }
    names
}

#[test]
fn a_save_reruns_exactly_the_tests_that_reach_it() {
    let root = large("reach");
    let (mut watch, first) =
        Watch::start_with(options(&root), Database::recording()).expect("starts");
    assert_eq!(first.reached, None);
    assert_eq!(run_of(&first).results.len(), 312);
    assert_eq!(first.files, 230);
    watch.executions();

    // The first step every other step builds on: the whole domain reaches
    // it, and only the edited file's ten bodies are checked again.
    edit(&root, LIBRARY, "(a + b)", "(b + a)");
    let started = Instant::now();
    let event = watch.apply(saved(LIBRARY)).expect("a change");
    // Printed with `--nocapture`; the counts are asserted, not the time.
    println!(
        "recheck and rerun after a library save: {} ms",
        started.elapsed().as_millis()
    );
    let executions = watch.executions();
    assert_eq!(event.reached, Some(26));
    assert_eq!(
        ran(&event).into_iter().collect::<BTreeSet<_>>(),
        whole_domain("north")
    );
    assert_eq!(
        (bodies(&executions), resolved_files(&executions)),
        (10, 1),
        "{executions:?}"
    );
    assert_eq!(event.edits, saved(LIBRARY));

    // The last step: its own test and the action that calls it.
    edit(
        &root,
        LIBRARY,
        "(north_step_8(a, b) + 1)",
        "(1 + north_step_8(a, b))",
    );
    let event = watch.apply(saved(LIBRARY)).expect("a change");
    assert_eq!(
        ran(&event),
        [
            "north settles 0",
            "north settles 0 from zero",
            "north step 9 adds"
        ]
    );
    assert_eq!(bodies(&watch.executions()), 10);

    // One action: its two tests; one body checked again.
    edit(&root, ACTION, "(x, 1)", "(1, x)");
    let event = watch.apply(saved(ACTION)).expect("a change");
    assert_eq!(
        ran(&event),
        ["north settles 3", "north settles 3 from zero"]
    );
    assert_eq!(bodies(&watch.executions()), 1);

    // One test: itself.
    edit(
        &root,
        LIBRARY_TESTS,
        "north_step_4(1, 2) == 7",
        "north_step_4(2, 1) == 7",
    );
    let event = watch.apply(saved(LIBRARY_TESTS)).expect("a change");
    assert_eq!(ran(&event), ["north step 4 adds"]);

    // A comment changes no digest, so nothing reaches it.
    edit(&root, ACTION, "{\n", "{\n    // Settles one.\n");
    let event = watch.apply(saved(ACTION)).expect("a change");
    assert_eq!((event.reached, event.ran.is_none()), (Some(0), true));

    // A save of the same text changes nothing at all.
    let text = read(&root, ACTION);
    write(&root, ACTION, &text);
    assert!(watch.apply(saved(ACTION)).is_none());
    assert!(watch.apply(saved("notes.txt")).is_none());

    // The shared type every action takes: every action test.
    edit(
        &root,
        "shared/types/count.jadpo",
        "    min: 0\n",
        "    min: 0\n    max: 1000\n",
    );
    let event = watch
        .apply(saved("shared/types/count.jadpo"))
        .expect("a change");
    assert_eq!(event.reached, Some(12 * 16));
    assert!(ran(&event).iter().all(|name| name.contains("settles")));
}

#[test]
fn a_broken_build_runs_nothing_and_the_fix_runs_what_changed_since() {
    let root = large("broken");
    let (mut watch, _) = Watch::start(options(&root)).expect("starts");
    edit(
        &root,
        ACTION,
        "north_maths.north_step_3",
        "north_mths.north_step_3",
    );
    let broken = watch.apply(saved(ACTION)).expect("a change");
    assert_eq!(broken.reached, None);
    match &broken.ran {
        Some(Ran::Refused { diagnostics, .. }) => {
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(diagnostics[0].rule.id, "semantic.unknown_name");
        }
        other => panic!("expected refused, got {other:?}"),
    }
    assert_eq!(broken.edits, saved(ACTION));

    // While it is broken, another file changes; the fix reruns both.
    edit(
        &root,
        "domains/south/actions/south_settle_1.jadpo",
        "(x, 1)",
        "(1, x)",
    );
    let still = watch
        .apply(saved("domains/south/actions/south_settle_1.jadpo"))
        .expect("a change");
    assert!(matches!(still.ran, Some(Ran::Refused { .. })));
    edit(
        &root,
        ACTION,
        "north_mths.north_step_3(x, 1)",
        "north_maths.north_step_3(1, x)",
    );
    let fixed = watch.apply(saved(ACTION)).expect("a change");
    assert_eq!(
        ran(&fixed),
        [
            "north settles 3",
            "north settles 3 from zero",
            "south settles 1",
            "south settles 1 from zero"
        ]
    );
    assert_eq!(
        fixed.edits,
        vec![
            Edit::new(ACTION, EditKind::Saved),
            Edit::new(
                "domains/south/actions/south_settle_1.jadpo",
                EditKind::Saved
            ),
        ]
    );

    // A fix back to exactly what last ran reruns nothing.
    edit(
        &root,
        ACTION,
        "north_maths.north_step_3",
        "north_mths.north_step_3",
    );
    watch.apply(saved(ACTION)).expect("a change");
    edit(
        &root,
        ACTION,
        "north_mths.north_step_3",
        "north_maths.north_step_3",
    );
    let same = watch.apply(saved(ACTION)).expect("a change");
    assert_eq!((same.reached, same.ran.is_none()), (Some(0), true));
}

#[test]
fn files_added_and_removed_and_failures_are_tracked() {
    let root = large("files");
    let (mut watch, _) = Watch::start(options(&root)).expect("starts");
    let action = "domains/north/actions/north_settle_9.jadpo";
    let tests = "domains/north/actions/north_settle_9.test.jadpo";
    write(
        &root,
        action,
        "action north_settle_9(x: NorthAmount, n: Count) -> NorthAmount {\n    return north_maths.north_step_2(x, 1)\n}\n",
    );
    write(
        &root,
        tests,
        "test \"north settles nine\" {\n    assert north_settle_9(1, 0) == 5\n}\n",
    );
    let added = vec![
        Edit::new(action, EditKind::Added),
        Edit::new(tests, EditKind::Added),
    ];
    let event = watch.apply(added.clone()).expect("a change");
    assert_eq!(ran(&event), ["north settles nine"]);
    assert_eq!(event.edits, added);
    assert_eq!((event.failing, event.files), (1, 232));
    assert_eq!(run_of(&event).total(), 313);

    fs::remove_file(root.join(tests)).expect("remove");
    fs::remove_file(root.join(action)).expect("remove");
    let event = watch
        .apply(vec![
            Edit::new(action, EditKind::Removed),
            Edit::new(tests, EditKind::Removed),
        ])
        .expect("a change");
    assert_eq!(
        (event.reached, event.failing, event.files),
        (Some(0), 0, 230)
    );
}

#[test]
fn a_rule_on_a_field_reaches_the_tests_using_its_record() {
    let root = folder("field");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/plots/types/size.jadpo",
        "type Size = Int {\n    min: 0\n}\n",
    );
    write(
        &root,
        "domains/plots/types/corner.jadpo",
        "type Corner = Object {\n    x: Size\n    y: Size\n}\n",
    );
    write(
        &root,
        "domains/plots/actions/shift.jadpo",
        "action shift(corner: Corner) -> Corner {\n    return corner\n}\n",
    );
    write(
        &root,
        "domains/plots/actions/shift.test.jadpo",
        "test \"a corner stays put\" {\n    var start: Corner = Corner {\n        x: 1\n        y: 2\n    }\n    assert shift(start) == start\n}\n",
    );
    let (mut watch, first) = Watch::start(options(&root)).expect("starts");
    assert_eq!(run_of(&first).results.len(), 1);
    edit(
        &root,
        "domains/plots/types/size.jadpo",
        "    min: 0\n",
        "    min: 0\n    max: 99\n",
    );
    let event = watch
        .apply(saved("domains/plots/types/size.jadpo"))
        .expect("a change");
    assert_eq!(ran(&event), ["a corner stays put"]);
}

#[test]
fn filters_given_with_watch_narrow_every_rerun() {
    let root = large("filters");
    let (mut watch, first) = Watch::start(Options {
        name: Some("south".into()),
        ..options(&root)
    })
    .expect("starts");
    assert_eq!(run_of(&first).results.len(), 26);
    edit(&root, LIBRARY, "(a + b)", "(b + a)");
    let event = watch.apply(saved(LIBRARY)).expect("a change");
    assert_eq!((event.reached, event.ran.is_none()), (Some(0), true));
    let south = "domains/south/libraries/south_maths.jadpo";
    edit(&root, south, "(a + b)", "(b + a)");
    let event = watch.apply(saved(south)).expect("a change");
    assert_eq!(
        ran(&event).into_iter().collect::<BTreeSet<_>>(),
        whole_domain("south")
    );

    // A path narrows to the tests under it.
    let (mut watch, first) = Watch::start(Options {
        target: Some("domains/north/actions".into()),
        ..options(&root)
    })
    .expect("starts");
    assert_eq!(run_of(&first).results.len(), 16);
    edit(&root, LIBRARY, "(b + a)", "(a + b)");
    let event = watch.apply(saved(LIBRARY)).expect("a change");
    assert_eq!(event.reached, Some(16));
}

// ---------------------------------------------------------------------
// Screens, through `Watcher`: every state's exact output.

/// Two domains with two steps and one action each: 12 files, 8 tests.
fn small(name: &str) -> PathBuf {
    let root = folder(name);
    generate(&root, 2, 2, 1);
    root
}

fn args(root: &Path, json: bool) -> WatchArgs {
    WatchArgs {
        options: options(root),
        json,
        choice: ColorChoice::Auto,
    }
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

fn stripped(frame: Frame) -> Frame {
    Frame {
        erase: frame.erase,
        stdout: support::without_run_time(&strip_ansi(&frame.stdout)),
        stderr: strip_ansi(&frame.stderr),
    }
}

fn frame(erase: bool, stdout: &str, stderr: &str) -> Frame {
    Frame {
        erase,
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    }
}

const SMALL_LIBRARY: &str = "domains/north/libraries/north_maths.jadpo";
const SMALL_TESTS: &str = "domains/north/libraries/north_maths.test.jadpo";

/// Every state in order: the first run, a rerun that passes, one that
/// fails, a save no test reaches, a broken build and its fix.
fn states(env: Environment, root: &Path) -> Vec<Frame> {
    let args = args(root, false);
    let mut frames = vec![Watcher::header(&args, &env, "14:01:58")];
    let (mut watcher, first) = Watcher::start(&args, &env, Database::default()).expect("starts");
    frames.push(first);
    let mut step = |path: &str, from: &str, to: &str, time: &str| {
        edit(root, path, from, to);
        frames.push(watcher.update(saved(path), time).expect("a change"));
    };
    step(SMALL_LIBRARY, "(a + b)", "(b + a)", "14:02:31");
    step(SMALL_TESTS, "== 4", "== 5", "14:03:05");
    step(
        "domains/north/types/north_amount.jadpo",
        "{\n",
        "{\n    // Pence.\n",
        "14:03:40",
    );
    step(
        SMALL_LIBRARY,
        "north_step_0(a, b)",
        "north_stp_0(a, b)",
        "14:04:02",
    );
    step(
        SMALL_LIBRARY,
        "north_stp_0(a, b)",
        "north_step_0(b, a)",
        "14:04:30",
    );
    step(SMALL_TESTS, "== 5", "== 4", "14:05:12");
    // Two files in one burst.
    edit(
        root,
        "domains/north/actions/north_settle_0.jadpo",
        "(x, 1)",
        "(1, x)",
    );
    edit(
        root,
        "domains/south/actions/south_settle_0.jadpo",
        "(x, 1)",
        "(1, x)",
    );
    frames.push(
        watcher
            .update(
                vec![
                    Edit::new(
                        "domains/north/actions/north_settle_0.jadpo",
                        EditKind::Saved,
                    ),
                    Edit::new(
                        "domains/south/actions/south_settle_0.jadpo",
                        EditKind::Saved,
                    ),
                ],
                "14:06:00",
            )
            .expect("a change"),
    );
    frames
}

#[test]
fn every_state_reads_plainly_when_piped() {
    let root = small("plain");
    let expected = [
        frame(false, "jadpo test --watch, started 14:01:58\n", ""),
        frame(
            false,
            r#"
tested: 8 passed, 4 pure tests, 4 action tests
"#,
            "watching: 12 files, press ctrl-c to quit\n",
        ),
        frame(
            false,
            r#"
watch: 14:02:31, saved 1 file, 4 tests reach it
  ~ domains/north/libraries/north_maths.jadpo

tested: 4 passed, 4 of 8 tests
"#,
            "",
        ),
        frame(
            false,
            r#"
watch: 14:03:05, saved 1 file, 1 test reaches it
  ~ domains/north/libraries/north_maths.test.jadpo

FAIL  north step 1 adds
 --> domains/north/libraries/north_maths.test.jadpo:6

  assert north_maths.north_step_1(1, 2) == 5
         |
         4

  expected: 5
  actual: 4

  rule: test.assertion_failed
  rerun: jadpo test domains/north/libraries/north_maths.test.jadpo:6 --seed 1

test failed: 1 failed, 1 of 8 tests
"#,
            "watching: 12 files, 1 failing, press ctrl-c to quit\n",
        ),
        frame(
            false,
            r#"
watch: 14:03:40, saved domains/north/types/north_amount.jadpo, nothing to rerun
"#,
            "",
        ),
        frame(
            false,
            r#"
watch: 14:04:02, saved domains/north/libraries/north_maths.jadpo

error: `north_step_0` isn't used
 --> domains/north/libraries/north_maths.jadpo:2:14
  |
2 |     function north_step_0(a: NorthAmount, b: NorthAmount) -> NorthAmount {
  |              ^^^^^^^^^^^^ nothing uses this
  reason: A library function exists to be used by several files; one that nothing uses is dead code. Tests don't count as users.
  next: Remove `north_step_0` (agent)
  rule: layout.unused_declaration
  help: docs/language/01-project-layout-and-modules.html#layout-unused-declaration

error: `north_stp_0` isn't a function, action or query
 --> domains/north/libraries/north_maths.jadpo:7:28
  |
7 |         return NorthAmount(north_stp_0(a, b) + 1)
  |                            ^^^^^^^^^^^ not declared
  reason: Nothing in this project called `north_stp_0` can be called.
  next: Declare `north_stp_0`, or call an existing function (agent)
  rule: semantic.unknown_callee
  help: docs/language/01-project-layout-and-modules.html#semantic-unknown-callee

tests not run: 2 errors, 12 files
"#,
            "",
        ),
        frame(
            false,
            r#"
watch: 14:04:30, saved 1 file, 3 tests reach it
  ~ domains/north/libraries/north_maths.jadpo

FAIL  north step 1 adds
 --> domains/north/libraries/north_maths.test.jadpo:6

  assert north_maths.north_step_1(1, 2) == 5
         |
         4

  expected: 5
  actual: 4

  rule: test.assertion_failed
  rerun: jadpo test domains/north/libraries/north_maths.test.jadpo:6 --seed 1

test failed: 1 failed, 2 passed, 3 of 8 tests
"#,
            "",
        ),
        frame(
            false,
            r#"
watch: 14:05:12, saved 1 file, 1 test reaches it
  ~ domains/north/libraries/north_maths.test.jadpo

tested: 1 passed, 1 of 8 tests
"#,
            "watching: 12 files, press ctrl-c to quit\n",
        ),
        frame(
            false,
            r#"
watch: 14:06:00, 2 files changed, 4 tests reach it
  ~ domains/north/actions/north_settle_0.jadpo
  ~ domains/south/actions/south_settle_0.jadpo

tested: 4 passed, 4 of 8 tests
"#,
            "",
        ),
    ];
    let mut frames = states(Environment::empty(), &root);
    for frame in &mut frames {
        frame.stdout = support::without_run_time(&frame.stdout);
    }
    assert_eq!(frames.len(), expected.len());
    for (i, (got, want)) in frames.into_iter().zip(expected).enumerate() {
        assert_eq!(got, want, "frame {i}:\n{}", got.stdout);
    }
}

#[test]
fn every_state_in_a_terminal() {
    let root = small("rich");
    let expected = [
        frame(false, "jadpo test --watch · started 14:01:58\n", ""),
        frame(
            false,
            r#"
✓ 8 passed  4 pure tests · 4 action tests
"#,
            r#"
watching 12 files  ctrl-c quit"#,
        ),
        frame(
            true,
            r#"── 14:02:31 · saved 1 file · 4 tests reach it ──
  ~ domains/north/libraries/north_maths.jadpo

✓ 4 passed  4 of 8 tests
"#,
            r#"
watching 12 files  ctrl-c quit"#,
        ),
        frame(
            true,
            r#"── 14:03:05 · saved 1 file · 1 test reaches it ──
  ~ domains/north/libraries/north_maths.test.jadpo

✗ failed  north step 1 adds
  domains/north/libraries/north_maths.test.jadpo:6

  assert north_maths.north_step_1(1, 2) == 5
         │
         4

  expected  5
  actual    4

  rule      test.assertion_failed
  rerun     jadpo test domains/north/libraries/north_maths.test.jadpo:6 --seed 1

✗ 1 failed  1 of 8 tests
"#,
            r#"
watching 12 files · 1 failing  ctrl-c quit"#,
        ),
        frame(
            true,
            r#"── 14:03:40 · saved domains/north/types/north_amount.jadpo · nothing to rerun ──
"#,
            r#"
watching 12 files · 1 failing  ctrl-c quit"#,
        ),
        frame(
            true,
            r#"── 14:04:02 · saved domains/north/libraries/north_maths.jadpo ──

✗ error  `north_step_0` isn't used
  domains/north/libraries/north_maths.jadpo:2:14
    │
  2 │     function north_step_0(a: NorthAmount, b: NorthAmount) -> NorthAmount {
    │              ^^^^^^^^^^^^ nothing uses this
    │
  reason    A library function exists to be used by several files; one that
            nothing uses is dead code. Tests don't count as users.
  next      Remove `north_step_0` (agent)
  rule      layout.unused_declaration
  help      docs/language/01-project-layout-and-modules.html#layout-unused-declaration

✗ error  `north_stp_0` isn't a function, action or query
  domains/north/libraries/north_maths.jadpo:7:28
    │
  7 │         return NorthAmount(north_stp_0(a, b) + 1)
    │                            ^^^^^^^^^^^ not declared
    │
  reason    Nothing in this project called `north_stp_0` can be called.
  next      Declare `north_stp_0`, or call an existing function (agent)
  rule      semantic.unknown_callee
  help      docs/language/01-project-layout-and-modules.html#semantic-unknown-callee

✗ tests not run  2 errors · 12 files
"#,
            r#"
watching 12 files · 1 failing  ctrl-c quit"#,
        ),
        frame(
            true,
            r#"── 14:04:30 · saved 1 file · 3 tests reach it ──
  ~ domains/north/libraries/north_maths.jadpo

✗ failed  north step 1 adds
  domains/north/libraries/north_maths.test.jadpo:6

  assert north_maths.north_step_1(1, 2) == 5
         │
         4

  expected  5
  actual    4

  rule      test.assertion_failed
  rerun     jadpo test domains/north/libraries/north_maths.test.jadpo:6 --seed 1

✗ 1 failed  2 passed · 3 of 8 tests
"#,
            r#"
watching 12 files · 1 failing  ctrl-c quit"#,
        ),
        frame(
            true,
            r#"── 14:05:12 · saved 1 file · 1 test reaches it ──
  ~ domains/north/libraries/north_maths.test.jadpo

✓ 1 passed  1 of 8 tests
"#,
            r#"
watching 12 files  ctrl-c quit"#,
        ),
        frame(
            true,
            r#"── 14:06:00 · 2 files changed · 4 tests reach it ──
  ~ domains/north/actions/north_settle_0.jadpo
  ~ domains/south/actions/south_settle_0.jadpo

✓ 4 passed  4 of 8 tests
"#,
            r#"
watching 12 files  ctrl-c quit"#,
        ),
    ];
    let frames = states(terminal(), &root);
    // The rule is muted, the change mark in its colour, the failing count
    // in the failure colour, the key bold.
    assert!(
        frames[2].stdout.starts_with("\x1b[2m── 14:02:31"),
        "{:?}",
        frames[2].stdout
    );
    assert!(
        frames[2].stdout.contains("\x1b[33m~\x1b[0m"),
        "{:?}",
        frames[2].stdout
    );
    assert!(
        frames[3].stderr.contains("\x1b[31m1 failing\x1b[0m"),
        "{:?}",
        frames[3].stderr
    );
    assert!(
        frames[3].stderr.contains("\x1b[1mctrl-c\x1b[0m"),
        "{:?}",
        frames[3].stderr
    );
    assert_eq!(frames.len(), expected.len());
    for (i, (got, want)) in frames.into_iter().map(stripped).zip(expected).enumerate() {
        assert_eq!(got, want, "frame {i}:\n{}", got.stdout);
    }
}

#[test]
fn json_streams_one_envelope_per_line_and_nothing_on_stderr() {
    let root = small("json");
    let env = Environment::empty();
    let args = args(&root, true);
    assert_eq!(Watcher::header(&args, &env, "14:01:58"), Frame::default());
    let (mut watcher, first) = Watcher::start(&args, &env, Database::default()).expect("starts");
    let mut frames = vec![first];
    let mut step = |path: &str, from: &str, to: &str| {
        edit(&root, path, from, to);
        frames.push(watcher.update(saved(path), "14:02:31").expect("a change"));
    };
    step(SMALL_TESTS, "== 4", "== 5");
    step(
        "domains/north/types/north_amount.jadpo",
        "{\n",
        "{\n    // Pence.\n",
    );
    step(SMALL_LIBRARY, "north_step_0(a, b)", "north_stp_0(a, b)");
    let documents: Vec<serde_json::Value> = frames
        .iter()
        .map(|frame| {
            assert!(!frame.erase);
            assert_eq!(frame.stderr, "");
            assert_eq!(frame.stdout.matches('\n').count(), 1, "{}", frame.stdout);
            assert!(frame.stdout.ends_with('\n'));
            serde_json::from_str(&frame.stdout).expect("one JSON object")
        })
        .collect();
    for document in &documents {
        assert_eq!(document["schemaVersion"], 2);
        assert_eq!(document["command"], "test");
    }
    let start = &documents[0];
    assert_eq!(start["ok"], true);
    assert_eq!(start["data"]["watch"]["event"], "start");
    assert_eq!(start["data"]["watch"]["reached"], serde_json::Value::Null);
    assert_eq!(start["data"]["watch"]["files"], 12);
    assert_eq!(start["data"]["counts"]["ran"], 8);

    let failed = &documents[1];
    assert_eq!(failed["ok"], false);
    assert_eq!(failed["data"]["watch"]["event"], "change");
    assert_eq!(
        failed["data"]["watch"]["changed"],
        serde_json::json!([{ "path": SMALL_TESTS, "change": "saved" }])
    );
    assert_eq!(failed["data"]["watch"]["reached"], 1);
    assert_eq!(failed["data"]["watch"]["failing"], 1);
    assert_eq!(
        failed["data"]["tests"][0]["failure"]["code"],
        "test.assertion_failed"
    );
    assert_eq!(failed["diagnostics"][0]["rule"], "test.failed");

    let nothing = &documents[2];
    assert_eq!(nothing["data"]["watch"]["reached"], 0);
    assert_eq!(nothing["data"]["tests"], serde_json::json!([]));

    let broken = &documents[3];
    assert_eq!(broken["ok"], false);
    assert_eq!(broken["data"]["ran"], false);
    assert_eq!(broken["data"]["watch"]["reached"], serde_json::Value::Null);
    assert_eq!(broken["diagnostics"][1]["rule"], "semantic.unknown_callee");
}

fn jadpo(cwd: &Path, args: &[&str]) -> jadpo_cli::Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

#[test]
fn the_command_starts_a_session_or_ends_on_a_mistake() {
    let root = small("command");
    let output = jadpo(&root, &["test", "--watch", "--name", "north"]);
    assert_eq!(
        (output.stdout.as_str(), output.stderr.as_str(), output.code),
        ("", "", 0)
    );
    match output.session {
        Some(Session::Watch(args)) => {
            assert_eq!(args.options.name.as_deref(), Some("north"));
            assert!(!args.json);
        }
        other => panic!("expected a watch session, got {other:?}"),
    }
    match jadpo(&root, &["test", "--watch", "--json"]).session {
        Some(Session::Watch(args)) => assert!(args.json),
        other => panic!("expected a watch session, got {other:?}"),
    }
    // A path that doesn't exist, or a kind not built, ends it at once.
    let missing = jadpo(&root, &["test", "elsewhere.test.jadpo", "--watch"]);
    assert_eq!((missing.code, missing.session.is_none()), (2, true));
    assert!(
        missing
            .stderr
            .starts_with("error: `elsewhere.test.jadpo` doesn't exist\n"),
        "{}",
        missing.stderr
    );
    let route = jadpo(&root, &["test", "--watch", "--kind", "route"]);
    assert_eq!((route.code, route.session.is_none()), (1, true));
    let nowhere = jadpo(&folder("command-nowhere"), &["test", "--watch"]);
    assert_eq!((nowhere.code, nowhere.session.is_none()), (1, true));
    assert!(nowhere.stdout.starts_with("error: No Jadpo project here\n"));
}

#[test]
fn help_and_completion_offer_watch() {
    let help = jadpo(Path::new("."), &["help", "test"]);
    assert!(
        help.stdout
            .contains("  --watch       Rerun the tests each save reaches\n"),
        "{}",
        help.stdout
    );
    assert!(
        help.stdout.contains("    jadpo test --watch\n"),
        "{}",
        help.stdout
    );
    let document: serde_json::Value =
        serde_json::from_str(&jadpo(Path::new("."), &["help", "test", "--json"]).stdout)
            .expect("JSON");
    assert!(document.to_string().contains("\"--watch\""), "{document}");
    let completion = jadpo(Path::new("."), &["__complete", "--", "test", "--w"]);
    assert!(
        completion.stdout.contains("--watch"),
        "{}",
        completion.stdout
    );
}

// ---------------------------------------------------------------------
// Save to results, through the real loop.

/// Everything written to one stream, shared with the session's thread.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("capture").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Capture {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("capture")).into_owned()
    }

    fn len(&self) -> usize {
        self.0.lock().expect("capture").len()
    }

    /// Waits until the text written after byte `from` contains `wanted`.
    fn wait_for(&self, from: usize, wanted: &str, limit: Duration) -> String {
        let started = Instant::now();
        loop {
            let text = self.text();
            let after = &text[from.min(text.len())..];
            if after.contains(wanted) {
                return after.to_string();
            }
            assert!(
                started.elapsed() < limit,
                "`{wanted}` didn't appear within {limit:?}; got:\n{after}"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

#[test]
fn a_save_shows_the_tests_it_reaches_within_the_target() {
    let root = large("latency");
    let out = Capture::default();
    let err = Capture::default();
    let stop = Arc::new(AtomicBool::new(false));
    let session = {
        let (mut out, mut err, stop) = (out.clone(), err.clone(), Arc::clone(&stop));
        let args = args(&root, false);
        std::thread::spawn(move || {
            serve_to(
                &args,
                &Environment::empty(),
                &mut out,
                &mut err,
                &|| "12:00:00".to_string(),
                &|| stop.load(Ordering::Relaxed),
            )
        })
    };
    out.wait_for(0, "tested: 312 passed", Duration::from_secs(60));
    err.wait_for(0, "watching: 230 files", Duration::from_secs(5));
    // Each save: the file, what to change, and the footer its reruns end
    // in, which counts exactly the tests that reach it (a slow machine adds
    // the run's duration after it).
    let saves = [
        (
            LIBRARY,
            "(a + b)",
            "(b + a)",
            "tested: 26 passed, 26 of 312 tests",
        ),
        (
            ACTION,
            "(x, 1)",
            "(1, x)",
            "tested: 2 passed, 2 of 312 tests",
        ),
        (
            "domains/river/libraries/river_maths.test.jadpo",
            "river_step_9(1, 2) == 12",
            "river_step_9(2, 1) == 12",
            "tested: 1 passed, 1 of 312 tests",
        ),
        (
            LIBRARY,
            "(b + a)",
            "(a + b)",
            "tested: 26 passed, 26 of 312 tests",
        ),
    ];
    let mut times = Vec::new();
    for (path, from, to, footer) in saves {
        let before = out.len();
        let saved_at = Instant::now();
        edit(&root, path, from, to);
        let shown = out.wait_for(before, footer, CEILING * 2);
        let took = saved_at.elapsed();
        assert!(shown.contains("watch: 12:00:00, "), "{shown}");
        times.push(took);
        println!("save {path} → results in {} ms", took.as_millis());
    }
    stop.store(true, Ordering::Relaxed);
    assert_eq!(session.join().expect("the session ends"), 0);
    let slowest = times.iter().max().copied().unwrap_or_default();
    assert!(
        slowest < CEILING,
        "a save took {} ms to show its results",
        slowest.as_millis()
    );
}

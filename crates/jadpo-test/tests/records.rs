//! The outcome-and-effect record (roadmap chunk 8.1c;
//! docs/compiler-runtime.md §2.5, "Differential test"): `jadpo test` keeps
//! each test's record in its canonical form, the same every time a test
//! runs, and `jadpo_test::differential` names the first line two runs'
//! records differ at.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use jadpo_test::{Options, Ran, Run, differential};

use crate::syntax_examples as examples;

/// A new, empty folder of its own under the system's temporary folder,
/// removed when dropped.
struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Folder {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "jadpo-records-{}-{}-{name}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Folder(path)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Copies the folder `from` into `to`, leaving out `build/`.
fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.file_name().is_some_and(|name| name == "build") {
            continue;
        }
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

/// A copy of teaching example `name`, with its migrations.
fn example(name: &str) -> Folder {
    let folder = Folder::new(name);
    copy(&examples::root().join("examples").join(name), &folder.0);
    folder
}

/// Every test in the project at `folder`, run with records, seed 1; `None`
/// when nothing ran (no tests, errors, nothing to run).
fn recorded(folder: &Path) -> Option<Box<Run>> {
    let ran = jadpo_test::run(Options {
        cwd: folder.to_path_buf(),
        target: Some(".".into()),
        seed: Some(1),
        record: true,
        ..Options::default()
    });
    match ran {
        Ran::Done(run) => Some(run),
        _ => None,
    }
}

/// Replaces `from` with `to` in the file at `path`, which must hold it.
fn perturb(path: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains(from), "{} holds `{from}`", path.display());
    std::fs::write(path, text.replacen(from, to, 1)).unwrap();
}

/// A run keeps a record of each test only when asked, and each record
/// ends with how its test ended.
#[test]
fn a_run_keeps_records_only_when_asked() {
    let folder = example("01-first-project");
    let run = recorded(&folder.0).expect("it runs");
    assert!(!run.results.is_empty());
    for result in &run.results {
        let record = result.record.as_ref().expect("a record");
        assert!(
            record.lines().iter().any(|line| line == "test passed"),
            "{:?}",
            record.lines()
        );
        assert!(
            record
                .lines()
                .iter()
                .any(|line| line.starts_with("outcome value")),
            "{:?}",
            record.lines()
        );
    }
    let ran = jadpo_test::run(Options {
        cwd: folder.0.clone(),
        target: Some(".".into()),
        seed: Some(1),
        ..Options::default()
    });
    let Ran::Done(run) = ran else {
        panic!("it runs")
    };
    assert!(run.results.iter().all(|result| result.record.is_none()));
}

/// A change to what a test does shows as the first line its record
/// differs at: here a failed assertion's values, where the run before
/// passed.
#[test]
fn a_perturbed_run_is_named_at_its_first_difference() {
    let folder = example("01-first-project");
    let before = recorded(&folder.0).expect("it runs");
    perturb(
        &folder.0.join("domains/baking/actions/count_rolls.jadpo"),
        "RollCount(total + 5)",
        "RollCount(total + 6)",
    );
    let after = recorded(&folder.0).expect("it runs");
    let difference = differential(&before, &after).unwrap_err();
    assert!(
        difference
            .test
            .ends_with("\"four trays come with 5 extra rolls\""),
        "{difference}"
    );
    let first = difference.first.as_deref().unwrap_or_default();
    let second = difference.second.as_deref().unwrap_or_default();
    assert!(first.starts_with("outcome value"), "{difference}");
    assert!(
        second.starts_with("outcome assertion at domains/baking/actions/count_rolls.test.jadpo:")
            && second.contains("1006 == 1005"),
        "{difference}"
    );
    let shown = difference.to_string();
    assert!(shown.contains("  first:  outcome value"), "{shown}");
    assert!(shown.contains("  second: outcome assertion"), "{shown}");
}

/// A loose project of `files` in a folder of its own.
fn written(name: &str, files: &[(&str, &str)]) -> Folder {
    let folder = Folder::new(name);
    for (path, text) in files {
        let target = folder.0.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, text).unwrap();
    }
    folder
}

/// The project named `name` among the language pages', written into a
/// folder of its own.
fn page(name: &str) -> Folder {
    let files = examples::projects()
        .into_iter()
        .find(|(project, _)| project == name)
        .map(|(_, files)| files)
        .unwrap_or_else(|| panic!("{name} is a page's project"));
    let folder = Folder::new("page");
    for (path, source) in &files {
        let target = folder.0.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, source).unwrap();
    }
    folder
}

/// The first difference between `folder`'s records before and after
/// `change` changes it, which must be one.
fn changed(folder: &Folder, change: impl FnOnce(&Path)) -> Box<jadpo_test::Difference> {
    let before = recorded(&folder.0).expect("it runs");
    change(&folder.0);
    let after = recorded(&folder.0).expect("it runs");
    differential(&before, &after).expect_err("the records differ")
}

/// Where a failure was raised is in its record, so the same failure
/// raised elsewhere differs there, though its code and fields don't
/// (chunk 8.4a).
#[test]
fn a_failure_raised_elsewhere_is_named_at_its_site() {
    let folder = written(
        "failure-site",
        &[
            (
                "main.jadpo",
                "type Count = Int {}\n\ntype TooMany = Failure {\n    kind: FailureKind.rejected\n}\n\nfunction checked(count: Count) fails TooMany -> Count {\n    if (count > 9) {\n        reject TooMany\n    }\n    if (count > 4) {\n        reject TooMany\n    }\n    return count\n}\n",
            ),
            (
                "main.test.jadpo",
                "test \"a large count fails\" {\n    var count: Count = attempt checked(12)\n    assert count == 12\n}\n",
            ),
        ],
    );
    let difference = changed(&folder, |root| {
        perturb(&root.join("main.test.jadpo"), "checked(12)", "checked(7)");
    });
    let (first, second) = (
        difference.first.clone().unwrap_or_default(),
        difference.second.clone().unwrap_or_default(),
    );
    assert!(
        first.starts_with("outcome failed too_many at main.jadpo:9  function checked")
            && second.starts_with("outcome failed too_many at main.jadpo:12  function checked"),
        "{difference}"
    );
}

/// A fault's stack is in its record, so a recursion reached another way
/// differs there, though its innermost site doesn't (chunk 8.4a).
#[test]
fn a_fault_reached_another_way_is_named_by_its_stack() {
    let folder = written(
        "fault-stack",
        &[
            (
                "main.jadpo",
                "type Depth = Int {}\n\n/// One frame a level.\nfunction down(depth: Depth) -> Depth {\n    if (depth == 0) {\n        return 0\n    }\n    return Depth(down(Depth(depth - 1)) + 1)\n}\n\n/// The same walk, a frame further in.\nfunction around(depth: Depth) -> Depth {\n    return down(depth)\n}\n",
            ),
            (
                "main.test.jadpo",
                "test \"a walk too deep\" {\n    assert down(2000) == around(2000)\n}\n",
            ),
        ],
    );
    let difference = changed(&folder, |root| {
        perturb(
            &root.join("main.test.jadpo"),
            "down(2000) == around(2000)",
            "around(2000) == down(2000)",
        );
    });
    let (first, second) = (
        difference.first.clone().unwrap_or_default(),
        difference.second.clone().unwrap_or_default(),
    );
    let site = "outcome fault recursion_limit at main.jadpo:8  function down; stack [";
    assert!(
        first.starts_with(site) && second.starts_with(site),
        "{difference}"
    );
    assert!(
        first.contains("function down ×999, main.test.jadpo:2")
            && second
                .contains("function down ×998, main.jadpo:13  function around, main.test.jadpo:2"),
        "{difference}"
    );
}

/// An outside call's input is in its record as a keyed digest, so a call
/// sent with another input differs there though its answer doesn't, and
/// the input itself is in no line (chunk 8.4a).
#[test]
fn a_call_sent_with_another_input_differs_at_its_digest() {
    let folder = page("docs/language/13-testing.md fakes-in-tests");
    let difference = changed(&folder, |root| {
        perturb(
            &root.join("domains/shipping/actions/book.test.jadpo"),
            "book(\"N1 9GU\")",
            "book(\"N1 9GV\")",
        );
    });
    assert!(
        difference
            .test
            .ends_with("\"the courier's reference is kept\""),
        "{difference}"
    );
    let (first, second) = (
        difference.first.clone().unwrap_or_default(),
        difference.second.clone().unwrap_or_default(),
    );
    assert_eq!(
        without_digest(&first),
        without_digest(&second),
        "{difference}"
    );
    assert!(
        first.starts_with("call couriers.bookings.create #")
            && first != second
            && !first.contains("N1 9GU")
            && !second.contains("N1 9GV"),
        "{difference}"
    );
}

/// A call's line without its input's digest.
fn without_digest(line: &str) -> String {
    line.split(' ')
        .filter(|word| !word.starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ")
}

/// What a test with a database did: each statement, each event written
/// with its payload, each outside call with its answer, and the rows left;
/// a perturbed write is named where it first shows.
#[test]
fn a_test_with_a_database_keeps_its_statements_events_calls_and_rows() {
    if !jadpo_test::live() {
        return;
    }
    let folder = example("26-delivery-desk");
    let run = recorded(&folder.0).expect("it runs");
    let lines: Vec<&String> = run
        .results
        .iter()
        .filter_map(|result| result.record.as_ref())
        .flat_map(|record| record.lines())
        .collect();
    let has = |prefix: &str| lines.iter().any(|line| line.starts_with(prefix));
    assert!(has("sql INSERT INTO"), "{lines:#?}");
    assert!(has("commit | done"), "{lines:#?}");
    assert!(has("event "), "{lines:#?}");
    assert!(has("call "), "{lines:#?}");
    assert!(has("row main."), "{lines:#?}");
    // Identities are numbered; no statement holds a value.
    assert!(lines.iter().any(|line| line.contains("id#1")), "{lines:#?}");
    assert!(
        lines
            .iter()
            .filter(|line| line.starts_with("sql "))
            .all(|line| !line.contains("id#")),
        "{lines:#?}"
    );

    // A value written differently shows first in the rows the test left,
    // the test itself passing as before.
    perturb(
        &folder
            .0
            .join("domains/courier_desk/actions/keep_tariff.test.jadpo"),
        "afternoon_pence: 550",
        "afternoon_pence: 551",
    );
    let after = recorded(&folder.0).expect("it runs");
    let difference = differential(&run, &after).unwrap_err();
    assert!(
        difference.test.ends_with("\"a new price list is kept\""),
        "{difference}"
    );
    assert_eq!(
        difference.before.as_deref(),
        Some("test passed"),
        "{difference}"
    );
    assert_eq!(
        difference.first.as_deref(),
        Some(
            "row main.courier_tariff {\"id\":\"id#1\",\"morning_pence\":650,\"afternoon_pence\":550}"
        ),
        "{difference}"
    );
    assert_eq!(
        difference.second.as_deref(),
        Some(
            "row main.courier_tariff {\"id\":\"id#1\",\"morning_pence\":650,\"afternoon_pence\":551}"
        ),
        "{difference}"
    );
}

/// A read's bound values are in its record as a keyed digest, so a query
/// asked for another value differs there though it reads as many rows,
/// and the value itself is in no line (chunk 8.4a).
#[test]
fn a_read_asked_for_another_value_differs_at_its_digest() {
    if !jadpo_test::live() {
        return;
    }
    let folder = example("10-library-loans");
    let difference = changed(&folder, |root| {
        perturb(
            &root.join("domains/lending/queries/member_by_email.test.jadpo"),
            "member_by_email(\"bob@example.test\")",
            "member_by_email(\"cy@example.test\")",
        );
    });
    assert!(
        difference
            .test
            .ends_with("\"a member is found by address\""),
        "{difference}"
    );
    let (first, second) = (
        difference.first.clone().unwrap_or_default(),
        difference.second.clone().unwrap_or_default(),
    );
    let digest = |line: &str| {
        line.split(" | ")
            .nth(1)
            .and_then(|types| types.split_once(") #"))
            .map(|(_, digest)| digest.to_string())
    };
    assert!(
        first.starts_with("sql SELECT")
            && first.ends_with("| read 0")
            && digest(&first).is_some()
            && digest(&first) != digest(&second)
            && first.replace(&digest(&first).unwrap_or_default(), "")
                == second.replace(&digest(&second).unwrap_or_default(), "")
            && !second.contains("cy@"),
        "{difference}"
    );
}

/// A recording's test keeps the value the package's decoders read it as,
/// so a recording that decodes to another value differs there, though it
/// still decodes (chunk 8.4a).
#[test]
fn a_recording_read_as_another_value_differs_at_its_outcome() {
    if !jadpo_test::live() {
        return;
    }
    let folder = example("26-delivery-desk");
    let difference = changed(&folder, |root| {
        perturb(
            &root.join("shared/providers/parcel_rates/v1/recordings/parcel_quotes.create.json"),
            "\"amount\": 650",
            "\"amount\": 651",
        );
    });
    assert!(
        difference.test.starts_with("recording ")
            && difference.test.ends_with("parcel_quotes.create.json"),
        "{difference}"
    );
    let (first, second) = (
        difference.first.clone().unwrap_or_default(),
        difference.second.clone().unwrap_or_default(),
    );
    assert!(
        first.starts_with("outcome value Quote { amount: money.from_minor(650,")
            && second.starts_with("outcome value Quote { amount: money.from_minor(651,"),
        "{difference}"
    );
    assert_eq!(difference.at, 1, "{difference}");
}

/// The corpus (every language page's project and every teaching example,
/// with its migrations) run twice gives the same record for every test.
#[test]
fn the_corpus_run_twice_gives_the_same_records() {
    if !jadpo_test::live() {
        return;
    }
    let mut compared = 0;
    let mut problems = Vec::new();
    for (name, files) in examples::projects() {
        let folder = match name.strip_prefix("examples ") {
            Some(project) => example(project),
            None => {
                let folder = Folder::new("page");
                for (path, source) in &files {
                    let target = folder.0.join(path);
                    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                    std::fs::write(target, source).unwrap();
                }
                folder
            }
        };
        let Some(first) = recorded(&folder.0) else {
            continue;
        };
        let second = recorded(&folder.0).expect("it runs again");
        match differential(&first, &second) {
            Ok(tests) => compared += tests,
            Err(difference) => problems.push(format!("{name}: {difference}")),
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n\n"));
    assert!(
        compared > 400,
        "the corpus's tests were compared: {compared}"
    );
}

/// The ways a body, a loop's turns and an inline function's runs went.
const GRADED: &str = "type Count = Int {}\n\n/// A count's grade, the same either way past ten.\nfunction graded(count: Count) -> Count {\n    var mut grade: Count = 0\n    if (count > 10) {\n        grade = 1\n    } else {\n        grade = 1\n    }\n    for step in [1, 2, 3] {\n        if (step > 1) {\n            grade = Count(grade + step)\n        }\n    }\n    return Count(list.sum(list.map([count], each -> match each > 2 {\n        true -> Count(grade)\n        false -> Count(0)\n    })))\n}\n";

/// Each body's paths are in its record: the body's own, each loop's turns
/// and each inline function's runs, by the path each took and how many
/// times (chunk 9.3).
#[test]
fn the_paths_each_body_took_are_in_its_record() {
    let folder = written(
        "paths",
        &[
            ("main.jadpo", GRADED),
            (
                "main.test.jadpo",
                "test \"a count is graded\" {\n    assert graded(12) == 6\n}\n",
            ),
        ],
    );
    let run = recorded(&folder.0).expect("it runs");
    let record = run.results[0].record.as_ref().expect("a record");
    let paths: Vec<&str> = record
        .lines()
        .iter()
        .filter(|line| line.starts_with("path "))
        .map(String::as_str)
        .collect();
    assert_eq!(
        paths,
        [
            "path main.jadpo:4  function graded body | 1×1",
            "path main.jadpo:11  function graded turn | 0×1, 1×2",
            "path main.jadpo:16  function graded turn | 0×1",
            "path main.test.jadpo:1  test \"a count is graded\" body | 0×1",
        ],
        "{:#?}",
        record.lines()
    );
}

/// A branch taken the other way, the outcome the same, shows in the record
/// as the body and path that differ.
#[test]
fn a_branch_taken_another_way_is_named_by_its_body_and_path() {
    let folder = written(
        "paths-differ",
        &[
            ("main.jadpo", GRADED),
            (
                "main.test.jadpo",
                "test \"a count is graded\" {\n    assert graded(12) == 6\n}\n",
            ),
        ],
    );
    let difference = changed(&folder, |root| {
        perturb(&root.join("main.jadpo"), "count > 10", "count > 20");
    });
    assert_eq!(
        difference.first.as_deref(),
        Some("path main.jadpo:4  function graded body | 1×1"),
        "{difference}"
    );
    assert_eq!(
        difference.second.as_deref(),
        Some("path main.jadpo:4  function graded body | 0×1"),
        "{difference}"
    );
}

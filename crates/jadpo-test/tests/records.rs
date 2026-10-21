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
        &folder.0.join("domains/baking/actions/price_loaves.jadpo"),
        "Pence(total + 5)",
        "Pence(total + 6)",
    );
    let after = recorded(&folder.0).expect("it runs");
    let difference = differential(&before, &after).unwrap_err();
    assert!(
        difference
            .test
            .ends_with("\"four loaves come with a 5 pence bag\""),
        "{difference}"
    );
    let first = difference.first.as_deref().unwrap_or_default();
    let second = difference.second.as_deref().unwrap_or_default();
    assert!(first.starts_with("outcome value"), "{difference}");
    assert!(
        second.starts_with("outcome assertion at domains/baking/actions/price_loaves.test.jadpo:")
            && second.contains("1006 == 1005"),
        "{difference}"
    );
    let shown = difference.to_string();
    assert!(shown.contains("  first:  outcome value"), "{shown}");
    assert!(shown.contains("  second: outcome assertion"), "{shown}");
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

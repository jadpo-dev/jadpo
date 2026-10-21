//! The runtime's plan in its canonical bytes (roadmap chunk 8.1a): compiled
//! code holds the plan as a constant and decodes it as it starts
//! (docs/compiler-runtime.md §2.5), so for every project of the
//! specification corpus (the language pages' `check` projects and the
//! teaching examples) the bytes must give back an equal plan, that plan
//! must write the same bytes again, and the same project must give the
//! same bytes however often it is planned.

use crate::syntax_examples as examples;

use jadpo_db::{Database, File, Layout, Project};
use jadpo_plan::Plan;
use jadpo_std::RuntimePlan;

/// The plan of `files`, as a page's checker loads them, when they check
/// without an error that stops a run.
fn planned(files: &[(String, String)]) -> Option<Vec<u8>> {
    let db = Database::default();
    let mut sorted = files.to_vec();
    sorted.sort();
    let inputs: Vec<File> = sorted
        .iter()
        .map(|(path, text)| File::new(&db, path.clone(), text.clone()))
        .collect();
    let layout = if files.iter().any(|(path, _)| path == "app.jadpo") {
        Layout::Project
    } else {
        Layout::Loose
    };
    let project = Project::new(
        &db,
        inputs,
        Vec::new(),
        layout,
        Default::default(),
        Vec::new(),
    );
    bytes_of(&db, project)
}

/// The bytes of `project`'s runtime plan, checked to read back equal and
/// to write the same bytes again; `None` for a project with errors.
fn bytes_of(db: &Database, project: Project) -> Option<Vec<u8>> {
    let blocked = jadpo_check::check(db, project)
        .into_iter()
        .any(|diagnostic| jadpo_check::blocks_running(&diagnostic));
    if blocked {
        return None;
    }
    let plan = Plan::build(db, project);
    let bytes = plan
        .runtime()
        .encode()
        .expect("the plan has a canonical form");
    let read = RuntimePlan::decode(&bytes).expect("its bytes read back");
    assert!(read == *plan.runtime(), "the plan read back differs");
    assert_eq!(read.encode().expect("encodes"), bytes, "written again");
    Some(bytes)
}

#[test]
fn every_page_projects_plan_reads_back_from_its_bytes() {
    let mut planned_count = 0;
    let mut problems = Vec::new();
    for (name, files) in examples::projects() {
        if name.starts_with("examples ") {
            continue;
        }
        let result = std::panic::catch_unwind(|| planned(&files));
        match result {
            Ok(Some(_)) => planned_count += 1,
            Ok(None) => {}
            Err(_) => problems.push(name),
        }
    }
    assert!(
        problems.is_empty(),
        "plans that don't read back: {problems:?}"
    );
    assert!(planned_count > 150, "the pages were read: {planned_count}");
}

#[test]
fn every_teaching_examples_plan_reads_back_and_is_the_same_twice() {
    let root = examples::root().join("examples");
    let mut folders: Vec<_> = std::fs::read_dir(&root)
        .expect("examples/")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.join("app.jadpo").is_file())
        .collect();
    folders.sort();
    assert!(folders.len() >= 28, "the examples were found: {folders:?}");
    for folder in folders {
        let first = jadpo_db::load(&folder);
        let bytes = bytes_of(&first.db, first.project)
            .unwrap_or_else(|| panic!("{} checks", folder.display()));
        // A second reading, into a database of its own: a plan holding
        // anything in hash order would come out in another order.
        let second = jadpo_db::load(&folder);
        let again = bytes_of(&second.db, second.project).expect("it checks again");
        assert!(bytes == again, "{} planned twice differs", folder.display());
        // Every example's plan holds something: the format's header at
        // least, and its routes or settings.
        assert!(
            bytes.starts_with(RuntimePlan::HEADER),
            "{}",
            folder.display()
        );
    }
}

#[test]
fn bytes_of_another_format_or_cut_short_are_refused() {
    let files = vec![(
        "main.jadpo".to_string(),
        "type Count = Int {\n    max: 12\n}\n\nfunction one() -> Count {\n    return 1\n}\n"
            .to_string(),
    )];
    let bytes = planned(&files).expect("it checks");
    assert!(RuntimePlan::decode(&bytes).is_ok());
    let mut other = bytes.clone();
    if let Some(byte) = other.get_mut(RuntimePlan::HEADER.len() - 2) {
        *byte = byte.wrapping_add(1);
    }
    assert!(
        RuntimePlan::decode(&other).is_err(),
        "another format's header"
    );
    assert!(RuntimePlan::decode(&bytes[..bytes.len() - 1]).is_err());
    assert!(RuntimePlan::decode(b"").is_err());
}

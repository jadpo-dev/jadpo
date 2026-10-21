//! The generator over the specification corpus (roadmap chunk 8.1f): every
//! language page's project and teaching example that checks is either
//! written as a crate whose every file parses as Rust, or refused, each
//! form not compiled yet named where it is written, which is the work
//! chunks 8.1g to 8.1i have left. Nothing panics, and no file is written
//! that isn't Rust.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use jadpo_codegen::{Error, Options, generate};
use jadpo_db::{Database, File, Layout, Project};
use jadpo_plan::Plan;

use crate::syntax_examples as examples;

/// How one project went.
enum Went {
    /// It has errors, so nothing runs it.
    Unchecked,
    Compiled,
    /// Refused: each form refused, with where.
    Refused(Vec<jadpo_codegen::Refusal>),
    Problem(String),
}

/// What generating `files`'s crate gives.
fn generated(files: &[(String, String)], options: &Options) -> Went {
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
    if jadpo_check::check(&db, project)
        .iter()
        .any(jadpo_check::blocks_running)
    {
        return Went::Unchecked;
    }
    let plan = Plan::build(&db, project);
    match std::panic::catch_unwind(|| generate(&plan, options)) {
        Ok(Ok(_)) => Went::Compiled,
        Ok(Err(Error::Refused(refusals))) => Went::Refused(refusals),
        Ok(Err(error)) => Went::Problem(error.to_string()),
        Err(_) => Went::Problem("the generator panicked".into()),
    }
}

#[test]
fn every_corpus_project_is_compiled_or_refused_by_name() {
    let options = Options {
        name: "jadpo-compiled".into(),
        runtime: "../crates".into(),
    };
    let projects = examples::projects();
    // The projects shared out over a thread per core, each checking its
    // own in a database of its own.
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut went: Vec<(usize, String, Went)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some((name, files)) = projects.get(at) else {
                            break;
                        };
                        done.push((at, name.clone(), generated(files, &options)));
                    }
                    done
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("a worker"))
            .collect()
    });
    went.sort_by_key(|(at, _, _)| *at);
    let mut compiled = 0;
    let mut checked = 0;
    let mut refused: BTreeMap<String, usize> = BTreeMap::new();
    let mut problems = Vec::new();
    for (_, name, went) in went {
        match went {
            Went::Unchecked => {}
            Went::Compiled => {
                checked += 1;
                compiled += 1;
            }
            Went::Refused(refusals) => {
                checked += 1;
                assert!(!refusals.is_empty(), "{name}: refused for nothing");
                for refusal in refusals {
                    assert!(
                        refusal.line > 0 && !refusal.file.is_empty(),
                        "{name}: {refusal}"
                    );
                    *refused.entry(refusal.form).or_default() += 1;
                }
            }
            Went::Problem(problem) => {
                checked += 1;
                problems.push(format!("{name}: {problem}"));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    // What is left, the forms most often refused first: `cargo test -p
    // jadpo-codegen corpus -- --nocapture` shows it.
    let mut left: Vec<(usize, String)> = refused
        .into_iter()
        .map(|(form, count)| (count, form))
        .collect();
    left.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    println!("compiled: {compiled} of {checked} projects that check; refused forms:");
    for (count, form) in &left {
        println!("{count:>6}  {form}");
    }
    assert!(compiled > 0, "no project compiled");
}

//! Compiled code (roadmap chunks 8.1f and 8.1g; docs/compiler-runtime.md
//! §2.5, "Compiled code" and "Differential test"). Every project of the
//! specification corpus that compiles, the language pages' and the
//! teaching examples', written by `jadpo-codegen` as crates of one harness
//! built with cargo and run beside the interpreter: every test gives the
//! same record both ways, a failed assertion's values among them. Area 7's
//! `todos` project, a route that creates a todo and one that reads it
//! back, is the first route.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live part.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;
use jadpo_test::compiled::{Compared, differential_all};

use crate::support;
use crate::syntax_examples as examples;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// A project of the specification corpus with `files`, written into a
/// folder of its own, `folder`, with its migration and its API's record
/// when it is laid out as a project, as the page's checker writes them;
/// `None` when it doesn't check, so nothing runs it.
fn corpus_project(files: &[(String, String)], folder: &str) -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(folder);
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    // A project with errors writes no migration, and is left out below.
    if files.iter().any(|(path, _)| path == "app.jadpo") {
        for args in [&["migrate", "new", "initial"][..], &["api", "update"]] {
            if jadpo(&root, args).code != 0 {
                return None;
            }
        }
    }
    let loaded = jadpo_db::load(&root);
    let blocked = jadpo_check::check(&loaded.db, loaded.project)
        .iter()
        .any(jadpo_check::blocks_running);
    (!blocked).then_some(root)
}

/// Where the harness for `project` is kept between runs: beside the
/// build's own folders, so a second run compiles only what changed.
fn harness_folder(project: &str) -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled")
        .join(project)
}

/// The corpus's projects whose names start with one of `prefixes` and that
/// check, each written into a folder of its own: their names and folders.
fn corpus_projects(prefixes: &[&str]) -> Vec<(String, PathBuf)> {
    let projects: Vec<(String, Vec<(String, String)>)> = examples::projects()
        .into_iter()
        .filter(|(name, _)| prefixes.iter().any(|prefix| name.starts_with(prefix)))
        .collect();
    // Written and migrated a few at a time.
    let next = AtomicUsize::new(0);
    let mut written: Vec<(usize, String, PathBuf)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some((name, files)) = projects.get(at) else {
                            break;
                        };
                        if let Some(folder) =
                            corpus_project(files, &format!("compiled-corpus-{at}"))
                        {
                            done.push((at, name.clone(), folder));
                        }
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
    written.sort_by_key(|(at, _, _)| *at);
    written
        .into_iter()
        .map(|(_, name, folder)| (name, folder))
        .collect()
}

/// Every project in `projects` compiled into one harness kept at `work`
/// and compared: each one's name with what it compared, or `Err` when it
/// has a form not compiled yet; every other problem fails the test, named
/// by its project.
fn compared(projects: &[(String, PathBuf)], work: &str) -> Vec<(String, Result<Compared, ()>)> {
    let folders: Vec<PathBuf> = projects.iter().map(|(_, folder)| folder.clone()).collect();
    let results = differential_all(&folders, &harness_folder(work))
        .unwrap_or_else(|problem| panic!("{problem}"));
    let mut problems = Vec::new();
    let mut done = Vec::new();
    for ((name, _), result) in projects.iter().zip(results) {
        match result {
            Ok(compared) => done.push((name.clone(), Ok(compared))),
            Err(problem) if problem.refused() => done.push((name.clone(), Err(()))),
            Err(problem) => problems.push(format!("{name}: {problem}")),
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n\n"));
    done
}

/// A `return` inside an inline function, which the checker allows in a
/// `match` arm's block there: it leaves the function, as it does anywhere
/// else, rather than giving the item's value.
const RETURNS: &str = r#"type Count = Int {}

function picked(counts: List<Count>) -> List<Count> {
    return list.map(counts, count -> match count > 3 {
        true -> {
            return counts
        }
        false -> count
    })
}
"#;

/// Its tests, one failing so its assertion's values are compared too.
const RETURNS_TEST: &str = r#"test "a return inside an inline function leaves the function" {
    assert picked([1, 5]) == [1, 5]
    assert picked([1, 2]) == [1, 2]
}

test "a failed assertion shows its values" {
    assert picked([1, 2]) == [2, 1]
}
"#;

/// Every project of the specification corpus that compiles, the pages'
/// and the teaching examples', built as crates of one harness and
/// compared (chunks 8.1f and 8.1g). Every project of the pages for areas 1
/// to 4 compiles (functions, failures, `attempt`, outcome `match`,
/// constructions, collections and inline functions, constants, the
/// standard library and assertions with their values), though those pages'
/// projects declare no tests: the corpus's tests run their forms. Area
/// 7's first routes pass as they did. A project with a form not compiled
/// yet is listed; chunk 8.1j gives the corpus as one crate a CI job of its
/// own.
#[test]
fn the_corpus_passes_compiled_with_the_interpreters_records() {
    if !jadpo_test::live() {
        return;
    }
    let mut projects = corpus_projects(&["docs/language/", "examples "]);
    let returns = [
        ("main.jadpo".to_string(), RETURNS.to_string()),
        ("main.test.jadpo".to_string(), RETURNS_TEST.to_string()),
    ];
    projects.push((
        "a return inside an inline function".into(),
        corpus_project(&returns, "compiled-returns").expect("it checks"),
    ));
    let results = compared(&projects, "corpus");
    let mut done = Vec::new();
    let mut refused = Vec::new();
    for (name, result) in results {
        match result {
            Ok(compared) => done.push((name, compared)),
            Err(()) => refused.push(name),
        }
    }
    let early: Vec<&String> = refused
        .iter()
        .filter(|name| {
            ["01-", "02-", "03-", "04-"]
                .iter()
                .any(|area| name.starts_with(&format!("docs/language/{area}")))
        })
        .collect();
    assert!(early.is_empty(), "areas 1 to 4 not compiled: {early:?}");
    let pages = done
        .iter()
        .filter(|(name, _)| name.starts_with("docs/language/0"))
        .count();
    assert!(pages > 60, "{done:?}");
    let todos = done
        .iter()
        .find(|(name, _)| name == "docs/language/07-http.md todos")
        .map(|(_, compared)| *compared);
    assert_eq!(
        todos,
        Some(Compared {
            tests: 3,
            passed: 3
        })
    );
    let returned = done
        .iter()
        .find(|(name, _)| name == "a return inside an inline function")
        .map(|(_, compared)| *compared);
    assert_eq!(
        returned,
        Some(Compared {
            tests: 2,
            passed: 1
        })
    );
    let tests: usize = done.iter().map(|(_, compared)| compared.tests).sum();
    assert!(tests > 300, "{done:?}");
    println!(
        "compared {} projects, {tests} tests; not compiled yet: {}",
        done.len(),
        refused.len()
    );
    for name in &refused {
        println!("  {name}");
    }
}

//! Compiled code: the first route (roadmap chunk 8.1f;
//! docs/compiler-runtime.md §2.5, "Compiled code" and "Differential
//! test"). Area 7's `todos` project, a route that creates a todo and one
//! that reads it back, with their route tests, compiled by
//! `jadpo-codegen`, built with cargo and run beside the interpreter: every
//! test passes both ways, with the same records.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live part.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;
use jadpo_test::compiled::{Compared, differential};

use crate::support;
use crate::syntax_examples as examples;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// Language page `page`'s project `project`, written into a folder of
/// its own with its migration and its API's record, as the page's
/// checker writes them.
fn page_project(page: &str, project: &str) -> PathBuf {
    let name = format!("docs/language/{page} {project}");
    let (_, files) = examples::projects()
        .into_iter()
        .find(|(found, _)| *found == name)
        .unwrap_or_else(|| panic!("no project {name}"));
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("compiled-{project}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    for args in [&["migrate", "new", "initial"][..], &["api", "update"]] {
        let ran = jadpo(&root, args);
        assert_eq!(ran.code, 0, "{args:?}: {}{}", ran.stdout, ran.stderr);
    }
    root
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

#[test]
fn the_first_routes_pass_compiled_with_the_interpreters_records() {
    if !jadpo_test::live() {
        return;
    }
    let root = page_project("07-http.md", "todos");
    let compared =
        differential(&root, &harness_folder("todos")).unwrap_or_else(|problem| panic!("{problem}"));
    assert_eq!(
        compared,
        Compared {
            tests: 3,
            passed: 3
        }
    );
}

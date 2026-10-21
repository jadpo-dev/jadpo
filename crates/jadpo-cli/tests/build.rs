//! `jadpo build`'s refusals, which need no Rust compiler (roadmap chunk
//! 8.1o; area 14, "The `jadpo` command"): outside a project; a project
//! that doesn't check, whose errors are shown as `jadpo check` shows them
//! and nothing is built; and a deployable the project doesn't have. With
//! none declared, the one deployable, `app`, runs everything. Building one
//! and running it is `deployable.rs`, against live PostgreSQL, and one
//! built for Workers `workers.rs`, under workerd.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &Environment::empty().with_cwd(cwd))
}

/// A project of `files` in a folder of its own.
fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("build-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
        fs::write(path, text).expect("written");
    }
    root
}

#[test]
fn outside_a_project_there_is_nothing_to_build() {
    let root = project("nowhere", &[("notes.txt", "nothing here\n")]);
    let output = jadpo(&root, &["build"]);
    assert_eq!(output.code, 1, "{}{}", output.stdout, output.stderr);
    assert!(
        output.stdout.contains("No Jadpo project here"),
        "{}",
        output.stdout
    );
}

#[test]
fn a_project_that_does_not_check_is_not_built() {
    let root = project(
        "unchecked",
        &[
            ("app.jadpo", "application {}\n"),
            (
                "domains/bakery/types/loaves.jadpo",
                "/// How many loaves.\ntype Loaves = Int {\n    min: 0\n    max: \"many\"\n}\n",
            ),
        ],
    );
    let output = jadpo(&root, &["build"]);
    assert_eq!(output.code, 1, "{}{}", output.stdout, output.stderr);
    assert!(output.stdout.contains("error:"), "{}", output.stdout);
    assert!(
        output.stdout.contains("build stopped: 2 errors"),
        "{}",
        output.stdout
    );
    assert!(!root.join("build/target").exists());
    assert!(!root.join("build/rust").exists());
    let json = jadpo(&root, &["build", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    assert_eq!(document["ok"], false, "{document}");
}

#[test]
fn with_none_declared_the_one_deployable_is_app() {
    let root = project("named", &[("app.jadpo", "application {}\n")]);
    let output = jadpo(&root, &["build", "kiosk"]);
    assert_eq!(output.code, 2, "{}{}", output.stdout, output.stderr);
    assert!(
        output
            .stderr
            .contains("error: `kiosk` isn't one of the app's deployables"),
        "{}",
        output.stderr
    );
    assert!(
        output.stderr.contains("rule: cli.build_arguments"),
        "{}",
        output.stderr
    );
    assert!(
        output.stderr.contains("`jadpo build app`"),
        "{}",
        output.stderr
    );
}

/// The café declares a kiosk serving `main`, built natively, and an edge
/// built for Workers (chunk 8.1o2), which `jadpo build` builds since chunk
/// 8.2b (`workers.rs`).
const DECLARED: &str = "application {\n    deployables: {\n        kiosk: {\n            serves: [main]\n            target: Target.native\n        }\n        edge: {\n            serves: [main]\n            target: Target.wasm\n        }\n    }\n}\n";

#[test]
fn a_declared_deployable_is_built_by_its_name() {
    let root = project("declared", &[("app.jadpo", DECLARED)]);
    let output = jadpo(&root, &["build", "kisok"]);
    assert_eq!(output.code, 2, "{}{}", output.stdout, output.stderr);
    for expected in [
        "error: `kisok` isn't one of the app's deployables",
        "`app.jadpo` declares `edge`, `kiosk`. Did you mean `kiosk`?",
        "Run `jadpo build kiosk`",
        "rule: cli.build_arguments",
    ] {
        assert!(
            output.stderr.contains(expected),
            "{expected}: {}",
            output.stderr
        );
    }
}

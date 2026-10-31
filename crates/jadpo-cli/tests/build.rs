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
/// 8.2b (`workers.rs`), their rate limits counted in Redis, which both
/// reach (chunk 8.2m2): until Redis's driver comes the edge's instances
/// would each count in their own memory, which it declines (chunk 8.4b).
const DECLARED: &str = "application {\n    connections: {\n        cache: {\n            driver: Driver.redis\n        }\n    }\n    rate_limits: {\n        store: cache\n    }\n    deployables: {\n        kiosk: {\n            serves: [main]\n            target: Target.native\n        }\n        decline rate_limits.counted_in_memory: \"One instance until Redis's driver comes\"\n        edge: {\n            serves: [main]\n            target: Target.wasm\n        }\n    }\n}\n";

#[test]
fn a_declared_deployable_is_built_by_its_name() {
    let root = project("declared", &[("app.jadpo", DECLARED)]);
    // Its `connections:` block keeps `network.jadpo_record` (chunk A8).
    jadpo(&root, &["records", "update"]);
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

/// The built `jadpo build` in `root`, its cargo (the one running these
/// tests) with an empty cache whose configuration puts in crates.io's
/// place a registry nothing answers at, and a cache folder of its own.
fn unreachable_crates(root: &Path, args: &[&str]) -> std::process::Output {
    let cargo_home = root.join("cargo-home");
    fs::create_dir_all(&cargo_home).expect("a folder");
    fs::write(
        cargo_home.join("config.toml"),
        "[source.crates-io]\nreplace-with = \"nowhere\"\n\n[source.nowhere]\nregistry = \"sparse+http://127.0.0.1:9/index/\"\n",
    )
    .expect("cargo's configuration");
    std::process::Command::new(env!("CARGO_BIN_EXE_jadpo"))
        .args(args)
        .current_dir(root)
        .env("CARGO_HOME", &cargo_home)
        .env("CARGO_NET_RETRY", "0")
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env_remove("FORCE_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("COLUMNS")
        .output()
        .expect("jadpo runs")
}

/// Area 14, "What it compiles with": crates cargo can't fetch are
/// `toolchain.fetch_failed`, not a build that didn't compile, and cargo's
/// lines in its reason print as lines, in plain output and in `--json`.
#[test]
fn crates_that_cant_be_fetched_are_a_failed_fetch() {
    let root = project("unfetched", &[("app.jadpo", "application {}\n")]);
    let output = unreachable_crates(&root, &["build"]);
    let shown = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(1), "{shown}");
    assert!(shown.contains("rule: toolchain.fetch_failed\n"), "{shown}");
    assert!(!shown.contains("toolchain.build_failed"), "{shown}");
    assert!(
        shown.contains("error: `app`'s crates couldn't be fetched\n"),
        "{shown}"
    );
    assert!(!shown.contains("\\u{a}"), "{shown}");
    let reason = shown
        .lines()
        .skip_while(|line| !line.starts_with("  reason: "))
        .take_while(|line| !line.starts_with("  next: "))
        .count();
    assert!(reason > 1, "cargo's lines are lines of their own: {shown}");

    let json = unreachable_crates(&root, &["build", "--json"]);
    let document: serde_json::Value = serde_json::from_slice(&json.stdout).expect("JSON");
    assert_eq!(document["ok"], false, "{document}");
    let diagnostic = &document["diagnostics"][0];
    assert_eq!(diagnostic["rule"], "toolchain.fetch_failed", "{document}");
    let reason = diagnostic["reason"].as_str().expect("a reason");
    assert!(reason.contains('\n'), "{reason:?}");
    assert!(!reason.contains("\\u{a}"), "{reason:?}");
    assert!(
        String::from_utf8_lossy(&json.stdout).contains("\\n"),
        "a line break is escaped as JSON escapes it"
    );
}

/// A cargo that can't read the workspace Jadpo wrote fails online as it
/// did offline: that isn't a fetch, but a toolchain the build can't run,
/// `toolchain.build_failed`.
#[cfg(unix)]
#[test]
fn a_cargo_that_cant_read_the_workspace_is_a_failed_build_not_a_fetch() {
    use std::os::unix::fs::PermissionsExt;
    let root = project("unread", &[("app.jadpo", "application {}\n")]);
    let cargo = root.join("old-cargo");
    fs::write(
        &cargo,
        "#!/bin/sh\necho 'error: failed to parse manifest' >&2\nexit 101\n",
    )
    .expect("a cargo");
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).expect("runnable");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_jadpo"))
        .arg("build")
        .current_dir(&root)
        .env("CARGO", &cargo)
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env_remove("FORCE_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .output()
        .expect("jadpo runs");
    let shown = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(1), "{shown}");
    assert!(shown.contains("rule: toolchain.build_failed\n"), "{shown}");
    assert!(shown.contains("error: failed to parse manifest"), "{shown}");
    assert!(!shown.contains("Fetching the runtime's crates"), "{shown}");
}

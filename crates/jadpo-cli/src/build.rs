//! `jadpo build` (area 14, "The `jadpo` command"; roadmap chunk 8.1o;
//! docs/compiler-runtime.md §2.6, "A native deployable", and §2.8): checks
//! the project as `jadpo check` does, writing the same derived files, and
//! stops at its errors; otherwise writes the deployable's Rust into
//! `build/rust/` and compiles it in release, offline, with the toolchain
//! this Jadpo pins and the runtime crates' lock, into one native binary in
//! `build/target/<deployable>/`, beside its APIs' OpenAPI documents and
//! each `<api>.failures.<locale>.json` (area 10). The binary carries the
//! CLDR data of the app's supported locales only (chunk 8.2c), where
//! `jadpo test` and `jadpo dev` keep every locale.
//!
//! With no `deployables:` declared there is one deployable, `app`, which
//! runs everything natively (area 14, question 14). Its build stamp
//! (area 18) is a constant in the binary and on every event it logs: the
//! build id, `b_` and twelve hex digits of a SHA-256 over the source it
//! reaches (the graph's digest), the compiler, the target and the
//! package lock; the commit, and whether the tree held changes it doesn't
//! (a project git doesn't track counts as changed, with no commit); the
//! compiler; the deployable; and the target.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use jadpo_diag::{Decider, Diagnostic, rule};
use jadpo_term::{Block, Fact, Json, Outcome, Screen};
use sha2::{Digest, Sha256};

use crate::check::{Checked, checked, reported};
use crate::fmt::project_needed;
use crate::help::VERSION;
use crate::run::{Call, Reply, say};

/// The one deployable of an app that declares none: it runs everything.
pub const APP: &str = "app";

/// The target a deployable is built for until WebAssembly is (chunk 8.2).
const TARGET: &str = "native";

pub fn build(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_needed("build");
    };
    if let Some(name) = call.arguments.first()
        && name != APP
    {
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_BUILD_ARGUMENTS,
                format!("`{name}` isn't one of the app's deployables"),
            )
            .reason(format!(
                "The app declares no `deployables:` in `app.jadpo`, so it has one deployable, `{APP}`, which runs everything."
            ))
            .next(format!("Run `jadpo build`, or `jadpo build {APP}`")),
        );
    }
    let started = Instant::now();
    say("Checking");
    let checked = checked(call, &root);
    if !checked.diagnostics.is_empty() {
        return reported(&cwd, checked, started.elapsed(), "build stopped");
    }
    let Checked {
        loaded, written, ..
    } = checked;
    say("Writing the deployable");
    let db = &loaded.db;
    let project = loaded.project;
    let lock = std::fs::read(root.join(jadpo_db::LOCK)).unwrap_or_default();
    let plan = jadpo_plan::Plan::build(db, project);
    let build = build_id(plan.build_id(), VERSION, TARGET, &lock);
    let plan = plan.with_build(build.clone());
    let (commit, modified) = commit(&root);
    let stores: BTreeMap<String, Vec<(String, String)>> =
        jadpo_check::data::migrations::store_steps(jadpo_check::data::migrations::migrations(
            db, project,
        ))
        .into_iter()
        .map(|(store, migrations)| {
            let migrations = migrations
                .into_iter()
                .map(|migration| (migration.name, migration.digest))
                .collect();
            (store, migrations)
        })
        .collect();
    let settings: Vec<String> = plan
        .runtime()
        .config
        .fields
        .iter()
        .map(|field| field.variable.clone())
        .collect();
    let sources = runtime_sources();
    let (toolchain, runtime_lock) = match (
        std::fs::read_to_string(sources.join("../rust-toolchain.toml")),
        std::fs::read(sources.join("../Cargo.lock")),
    ) {
        (Ok(toolchain), Ok(lock)) => (toolchain, lock),
        _ => {
            return Reply::failed(failed(
                "This Jadpo can't find its runtime's source".to_string(),
                format!(
                    "A deployable is compiled against the source of the runtime this Jadpo was built from, with its lock and toolchain, which should be in `{}`.",
                    sources.display()
                ),
            ));
        }
    };
    let folder = root.join("build/rust");
    let runtime = sources.display().to_string();
    // The CLDR data for the supported locales only (area 10, "Locales"):
    // `jadpo test` and `jadpo dev` keep every locale, the deployable
    // carries these.
    let tags: Vec<&str> = plan
        .runtime()
        .locales
        .as_ref()
        .map(|locales| {
            locales
                .supported
                .iter()
                .filter_map(|&ordinal| jadpo_std::LOCALES.get(ordinal as usize))
                .map(|(_, tag)| *tag)
                .collect()
        })
        .unwrap_or_default();
    let cldr = match jadpo_cldr::export(&tags, &folder.join(".cldr-export")) {
        Ok(cldr) => cldr,
        Err(error) => {
            return Reply::failed(failed(
                "The deployable's locale data couldn't be written".to_string(),
                format!(
                    "Jadpo exports the CLDR data of the app's supported locales for the deployable, and that failed, which is Jadpo's own mistake: {error}."
                ),
            ));
        }
    };
    let workspace = match jadpo_codegen::deployable::workspace(
        &plan,
        &jadpo_codegen::deployable::Deployable {
            name: APP,
            stamp: jadpo_codegen::deployable::Stamp {
                build: &build,
                commit: commit.as_deref(),
                modified,
                compiler: VERSION,
                target: TARGET,
            },
            stores: &stores,
            runtime: &runtime,
            toolchain: &toolchain,
            lock: &runtime_lock,
            cldr: &cldr,
        },
    ) {
        Ok(workspace) => workspace,
        Err(error) => {
            return Reply::failed(failed(
                "The deployable's Rust couldn't be written".to_string(),
                format!(
                    "Jadpo wrote Rust it can't compile, which is Jadpo's own mistake: {error}."
                ),
            ));
        }
    };
    if let Err(error) = workspace.write(&folder) {
        return Reply::failed(write_failed("build/rust/", &error));
    }
    say("Compiling the deployable");
    let epoch = call
        .environment
        .var("SOURCE_DATE_EPOCH")
        .and_then(|epoch| epoch.parse().ok())
        .or_else(|| commit.as_ref().and_then(|_| commit_time(&root)))
        .unwrap_or(0);
    let mut remap = vec![
        (folder.clone(), "/jadpo/build".to_string()),
        (sources.clone(), "/jadpo/runtime".to_string()),
    ];
    if let Some(home) = call
        .environment
        .var("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            call.environment
                .var("HOME")
                .map(|home| Path::new(&home).join(".cargo"))
        })
    {
        remap.push((home, "/cargo".to_string()));
    }
    let compiled = jadpo_codegen::deployable::build(&jadpo_codegen::deployable::Release {
        folder: &folder,
        name: APP,
        target: &folder.join("target"),
        remap: &remap,
        epoch,
    });
    let compiled = match compiled {
        Ok(binary) => binary,
        Err(error) => {
            return Reply::failed(failed(
                "The deployable didn't compile".to_string(),
                format!(
                    "cargo couldn't build the Rust Jadpo wrote for it in `build/rust/`, which is Jadpo's own mistake or a toolchain it can't run: {}",
                    last_lines(&error.to_string(), 12)
                ),
            ));
        }
    };
    let out = format!("build/target/{APP}");
    let binary = format!("{out}/{APP}");
    if let Err(error) = place(&root, &compiled, &binary) {
        return Reply::failed(write_failed(&binary, &error));
    }
    let openapi = match documents(&root, &written, &format!("{out}/openapi")) {
        Ok(documents) => documents,
        Err(error) => return Reply::failed(write_failed(&format!("{out}/openapi/"), &error)),
    };
    let elapsed = started.elapsed();
    built(Built {
        binary,
        openapi,
        build,
        commit,
        modified,
        settings,
        migrations: !stores.is_empty(),
        seconds: elapsed.as_secs_f64(),
    })
}

/// What a build wrote and the stamp it carries.
struct Built {
    binary: String,
    openapi: Vec<String>,
    build: String,
    commit: Option<String>,
    modified: bool,
    /// The variables its settings read.
    settings: Vec<String>,
    /// Whether it keeps migrations, which a database must have applied
    /// before it starts.
    migrations: bool,
    seconds: f64,
}

/// The success screen: where the binary is, its stamp and what it reads,
/// then the outcome and the one thing to do next.
fn built(built: Built) -> Reply {
    let mut fields = vec![("binary".to_string(), built.binary.clone())];
    fields.push(("build".to_string(), built.build.clone()));
    fields.push((
        "commit".to_string(),
        built.commit.as_deref().map_or_else(
            || "none".to_string(),
            |commit| commit.chars().take(7).collect::<String>(),
        ),
    ));
    if !built.settings.is_empty() {
        fields.push(("settings".to_string(), built.settings.join(", ")));
    }
    let mut screen = Screen::new().with(Block::fields(fields));
    if built.modified {
        screen.push(Block::Alerts(vec![match built.commit {
            Some(_) => {
                "Built from changes not committed: only this tree explains its events".to_string()
            }
            None => "Built from files git doesn't track: no commit explains its events".to_string(),
        }]));
    }
    screen.push(Block::summary(
        Outcome::Success,
        "built",
        [
            Fact::plain(APP),
            Fact::plain(format!("{TARGET} in {:.1} s", built.seconds)),
        ],
    ));
    screen.push(if built.migrations {
        Block::hint(
            "jadpo migrate apply",
            "it starts only once its database has every migration",
        )
    } else {
        Block::hint(
            format!("./{}", built.binary),
            "runs it, reading its settings from the environment",
        )
    });
    let data = Json::object([
        ("deployable", APP.into()),
        ("target", TARGET.into()),
        ("binary", built.binary.as_str().into()),
        (
            "openapi",
            Json::array(built.openapi.iter().map(String::as_str)),
        ),
        ("build", built.build.as_str().into()),
        (
            "commit",
            built.commit.as_deref().map_or(Json::Null, Json::from),
        ),
        ("modified", built.modified.into()),
        ("compiler", VERSION.into()),
        (
            "settings",
            Json::array(built.settings.iter().map(String::as_str)),
        ),
    ]);
    Reply::screen(screen, data)
}

/// The folder holding the runtime crates this Jadpo was built from.
fn runtime_sources() -> PathBuf {
    let cli = Path::new(env!("CARGO_MANIFEST_DIR"));
    cli.parent()
        .map_or_else(|| cli.to_path_buf(), Path::to_path_buf)
}

/// Area 18's build id: `b_` and the first twelve hex digits of a SHA-256
/// over the source's digest, the compiler, the target and the package
/// lock.
pub fn build_id(source: &str, compiler: &str, target: &str, lock: &[u8]) -> String {
    let mut hasher = Sha256::new();
    for part in [
        b"jadpo build id 1".as_slice(),
        source.as_bytes(),
        compiler.as_bytes(),
        target.as_bytes(),
        lock,
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("b_{}", &hex[..12])
}

/// The commit the project at `root` was built from, and whether its tree
/// holds changes the commit doesn't: no commit, and changed, when git
/// doesn't track the project.
fn commit(root: &Path) -> (Option<String>, bool) {
    let git = |args: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    let Some(commit) = git(&["rev-parse", "HEAD"]) else {
        return (None, true);
    };
    if git(&["ls-files", "--error-unmatch", "app.jadpo"]).is_none() {
        return (None, true);
    }
    let changes = git(&[
        "status",
        "--porcelain",
        "--",
        ".",
        ":(exclude)build",
        ":(exclude).jadpo",
    ]);
    (
        Some(commit),
        changes.is_none_or(|changes| !changes.is_empty()),
    )
}

/// When the commit at `root` was made, in seconds since 1970.
fn commit_time(root: &Path) -> Option<u64> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["log", "-1", "--format=%ct"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

/// Copies the compiled binary to `to`, from the project's root.
fn place(root: &Path, compiled: &Path, to: &str) -> std::io::Result<()> {
    let to = root.join(to);
    if let Some(folder) = to.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::copy(compiled, &to).map(|_| ())
}

/// Copies each OpenAPI document and failure file the check wrote in
/// `build/openapi/` into `to`, which holds nothing else: their paths, from
/// the project's root.
fn documents(root: &Path, written: &[String], to: &str) -> std::io::Result<Vec<String>> {
    let folder = root.join(to);
    if folder.exists() {
        std::fs::remove_dir_all(&folder)?;
    }
    let mut out = Vec::new();
    let mut found: Vec<&String> = written
        .iter()
        .filter(|path| path.starts_with("build/openapi/"))
        .collect();
    found.sort();
    for path in found {
        let Some(name) = Path::new(path).file_name() else {
            continue;
        };
        std::fs::create_dir_all(&folder)?;
        std::fs::copy(root.join(path), folder.join(name))?;
        out.push(format!("{to}/{}", name.to_string_lossy()));
    }
    Ok(out)
}

/// The last `count` lines of `text`, joined.
fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text.trim_end().lines().collect();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

fn failed(message: String, reason: String) -> Diagnostic {
    Diagnostic::new(&rule::TOOLCHAIN_BUILD_FAILED, message)
        .reason(reason)
        .next_for(
            "Run `jadpo build` again; if it fails the same way, report it with this output",
            Decider::Human,
        )
}

fn write_failed(path: &str, error: &std::io::Error) -> Diagnostic {
    Diagnostic::new(
        &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
        format!("`{path}` couldn't be written"),
    )
    .reason(format!("Writing it failed: {error}."))
    .next_for(
        "Check the project's `build/` folder can be written, then run `jadpo build` again",
        Decider::Human,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_id_digests_the_source_the_compiler_the_target_and_the_lock() {
        let id = build_id("4b1d", "0.1.0", "native", b"");
        assert!(id.starts_with("b_") && id.len() == 14, "{id}");
        assert_eq!(id, build_id("4b1d", "0.1.0", "native", b""));
        for other in [
            build_id("4b1e", "0.1.0", "native", b""),
            build_id("4b1d", "0.1.1", "native", b""),
            build_id("4b1d", "0.1.0", "wasm", b""),
            build_id("4b1d", "0.1.0", "native", b"link ../couriers\n"),
        ] {
            assert_ne!(id, other);
        }
    }

    #[test]
    fn only_the_last_lines_of_what_cargo_said_are_kept() {
        assert_eq!(last_lines("a\nb\nc\n", 2), "b\nc");
        assert_eq!(last_lines("a", 5), "a");
    }
}

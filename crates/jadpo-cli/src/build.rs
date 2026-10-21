//! `jadpo build` (area 14, "The `jadpo` command"; roadmap chunks 8.1o
//! and 8.1o2; docs/compiler-runtime.md §2.6, "A native deployable", and
//! §2.8): checks the project as `jadpo check` does, writing the same
//! derived files, and stops at its errors; otherwise writes each
//! deployable's Rust into `build/rust/<deployable>/` and compiles it in
//! release, offline, with the toolchain this Jadpo pins and the runtime
//! crates' lock, into one native binary in `build/target/<deployable>/`,
//! beside the OpenAPI documents of the APIs it serves and each
//! `<api>.failures.<locale>.json` (area 10).
//!
//! `jadpo build` builds every deployable, `jadpo build <deployable>` one.
//! With no `deployables:` declared there is one, `app`, which serves every
//! API and runs everything, natively (area 14, question 14); a declared
//! one serves only its APIs' routes and runs only the background work its
//! `runs:` names (`jadpo_plan::Plan::for_deployable`). One built for
//! `Target.wasm` is refused as not built yet: WebAssembly on Workers is
//! roadmap chunk 8.2. Each deployable's build stamp (area 18;
//! [`crate::stamp`]) is a constant in its binary and on every event it
//! logs: the build id; the commit, and whether the tree held changes it
//! doesn't (a project git doesn't track counts as changed, with no
//! commit); the compiler; the deployable; and the target.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use jadpo_check::deployables::Deployable;
use jadpo_diag::{Decider, Diagnostic, nearest, rule};
use jadpo_std::DeployTarget;
use jadpo_term::{Block, Fact, Json, Outcome, Screen};

use crate::check::{Checked, checked, reported};
use crate::fmt::project_needed;
use crate::help::VERSION;
use crate::run::{Call, Reply, say};
use crate::stamp;

pub fn build(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_needed("build");
    };
    let started = Instant::now();
    say("Checking");
    let checked = checked(call, &root);
    if !checked.diagnostics.is_empty() {
        return reported(&cwd, checked, started.elapsed(), "build stopped");
    }
    let Checked {
        loaded, written, ..
    } = checked;
    let db = &loaded.db;
    let project = loaded.project;
    let declared = jadpo_check::deployables::deployables(db, project);
    let chosen: Vec<&Deployable> = match call.arguments.first() {
        Some(name) => match declared.get(name) {
            Some(deployable) => vec![deployable],
            None => return Reply::failed(unknown(name, declared)),
        },
        None => declared.list.iter().collect(),
    };
    if let Some(wasm) = chosen
        .iter()
        .find(|deployable| deployable.builds_for() == DeployTarget::Wasm)
    {
        return Reply::failed(not_built_yet(wasm, declared));
    }
    let lock = std::fs::read(root.join(jadpo_db::LOCK)).unwrap_or_default();
    let graph = jadpo_check::graph::graph(db, project).digest();
    let source = stamp::commit(&root);
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
    // Every deployable's workspace is under one folder and builds into one
    // target, its paths remapped alike, so the runtime crates compile once
    // for them all.
    let rust = root.join("build/rust");
    let epoch = call
        .environment
        .var("SOURCE_DATE_EPOCH")
        .and_then(|epoch| epoch.parse().ok())
        .or_else(|| source.commit.as_ref().and_then(|_| commit_time(&root)))
        .unwrap_or(0);
    let mut remap = vec![
        (rust.clone(), "/jadpo/build".to_string()),
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
    let runtime = sources.display().to_string();
    let mut settings: Vec<String> = Vec::new();
    let mut built = Vec::new();
    for deployable in chosen {
        let name = deployable.name.as_str();
        say(&format!("Writing `{name}`"));
        let target = deployable.builds_for().name();
        let build = stamp::build_id(&stamp::source(&graph, deployable), VERSION, target, &lock);
        let plan = jadpo_plan::Plan::build(db, project)
            .for_deployable(&deployable.serves, deployable.runs)
            .with_build(build.clone());
        settings = plan
            .runtime()
            .config
            .fields
            .iter()
            .map(|field| field.variable.clone())
            .collect();
        let folder = rust.join(name);
        let workspace = match jadpo_codegen::deployable::workspace(
            &plan,
            &jadpo_codegen::deployable::Deployable {
                name,
                stamp: jadpo_codegen::deployable::Stamp {
                    build: &build,
                    commit: source.commit.as_deref(),
                    modified: source.modified,
                    compiler: VERSION,
                    target,
                },
                stores: &stores,
                runtime: &runtime,
                toolchain: &toolchain,
                lock: &runtime_lock,
            },
        ) {
            Ok(workspace) => workspace,
            Err(error) => {
                return Reply::failed(failed(
                    format!("`{name}`'s Rust couldn't be written"),
                    format!(
                        "Jadpo wrote Rust it can't compile, which is Jadpo's own mistake: {error}."
                    ),
                ));
            }
        };
        if let Err(error) = workspace.write(&folder) {
            return Reply::failed(write_failed(&format!("build/rust/{name}/"), &error));
        }
        say(&format!("Compiling `{name}`"));
        let compiled = jadpo_codegen::deployable::build(&jadpo_codegen::deployable::Release {
            folder: &folder,
            name,
            target: &rust.join("target"),
            remap: &remap,
            epoch,
        });
        let compiled = match compiled {
            Ok(binary) => binary,
            Err(error) => {
                return Reply::failed(failed(
                    format!("`{name}` didn't compile"),
                    format!(
                        "cargo couldn't build the Rust Jadpo wrote for it in `build/rust/{name}/`, which is Jadpo's own mistake or a toolchain it can't run: {}",
                        last_lines(&error.to_string(), 12)
                    ),
                ));
            }
        };
        let out = format!("build/target/{name}");
        let binary = format!("{out}/{name}");
        if let Err(error) = place(&root, &compiled, &binary) {
            return Reply::failed(write_failed(&binary, &error));
        }
        let openapi = match documents(
            &root,
            &written,
            &deployable.serves,
            &format!("{out}/openapi"),
        ) {
            Ok(documents) => documents,
            Err(error) => return Reply::failed(write_failed(&format!("{out}/openapi/"), &error)),
        };
        built.push(Binary {
            name: name.to_string(),
            declared: deployable.at.is_some(),
            target,
            binary,
            openapi,
            build,
            serves: deployable.serves.clone(),
            runs: deployable.runs_words(),
        });
    }
    let elapsed = started.elapsed();
    shown(Built {
        binaries: built,
        commit: source.commit,
        modified: source.modified,
        settings,
        migrations: !stores.is_empty(),
        seconds: elapsed.as_secs_f64(),
    })
}

/// The project's deployables, for Tab completion: each declared one, or
/// `app`.
pub(crate) fn names(cwd: &Path) -> Vec<(String, &'static str)> {
    let Some(root) = jadpo_db::find_root(cwd) else {
        return Vec::new();
    };
    let loaded = jadpo_db::load(&root);
    jadpo_check::deployables::deployables(&loaded.db, loaded.project)
        .list
        .iter()
        .map(|deployable| (deployable.name.clone(), "Deployable"))
        .collect()
}

/// A name `jadpo build` was given that isn't one of the deployables.
fn unknown(name: &str, declared: &jadpo_check::deployables::Deployables) -> Diagnostic {
    let problem = Diagnostic::new(
        &rule::CLI_BUILD_ARGUMENTS,
        format!("`{name}` isn't one of the app's deployables"),
    );
    if !declared.declared {
        return problem
            .reason(format!(
                "The app declares no `deployables:` in `app.jadpo`, so it has one deployable, `{}`, which runs everything.",
                jadpo_check::deployables::APP
            ))
            .next(format!(
                "Run `jadpo build`, or `jadpo build {}`",
                jadpo_check::deployables::APP
            ));
    }
    let names = declared.names();
    let near = nearest(name, names.iter().copied());
    let listed: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    problem
        .reason(format!(
            "`app.jadpo` declares {}.{}",
            listed.join(", "),
            near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
        ))
        .next(match near {
            Some(near) => format!("Run `jadpo build {near}`"),
            None => "Run `jadpo build` to build them all, or name one of them".to_string(),
        })
}

/// `deployable` is built for Workers, which comes with chunk 8.2.
fn not_built_yet(
    deployable: &Deployable,
    declared: &jadpo_check::deployables::Deployables,
) -> Diagnostic {
    let native = declared
        .list
        .iter()
        .find(|other| other.builds_for() == DeployTarget::Native);
    Diagnostic::new(
        &rule::TOOLCHAIN_NOT_BUILT_YET,
        format!(
            "`{}` is built for Workers, which `jadpo build` can't do yet",
            deployable.name
        ),
    )
    .reason(format!(
        "`{}` declares `target: Target.wasm`; building WebAssembly for Cloudflare Workers comes with chunk 8.2 of the roadmap. `jadpo check` already holds it to what Workers runs.",
        deployable.name
    ))
    .next(match native {
        Some(native) => format!(
            "Build a native deployable by name, `jadpo build {}`",
            native.name
        ),
        None => format!(
            "Give `{}` `target: Target.native` to build it as a server for now",
            deployable.name
        ),
    })
}

/// One deployable's binary, as a build wrote it.
struct Binary {
    name: String,
    /// Whether `deployables:` declares it, rather than it being `app`.
    declared: bool,
    target: &'static str,
    binary: String,
    openapi: Vec<String>,
    build: String,
    serves: Vec<String>,
    runs: Vec<&'static str>,
}

/// What a build wrote and the stamp its binaries carry.
struct Built {
    binaries: Vec<Binary>,
    commit: Option<String>,
    modified: bool,
    /// The variables its settings read.
    settings: Vec<String>,
    /// Whether it keeps migrations, which a database must have applied
    /// before it starts.
    migrations: bool,
    seconds: f64,
}

/// The success screen: where each binary is, its build id and, for a
/// declared deployable, what it serves and runs; then the commit and the
/// settings they read, the outcome and the one thing to do next.
fn shown(built: Built) -> Reply {
    let mut screen = Screen::new();
    let mut shared = vec![(
        "commit".to_string(),
        built.commit.as_deref().map_or_else(
            || "none".to_string(),
            |commit| commit.chars().take(7).collect::<String>(),
        ),
    )];
    if !built.settings.is_empty() {
        shared.push(("settings".to_string(), built.settings.join(", ")));
    }
    let one = built.binaries.len() == 1;
    for binary in &built.binaries {
        let mut fields = vec![
            ("binary".to_string(), binary.binary.clone()),
            ("build".to_string(), binary.build.clone()),
        ];
        if binary.declared {
            if !binary.serves.is_empty() {
                fields.push(("serves".to_string(), binary.serves.join(", ")));
            }
            if !binary.runs.is_empty() {
                fields.push(("runs".to_string(), binary.runs.join(", ")));
            }
        }
        if one {
            fields.append(&mut shared);
        }
        screen.push(Block::fields(fields));
    }
    if !shared.is_empty() {
        screen.push(Block::fields(shared));
    }
    if built.modified {
        screen.push(Block::Alerts(vec![match built.commit {
            Some(_) => {
                "Built from changes not committed: only this tree explains its events".to_string()
            }
            None => "Built from files git doesn't track: no commit explains its events".to_string(),
        }]));
    }
    let names: Vec<&str> = built
        .binaries
        .iter()
        .map(|binary| binary.name.as_str())
        .collect();
    screen.push(Block::summary(
        Outcome::Success,
        "built",
        [
            Fact::plain(names.join(", ")),
            Fact::plain(format!("native in {:.1} s", built.seconds)),
        ],
    ));
    screen.push(if built.migrations {
        Block::hint(
            "jadpo migrate apply",
            if one {
                "it starts only once its database has every migration"
            } else {
                "they start only once their database has every migration"
            },
        )
    } else {
        Block::hint(
            format!("./{}", built.binaries[0].binary),
            "runs it, reading its settings from the environment",
        )
    });
    let data = Json::object([
        (
            "deployables",
            Json::array(built.binaries.iter().map(|binary| {
                Json::object([
                    ("deployable", binary.name.as_str().into()),
                    ("target", binary.target.into()),
                    ("binary", binary.binary.as_str().into()),
                    (
                        "openapi",
                        Json::array(binary.openapi.iter().map(String::as_str)),
                    ),
                    ("build", binary.build.as_str().into()),
                    (
                        "serves",
                        Json::array(binary.serves.iter().map(String::as_str)),
                    ),
                    ("runs", Json::array(binary.runs.iter().copied())),
                ])
            })),
        ),
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

/// Copies the compiled binary to `to`, from the project's root, as a new
/// file: macOS kills a binary copied over one it has run, whose signature
/// it keeps for that file.
fn place(root: &Path, compiled: &Path, to: &str) -> std::io::Result<()> {
    let to = root.join(to);
    if let Some(folder) = to.parent() {
        std::fs::create_dir_all(folder)?;
    }
    match std::fs::remove_file(&to) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    std::fs::copy(compiled, &to).map(|_| ())
}

/// Copies each OpenAPI document and failure file the check wrote in
/// `build/openapi/` for an API in `serves` into `to`, which holds nothing
/// else: their paths, from the project's root.
fn documents(
    root: &Path,
    written: &[String],
    serves: &[String],
    to: &str,
) -> std::io::Result<Vec<String>> {
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
        // `<api>.json`, `<api>.failures.<locale>.json`.
        let api = name.to_string_lossy();
        let api = api.split('.').next().unwrap_or_default();
        if !serves.iter().any(|served| served == api) {
            continue;
        }
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
    fn only_the_last_lines_of_what_cargo_said_are_kept() {
        assert_eq!(last_lines("a\nb\nc\n", 2), "b\nc");
        assert_eq!(last_lines("a", 5), "a");
    }
}

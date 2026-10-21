//! `jadpo build` (area 14, "The `jadpo` command"; roadmap chunks 8.1o,
//! 8.1o2, 8.1o3 and 8.2b; docs/compiler-runtime.md §2.6, "A native deployable"
//! and "A Workers deployable", and §2.8): checks the project as `jadpo
//! check` does, writing the same derived files, and stops at its errors;
//! otherwise writes each deployable's Rust into `build/rust/<deployable>/`
//! and compiles it in release, offline, with the toolchain this Jadpo pins
//! and the runtime crates' lock, into `build/target/<deployable>/`, beside
//! the OpenAPI documents of the APIs it serves and each
//! `<api>.failures.<locale>.json` (area 10): natively one binary; for
//! Workers (`Target.wasm`) a WebAssembly module bound by wasm-bindgen's
//! library, Jadpo's module shim and a Wrangler file, the module's size
//! shown raw and gzipped. Each carries the CLDR data of the app's
//! supported locales only (chunk 8.2c), from `build/rust/cldr/`, where
//! `jadpo test` and `jadpo dev` keep every locale.
//!
//! `jadpo build` builds every deployable, `jadpo build <deployable>` one.
//! With no `deployables:` declared there is one, `app`, which serves every
//! API and runs everything, natively (area 14, question 14); a declared
//! one serves only its APIs' routes and runs only the background work its
//! `runs:` names. Each is compiled from only what it reaches (chunk
//! 8.1o3; `jadpo_plan::Plan::for_deployable`): its callables, routes,
//! connections and settings, so a Worker links only the drivers it
//! reaches. Each deployable's build stamp (area 18;
//! [`crate::stamp`]) is a constant in its binary or module and on every
//! event it logs: the build id; the commit, and whether the tree held
//! changes it doesn't (a project git doesn't track counts as changed, with
//! no commit); the compiler; the deployable; and the target, `native` or
//! `wasm`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use jadpo_check::deployables::Deployable;
use jadpo_codegen::deployable::{Drivers, Host};
use jadpo_codegen::workers;
use jadpo_diag::{Decider, Diagnostic, nearest, rule};
use jadpo_plan::Plan;
use jadpo_std::DeployTarget;
use jadpo_std::connections::ConnectionDriver;
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
    let lock = std::fs::read(root.join(jadpo_db::LOCK)).unwrap_or_default();
    let source = stamp::commit(&root);
    let stores = stamp::stores(db, project);
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
    let cldr = match carried_locales(db, project, &rust) {
        Ok(cldr) => cldr,
        Err(problem) => return Reply::failed(*problem),
    };
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
    // The variables the deployables' settings read, each once, in the
    // order they come: each reads only those of what it reaches.
    let mut settings: Vec<String> = Vec::new();
    let mut built = Vec::new();
    for deployable in chosen {
        let name = deployable.name.as_str();
        say(&format!("Writing `{name}`"));
        let target = deployable.builds_for().name();
        let stamp::Program { plan, build } = match stamp::program(
            db, project, deployable, VERSION, &lock, &stores,
        ) {
            Ok(program) => program,
            Err(error) => {
                return Reply::failed(failed(
                    format!("`{name}`'s program couldn't be planned"),
                    format!(
                        "Jadpo plans a deployable from what it reaches, and that went wrong, which is Jadpo's own mistake: {error}."
                    ),
                ));
            }
        };
        let host = match deployable.builds_for() {
            DeployTarget::Native => Host::Native,
            DeployTarget::Wasm => Host::Workers(drivers(&plan)),
        };
        for field in &plan.runtime().config.fields {
            if !settings.contains(&field.variable) {
                settings.push(field.variable.clone());
            }
        }
        let folder = rust.join(name);
        let workspace = match jadpo_codegen::deployable::workspace(
            &plan,
            &jadpo_codegen::deployable::Deployable {
                name,
                host,
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
            host,
            target: &rust.join("target"),
            remap: &remap,
            epoch,
            cldr: &cldr,
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
        // What an earlier build left there, perhaps for the other target,
        // goes first.
        if let Err(error) = cleared(&root.join(&out)) {
            return Reply::failed(write_failed(&format!("{out}/"), &error));
        }
        let artifact = match host {
            Host::Native => {
                let binary = format!("{out}/{name}");
                if let Err(error) = place(&root, &compiled, &binary) {
                    return Reply::failed(write_failed(&binary, &error));
                }
                Artifact::Native { binary }
            }
            Host::Workers(_) => {
                say(&format!("Binding `{name}`"));
                let bound = match workers::bind(&compiled, name) {
                    Ok(bound) => bound,
                    Err(error) => {
                        return Reply::failed(failed(
                            format!("`{name}`'s module couldn't be bound"),
                            format!(
                                "wasm-bindgen couldn't bind the module cargo built for it, which is Jadpo's own mistake: {error}."
                            ),
                        ));
                    }
                };
                let required: Vec<String> = plan
                    .runtime()
                    .config
                    .fields
                    .iter()
                    .filter(|field| field.default.is_none())
                    .map(|field| field.variable.clone())
                    .collect();
                let module = format!("{out}/{name}.wasm");
                let wrangler = format!("{out}/{}", workers::WRANGLER);
                let (bytes, gzip) = (bound.wasm.len(), gzipped(&bound.wasm));
                for (path, contents) in [
                    (module.clone(), bound.wasm),
                    (format!("{out}/{name}.js"), bound.js.into_bytes()),
                    (
                        format!("{out}/{}", workers::SHIM),
                        workers::shim(name).into_bytes(),
                    ),
                    (
                        wrangler.clone(),
                        workers::wrangler(name, &required).into_bytes(),
                    ),
                ] {
                    if let Err(error) = write_new(&root, &path, &contents) {
                        return Reply::failed(write_failed(&path, &error));
                    }
                }
                Artifact::Worker {
                    wrangler,
                    module,
                    bytes,
                    gzipped: gzip,
                }
            }
        };
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
            artifact,
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

/// Writes the CLDR data every deployable carries, its app's supported
/// locales' only (area 10, "Locales"; chunk 8.2c), into `rust`'s `cldr/`,
/// each file only where it changed: the folder. The locales are the app's,
/// so every deployable reads the one folder and the runtime crates compile
/// once for them all.
fn carried_locales(
    db: &dyn jadpo_db::Db,
    project: jadpo_db::Project,
    rust: &Path,
) -> Result<PathBuf, Box<Diagnostic>> {
    let tags: Vec<&str> = jadpo_check::locales::locales(db, project)
        .plan
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
    let files = jadpo_cldr::export(&tags, &rust.join(".cldr-export")).map_err(|error| {
        Box::new(failed(
            "The deployables' locale data couldn't be written".to_string(),
            format!(
                "Jadpo exports the CLDR data of the app's supported locales for its deployables, and that failed, which is Jadpo's own mistake: {error}."
            ),
        ))
    })?;
    let folder = rust.join("cldr");
    jadpo_codegen::Generated { files }
        .write(&folder)
        .map_err(|error| Box::new(write_failed("build/rust/cldr/", &error)))?;
    Ok(folder)
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

/// The drivers a Worker built from `plan` links beyond the package
/// driver: PostgreSQL when the project keeps a database, since every
/// deployable's tick reaches each store, and SMTP when it reaches an SMTP
/// connection, the only ones its plan holds (chunk 8.1o3).
fn drivers(plan: &Plan) -> Drivers {
    let connections = &plan.runtime().connections;
    Drivers {
        postgres: connections
            .iter()
            .any(|connection| matches!(connection.driver, ConnectionDriver::Postgres { .. })),
        smtp: connections
            .iter()
            .any(|connection| matches!(connection.driver, ConnectionDriver::Smtp(_))),
    }
}

/// One deployable as a build wrote it.
struct Binary {
    name: String,
    /// Whether `deployables:` declares it, rather than it being `app`.
    declared: bool,
    target: &'static str,
    artifact: Artifact,
    openapi: Vec<String>,
    build: String,
    serves: Vec<String>,
    runs: Vec<&'static str>,
}

/// What a deployable is deployed from, by path from the project's root.
enum Artifact {
    /// A server's binary.
    Native { binary: String },
    /// A Worker: its Wrangler file and its module, beside the module's
    /// JavaScript and Jadpo's shim, and the module's size, raw and
    /// gzipped.
    Worker {
        wrangler: String,
        module: String,
        bytes: usize,
        gzipped: usize,
    },
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

/// The success screen: where each binary or Worker is, a module's size,
/// its build id and, for a declared deployable, what it serves and runs;
/// then the commit and the settings they read, the outcome and the one
/// thing to do next.
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
        let mut fields = match &binary.artifact {
            Artifact::Native { binary } => vec![("binary".to_string(), binary.clone())],
            Artifact::Worker {
                wrangler,
                module,
                bytes,
                gzipped,
            } => vec![
                ("worker".to_string(), wrangler.clone()),
                ("module".to_string(), module.clone()),
                (
                    "size".to_string(),
                    format!("{}, {} gzipped", size(*bytes), size(*gzipped)),
                ),
            ],
        };
        fields.push(("build".to_string(), binary.build.clone()));
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
    let mut alerts = Vec::new();
    if built.modified {
        alerts.push(match built.commit {
            Some(_) => {
                "Built from changes not committed: only this tree explains its events".to_string()
            }
            None => "Built from files git doesn't track: no commit explains its events".to_string(),
        });
    }
    let workers: Vec<String> = built
        .binaries
        .iter()
        .filter(|binary| matches!(binary.artifact, Artifact::Worker { .. }))
        .map(|binary| format!("`{}`", binary.name))
        .collect();
    if !workers.is_empty() {
        alerts.push(format!(
            "{} {} 503 to every request: serving on Workers is chunk 8.2d",
            joined(&workers),
            if workers.len() == 1 {
                "answers"
            } else {
                "answer"
            }
        ));
    }
    if !alerts.is_empty() {
        screen.push(Block::Alerts(alerts));
    }
    let names: Vec<String> = built
        .binaries
        .iter()
        .map(|binary| binary.name.clone())
        .collect();
    // `native`, `wasm`, or `wasm and native`, as the deployables come.
    let mut targets: Vec<String> = Vec::new();
    for binary in &built.binaries {
        if !targets.iter().any(|target| target == binary.target) {
            targets.push(binary.target.to_string());
        }
    }
    screen.push(Block::summary(
        Outcome::Success,
        "built",
        [
            Fact::plain(joined(&names)),
            Fact::plain(format!("{} in {:.1} s", joined(&targets), built.seconds)),
        ],
    ));
    if built.binaries.is_empty() {
        return Reply::screen(screen, Json::Null);
    }
    let native = built
        .binaries
        .iter()
        .find_map(|binary| match &binary.artifact {
            Artifact::Native { binary } => Some(binary.as_str()),
            Artifact::Worker { .. } => None,
        });
    if built.migrations {
        screen.push(Block::hint(
            "jadpo migrate apply",
            if one {
                "it starts only once its database has every migration"
            } else {
                "they start only once their database has every migration"
            },
        ));
    } else if let Some(binary) = native {
        screen.push(Block::hint(
            format!("./{binary}"),
            "runs it, reading its settings from the environment",
        ));
    }
    let data = Json::object([
        (
            "deployables",
            Json::array(built.binaries.iter().map(|binary| {
                let (native, wrangler, module) = match &binary.artifact {
                    Artifact::Native { binary } => (binary.as_str().into(), Json::Null, Json::Null),
                    Artifact::Worker {
                        wrangler,
                        module,
                        bytes,
                        gzipped,
                    } => (
                        Json::Null,
                        wrangler.as_str().into(),
                        Json::object([
                            ("path", module.as_str().into()),
                            ("bytes", (*bytes).into()),
                            ("gzip_bytes", (*gzipped).into()),
                        ]),
                    ),
                };
                Json::object([
                    ("deployable", binary.name.as_str().into()),
                    ("target", binary.target.into()),
                    ("binary", native),
                    ("wrangler", wrangler),
                    ("module", module),
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

/// `kiosk`, `kiosk and office`, `kiosk, office and edge`: one fact,
/// which plain output joins to the next with a comma.
fn joined(names: &[String]) -> String {
    match names {
        [rest @ .., last] if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => names.join(""),
    }
}

/// `bytes` for people, in decimal units as Cloudflare states its limits:
/// `912 bytes`, `412 KB`, `4.1 MB`.
fn size(bytes: usize) -> String {
    match bytes {
        0..1_000 => format!("{bytes} bytes"),
        1_000..999_500 => format!("{} KB", (bytes + 500) / 1_000),
        _ => format!("{:.1} MB", bytes as f64 / 1_000_000.0),
    }
}

/// How many bytes `data` takes gzipped at zlib's default level, as
/// Wrangler reports an upload's: the deflated data and gzip's 18 bytes
/// around it (RFC 1952).
fn gzipped(data: &[u8]) -> usize {
    miniz_oxide::deflate::compress_to_vec(data, 6).len() + 18
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

/// Removes `folder` and what it holds, if it is there.
fn cleared(folder: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(folder) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Writes `bytes` to `to`, from the project's root, as a new file.
fn write_new(root: &Path, to: &str, bytes: &[u8]) -> std::io::Result<()> {
    let to = root.join(to);
    if let Some(folder) = to.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(to, bytes)
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
    fn a_size_is_given_in_decimal_units() {
        assert_eq!(size(912), "912 bytes");
        assert_eq!(size(412_300), "412 KB");
        assert_eq!(size(999_499), "999 KB");
        assert_eq!(size(999_500), "1.0 MB");
        assert_eq!(size(19_665_209), "19.7 MB");
    }

    #[test]
    fn a_size_gzipped_is_the_deflated_data_and_gzips_frame() {
        let data = b"jadpo ".repeat(10_000);
        let deflated = miniz_oxide::deflate::compress_to_vec(&data, 6).len();
        assert_eq!(gzipped(&data), deflated + 18);
        assert!(gzipped(&data) < data.len() / 20);
    }

    #[test]
    fn names_and_targets_are_joined_as_one_fact() {
        let words = |list: &[&str]| list.iter().map(|word| word.to_string()).collect::<Vec<_>>();
        assert_eq!(joined(&words(&["kiosk"])), "kiosk");
        assert_eq!(joined(&words(&["wasm", "native"])), "wasm and native");
        assert_eq!(
            joined(&words(&["kiosk", "office", "edge"])),
            "kiosk, office and edge"
        );
    }

    #[test]
    fn only_the_last_lines_of_what_cargo_said_are_kept() {
        assert_eq!(last_lines("a\nb\nc\n", 2), "b\nc");
        assert_eq!(last_lines("a", 5), "a");
    }
}

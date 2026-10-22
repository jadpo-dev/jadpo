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
//! shown raw and gzipped, each database's address a Hyperdrive binding
//! whose id is read from `JADPO_HYPERDRIVE_<variable>` (chunk 8.2i). Each
//! carries the CLDR data of the app's
//! supported locales only (chunk 8.2c), where `jadpo test` and `jadpo
//! dev` keep every locale. Each is compiled into a cargo target shared by
//! every project this Jadpo builds for the same host and locale data, in
//! the user's cache folder (chunk 8.4g; [`builds`]), so only an app's own
//! crates compile after the first build.
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
    // Every deployable's workspace is under one folder, each compiled into
    // a target shared with every project this Jadpo builds for its host
    // and locale set (docs/compiler-runtime.md §2.5, "Keeping rustc time
    // down"), so the runtime crates compile once for them all.
    let rust = root.join("build/rust");
    let builds = builds(call, &root, &sources);
    let cldr = match carried_locales(db, project, &builds) {
        Ok(cldr) => cldr,
        Err(problem) => return Reply::failed(*problem),
    };
    // What a build before chunk 8.4g kept in the project, which nothing
    // reads now.
    for leftover in ["target", "cldr", ".cldr-export"] {
        let _ = std::fs::remove_dir_all(rust.join(leftover));
    }
    let epoch = call
        .environment
        .var("SOURCE_DATE_EPOCH")
        .and_then(|epoch| epoch.parse().ok())
        .or_else(|| source.commit.as_ref().and_then(|_| commit_time(&root)))
        .unwrap_or(0);
    // The workspace's own paths are relative to it; every other path a
    // build reads is remapped, the same for every project, so the shared
    // target's units are the same for each.
    let mut remap = vec![
        (builds.clone(), "/jadpo/builds".to_string()),
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
    // Each `max_concurrent:` a Worker reaches, which counts in each of its
    // instances (area 11, "Decisions", 2026-10-21).
    let caps = jadpo_check::deployables::per_instance(db, project);
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
            target: &builds.join("targets").join(format!(
                "{target}-{}{}",
                cldr.digest,
                profile_overrides()
            )),
            remap: &remap,
            epoch,
            cldr: &cldr.folder,
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
        // Another scheduler runs a native deployable's `tick` command
        // when `schedules: { trigger: }` leaves its ticks to one (chunk
        // 8.2g), as often as the server's timer would tick.
        let schedules = plan.schedules();
        let tick = (host == Host::Native
            && schedules.trigger == jadpo_std::ScheduleTrigger::External
            && schedules.ticks())
        .then(|| jadpo_runtime::schedules::tick_every_ms(Some(schedules)));
        let artifact = match host {
            Host::Native => {
                let binary = format!("{out}/{name}");
                if let Err(error) = place(&root, &compiled, &binary) {
                    return Reply::failed(write_failed(&binary, &error));
                }
                Artifact::Native { binary, tick }
            }
            Host::Workers(_) => {
                say(&format!("Binding `{name}`"));
                let bound = match workers::bind(&compiled, name, workers::Names::Removed) {
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
                let hyperdrive = hyperdrive(&plan, call);
                let required: Vec<String> = plan
                    .runtime()
                    .config
                    .fields
                    .iter()
                    .filter(|field| field.default.is_none())
                    .map(|field| field.variable.clone())
                    .filter(|variable| {
                        !hyperdrive
                            .iter()
                            .any(|binding| &binding.binding == variable)
                    })
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
                        workers::wrangler(&workers::Wrangler {
                            app: project.name(db),
                            deployable: name,
                            required: &required,
                            hyperdrive: &hyperdrive,
                            ticks: plan.schedules().ticks(),
                            rate_limits: plan
                                .routes()
                                .rate_limits
                                .as_ref()
                                .map_or(&[][..], |store| &store.bindings),
                            // Its queues on Cloudflare Queues, kept where
                            // it runs the subscribers (chunk 8.2k).
                            queues: &plan.providers().queues,
                        })
                        .into_bytes(),
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
                    hyperdrive,
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
            per_instance: caps
                .iter()
                .filter(|cap| cap.deployable == name)
                .map(|cap| (cap.connection.clone(), cap.cap))
                .collect(),
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

/// The folder this Jadpo keeps every project's deployable builds in
/// (docs/compiler-runtime.md §2.5, "Keeping rustc time down"):
/// `jadpo/builds/<version>-<runtime>/` in the user's cache folder, as the
/// package cache is (`$XDG_CACHE_HOME`, else `~/.cache`), `<runtime>` a
/// digest of the folder of the runtime's source, so two copies of Jadpo
/// of one version never share a target; with neither variable, the
/// project's own `build/rust/cache/`.
fn builds(call: &Call, root: &Path, sources: &Path) -> PathBuf {
    let Some(cache) = crate::package_cache::home(&call.environment) else {
        return root.join("build/rust/cache");
    };
    let cache = std::path::absolute(&cache).unwrap_or(cache);
    cache.join("jadpo").join("builds").join(format!(
        "{VERSION}-{}",
        digest([sources.as_os_str().as_encoded_bytes()], 12)
    ))
}

/// The first `length` hex digits of the SHA-256 of `parts`, each
/// length-prefixed.
fn digest<'a>(parts: impl IntoIterator<Item = &'a [u8]>, length: usize) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    let hex: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    hex[..length].to_string()
}

/// Cargo's own `CARGO_PROFILE_RELEASE_*` variables override the profile a
/// deployable's workspace gives (chunk 8.4f), as the gate's do for the
/// deployables its tests build, so a build with them keeps a target of
/// its own rather than rebuilding the shared one's units each time it
/// alternates with one without: `-` and their digest, else nothing.
fn profile_overrides() -> String {
    let mut overrides: Vec<(String, String)> = std::env::vars_os()
        .filter_map(|(name, value)| {
            let name = name.into_string().ok()?;
            name.starts_with("CARGO_PROFILE_RELEASE_")
                .then(|| (name, value.to_string_lossy().into_owned()))
        })
        .collect();
    if overrides.is_empty() {
        return String::new();
    }
    overrides.sort();
    let parts: Vec<&[u8]> = overrides
        .iter()
        .flat_map(|(name, value)| [name.as_bytes(), value.as_bytes()])
        .collect();
    format!("-{}", digest(parts, 8))
}

/// The CLDR data a deployable carries, in its own folder.
struct Locales {
    /// `locales/<digest>/` in [`builds`]' folder.
    folder: PathBuf,
    /// The digest of the files, which names the folder and the targets
    /// built with it.
    digest: String,
}

/// Writes the CLDR data every deployable carries, its app's supported
/// locales' only (area 10, "Locales"; chunk 8.2c), into a folder of
/// `builds` named by the digest of the files, shared by every project
/// whose locales give the same files (chunk 8.4g), so their builds share
/// a target. The folder is written whole under another name and renamed
/// into place, so a build reading it never sees it half written; a build
/// beside this one reuses the complete folder. Verification/publication
/// holds a per-digest filesystem lock; missing or corrupt cached files are
/// rebuilt before the folder is handed to cargo.
fn carried_locales(
    db: &dyn jadpo_db::Db,
    project: jadpo_db::Project,
    builds: &Path,
) -> Result<Locales, Box<Diagnostic>> {
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
    let locales = builds.join("locales");
    let written = locales_written(&tags, &locales).map_err(|error| {
        Box::new(failed(
            "The deployables' locale data couldn't be written".to_string(),
            format!(
                "Jadpo exports the CLDR data of the app's supported locales for its deployables into `{}`, and that failed: {error}.",
                locales.display()
            ),
        ))
    })?;
    Ok(written)
}

/// [`carried_locales`]' files for `tags`, in `locales/<digest>/`.
fn locales_written(tags: &[&str], locales: &Path) -> Result<Locales, String> {
    std::fs::create_dir_all(locales).map_err(|error| error.to_string())?;
    // Clock resolution alone cannot distinguish threads exporting at once.
    // The process-local sequence also keeps their scratch names apart.
    static NEXT_EXPORT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = format!(
        "{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos()),
        NEXT_EXPORT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let files = jadpo_cldr::export(tags, &locales.join(format!(".export-{unique}")))
        .map_err(|error| error.to_string())?;
    let digest = digest(
        files
            .iter()
            .flat_map(|(name, bytes)| [name.as_bytes(), bytes.as_slice()]),
        16,
    );
    let folder = locales.join(&digest);
    // A restored/pruned cargo cache can leave the digest's directory but
    // not its files. Serialize verification and repair for this digest, so
    // one repairer cannot remove another's newly completed publication.
    // Export is above the lock; cargo runs after it is dropped on return.
    let lock = std::fs::File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(locales.join(format!(".{digest}.lock")))
        .map_err(|error| error.to_string())?;
    lock.lock().map_err(|error| error.to_string())?;
    let complete = files
        .iter()
        .all(|(name, bytes)| std::fs::read(folder.join(name)).is_ok_and(|there| there == *bytes));
    if !complete {
        let writing = locales.join(format!(".writing-{unique}"));
        let published = (|| {
            jadpo_codegen::Generated { files }.write(&writing)?;
            if folder.exists() {
                std::fs::remove_dir_all(&folder)?;
            }
            std::fs::rename(&writing, &folder)
        })();
        if let Err(error) = published {
            let _ = std::fs::remove_dir_all(&writing);
            return Err(error.to_string());
        }
    }
    Ok(Locales { folder, digest })
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

/// The Hyperdrive binding of each database's address a Worker reads (area
/// 5, "PostgreSQL through Hyperdrive"; chunk 8.2i): its setting's variable,
/// and the id of the Hyperdrive configuration `JADPO_HYPERDRIVE_<variable>`
/// names in the build's environment, if it names one.
fn hyperdrive(plan: &Plan, call: &Call) -> Vec<workers::Hyperdrive> {
    let runtime = plan.runtime();
    let mut bindings: Vec<workers::Hyperdrive> = Vec::new();
    for connection in &runtime.connections {
        let ConnectionDriver::Postgres { url: Some(at), .. } = connection.driver else {
            continue;
        };
        let Some(field) = runtime.config.fields.get(at as usize) else {
            continue;
        };
        if bindings
            .iter()
            .any(|binding| binding.binding == field.variable)
        {
            continue;
        }
        let id = call
            .environment
            .var(&format!("{HYPERDRIVE_ID}{}", field.variable))
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty());
        bindings.push(workers::Hyperdrive {
            binding: field.variable.clone(),
            id,
        });
    }
    bindings
}

/// What a Hyperdrive binding's id is read from, before its variable:
/// `JADPO_HYPERDRIVE_DATABASE_URL`.
const HYPERDRIVE_ID: &str = "JADPO_HYPERDRIVE_";

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
    /// Each connection's `max_concurrent:` a Worker reaches, which each of
    /// its instances counts on its own.
    per_instance: Vec<(String, u32)>,
}

/// What a deployable is deployed from, by path from the project's root.
enum Artifact {
    /// A server's binary, and how often another scheduler runs its `tick`
    /// command, in milliseconds, when the server's own timer doesn't tick
    /// it.
    Native { binary: String, tick: Option<u64> },
    /// A Worker: its Wrangler file and its module, beside the module's
    /// JavaScript and Jadpo's shim, the module's size, raw and gzipped,
    /// and its databases' Hyperdrive bindings.
    Worker {
        wrangler: String,
        module: String,
        bytes: usize,
        gzipped: usize,
        hyperdrive: Vec<workers::Hyperdrive>,
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
/// its build id and, for a declared deployable, what it serves and runs
/// and, for a Worker, each cap on calls it counts in each instance;
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
            Artifact::Native { binary, tick } => {
                let mut fields = vec![("binary".to_string(), binary.clone())];
                if let Some(every) = tick {
                    fields.push((
                        "tick".to_string(),
                        format!("{binary} tick · {}", how_often(*every)),
                    ));
                }
                fields
            }
            Artifact::Worker {
                wrangler,
                module,
                bytes,
                gzipped,
                hyperdrive,
            } => {
                let mut fields = vec![
                    ("worker".to_string(), wrangler.clone()),
                    ("module".to_string(), module.clone()),
                    (
                        "size".to_string(),
                        format!("{}, {} gzipped", size(*bytes), size(*gzipped)),
                    ),
                ];
                if !hyperdrive.is_empty() {
                    let through: Vec<String> = hyperdrive
                        .iter()
                        .map(|binding| match &binding.id {
                            Some(id) => format!("{} through Hyperdrive {id}", binding.binding),
                            None => format!("{} through Hyperdrive, no id", binding.binding),
                        })
                        .collect();
                    fields.push(("database".to_string(), through.join(", ")));
                }
                fields
            }
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
        if !binary.per_instance.is_empty() {
            fields.push(("calls".to_string(), calls(&binary.per_instance)));
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
    for binary in &built.binaries {
        let Artifact::Worker { hyperdrive, .. } = &binary.artifact else {
            continue;
        };
        for binding in hyperdrive.iter().filter(|binding| binding.id.is_none()) {
            let variable = &binding.binding;
            alerts.push(format!(
                "`{variable}` has no Hyperdrive id: create one with caching off and build with it in `{HYPERDRIVE_ID}{variable}`"
            ));
        }
    }
    if built.modified {
        alerts.push(match built.commit {
            Some(_) => {
                "Built from changes not committed: only this tree explains its events".to_string()
            }
            None => "Built from files git doesn't track: no commit explains its events".to_string(),
        });
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
            Artifact::Native { binary, .. } => Some(binary.as_str()),
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
                let (native, tick, wrangler, module, hyperdrive) = match &binary.artifact {
                    Artifact::Native { binary, tick } => (
                        binary.as_str().into(),
                        tick.map_or(Json::Null, |every| {
                            Json::object([
                                ("command", format!("{binary} tick").into()),
                                ("every_ms", i64::try_from(every).unwrap_or(i64::MAX).into()),
                            ])
                        }),
                        Json::Null,
                        Json::Null,
                        Json::Null,
                    ),
                    Artifact::Worker {
                        wrangler,
                        module,
                        bytes,
                        gzipped,
                        hyperdrive,
                    } => (
                        Json::Null,
                        Json::Null,
                        wrangler.as_str().into(),
                        Json::object([
                            ("path", module.as_str().into()),
                            ("bytes", (*bytes).into()),
                            ("gzip_bytes", (*gzipped).into()),
                        ]),
                        Json::array(hyperdrive.iter().map(|binding| {
                            Json::object([
                                ("binding", binding.binding.as_str().into()),
                                ("id", binding.id.as_deref().map_or(Json::Null, Json::from)),
                            ])
                        })),
                    ),
                };
                Json::object([
                    ("deployable", binary.name.as_str().into()),
                    ("target", binary.target.into()),
                    ("binary", native),
                    ("tick", tick),
                    ("wrangler", wrangler),
                    ("module", module),
                    ("hyperdrive", hyperdrive),
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
                    (
                        "per_instance",
                        Json::array(binary.per_instance.iter().map(|(connection, cap)| {
                            Json::object([
                                ("connection", connection.as_str().into()),
                                ("max_concurrent", (*cap).into()),
                            ])
                        })),
                    ),
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

/// How often a scheduler runs a tick every `ms` milliseconds: `each
/// minute`, `every 10 s`.
fn how_often(ms: u64) -> String {
    match ms {
        60_000 => "each minute".to_string(),
        ms if ms % 60_000 == 0 => format!("every {} min", ms / 60_000),
        ms => format!("every {} s", ms.div_ceil(1_000)),
    }
}

/// `pdf 2 at once in each instance`, `pdf 2 and ocr 4 at once in each
/// instance`: a Worker's caps on calls, which Workers counts in each of
/// the instances it runs as traffic needs.
fn calls(caps: &[(String, u32)]) -> String {
    let each: Vec<String> = caps
        .iter()
        .map(|(connection, cap)| format!("{connection} {cap}"))
        .collect();
    format!("{} at once in each instance", joined(&each))
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

    /// Builds writing one locale set's data at once (chunk 8.4g) each
    /// read the one folder, whole, named by its digest, and leave nothing
    /// half written behind; another set has a folder of its own.
    #[test]
    fn locale_data_is_written_whole_once_for_builds_at_once() {
        let locales =
            std::env::temp_dir().join(format!("jadpo-locales-{}-{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&locales);
        let written: Vec<Locales> = std::thread::scope(|scope| {
            let builds: Vec<_> = (0..4)
                .map(|_| scope.spawn(|| locales_written(&["en-GB", "fr-FR"], &locales)))
                .collect();
            builds
                .into_iter()
                .map(|build| build.join().expect("a build").expect("written"))
                .collect()
        });
        for locales in &written {
            assert_eq!(locales.digest, written[0].digest);
            assert_eq!(locales.folder, written[0].folder);
        }
        let months = std::fs::read_to_string(
            written[0]
                .folder
                .join("datetime_names_month_gregorian_v1.rs.data"),
        )
        .expect("the month names");
        assert!(months.contains("septembre"), "{months}");
        let mut left: Vec<String> = std::fs::read_dir(&locales)
            .expect("the folder")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                format!(".{}.lock", written[0].digest),
                written[0].digest.clone()
            ]
        );
        let other = locales_written(&["de-DE"], &locales).expect("written");
        assert_ne!(other.digest, written[0].digest);
        std::fs::remove_dir_all(&locales).expect("removed");
    }

    /// A restored cargo cache can retain a locale digest directory while
    /// pruning its generated files. Its name alone does not prove it is
    /// complete: ICU4X includes both mod.rs and the marker files at compile
    /// time. Every concurrent build must receive the whole exported set.
    #[test]
    fn an_incomplete_locale_cache_is_rebuilt_before_it_is_used() {
        let locales = std::env::temp_dir().join(format!(
            "jadpo-locales-repair-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_dir_all(&locales);
        let original = locales_written(&["en-GB"], &locales).expect("written");
        let expected: Vec<_> = std::fs::read_dir(&original.folder)
            .expect("locale files")
            .map(|entry| {
                let path = entry.expect("a file").path();
                (
                    path.file_name().unwrap().to_owned(),
                    std::fs::read(path).unwrap(),
                )
            })
            .collect();
        assert!(expected.iter().any(|(name, _)| name == "mod.rs"));
        let marker = expected
            .iter()
            .find(|(name, _)| name != "mod.rs")
            .unwrap()
            .0
            .clone();
        for damage in 0..3 {
            match damage {
                0 => {
                    // rust-cache's recursive cleanup leaves empty directories.
                    for (name, _) in &expected {
                        std::fs::remove_file(original.folder.join(name)).unwrap();
                    }
                }
                1 => std::fs::write(original.folder.join("mod.rs"), b"truncated").unwrap(),
                _ => std::fs::remove_file(original.folder.join(&marker)).unwrap(),
            }
            assert!(original.folder.is_dir());
            let recovered: Vec<_> = std::thread::scope(|scope| {
                let builds: Vec<_> = (0..4)
                    .map(|_| scope.spawn(|| locales_written(&["en-GB"], &locales)))
                    .collect();
                builds
                    .into_iter()
                    .map(|build| build.join().unwrap().unwrap())
                    .collect()
            });
            for recovered in recovered {
                assert_eq!(recovered.folder, original.folder);
                assert_eq!(recovered.digest, original.digest);
                assert!(
                    recovered.folder.join("mod.rs").is_file(),
                    "mod.rs missing after damage {damage}"
                );
                for (name, bytes) in &expected {
                    assert!(
                        std::fs::read(recovered.folder.join(name))
                            .is_ok_and(|there| there == *bytes),
                        "{name:?} differs after damage {damage}"
                    );
                }
            }
        }
        std::fs::remove_dir_all(&locales).expect("removed");
    }

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

    /// A Worker's caps on calls are said on its block, each counted in
    /// each instance (area 11, "Decisions", 2026-10-21; chunk 8.2m).
    #[test]
    fn a_workers_caps_on_calls_are_said_to_count_in_each_instance() {
        let worker = |per_instance: Vec<(String, u32)>| Binary {
            name: "edge".to_string(),
            declared: true,
            target: "wasm",
            artifact: Artifact::Worker {
                wrangler: "build/target/edge/wrangler.json".to_string(),
                module: "build/target/edge/edge.wasm".to_string(),
                bytes: 12_800_000,
                gzipped: 4_000_000,
                hyperdrive: Vec::new(),
            },
            openapi: Vec::new(),
            build: "b_1c5e09d2a7f3".to_string(),
            serves: vec!["main".to_string()],
            runs: Vec::new(),
            per_instance,
        };
        let shown_as = |per_instance: Vec<(String, u32)>, mode: jadpo_term::Mode| {
            let reply = shown(Built {
                binaries: vec![worker(per_instance)],
                commit: Some("8d113e6".to_string()),
                modified: false,
                settings: Vec::new(),
                migrations: false,
                seconds: 1.0,
            });
            let Reply::Done {
                text: crate::run::Text::Screen(screen),
                data,
            } = reply
            else {
                panic!("a screen");
            };
            (screen.render(&jadpo_term::Style::new(mode)), data.to_line())
        };
        let one = vec![("pdf".to_string(), 2)];
        let (rich, data) = shown_as(one.clone(), jadpo_term::Mode::Rich);
        assert!(
            rich.contains("  serves    main\n  calls     pdf 2 at once in each instance\n"),
            "{rich}"
        );
        assert!(
            data.contains(r#""per_instance":[{"connection":"pdf","max_concurrent":2}]"#),
            "{data}"
        );
        let (plain, _) = shown_as(one, jadpo_term::Mode::Plain);
        assert!(
            plain.contains("  calls: pdf 2 at once in each instance\n"),
            "{plain}"
        );
        let (two, _) = shown_as(
            vec![("pdf".to_string(), 2), ("ocr".to_string(), 4)],
            jadpo_term::Mode::Rich,
        );
        assert!(
            two.contains("  calls     pdf 2 and ocr 4 at once in each instance\n"),
            "{two}"
        );
        let (none, data) = shown_as(Vec::new(), jadpo_term::Mode::Rich);
        assert!(!none.contains("calls"), "{none}");
        assert!(data.contains(r#""per_instance":[]"#), "{data}");
    }

    #[test]
    fn a_deployable_another_scheduler_ticks_says_how_often_to_run_its_tick() {
        assert_eq!(how_often(60_000), "each minute");
        assert_eq!(how_often(10_000), "every 10 s");
        assert_eq!(how_often(1_000), "every 1 s");
        assert_eq!(how_often(120_000), "every 2 min");
        let built = |tick: Option<u64>| Built {
            binaries: vec![Binary {
                name: "office".into(),
                declared: true,
                target: "native",
                artifact: Artifact::Native {
                    binary: "build/target/office/office".into(),
                    tick,
                },
                openapi: Vec::new(),
                build: "b_e20fc10f5474".into(),
                serves: vec!["staff".into()],
                runs: vec!["schedules"],
                per_instance: Vec::new(),
            }],
            commit: Some("8d113e6".into()),
            modified: false,
            settings: vec!["DATABASE_URL".into()],
            migrations: true,
            seconds: 20.0,
        };
        let Reply::Done {
            text: crate::run::Text::Screen(screen),
            data,
        } = shown(built(Some(60_000)))
        else {
            panic!("a screen");
        };
        let text = screen.render(&jadpo_term::Style::new(jadpo_term::Mode::Plain));
        assert!(
            text.contains("  tick: build/target/office/office tick · each minute\n"),
            "{text}"
        );
        let data = data.to_line();
        assert!(
            data.contains(
                r#""tick":{"command":"build/target/office/office tick","every_ms":60000}"#
            ),
            "{data}"
        );
        let Reply::Done {
            text: crate::run::Text::Screen(screen),
            data,
        } = shown(built(None))
        else {
            panic!("a screen");
        };
        let text = screen.render(&jadpo_term::Style::new(jadpo_term::Mode::Plain));
        assert!(!text.contains("tick:"), "{text}");
        assert!(data.to_line().contains(r#""tick":null"#));
    }

    #[test]
    fn only_the_last_lines_of_what_cargo_said_are_kept() {
        assert_eq!(last_lines("a\nb\nc\n", 2), "b\nc");
        assert_eq!(last_lines("a", 5), "a");
    }
}

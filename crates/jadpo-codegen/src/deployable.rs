//! A deployable's workspace (roadmap chunks 8.1o and 8.2b;
//! docs/compiler-runtime.md §2.6, "A native deployable" and "A Workers
//! deployable", and §2.8): what `jadpo build` compiles in release,
//! offline, against the runtime crates' own lock, natively or for
//! Cloudflare Workers.
//!
//! [`workspace`] writes, by path from the workspace's folder:
//!
//! - `Cargo.toml`, a workspace of the two packages below;
//! - `rust-toolchain.toml`, the toolchain this Jadpo pins, so rustup builds
//!   with it whatever the machine's default is;
//! - `Cargo.lock`, the runtime crates' lock, which the build keeps to
//!   ([`build`] adds the two packages to it and builds `--locked`);
//! - `program/`, the project's crate as [`crate::generate`] writes it,
//!   named `jadpo-program-<deployable>`, so the programs of several
//!   deployables built into one cargo target never share a unit (cargo
//!   names a path package's unit by its name and its path within its
//!   workspace, which is `program` in each; chunk 8.1o2);
//! - `deployable/`: natively, a binary named after the deployable,
//!   depending on the program and the native host without the `testing`
//!   feature, whose `main` hands the native host the program, the build
//!   stamp and each store's migrations
//!   (`jadpo_host_native::deployable::run`); for Workers, a `cdylib`,
//!   `jadpo-worker-<deployable>`, depending on the program and the Workers
//!   host with only the drivers the deployable reaches ([`Drivers`]),
//!   which exports the Worker's `fetch`, `scheduled` and `queue`, and its
//!   `JadpoHandOn` entrypoint's `hand_on`, through the host's
//!   wasm-bindgen, handing each the same three
//!   (`jadpo_host_workers::deployable::fetch`, `scheduled`, `queue` and
//!   `hand_on`), and a build script linking it with
//!   `jadpo_std::compiled::WASM_STACK_BYTES` of stack (chunk 8.4a).
//!
//! [`build`] runs cargo over it, for `wasm32-unknown-unknown` when it is
//! a Worker, into a target shared with other projects' builds (chunk
//! 8.4g), the workspace's own paths relative and every other path it was
//! built from remapped, and `SOURCE_DATE_EPOCH` fixed, so what it builds
//! holds no machine's paths (reproducible builds are a goal CI can check,
//! §2.8), and with
//! `ICU4X_DATA_DIR` naming the folder of the Unicode CLDR data it
//! carries, the application's supported locales' only as `jadpo-cldr`
//! exports it (area 10, "Locales"; chunk 8.2c), so ICU4X's data crates
//! compile that in place of every locale's. [`crate::workers`] binds a
//! Worker's module and writes the files Workers runs it from.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use jadpo_plan::Plan;
use proc_macro2::Literal;
use quote::quote;

use crate::cargo::Failed;
use crate::{Error, Generated, Options, generate, printed};

/// The program's package, before the deployable's name: the binary
/// depends on it as `jadpo-program`.
pub const PROGRAM: &str = "jadpo-program";

/// The program's package for deployable `name`: `jadpo-program-kiosk`.
pub fn program_package(name: &str) -> String {
    format!("{PROGRAM}-{name}")
}

/// The binary's package; the binary is named after the deployable.
pub const BINARY: &str = "jadpo-deployable";

/// The Worker's package for deployable `name`: `jadpo-worker-kiosk`,
/// named after the deployable for the reason the program's is.
pub fn worker_package(name: &str) -> String {
    format!("jadpo-worker-{name}")
}

/// The target a Worker is built for.
pub const WASM: &str = "wasm32-unknown-unknown";

/// What a deployable is built to run on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    /// A server: a binary on the native host.
    Native,
    /// Cloudflare Workers: a WebAssembly module on the Workers host,
    /// linking only the drivers it reaches.
    Workers(Drivers),
}

/// The drivers a Worker links beyond the package driver, which every one
/// has (HTTP is how the runtime fetches): those whose connections it
/// reaches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drivers {
    /// PostgreSQL, for a project that keeps a database.
    pub postgres: bool,
    /// SMTP, for a deployable that sends email through an SMTP server.
    pub smtp: bool,
}

impl Drivers {
    /// The Workers host's features that turn them on.
    fn features(self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.postgres {
            out.push("postgres");
        }
        if self.smtp {
            out.push("smtp");
        }
        out
    }
}

/// The build stamp a deployable carries (area 18, "Production builds").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp<'a> {
    /// `b_` and twelve hex digits.
    pub build: &'a str,
    pub commit: Option<&'a str>,
    pub modified: bool,
    pub compiler: &'a str,
    /// `native` or `wasm`.
    pub target: &'a str,
}

/// One deployable to write.
#[derive(Clone, Copy, Debug)]
pub struct Deployable<'a> {
    /// Its name, which its binary or module takes.
    pub name: &'a str,
    pub host: Host,
    pub stamp: Stamp<'a>,
    /// Each store's migrations, by the store's connection, in the order
    /// they apply: each one's name and digest.
    pub stores: &'a BTreeMap<String, Vec<(String, String)>>,
    /// The folder holding the runtime crates (`jadpo-runtime`,
    /// `jadpo-host-native` and the rest), absolute.
    pub runtime: &'a str,
    /// The text of the toolchain file this Jadpo pins.
    pub toolchain: &'a str,
    /// The runtime crates' `Cargo.lock`.
    pub lock: &'a [u8],
}

/// The workspace `deployable` of the program `plan` compiles to, by path
/// from its folder.
pub fn workspace(plan: &Plan, deployable: &Deployable<'_>) -> Result<Generated, Error> {
    let program = generate(
        plan,
        &Options {
            name: program_package(deployable.name),
            runtime: deployable.runtime.to_string(),
        },
    )?;
    let mut files: BTreeMap<String, Vec<u8>> = program
        .files
        .into_iter()
        .map(|(path, bytes)| (format!("program/{path}"), bytes))
        .collect();
    files.insert(
        "Cargo.toml".to_string(),
        format!(
            "# A Jadpo deployable's workspace, written by `jadpo build` (docs/compiler-runtime.md §2.5); don't edit it.\n\
             [workspace]\n\
             members = [\"program\", \"deployable\"]\n\
             resolver = \"3\"\n\
             \n\
             {}",
            release_profile(deployable.host)
        )
        .into_bytes(),
    );
    files.insert(
        "rust-toolchain.toml".to_string(),
        deployable.toolchain.as_bytes().to_vec(),
    );
    files.insert("Cargo.lock".to_string(), deployable.lock.to_vec());
    let runtime = deployable.runtime.trim_end_matches('/');
    let program = program_package(deployable.name);
    match deployable.host {
        Host::Native => {
            files.insert(
                "deployable/Cargo.toml".to_string(),
                format!(
                    "# The deployable `{name}`, written by `jadpo build` (docs/compiler-runtime.md §2.5); don't edit it.\n\
                     [package]\n\
                     name = \"{BINARY}\"\n\
                     version = \"0.0.0\"\n\
                     edition = \"2024\"\n\
                     publish = false\n\
                     \n\
                     [[bin]]\n\
                     name = \"{name}\"\n\
                     path = \"src/main.rs\"\n\
                     \n\
                     [dependencies]\n\
                     {PROGRAM} = {{ path = \"../program\", package = \"{program}\" }}\n\
                     jadpo-host-native = {{ path = \"{runtime}/jadpo-host-native\" }}\n",
                    name = deployable.name,
                )
                .into_bytes(),
            );
            let main = "deployable/src/main.rs";
            files.insert(main.to_string(), printed(main, main_rs(deployable))?);
        }
        Host::Workers(drivers) => {
            let features: Vec<String> = drivers
                .features()
                .iter()
                .map(|feature| format!("\"{feature}\""))
                .collect();
            files.insert(
                "deployable/Cargo.toml".to_string(),
                format!(
                    "# The deployable `{name}` for Cloudflare Workers, written by `jadpo build` (docs/compiler-runtime.md §2.5); don't edit it.\n\
                     [package]\n\
                     name = \"{package}\"\n\
                     version = \"0.0.0\"\n\
                     edition = \"2024\"\n\
                     publish = false\n\
                     \n\
                     [lib]\n\
                     path = \"src/lib.rs\"\n\
                     crate-type = [\"cdylib\"]\n\
                     \n\
                     [dependencies]\n\
                     {PROGRAM} = {{ path = \"../program\", package = \"{program}\" }}\n\
                     jadpo-host-workers = {{ path = \"{runtime}/jadpo-host-workers\", features = [{features}] }}\n",
                    name = deployable.name,
                    package = worker_package(deployable.name),
                    features = features.join(", "),
                )
                .into_bytes(),
            );
            let lib = "deployable/src/lib.rs";
            files.insert(lib.to_string(), printed(lib, worker_rs(deployable))?);
            // The module's own stack, given at its link alone, so nothing
            // it depends on builds again (chunk 8.4a).
            files.insert(
                "deployable/build.rs".to_string(),
                format!(
                    "// The module's stack, written by `jadpo build` (docs/compiler-runtime.md §2.6); don't edit it.\n\
                     fn main() {{\n    \
                         println!(\"cargo::rustc-link-arg=-zstack-size={}\");\n\
                     }}\n",
                    jadpo_std::compiled::WASM_STACK_BYTES
                )
                .into_bytes(),
            );
        }
    }
    Ok(Generated { files })
}

/// The release profile a deployable is built in (chunk 8.4f; area 14,
/// "A deployable's release profile"): the whole program optimised as one
/// unit at link time, fat LTO in one codegen unit; natively for speed,
/// keeping its symbols for a backtrace and a profiler; for Workers for
/// size, since a plan limits what a Worker uploads, its names left out
/// when it is bound ([`crate::workers::bind`]). Cargo's own
/// `CARGO_PROFILE_RELEASE_*` variables still override it, as they
/// override any manifest.
fn release_profile(host: Host) -> String {
    let optimised = match host {
        Host::Native => "3",
        Host::Workers(_) => "\"s\"",
    };
    format!(
        "[profile.release]\n\
         opt-level = {optimised}\n\
         lto = \"fat\"\n\
         codegen-units = 1\n"
    )
}

/// The binary's `main`: the native host given the program, the stamp and
/// each store's migrations.
fn main_rs(deployable: &Deployable<'_>) -> proc_macro2::TokenStream {
    let doc = Literal::string(&format!(
        " The deployable `{}`, written by `jadpo build` (docs/compiler-runtime.md §2.5). Generated by Jadpo; don't edit it.",
        deployable.name
    ));
    let constant = constant(deployable);
    quote! {
        #![doc = #doc]

        use jadpo_host_native::deployable::{Deployable, Migration, Stamp, Store};

        #constant

        fn main() -> std::process::ExitCode {
            jadpo_host_native::deployable::run(&DEPLOYABLE)
        }
    }
}

/// The Worker's crate: the Workers host given the program, the stamp and
/// each store's migrations by each handler it exports, through the host's
/// own wasm-bindgen.
fn worker_rs(deployable: &Deployable<'_>) -> proc_macro2::TokenStream {
    let doc = Literal::string(&format!(
        " The deployable `{}` for Cloudflare Workers, written by `jadpo build` (docs/compiler-runtime.md §2.5). Generated by Jadpo; don't edit it.",
        deployable.name
    ));
    let constant = constant(deployable);
    quote! {
        #![doc = #doc]

        use jadpo_host_workers::deployable::{Deployable, Migration, Stamp, Store};
        use jadpo_host_workers::wasm_bindgen::JsValue;

        #constant

        /// The Worker's `fetch`: Jadpo's module shim hands it each request
        /// with the Worker's `env` and the invocation's `ctx`.
        #[jadpo_host_workers::wasm_bindgen::prelude::wasm_bindgen(
            wasm_bindgen = jadpo_host_workers::wasm_bindgen
        )]
        pub fn fetch(request: JsValue, env: JsValue, ctx: JsValue) -> JsValue {
            jadpo_host_workers::deployable::fetch(&DEPLOYABLE, request, env, ctx)
        }

        /// The Worker's `scheduled`: Jadpo's module shim hands it each run
        /// of the minute's cron trigger with the Worker's `env` and the
        /// invocation's `ctx`.
        #[jadpo_host_workers::wasm_bindgen::prelude::wasm_bindgen(
            wasm_bindgen = jadpo_host_workers::wasm_bindgen
        )]
        pub fn scheduled(controller: JsValue, env: JsValue, ctx: JsValue) -> JsValue {
            jadpo_host_workers::deployable::scheduled(&DEPLOYABLE, controller, env, ctx)
        }

        /// The Worker's `queue`: Jadpo's module shim hands it each batch
        /// Cloudflare Queues pushes to its consumer, with the Worker's `env`
        /// and the invocation's `ctx` (chunk 8.2k).
        #[jadpo_host_workers::wasm_bindgen::prelude::wasm_bindgen(
            wasm_bindgen = jadpo_host_workers::wasm_bindgen
        )]
        pub fn queue(batch: JsValue, env: JsValue, ctx: JsValue) -> JsValue {
            jadpo_host_workers::deployable::queue(&DEPLOYABLE, batch, env, ctx)
        }

        /// The `handOn` of the Worker's `JadpoHandOn` entrypoint: Jadpo's
        /// module shim hands it the work another invocation handed on,
        /// with the Worker's `env` and this invocation's `ctx`.
        #[jadpo_host_workers::wasm_bindgen::prelude::wasm_bindgen(
            wasm_bindgen = jadpo_host_workers::wasm_bindgen
        )]
        pub fn hand_on(work: String, env: JsValue, ctx: JsValue) -> JsValue {
            jadpo_host_workers::deployable::hand_on(&DEPLOYABLE, work, env, ctx)
        }
    }
}

/// `DEPLOYABLE`: the program, its build stamp and each store's
/// migrations, as either host's `Deployable` names them.
fn constant(deployable: &Deployable<'_>) -> proc_macro2::TokenStream {
    let text = Literal::string;
    let Stamp {
        build,
        commit,
        modified,
        compiler,
        target,
    } = deployable.stamp;
    let (build, compiler, target, name) = (
        text(build),
        text(compiler),
        text(target),
        text(deployable.name),
    );
    let commit = match commit {
        Some(commit) => {
            let commit = text(commit);
            quote!(Some(#commit))
        }
        None => quote!(None),
    };
    let stores = deployable.stores.iter().map(|(store, migrations)| {
        let store = text(store);
        let migrations = migrations.iter().map(|(migration, digest)| {
            let (migration, digest) = (text(migration), text(digest));
            quote!(Migration { name: #migration, digest: #digest })
        });
        quote!(Store { name: #store, migrations: &[#(#migrations),*] })
    });
    quote! {
        /// The program, its build stamp and each store's migrations.
        const DEPLOYABLE: Deployable = Deployable {
            program: jadpo_program::program,
            stamp: Stamp {
                build: #build,
                commit: #commit,
                modified: #modified,
                compiler: #compiler,
                deployable: #name,
                target: #target,
            },
            stores: &[#(#stores),*],
        };
    }
}

/// How [`build`] builds a workspace.
#[derive(Clone, Copy)]
pub struct Release<'a> {
    /// The workspace's folder, as [`workspace`] wrote it.
    pub folder: &'a Path,
    /// The deployable's name: its binary's or its module's.
    pub name: &'a str,
    /// What it is built to run on.
    pub host: Host,
    /// Where cargo keeps what it builds, between builds: one folder for
    /// every project this Jadpo builds for one host and locale set
    /// (docs/compiler-runtime.md §2.5, "Keeping rustc time down"), so the
    /// runtime crates compile once for them all, and so shared by builds
    /// that may run at once.
    pub target: &'a Path,
    /// Each folder whose path the binary would otherwise hold, and what
    /// the binary names it instead.
    pub remap: &'a [(PathBuf, String)],
    /// The seconds since 1970 the build is dated, for whatever reads
    /// `SOURCE_DATE_EPOCH`.
    pub epoch: u64,
    /// The folder of the CLDR data it carries, `jadpo_cldr::export`'s
    /// files, which ICU4X's data crates compile in place of their own;
    /// one folder for every build with the same data, so a shared target
    /// compiles them once.
    pub cldr: &'a Path,
    /// The cargo that builds it.
    pub cargo: &'a crate::cargo::Cargo,
    /// The target cargo runs on (`aarch64-apple-darwin`), whose crates,
    /// with wasm32's for a Worker, are fetched when the cargo cache lacks
    /// them.
    pub machine: &'a str,
    /// Called once before crates are fetched, when the cargo cache can't
    /// build it offline.
    pub fetching: &'a dyn Fn(),
}

impl std::fmt::Debug for Release<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Release")
            .field("folder", &self.folder)
            .field("name", &self.name)
            .field("host", &self.host)
            .field("target", &self.target)
            .field("cargo", &self.cargo)
            .finish_non_exhaustive()
    }
}

/// What [`build`] built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compiled {
    /// A copy, in the workspace's folder, of what cargo built.
    pub path: PathBuf,
    /// How many crates it fetched first.
    pub crates: usize,
}

/// The file in a target whose lock a build holds from its first cargo
/// command until it has copied out what it built.
const TARGET_LOCK: &str = "jadpo-build.lock";

/// The folder in a target naming, for each deployable name, the
/// workspace whose packages it last built.
const BUILT_BY: &str = "jadpo-built-by";

/// Builds `release`'s binary, or its Worker's module for
/// `wasm32-unknown-unknown`, in the release profile, offline and keeping
/// to the lock, with the CLDR data in `release.cldr`: its path, a copy in
/// the workspace's folder of what cargo built.
///
/// The lock's crates come from the cargo cache. When it can't give them
/// (a first build, with nothing cached), cargo fetches them first,
/// online: the index entries the two generated packages need to join the
/// lock, then each crate the lock names for the host and the build's
/// target, each checked against the lock's checksum as cargo checks one.
/// The build itself is always offline.
///
/// The target is shared, so the build holds the target's lock until that
/// copy is made: cargo's own lock keeps two builds from writing the
/// target at once, but not a second build from replacing what the first
/// built before it is copied out. Cargo names a path package's unit by
/// its name and its path within its workspace, so two projects'
/// deployables of one name build the same units (which keeps what one
/// builds independent of the folder it is in); cargo tells a unit is out
/// of date by its files' times, which another project's may predate, so
/// a workspace building units another built last marks its files changed
/// first.
pub fn build(release: &Release<'_>) -> Result<Compiled, Failed> {
    let io = |error: std::io::Error| Failed::Start(error.to_string());
    std::fs::create_dir_all(release.target.join(BUILT_BY)).map_err(io)?;
    let lock = std::fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(release.target.join(TARGET_LOCK))
        .map_err(io)?;
    lock.lock().map_err(io)?;
    let folder = std::path::absolute(release.folder).map_err(io)?;
    let built_by = release.target.join(BUILT_BY).join(release.name);
    let claimed = folder.to_string_lossy().into_owned();
    if std::fs::read_to_string(&built_by).ok().as_deref() != Some(claimed.as_str()) {
        for packages in ["program", "deployable"] {
            touched(&folder.join(packages)).map_err(io)?;
        }
    }
    // The two generated packages join the runtime crates' lock; nothing
    // else in it moves, which the build then holds it to.
    let mut said = false;
    let mut say = || {
        if !said {
            (release.fetching)();
            said = true;
        }
    };
    if run(release, &["update", "--workspace", "--offline", "--quiet"]).is_err() {
        say();
        run(release, &["update", "--workspace", "--quiet"])?;
    }
    let mut targets = vec!["--target", release.machine];
    if matches!(release.host, Host::Workers(_)) {
        targets.extend(["--target", WASM]);
    }
    let mut crates = 0;
    if run(
        release,
        &[&["fetch", "--locked", "--offline", "--quiet"][..], &targets].concat(),
    )
    .is_err()
    {
        say();
        let said = run(release, &[&["fetch", "--locked"][..], &targets].concat())?;
        crates = downloaded(&said);
    }
    let built = ["build", "--release", "--locked", "--offline", "--quiet"];
    let compiled = match release.host {
        Host::Native => {
            run(
                release,
                &[&built[..], &["--package", BINARY, "--bin", release.name]].concat(),
            )?;
            let mut binary = release.target.join("release").join(release.name);
            if cfg!(windows) {
                binary.set_extension("exe");
            }
            binary
        }
        Host::Workers(_) => {
            let package = worker_package(release.name);
            run(
                release,
                &[
                    &built[..],
                    &["--target", WASM, "--package", &package, "--lib"],
                ]
                .concat(),
            )?;
            release
                .target
                .join(WASM)
                .join("release")
                .join(format!("{}.wasm", package.replace('-', "_")))
        }
    };
    std::fs::write(&built_by, &claimed).map_err(io)?;
    let kept = folder.join(compiled.file_name().unwrap_or_default());
    // A new file, never one written over, since macOS kills a binary
    // copied over one it has run.
    if let Err(error) = std::fs::remove_file(&kept)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(io(error));
    }
    std::fs::copy(&compiled, &kept).map_err(io)?;
    drop(lock);
    Ok(Compiled { path: kept, crates })
}

/// How many crates cargo says it downloaded: a line `Downloaded <crate>
/// v<version>` each, beside the line counting them.
fn downloaded(said: &str) -> usize {
    said.lines()
        .filter(|line| {
            let mut words = line.split_whitespace();
            words.next() == Some("Downloaded")
                && words.next().is_some()
                && words.next().is_some_and(|version| version.starts_with('v'))
        })
        .count()
}

/// Marks every file under `folder` changed now.
fn touched(folder: &Path) -> std::io::Result<()> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Ok(());
    };
    let now = std::time::SystemTime::now();
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            touched(&path)?;
        } else {
            std::fs::File::options()
                .write(true)
                .open(&path)?
                .set_modified(now)?;
        }
    }
    Ok(())
}

/// cargo with `args` in the workspace, as [`build`] runs it: what it
/// said.
fn run(release: &Release<'_>, args: &[&str]) -> Result<String, Failed> {
    // Each flag apart, so a path with a space in it stays one.
    let mut flags = OsString::new();
    for (at, (from, to)) in release.remap.iter().enumerate() {
        if at > 0 {
            flags.push("\u{1f}");
        }
        flags.push("--remap-path-prefix=");
        flags.push(from);
        flags.push("=");
        flags.push(to);
    }
    let cldr =
        std::path::absolute(release.cldr).map_err(|error| Failed::Start(error.to_string()))?;
    let output = Command::new(&release.cargo.program)
        .envs(release.cargo.env.iter().map(|(name, value)| (name, value)))
        .current_dir(release.folder)
        .args(args)
        .env("CARGO_TARGET_DIR", release.target)
        .env("CARGO_ENCODED_RUSTFLAGS", flags)
        .env_remove("RUSTFLAGS")
        .env("SOURCE_DATE_EPOCH", release.epoch.to_string())
        // ICU4X's data crates read the data the deployable carries from
        // here, in place of the full set they ship with; `include!` reads
        // a relative path from the including file, so it is absolute.
        .env("ICU4X_DATA_DIR", cldr)
        .output()
        .map_err(|error| Failed::Start(error.to_string()))?;
    let mut shown = String::from_utf8_lossy(&output.stderr).into_owned();
    shown.push_str(&String::from_utf8_lossy(&output.stdout));
    if output.status.success() {
        return Ok(shown);
    }
    Err(Failed::Refused(shown))
}

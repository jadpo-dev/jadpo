//! A native deployable's workspace (roadmap chunk 8.1o;
//! docs/compiler-runtime.md §2.6, "A native deployable", and §2.8): what
//! `jadpo build` compiles in release, offline, against the runtime crates'
//! own lock.
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
//! - `deployable/`, a binary named after the deployable, depending on the
//!   program and the native host without the `testing` feature, whose
//!   `main` hands the native host the program, the build stamp and each
//!   store's migrations (`jadpo_host_native::deployable::run`).
//!
//! [`build`] runs cargo over it with the paths it was built from remapped
//! and `SOURCE_DATE_EPOCH` fixed, so the binary holds no machine's paths
//! (reproducible builds are a goal CI can check, §2.8), and with
//! `ICU4X_DATA_DIR` naming the folder of the Unicode CLDR data it
//! carries, the application's supported locales' only as `jadpo-cldr`
//! exports it (area 10, "Locales"; chunk 8.2c), so ICU4X's data crates
//! compile that in place of every locale's.

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

/// The build stamp a deployable carries (area 18, "Production builds").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp<'a> {
    /// `b_` and twelve hex digits.
    pub build: &'a str,
    pub commit: Option<&'a str>,
    pub modified: bool,
    pub compiler: &'a str,
    /// `native`.
    pub target: &'a str,
}

/// One deployable to write.
#[derive(Clone, Copy, Debug)]
pub struct Deployable<'a> {
    /// Its name, which its binary takes.
    pub name: &'a str,
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
        "# A Jadpo deployable's workspace, written by `jadpo build` (docs/compiler-runtime.md §2.5); don't edit it.\n\
         [workspace]\n\
         members = [\"program\", \"deployable\"]\n\
         resolver = \"3\"\n"
            .as_bytes()
            .to_vec(),
    );
    files.insert(
        "rust-toolchain.toml".to_string(),
        deployable.toolchain.as_bytes().to_vec(),
    );
    files.insert("Cargo.lock".to_string(), deployable.lock.to_vec());
    let runtime = deployable.runtime.trim_end_matches('/');
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
            program = program_package(deployable.name),
        )
        .into_bytes(),
    );
    let main = "deployable/src/main.rs";
    files.insert(main.to_string(), printed(main, main_rs(deployable))?);
    Ok(Generated { files })
}

/// The binary's `main`: the native host given the program, the stamp and
/// each store's migrations.
fn main_rs(deployable: &Deployable<'_>) -> proc_macro2::TokenStream {
    let text = Literal::string;
    let doc = text(&format!(
        " The deployable `{}`, written by `jadpo build` (docs/compiler-runtime.md §2.5). Generated by Jadpo; don't edit it.",
        deployable.name
    ));
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
        #![doc = #doc]

        use jadpo_host_native::deployable::{Deployable, Migration, Stamp, Store};

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

        fn main() -> std::process::ExitCode {
            jadpo_host_native::deployable::run(&DEPLOYABLE)
        }
    }
}

/// How [`build`] builds a workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release<'a> {
    /// The workspace's folder, as [`workspace`] wrote it.
    pub folder: &'a Path,
    /// The deployable's name: its binary's.
    pub name: &'a str,
    /// Where cargo keeps what it builds, between builds.
    pub target: &'a Path,
    /// Each folder whose path the binary would otherwise hold, and what
    /// the binary names it instead.
    pub remap: &'a [(PathBuf, String)],
    /// The seconds since 1970 the build is dated, for whatever reads
    /// `SOURCE_DATE_EPOCH`.
    pub epoch: u64,
    /// The folder of the CLDR data it carries, `jadpo_cldr::export`'s
    /// files, which ICU4X's data crates compile in place of their own;
    /// the same folder for every deployable of an app, whose locales are
    /// the app's, so a shared target compiles them once.
    pub cldr: &'a Path,
}

/// Builds `release`'s binary in the release profile, offline and keeping
/// to the lock, with the CLDR data in `release.cldr`: its path.
pub fn build(release: &Release<'_>) -> Result<PathBuf, Failed> {
    // The two generated packages join the runtime crates' lock; nothing
    // else in it moves, which the build then holds it to.
    run(release, &["update", "--workspace", "--offline", "--quiet"])?;
    run(
        release,
        &[
            "build",
            "--release",
            "--locked",
            "--offline",
            "--quiet",
            "--package",
            BINARY,
            "--bin",
            release.name,
        ],
    )?;
    let mut binary = release.target.join("release").join(release.name);
    if cfg!(windows) {
        binary.set_extension("exe");
    }
    Ok(binary)
}

/// cargo with `args` in the workspace, as [`build`] runs it.
fn run(release: &Release<'_>, args: &[&str]) -> Result<(), Failed> {
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
    let output = Command::new(crate::cargo::cargo())
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
    if output.status.success() {
        return Ok(());
    }
    let mut shown = String::from_utf8_lossy(&output.stderr).into_owned();
    shown.push_str(&String::from_utf8_lossy(&output.stdout));
    Err(Failed::Refused(shown))
}

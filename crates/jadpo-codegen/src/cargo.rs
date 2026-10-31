//! Driving cargo over a generated crate (docs/compiler-runtime.md §2.5,
//! "Compiled code"): one `cargo build` of a package in the workspace the
//! caller wrote around the crate, offline, so nothing is fetched and every
//! version is the lock's, into a target folder of its own that is kept
//! between builds, so a second build compiles only what changed.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build<'a> {
    /// The workspace's folder, holding its `Cargo.toml` and `Cargo.lock`.
    pub workspace: &'a Path,
    /// The package and the binary to build.
    pub package: &'a str,
    pub bin: &'a str,
    /// Where cargo keeps what it builds.
    pub target: &'a Path,
}

/// Why a build gave no binary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failed {
    /// cargo couldn't be started.
    Start(String),
    /// cargo ran and refused: what it printed.
    Refused(String),
    /// cargo couldn't fetch the crates a build needs, before it compiled
    /// anything: what it printed (area 14, "What it compiles with").
    Fetch(String),
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failed::Start(error) => write!(f, "cargo couldn't be started: {error}"),
            Failed::Refused(output) => write!(f, "cargo couldn't build it:\n{output}"),
            Failed::Fetch(output) => write!(f, "cargo couldn't fetch the crates:\n{output}"),
        }
    }
}

impl std::error::Error for Failed {}

/// A cargo to run, and what its environment adds: `jadpo build`'s
/// fetched toolchain, or the cargo running this process's build (chunk
/// 9.2c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cargo {
    pub program: PathBuf,
    /// Variables set for it, over what it inherits.
    pub env: Vec<(OsString, OsString)>,
}

impl Cargo {
    /// The cargo running this process's build when there is one, else the
    /// one on `PATH`, with nothing added.
    pub fn running() -> Cargo {
        Cargo {
            program: cargo(),
            env: Vec::new(),
        }
    }
}

/// The cargo to run: the one running this process's build when there is
/// one, else the one on `PATH`, which picks the toolchain the repository
/// pins.
pub(crate) fn cargo() -> PathBuf {
    std::env::var_os("CARGO").map_or_else(|| PathBuf::from("cargo"), PathBuf::from)
}

/// Builds `build`'s binary in the dev profile: its path.
pub fn build(build: &Build<'_>) -> Result<PathBuf, Failed> {
    let output = Command::new(cargo())
        .current_dir(build.workspace)
        .args([
            "build",
            "--offline",
            "--quiet",
            "--package",
            build.package,
            "--bin",
            build.bin,
        ])
        .env("CARGO_TARGET_DIR", build.target)
        .output()
        .map_err(|error| Failed::Start(error.to_string()))?;
    if !output.status.success() {
        let mut shown = String::from_utf8_lossy(&output.stderr).into_owned();
        shown.push_str(&String::from_utf8_lossy(&output.stdout));
        return Err(Failed::Refused(shown));
    }
    let mut binary = build.target.join("debug").join(build.bin);
    if cfg!(windows) {
        binary.set_extension("exe");
    }
    Ok(binary)
}

/// One build of a WebAssembly module: a `cdylib` package of a workspace
/// the caller wrote, built for `wasm32-unknown-unknown` (a test Worker,
/// chunk 8.2e).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module<'a> {
    /// The workspace's folder, holding its `Cargo.toml` and `Cargo.lock`.
    pub workspace: &'a Path,
    /// The package whose library is the module.
    pub package: &'a str,
    /// Where cargo keeps what it builds.
    pub target: &'a Path,
}

/// Builds `build`'s module in the dev profile, offline, without debug
/// information, which binding the module leaves out anyway: its path.
pub fn build_module(build: &Module<'_>) -> Result<PathBuf, Failed> {
    let output = Command::new(cargo())
        .current_dir(build.workspace)
        .args([
            "build",
            "--offline",
            "--quiet",
            "--target",
            crate::deployable::WASM,
            "--package",
            build.package,
            "--lib",
        ])
        .env("CARGO_TARGET_DIR", build.target)
        .env("CARGO_PROFILE_DEV_DEBUG", "0")
        .output()
        .map_err(|error| Failed::Start(error.to_string()))?;
    if !output.status.success() {
        let mut shown = String::from_utf8_lossy(&output.stderr).into_owned();
        shown.push_str(&String::from_utf8_lossy(&output.stdout));
        return Err(Failed::Refused(shown));
    }
    Ok(build
        .target
        .join(crate::deployable::WASM)
        .join("debug")
        .join(format!("{}.wasm", build.package.replace('-', "_"))))
}

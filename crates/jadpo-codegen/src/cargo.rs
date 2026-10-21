//! Driving cargo over a generated crate (docs/compiler-runtime.md §2.5,
//! "Compiled code"): one `cargo build` of a package in the workspace the
//! caller wrote around the crate, offline, so nothing is fetched and every
//! version is the lock's, into a target folder of its own that is kept
//! between builds, so a second build compiles only what changed.

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
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failed::Start(error) => write!(f, "cargo couldn't be started: {error}"),
            Failed::Refused(output) => write!(f, "cargo couldn't build it:\n{output}"),
        }
    }
}

impl std::error::Error for Failed {}

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

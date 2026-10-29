//! A released `jadpo` builds deployables outside this checkout (roadmap
//! chunk 9.2c; area 14, "A native deployable"; docs/compiler-runtime.md
//! §2.6, "A native deployable", and §2.8, "Toolchain pinning").
//!
//! The binary is copied out of the checkout and run with no Rust on
//! `PATH`, no `CARGO` or `CARGO_HOME`, and an empty home and cache, in
//! projects outside the checkout. Its runtime's sources come from the
//! binary itself; its toolchain from Rust's dist server and its crates
//! from crates.io, each stood in for on the loopback address by
//! `support::rust_dist`: the toolchain this test runs on packed as a
//! release, and the crates the cargo cache already holds served as a
//! registry, so nothing reaches the network and each archive and crate
//! is checked against its digest as the real ones are. The deployables
//! are compiled with cargo's codegen units, no link-time optimisation and
//! no optimisation (what is proved doesn't depend on them), and the first
//! build compiles the runtime from nothing, so the test takes minutes:
//! it is a step of the gate's `compiled` group of its own. What it builds
//! is the golden todo (`benchmarks/golden-todo/`, roadmap chunk 10.1):
//! natively as it is written, and for Workers with the rate-limit store
//! and deployable area 16's "Rate limits on Workers" gives a Worker, as
//! `benchmarks/contract/worker.py` builds it for workerd.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::support::rust_dist::{self, Server};

const JADPO: &str = env!("CARGO_BIN_EXE_jadpo");
const WASM: &str = "wasm32-unknown-unknown";

/// A folder removed when the test ends, however it ends.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// This checkout's root.
fn checkout() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the checkout")
        .to_path_buf()
}

/// The Rust release `rust-toolchain.toml` pins: `1.99.0`.
fn pinned() -> String {
    let file = std::fs::read_to_string(checkout().join("rust-toolchain.toml")).expect("the pin");
    file.lines()
        .find_map(|line| line.strip_prefix("channel = \"")?.strip_suffix('"'))
        .expect("a channel")
        .to_string()
}

/// The toolchain this test runs on, the pinned one, with its host: the
/// sysroot of the `rustc` beside the `cargo` that runs it (cargo's own,
/// or rustup's, which picks the pin from the checkout).
fn sysroot() -> (PathBuf, String) {
    let cargo = PathBuf::from(std::env::var_os("CARGO").expect("CARGO names a cargo"));
    let rustc = cargo.with_file_name("rustc");
    let printed = Command::new(&rustc)
        .args(["--print", "sysroot"])
        .current_dir(checkout())
        .output()
        .expect("rustc runs");
    let root = PathBuf::from(text(&printed.stdout).trim());
    let verbose = Command::new(root.join("bin/rustc"))
        .arg("-vV")
        .output()
        .expect("rustc runs");
    let verbose = String::from_utf8_lossy(&verbose.stdout).into_owned();
    assert!(
        verbose.contains(&format!("release: {}", pinned())),
        "the test runs on the pinned toolchain: {verbose}"
    );
    let host = verbose
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("a host")
        .to_string();
    (root, host)
}

/// `PATH` without any folder holding cargo, rustc or rustup.
fn without_rust() -> String {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let kept: Vec<PathBuf> = std::env::split_paths(&path)
        .filter(|folder| {
            ["cargo", "rustc", "rustup"]
                .iter()
                .all(|tool| !folder.join(tool).exists())
        })
        .collect();
    std::env::join_paths(kept)
        .expect("a path")
        .to_string_lossy()
        .into_owned()
}

/// Copies the golden todo's Jadpo project into `to`, without its
/// TypeScript twin; with `workers`, its `app.jadpo` also counts rate
/// limits in Cloudflare's binding and declares one deployable for Workers.
fn golden(to: &Path, workers: bool) {
    let from = checkout().join("benchmarks/golden-todo");
    std::fs::create_dir_all(to).expect("a folder");
    for entry in std::fs::read_dir(&from).expect("a folder").flatten() {
        let name = entry.file_name();
        if name == "typescript" || name == "build" || name == ".jadpo" {
            continue;
        }
        if entry.path().is_dir() {
            copy(&entry.path(), &to.join(name));
        } else {
            std::fs::copy(entry.path(), to.join(name)).expect("copied");
        }
    }
    if workers {
        let app = std::fs::read_to_string(to.join("app.jadpo")).expect("app.jadpo");
        let app = app
            .replacen(
                "    connections: {\n",
                "    connections: {\n        limits: {\n            driver: Driver.cloudflare_rate_limits\n            namespace: 1101\n        }\n",
                1,
            )
            .replacen(
                "    revocation_delay: 5m\n",
                "    revocation_delay: 5m\n    rate_limits: {\n        store: limits\n    }\n    deployables: {\n        edge: {\n            serves: [main]\n            runs: [Trigger.subscribers, Trigger.schedules]\n            target: Target.wasm\n        }\n    }\n",
                1,
            );
        assert!(app.contains("Target.wasm"), "{app}");
        std::fs::write(to.join("app.jadpo"), app).expect("app.jadpo");
    }
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a folder");
    for entry in std::fs::read_dir(from).expect("a folder").flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if name == "build" || name == ".jadpo" {
            continue;
        }
        if path.is_dir() {
            copy(&path, &to.join(name));
        } else {
            std::fs::copy(&path, to.join(name)).expect("copied");
        }
    }
}

/// The copied `jadpo` with `args` in `folder`, in nothing but the
/// environment a machine without Rust gives it.
struct Released<'a> {
    jadpo: PathBuf,
    home: PathBuf,
    cache: PathBuf,
    path: String,
    server: &'a Server,
}

impl Released<'_> {
    fn run(&self, folder: &Path, args: &[&str]) -> Output {
        let mut command = Command::new(&self.jadpo);
        command
            .args(args)
            .current_dir(folder)
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_CACHE_HOME", &self.cache)
            .env("PATH", &self.path)
            .env("JADPO_RUST_DIST_SERVER", self.server.url())
            // What the test proves doesn't depend on optimisation, and the
            // runtime is compiled from nothing.
            .env("CARGO_PROFILE_RELEASE_OPT_LEVEL", "0")
            .env("CARGO_PROFILE_RELEASE_LTO", "false")
            .env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "16");
        if let Some(temp) = std::env::var_os("TMPDIR") {
            command.env("TMPDIR", temp);
        }
        command.output().expect("jadpo runs")
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn a_released_jadpo_fetches_its_toolchain_and_builds_both_targets_outside_this_checkout() {
    let checkout = checkout();
    let scratch =
        Scratch(std::env::temp_dir().join(format!("jadpo-released-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&scratch.0);
    let root = &scratch.0;
    assert!(
        !std::path::absolute(root)
            .expect("a path")
            .starts_with(&checkout),
        "the scratch folder is outside the checkout"
    );
    let (sysroot, host) = sysroot();
    let version = pinned();

    // Rust's dist server and crates.io, on this machine.
    let server = Server::start();
    std::fs::create_dir_all(root.join("dist")).expect("a folder");
    rust_dist::toolchain(&server, &sysroot, &version, &host, &root.join("dist"));
    let cargo_home = std::env::var_os("CARGO_HOME").map_or_else(
        || PathBuf::from(std::env::var_os("HOME").expect("a home")).join(".cargo"),
        PathBuf::from,
    );
    let lock = std::fs::read_to_string(checkout.join("Cargo.lock")).expect("the lock");
    rust_dist::registry(&server, &cargo_home, &lock, &root.join("index"));

    // Cargo's own configuration above the projects puts the registry in
    // crates.io's place, as a mirror would be.
    let work = root.join("work");
    std::fs::create_dir_all(work.join(".cargo")).expect("a folder");
    std::fs::write(
        work.join(".cargo/config.toml"),
        format!(
            "[source.crates-io]\nreplace-with = \"fixture\"\n\n[source.fixture]\nregistry = \"sparse+{}/index/\"\n",
            server.url()
        ),
    )
    .expect("cargo's configuration");

    // The binary, copied out, with nowhere to find Rust.
    std::fs::create_dir_all(root.join("bin")).expect("a folder");
    let jadpo = root.join("bin/jadpo");
    std::fs::copy(JADPO, &jadpo).expect("jadpo copied");
    let released = Released {
        jadpo,
        home: root.join("home"),
        cache: root.join("cache"),
        path: without_rust(),
        server: &server,
    };
    std::fs::create_dir_all(&released.home).expect("a home");
    for tool in ["cargo", "rustc", "rustup"] {
        let found = Command::new("sh")
            .args(["-c", &format!("command -v {tool}")])
            .env_clear()
            .env("PATH", &released.path)
            .output()
            .expect("sh runs");
        assert!(!found.status.success(), "{tool} is on PATH");
    }

    // A native deployable: the toolchain for the host is fetched, the
    // runtime's crates too, and the binary built and run.
    let shop = work.join("shop");
    golden(&shop, false);
    let first = released.run(&shop, &["build"]);
    let (stdout, stderr) = (text(&first.stdout), text(&first.stderr));
    assert!(first.status.success(), "{stdout}{stderr}");
    assert!(
        stderr.contains(&format!("Fetching Rust {version} for {host}\n")),
        "{stderr}"
    );
    assert!(
        stderr.contains("Fetching the runtime's crates\n"),
        "{stderr}"
    );
    let fetched = stdout
        .lines()
        .find(|line| line.trim_start().starts_with("fetched"))
        .unwrap_or_else(|| panic!("a `fetched` line: {stdout}"));
    assert!(fetched.contains(&format!("Rust {version}")), "{fetched}");
    assert!(fetched.contains(" crates"), "{fetched}");
    let asked = server.asked();
    assert!(
        asked
            .iter()
            .any(|path| path.ends_with(&format!("rustc-{version}-{host}.tar.gz"))),
        "{asked:?}"
    );
    assert!(
        !asked.iter().any(|path| path.contains(WASM)),
        "nothing for wasm32 until a Worker is built: {asked:?}"
    );
    let toolchain = released
        .cache
        .join(format!("jadpo/toolchains/{version}-{host}"));
    assert!(
        toolchain.join("bin/cargo").exists(),
        "{}",
        toolchain.display()
    );
    assert!(
        toolchain.join("bin/rustc").exists(),
        "{}",
        toolchain.display()
    );
    let crates: Vec<_> = std::fs::read_dir(released.cache.join("jadpo/cargo/registry/cache"))
        .expect("jadpo's own cargo cache")
        .flatten()
        .flat_map(|index| {
            std::fs::read_dir(index.path())
                .into_iter()
                .flatten()
                .flatten()
        })
        .collect();
    assert!(!crates.is_empty(), "crates fetched into jadpo's cache");

    // The binary holds no path of this checkout, and runs.
    let binary = shop.join("build/target/app/app");
    let bytes = std::fs::read(&binary).expect("the binary");
    let marker = checkout.to_string_lossy().into_owned();
    assert!(
        !bytes
            .windows(marker.len())
            .any(|window| window == marker.as_bytes()),
        "the binary names {marker}"
    );
    let program = std::fs::read_to_string(shop.join("build/rust/app/program/Cargo.toml"))
        .expect("the program's manifest");
    assert!(!program.contains(&marker), "{program}");
    let unknown = Command::new(&binary)
        .arg("nonsense")
        .env_clear()
        .output()
        .expect("the binary runs");
    assert_eq!(unknown.status.code(), Some(2), "{}", text(&unknown.stdout));
    assert!(
        text(&unknown.stdout).contains("command_unknown"),
        "{}",
        text(&unknown.stdout)
    );

    // A Workers deployable: only wasm32's library is fetched.
    let edge = work.join("edge");
    golden(&edge, true);
    let before = server.asked().len();
    let worker = released.run(&edge, &["build"]);
    let (stdout, stderr) = (text(&worker.stdout), text(&worker.stderr));
    assert!(worker.status.success(), "{stdout}{stderr}");
    assert!(
        stderr.contains(&format!("Fetching Rust {version} for {WASM}\n")),
        "{stderr}"
    );
    let asked = server.asked()[before..].to_vec();
    assert!(
        asked
            .iter()
            .any(|path| path.ends_with(&format!("rust-std-{version}-{WASM}.tar.gz"))),
        "{asked:?}"
    );
    assert!(
        !asked
            .iter()
            .any(|path| path.contains(&format!("rustc-{version}"))),
        "rustc is fetched once: {asked:?}"
    );
    for file in ["edge.wasm", "edge.js", "worker.mjs", "wrangler.json"] {
        assert!(
            edge.join("build/target/edge").join(file).exists(),
            "{file}: {stdout}"
        );
    }

    // Built again, nothing is fetched and nothing is said of fetching.
    let before = server.asked().len();
    let again = released.run(&shop, &["build", "--json"]);
    let (stdout, stderr) = (text(&again.stdout), text(&again.stderr));
    assert!(again.status.success(), "{stdout}{stderr}");
    assert_eq!(stderr, "", "--json writes nothing on stderr");
    assert_eq!(
        server.asked()[before..],
        [] as [String; 0],
        "nothing fetched again"
    );
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("JSON");
    assert_eq!(
        document["data"]["fetched"],
        serde_json::json!({ "rust": null, "crates": 0 }),
        "{document}"
    );
}

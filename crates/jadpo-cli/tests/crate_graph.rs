//! The intended crate graph (docs/compiler-runtime.md §2.1), checked against
//! `cargo metadata`. The layers are the workspace's own table
//! (`[workspace.metadata.layers]` in the root `Cargo.toml`), which
//! `tools/verify.py` also reads to build the shared and runtime crates for
//! wasm32. Every rule holds over everything a crate reaches, not only its
//! direct dependencies:
//!
//! - a shared crate depends on no workspace crate, so compiler and runtime
//!   crates may both depend on it;
//! - compiler crates never reach runtime crates, except `jadpo-interp`,
//!   which links the runtime it shares with compiled code, and never reach
//!   tooling crates;
//! - runtime crates reach only runtime and shared crates, so a generated
//!   application needs nothing else;
//! - nothing depends on `jadpo-cli`, the binary;
//! - salsa is confined to `jadpo-db` and `jadpo-check` (§2.4);
//! - `jadpo-sql`, which writes the compiler's PostgreSQL text, is a leaf:
//!   it reaches only shared crates, so the graph, the checker and the plan
//!   may all depend on it.
//!
//! Each crate's integration tests are one binary, `integration`, whose
//! modules are every file in its `tests/` folder (§2.7), so no test file is
//! left out of it.
//!
//! Every workspace crate must be in a layer, so a new crate is placed on
//! purpose. Dev-dependencies are not part of the graph: tests may reach
//! anywhere.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layer {
    Shared,
    Compiler,
    Runtime,
    Tooling,
}

use Layer::{Compiler, Runtime, Shared, Tooling};

/// The crates salsa may be a dependency of.
const SALSA: [&str; 2] = ["jadpo-db", "jadpo-check"];

/// Compiler crates that reach nothing but shared crates.
const LEAVES: [&str; 1] = ["jadpo-sql"];

/// The workspace as `cargo metadata` sees it.
struct Workspace {
    layers: BTreeMap<String, Layer>,
    members: Vec<String>,
    /// Each member's non-dev dependencies on other members.
    edges: BTreeMap<String, BTreeSet<String>>,
    /// The members that depend on salsa directly.
    salsa: BTreeSet<String>,
}

fn read() -> Workspace {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
            "--manifest-path",
            root,
        ])
        .output()
        .expect("cargo metadata runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("metadata is JSON");
    let mut layers = BTreeMap::new();
    for (word, layer) in [
        ("shared", Shared),
        ("compiler", Compiler),
        ("runtime", Runtime),
        ("tooling", Tooling),
    ] {
        for name in metadata["metadata"]["layers"][word]
            .as_array()
            .unwrap_or_else(|| panic!("[workspace.metadata.layers] lists `{word}`"))
        {
            layers.insert(name.as_str().expect("a crate name").to_string(), layer);
        }
    }
    let packages = metadata["packages"].as_array().expect("packages");
    let members: Vec<String> = packages
        .iter()
        .map(|package| package["name"].as_str().expect("name").to_string())
        .collect();
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut salsa = BTreeSet::new();
    for package in packages {
        let from = package["name"].as_str().expect("name").to_string();
        for dependency in package["dependencies"].as_array().expect("dependencies") {
            let to = dependency["name"].as_str().expect("name");
            if dependency["kind"].as_str() == Some("dev") {
                continue;
            }
            if to == "salsa" {
                salsa.insert(from.clone());
            }
            if members.iter().any(|member| member == to) {
                edges
                    .entry(from.clone())
                    .or_default()
                    .insert(to.to_string());
            }
        }
    }
    Workspace {
        layers,
        members,
        edges,
        salsa,
    }
}

/// Every crate `from` reaches through its dependencies, itself left out.
fn reach(edges: &BTreeMap<String, BTreeSet<String>>, from: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<&str> = vec![from];
    while let Some(next) = stack.pop() {
        for to in edges.get(next).into_iter().flatten() {
            if seen.insert(to.clone()) {
                stack.push(to);
            }
        }
    }
    seen
}

/// Why `from` reaching `to` breaks the intended graph, if it does.
fn violation(layers: &BTreeMap<String, Layer>, from: &str, to: &str) -> Option<String> {
    let (Some(&from_layer), Some(&to_layer)) = (layers.get(from), layers.get(to)) else {
        let unknown = if layers.contains_key(from) { to } else { from };
        return Some(format!(
            "{unknown} isn't in the intended crate graph: add it to §2.1 and to [workspace.metadata.layers]"
        ));
    };
    if to == "jadpo-cli" {
        return Some(format!(
            "{from} depends on jadpo-cli, the binary, which nothing may depend on"
        ));
    }
    if LEAVES.contains(&from) && to_layer != Shared {
        return Some(format!(
            "{from} reaches {to}; it is a leaf, which reaches only shared crates"
        ));
    }
    match (from_layer, to_layer) {
        (Shared, _) => Some(format!(
            "shared crate {from} reaches {to}; shared crates depend on no workspace crate"
        )),
        (Compiler, Runtime) if from != "jadpo-interp" => Some(format!(
            "compiler crate {from} reaches runtime crate {to}; only jadpo-interp may"
        )),
        (Compiler, Tooling) => Some(format!("compiler crate {from} reaches tooling crate {to}")),
        (Runtime, Compiler | Tooling) => Some(format!(
            "runtime crate {from} reaches {to}; runtime crates stand alone"
        )),
        _ => None,
    }
}

/// Every problem with a workspace.
fn problems(workspace: &Workspace) -> Vec<String> {
    let mut out: Vec<String> = workspace
        .members
        .iter()
        .filter(|member| !workspace.layers.contains_key(*member))
        .map(|member| {
            format!(
                "{member} isn't in the intended crate graph: add it to §2.1 and to [workspace.metadata.layers]"
            )
        })
        .collect();
    for from in &workspace.members {
        for to in reach(&workspace.edges, from) {
            out.extend(violation(&workspace.layers, from, &to));
        }
    }
    out.extend(
        workspace
            .salsa
            .iter()
            .filter(|name| !SALSA.contains(&name.as_str()))
            .map(|name| {
                format!("{name} depends on salsa, which is confined to jadpo-db and jadpo-check")
            }),
    );
    out
}

#[test]
fn the_workspace_follows_the_intended_graph() {
    let workspace = read();
    let problems = problems(&workspace);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(
        workspace.members.iter().any(|member| member == "jadpo-cli")
            && workspace.layers.get("jadpo-std") == Some(&Shared),
        "the graph and its layers were read"
    );
}

#[test]
fn the_rules_catch_each_kind_of_wrong_edge_however_far_away() {
    let mut workspace = read();
    let edge = |workspace: &mut Workspace, from: &str, to: &str| {
        workspace
            .edges
            .entry(from.to_string())
            .or_default()
            .insert(to.to_string());
        if !workspace.members.iter().any(|member| member == from) {
            workspace.members.push(from.to_string());
        }
    };
    assert!(problems(&workspace).is_empty());

    // Two steps away: the check reaches the runtime through the interpreter.
    let mut through = read();
    edge(&mut through, "jadpo-interp", "jadpo-value");
    assert!(problems(&through).is_empty(), "the interpreter may");
    edge(&mut through, "jadpo-check", "jadpo-interp");
    assert!(
        problems(&through)
            .iter()
            .any(|p| p.contains("jadpo-check reaches runtime crate jadpo-value")),
        "{:?}",
        problems(&through)
    );

    let cases: &[(&str, &str, &str)] = &[
        ("jadpo-std", "jadpo-diag", "shared crate jadpo-std"),
        ("jadpo-value", "jadpo-diag", "runtime crate jadpo-value"),
        ("jadpo-check", "jadpo-term", "compiler crate jadpo-check"),
        ("jadpo-lsp", "jadpo-cli", "jadpo-cli, the binary"),
        (
            "jadpo-sql",
            "jadpo-graph",
            "jadpo-sql reaches jadpo-graph; it is a leaf",
        ),
        (
            "jadpo-core",
            "jadpo-diag",
            "jadpo-core isn't in the intended",
        ),
    ];
    for (from, to, expected) in cases {
        let mut wrong = read();
        edge(&mut wrong, from, to);
        assert!(
            problems(&wrong).iter().any(|p| p.contains(expected)),
            "{from} -> {to}: {:?}",
            problems(&wrong)
        );
    }

    workspace.salsa.insert("jadpo-graph".to_string());
    assert!(
        problems(&workspace)
            .iter()
            .any(|p| p.contains("jadpo-graph depends on salsa"))
    );
}

/// The crate attribute each shared and runtime crate's `lib.rs` carries,
/// without whitespace, however rustfmt wraps it.
const PANIC_LINTS: &str = "#![cfg_attr(not(test),deny(clippy::unwrap_used,clippy::expect_used,clippy::panic,clippy::indexing_slicing,clippy::unreachable,clippy::unimplemented))]";

/// A panic aborts a Workers instance, so every shared and runtime crate
/// that exists denies `unwrap`, `expect`, `panic!`, indexing and slicing
/// that can panic, `unreachable!` and `unimplemented!` outside its tests
/// (docs/compiler-runtime.md §2.6).
#[test]
fn runtime_crates_deny_panics() {
    let workspace = read();
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut missing = Vec::new();
    for (name, layer) in &workspace.layers {
        if !matches!(layer, Shared | Runtime) || !workspace.members.contains(name) {
            continue;
        }
        let lib = std::path::Path::new(root)
            .join("crates")
            .join(name)
            .join("src/lib.rs");
        let text = std::fs::read_to_string(&lib).unwrap_or_default();
        // The crate attribute, however rustfmt wraps it.
        let flat: String = text.split_whitespace().collect();
        let denies = flat.contains(PANIC_LINTS);
        if !denies {
            missing.push(name.clone());
        }
    }
    assert!(
        missing.is_empty(),
        "these crates don't deny every way to panic, `{PANIC_LINTS}`: {missing:?}"
    );
}

/// A deployable holds no testing machinery (docs/compiler-runtime.md §2.6,
/// "The `testing` feature"; chunk 8.1e): no shared or runtime crate turns
/// on a `testing` feature by default or in what it depends on, outside
/// its own tests, so only a tool that asks for it (`jadpo test`, `jadpo
/// dev`) builds the fakes, recordings and crash points in.
/// `tools/deployable.py` builds a deployable and reads its symbols.
#[test]
fn no_runtime_crate_turns_on_testing() {
    let workspace = read();
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
            "--manifest-path",
            root,
        ])
        .output()
        .expect("cargo metadata runs");
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("metadata is JSON");
    let testing = |feature: &serde_json::Value| {
        feature
            .as_str()
            .is_some_and(|feature| feature == "testing" || feature.ends_with("/testing"))
    };
    let mut problems = Vec::new();
    for package in metadata["packages"].as_array().expect("packages") {
        let name = package["name"].as_str().expect("name");
        if !matches!(workspace.layers.get(name), Some(Shared | Runtime)) {
            continue;
        }
        let default = package["features"]["default"].as_array();
        if default.is_some_and(|features| features.iter().any(testing)) {
            problems.push(format!("{name} turns `testing` on by default"));
        }
        for dependency in package["dependencies"].as_array().expect("dependencies") {
            if dependency["kind"].as_str() == Some("dev") {
                continue;
            }
            let features = dependency["features"].as_array();
            if features.is_some_and(|features| features.iter().any(testing)) {
                problems.push(format!(
                    "{name} turns on `testing` of {}",
                    dependency["name"].as_str().unwrap_or_default()
                ));
            }
        }
    }
    let runtime = metadata["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .find(|package| package["name"] == "jadpo-runtime")
        .expect("jadpo-runtime");
    assert!(
        runtime["features"]["testing"].is_array(),
        "jadpo-runtime has a `testing` feature"
    );
    assert!(problems.is_empty(), "{problems:?}");
}

/// No native deployable links OpenSSL (area 8, "Passkeys"; docs/compiler-runtime.md
/// §2.6): what `jadpo build` writes for the native host depends on
/// jadpo-host-native and the program's runtime crates with their default
/// features, and nothing they reach, to run or to build, is OpenSSL, its
/// bindings or a crate built on them. `webauthn-rs`, which verifies with
/// OpenSSL, is only the passkey tests' reference, a dev-dependency.
#[test]
fn no_native_deployable_links_openssl() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    // The packages a native deployable's manifest names (jadpo-codegen's
    // `deployable`): the host, and the program's three.
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--locked",
            "--offline",
            "--manifest-path",
            root,
            "-p",
            "jadpo-host-native",
            "-p",
            "jadpo-runtime",
            "-p",
            "jadpo-std",
            "-p",
            "jadpo-value",
            "--edges",
            "normal,build",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ])
        .output()
        .expect("cargo tree runs");
    let tree = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reached: BTreeSet<&str> = tree
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    // The tree is a deployable's: its TLS and its passkey checks are there.
    for present in ["rustls", "p256", "serde_cbor_2"] {
        assert!(reached.contains(present), "{present} isn't reached: {tree}");
    }
    let openssl: Vec<&&str> = reached
        .iter()
        .filter(|name| name.starts_with("openssl") || name.starts_with("webauthn"))
        .collect();
    assert!(
        openssl.is_empty(),
        "a native deployable reaches {openssl:?}"
    );
}

/// Why the crate at `dir` doesn't have its integration tests as one binary
/// (docs/compiler-runtime.md §2.7): its `tests/*.rs` files are modules of
/// `tests/main.rs`, and cargo, told not to find tests itself, builds that
/// one, so a test file left undeclared would never run.
fn test_layout_problems(dir: &std::path::Path) -> Vec<String> {
    let name = dir
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let tests = dir.join("tests");
    let mut files: Vec<String> = std::fs::read_dir(&tests)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|file| file.ends_with(".rs") && file != "main.rs")
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    if files.is_empty() {
        return Vec::new();
    }
    let mut problems = Vec::new();
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
    if !manifest.contains("autotests = false")
        || !manifest.contains("[[test]]\nname = \"integration\"\npath = \"tests/main.rs\"")
    {
        problems.push(format!(
            "{name}: Cargo.toml needs `autotests = false` and one [[test]] named `integration` at tests/main.rs"
        ));
    }
    let main = std::fs::read_to_string(tests.join("main.rs")).unwrap_or_default();
    for file in files {
        let stem = file.trim_end_matches(".rs");
        let declared = main.lines().any(|line| line == format!("mod {stem};"))
            || main.contains(&format!("#[path = \"{file}\"]\nmod "));
        if !declared {
            problems.push(format!(
                "{name}: tests/{file} is not a module of tests/main.rs"
            ));
        }
    }
    problems
}

#[test]
fn each_crate_has_one_integration_test_binary_running_every_test_file() {
    let crates = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
    let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(crates)
        .expect("crates/")
        .flatten()
        .map(|entry| entry.path())
        .collect();
    dirs.sort();
    let problems: Vec<String> = dirs
        .iter()
        .flat_map(|dir| test_layout_problems(dir))
        .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));

    // A file nobody declared is caught.
    let scratch = std::env::temp_dir().join(format!("jadpo-layout-{}", std::process::id()));
    std::fs::create_dir_all(scratch.join("tests")).unwrap();
    std::fs::write(
        scratch.join("Cargo.toml"),
        "autotests = false\n\n[[test]]\nname = \"integration\"\npath = \"tests/main.rs\"\n",
    )
    .unwrap();
    std::fs::write(scratch.join("tests/main.rs"), "mod kept;\n").unwrap();
    std::fs::write(scratch.join("tests/kept.rs"), "").unwrap();
    std::fs::write(scratch.join("tests/stray.rs"), "").unwrap();
    let found = test_layout_problems(&scratch);
    std::fs::remove_dir_all(&scratch).unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].ends_with("tests/stray.rs is not a module of tests/main.rs"),
        "{found:?}"
    );
}

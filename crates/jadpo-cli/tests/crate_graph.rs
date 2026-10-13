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
//! - salsa is confined to `jadpo-db` and `jadpo-check` (§2.4).
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

/// A panic aborts a Workers instance, so every shared and runtime crate
/// that exists denies `unwrap`, `expect` and `panic!` outside its tests
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
        let denies = flat.contains(
            "#![cfg_attr(not(test),deny(clippy::unwrap_used,clippy::expect_used,clippy::panic))]",
        );
        if !denies {
            missing.push(name.clone());
        }
    }
    assert!(
        missing.is_empty(),
        "these crates don't deny unwrap, expect and panic!: {missing:?}"
    );
}

//! The intended crate graph (docs/compiler-runtime.md §2.1), checked against
//! `cargo metadata`. Compiler crates never depend on runtime crates, except
//! `jadpo-interp`, which links the runtime it shares with compiled code;
//! runtime crates never depend on compiler or tooling crates, so a generated
//! application needs only runtime crates; compiler crates never depend on
//! tooling crates; nothing depends on the `jadpo` binary's crate. Every
//! workspace crate must appear below, so a new crate is placed on purpose.
//! Dev-dependencies are not part of the graph: tests may reach anywhere.

use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layer {
    Compiler,
    Runtime,
    Tooling,
}

use Layer::{Compiler, Runtime, Tooling};

const INTENDED: &[(&str, Layer)] = &[
    ("jadpo-diag", Compiler),
    ("jadpo-syntax", Compiler),
    ("jadpo-fmt", Compiler),
    ("jadpo-graph", Compiler),
    ("jadpo-db", Compiler),
    ("jadpo-check", Compiler),
    ("jadpo-plan", Compiler),
    ("jadpo-interp", Compiler),
    ("jadpo-codegen", Compiler),
    ("jadpo-value", Runtime),
    ("jadpo-runtime", Runtime),
    ("jadpo-drivers", Runtime),
    ("jadpo-host-native", Runtime),
    ("jadpo-host-workers", Runtime),
    ("jadpo-term", Tooling),
    ("jadpo-test", Tooling),
    ("jadpo-lsp", Tooling),
    ("jadpo-cli", Tooling),
];

fn layer(name: &str) -> Option<Layer> {
    INTENDED
        .iter()
        .find(|(crate_name, _)| *crate_name == name)
        .map(|(_, layer)| *layer)
}

/// Why the edge `from -> to` breaks the intended graph, if it does.
fn violation(from: &str, to: &str) -> Option<String> {
    let (Some(from_layer), Some(to_layer)) = (layer(from), layer(to)) else {
        let unknown = if layer(from).is_none() { from } else { to };
        return Some(format!(
            "{unknown} isn't in the intended crate graph: add it to §2.1 and to this test"
        ));
    };
    if to == "jadpo-cli" {
        return Some(format!(
            "{from} depends on jadpo-cli, the binary, which nothing may depend on"
        ));
    }
    match (from_layer, to_layer) {
        (Compiler, Runtime) if from != "jadpo-interp" => Some(format!(
            "compiler crate {from} depends on runtime crate {to}; only jadpo-interp may"
        )),
        (Compiler, Tooling) => Some(format!(
            "compiler crate {from} depends on tooling crate {to}"
        )),
        (Runtime, Compiler | Tooling) => Some(format!(
            "runtime crate {from} depends on {to}; runtime crates stand alone"
        )),
        _ => None,
    }
}

/// Every non-dev edge between workspace crates, and every workspace crate.
fn workspace_graph() -> (Vec<String>, Vec<(String, String)>) {
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
    let packages = metadata["packages"].as_array().expect("packages");
    let members: Vec<String> = packages
        .iter()
        .map(|package| package["name"].as_str().expect("name").to_string())
        .collect();
    let mut edges = Vec::new();
    for package in packages {
        let from = package["name"].as_str().expect("name");
        for dependency in package["dependencies"].as_array().expect("dependencies") {
            let to = dependency["name"].as_str().expect("name");
            let dev = dependency["kind"].as_str() == Some("dev");
            if !dev && members.iter().any(|member| member == to) {
                edges.push((from.to_string(), to.to_string()));
            }
        }
    }
    (members, edges)
}

#[test]
fn the_workspace_follows_the_intended_graph() {
    let (members, edges) = workspace_graph();
    let mut problems: Vec<String> = members
        .iter()
        .filter(|member| layer(member).is_none())
        .map(|member| {
            format!("{member} isn't in the intended crate graph: add it to §2.1 and to this test")
        })
        .collect();
    problems.extend(edges.iter().filter_map(|(from, to)| violation(from, to)));
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(
        members.iter().any(|member| member == "jadpo-cli"),
        "the graph was read"
    );
}

#[test]
fn the_rules_catch_each_kind_of_wrong_edge() {
    assert_eq!(violation("jadpo-term", "jadpo-diag"), None);
    assert_eq!(violation("jadpo-cli", "jadpo-runtime"), None);
    assert_eq!(violation("jadpo-interp", "jadpo-runtime"), None);
    assert_eq!(violation("jadpo-check", "jadpo-db"), None);
    assert_eq!(violation("jadpo-runtime", "jadpo-value"), None);
    assert!(violation("jadpo-codegen", "jadpo-runtime").is_some());
    assert!(violation("jadpo-check", "jadpo-term").is_some());
    assert!(violation("jadpo-value", "jadpo-diag").is_some());
    assert!(violation("jadpo-host-workers", "jadpo-term").is_some());
    assert!(violation("jadpo-lsp", "jadpo-cli").is_some());
    assert!(violation("jadpo-core", "jadpo-diag").is_some());
}

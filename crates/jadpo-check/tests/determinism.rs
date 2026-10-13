//! Same source, same bytes (docs/compiler-runtime.md §2.8): every project
//! in the specification corpus, each language page's `check` blocks, is
//! checked twice, the second time with its files read in the opposite
//! order, and the two manifests must be identical. A project's files are
//! always listed in path order, as `jadpo_db::load` lists them; what
//! changes is the order they were read in. Each project is also formatted and checked
//! again: formatting may move lines but must change no key and no digest.

#[path = "../../jadpo-syntax/tests/support/examples.rs"]
mod examples;
mod support;

use std::collections::BTreeMap;

use jadpo_check::graph::graph;
use jadpo_db::{Database, Layout};
use jadpo_graph::Graph;
use jadpo_graph::manifest::{Stamp, manifest};

/// Each `check` project on the language pages: its name and its files.
fn corpus() -> Vec<(String, Vec<(String, String)>)> {
    let mut projects: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut counters: BTreeMap<String, usize> = BTreeMap::new();
    for example in examples::all() {
        if !example.area_page {
            continue;
        }
        let page = example
            .place
            .split(':')
            .next()
            .unwrap_or_default()
            .to_string();
        let n = counters.entry(page.clone()).or_default();
        *n += 1;
        if !example.attrs.iter().any(|attr| attr == "check") {
            continue;
        }
        let attr = |name: &str| {
            example
                .attrs
                .iter()
                .find_map(|attr| attr.strip_prefix(&format!("{name}=")))
                .map(str::to_string)
        };
        let project = attr("project").unwrap_or_else(|| format!("example-{n}"));
        let file = attr("file").unwrap_or_else(|| "main.jadpo".to_string());
        if !file.ends_with(".jadpo") {
            continue;
        }
        projects
            .entry(format!("{page} {project}"))
            .or_default()
            .push((file, example.source));
    }
    projects.into_iter().collect()
}

fn layout(files: &[(String, String)]) -> Layout {
    if files.iter().any(|(path, _)| path == "app.jadpo") {
        Layout::Project
    } else {
        Layout::Loose
    }
}

/// The project's manifest, checked first as `jadpo check` does.
fn checked(files: &[(String, String)]) -> (String, Graph) {
    let db = Database::default();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let project = support::project_in_path_order(&db, layout(files), &borrowed);
    let errors = jadpo_check::check(&db, project).len();
    let graph = graph(&db, project).clone();
    let stamp = Stamp {
        compiler: "test".into(),
        errors,
    };
    (manifest(&graph, &stamp).to_document(), graph)
}

fn identities(graph: &Graph) -> Vec<(String, Option<String>)> {
    graph
        .nodes()
        .iter()
        .map(|node| (node.key.to_string(), node.digest.map(|d| d.short())))
        .collect()
}

#[test]
fn every_corpus_project_gives_the_same_manifest_twice_in_any_file_order() {
    let corpus = corpus();
    assert!(corpus.len() > 100, "the corpus was read: {}", corpus.len());
    let mut failures = Vec::new();
    for (name, files) in &corpus {
        let mut sorted = files.clone();
        sorted.sort();
        let (first, _) = checked(&sorted);
        let (second, _) = checked(&sorted);
        let mut reversed = sorted.clone();
        reversed.reverse();
        let (third, _) = checked(&reversed);
        if first != second || first != third {
            failures.push(name.clone());
        }
    }
    assert!(failures.is_empty(), "manifests differ: {failures:?}");
}

#[test]
fn formatting_a_corpus_project_changes_no_key_and_no_digest() {
    let mut failures = Vec::new();
    let mut compared = 0;
    for (name, files) in corpus() {
        let formatted: Option<Vec<(String, String)>> = files
            .iter()
            .map(|(path, text)| Some((path.clone(), jadpo_fmt::format_file(text).ok()?)))
            .collect();
        let Some(formatted) = formatted else {
            continue;
        };
        compared += 1;
        let (_, before) = checked(&files);
        let (_, after) = checked(&formatted);
        if identities(&before) != identities(&after) {
            failures.push(name);
        }
    }
    assert!(compared > 100, "projects compared: {compared}");
    assert!(
        failures.is_empty(),
        "digests moved under formatting: {failures:?}"
    );
}

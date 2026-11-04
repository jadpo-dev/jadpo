//! What the app reaches of a package, for `jadpo package audit` (area 11,
//! "Yanks, takedowns and advisories"): an advisory names declarations, so
//! the tool can say whether the app's own code gets to each, directly or
//! through what the package calls itself. A test isn't the app: code reached
//! only from a test doesn't count.

use std::collections::{BTreeSet, VecDeque};

use jadpo_db::{Db, Project};
use jadpo_graph::{EdgeKind, Kind};

use crate::surface;

/// The declarations of the package in `folder` the app reaches, as the app
/// writes them after the package's name (`uk.parse`, `UkPostcode`).
pub fn reached(db: &dyn Db, project: Project, folder: &str) -> BTreeSet<String> {
    let Some(home) = surface::homes(db, project).remove(folder) else {
        return BTreeSet::new();
    };
    let graph = crate::graph::graph(db, project);
    let prefix = format!("{folder}/");
    let own = format!("{}.", home.through);
    let packages = format!("{}/", jadpo_db::PACKAGES);
    let mut waiting: VecDeque<_> = graph
        .ids()
        .filter(|&id| {
            let node = graph.node(id);
            node.kind != Kind::Test
                && node.place.as_ref().is_some_and(|place| {
                    !place.file.starts_with(&packages) && !place.file.ends_with(".test.jadpo")
                })
        })
        .collect();
    let mut seen: BTreeSet<_> = waiting.iter().copied().collect();
    let mut out = BTreeSet::new();
    while let Some(id) = waiting.pop_front() {
        let node = graph.node(id);
        if node
            .place
            .as_ref()
            .is_some_and(|place| place.file.starts_with(&prefix))
            && let Some(name) = node.name.strip_prefix(&own)
        {
            out.insert(name.to_string());
        }
        for (kind, targets) in graph.edges_from(id) {
            if matches!(
                kind,
                EdgeKind::Calls | EdgeKind::Uses | EdgeKind::Raises | EdgeKind::Runs
            ) {
                for target in targets {
                    if seen.insert(target) {
                        waiting.push_back(target);
                    }
                }
            }
        }
    }
    out
}

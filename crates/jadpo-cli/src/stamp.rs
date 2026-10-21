//! The build stamp (area 18, "Production builds"; docs/compiler-runtime.md
//! §2.8; chunks 8.1o and 8.1o2): what `jadpo build` puts in a deployable
//! and `jadpo check` writes in the manifest, so an event's build id finds
//! the manifest that explains it.
//!
//! A build id is `b_` and the first twelve hex digits of a SHA-256 over
//! the source it is built from, the compiler, the target and the package
//! lock ([`build_id`]). The source is the graph's digest, which every
//! node's content changes, with the deployable's own declaration (its
//! name, what it serves and runs and its target), so two deployables of
//! one app never share an id ([`source`]). That holds the whole project's
//! graph, not only what the deployable reaches: a change it doesn't reach
//! still gives it a new id until it is compiled from only what it reaches
//! (roadmap chunk 8.1o3).

use std::path::Path;
use std::process::Command;

use jadpo_check::deployables::Deployable;
use jadpo_graph::manifest::{DeployableStamp, Source};
use sha2::{Digest, Sha256};

/// Area 18's build id: `b_` and the first twelve hex digits of a SHA-256
/// over the source's digest, the compiler, the target and the package
/// lock.
pub fn build_id(source: &str, compiler: &str, target: &str, lock: &[u8]) -> String {
    let mut hasher = Sha256::new();
    for part in [
        b"jadpo build id 1".as_slice(),
        source.as_bytes(),
        compiler.as_bytes(),
        target.as_bytes(),
        lock,
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("b_{}", &hex[..12])
}

/// What a deployable's build id digests as its source: the graph's digest
/// and the deployable's declaration. The one deployable of an app that
/// declares none adds nothing, so its id is the one 8.1o gave it.
pub fn source(graph: &str, deployable: &Deployable) -> String {
    if deployable.at.is_none() {
        return graph.to_string();
    }
    format!(
        "{graph}\ndeployable {}\nserves {}\nruns {}\ntarget {}",
        deployable.name,
        deployable.serves.join(","),
        deployable.runs_words().join(","),
        deployable
            .target
            .as_ref()
            .map_or("none", |(target, _)| target.name()),
    )
}

/// Each of the project's deployables as the manifest lists it: the
/// target `jadpo build` builds it for and its build id.
pub fn deployables(
    deployables: &[Deployable],
    graph: &str,
    compiler: &str,
    lock: &[u8],
) -> Vec<DeployableStamp> {
    deployables
        .iter()
        .map(|deployable| {
            let target = deployable.builds_for().name();
            DeployableStamp {
                name: deployable.name.clone(),
                target: target.to_string(),
                build: build_id(&source(graph, deployable), compiler, target, lock),
            }
        })
        .collect()
}

/// The commit the project at `root` is at, and whether its tree holds
/// changes the commit doesn't (`build/` and `.jadpo/` aside): no commit,
/// and changed, when git doesn't track the project.
pub fn commit(root: &Path) -> Source {
    let git = |args: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    let untracked = Source {
        commit: None,
        modified: true,
    };
    let Some(commit) = git(&["rev-parse", "HEAD"]) else {
        return untracked;
    };
    if git(&["ls-files", "--error-unmatch", jadpo_db::PROJECT_FILE]).is_none() {
        return untracked;
    }
    let changes = git(&[
        "status",
        "--porcelain",
        "--",
        ".",
        ":(exclude)build",
        ":(exclude).jadpo",
    ]);
    Source {
        commit: Some(commit),
        modified: changes.is_none_or(|changes| !changes.is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_id_digests_the_source_the_compiler_the_target_and_the_lock() {
        let id = build_id("4b1d", "0.1.0", "native", b"");
        assert!(id.starts_with("b_") && id.len() == 14, "{id}");
        assert_eq!(id, build_id("4b1d", "0.1.0", "native", b""));
        for other in [
            build_id("4b1e", "0.1.0", "native", b""),
            build_id("4b1d", "0.1.1", "native", b""),
            build_id("4b1d", "0.1.0", "wasm", b""),
            build_id("4b1d", "0.1.0", "native", b"link ../couriers\n"),
        ] {
            assert_ne!(id, other);
        }
    }

    #[test]
    fn two_declared_deployables_of_one_source_have_ids_of_their_own() {
        let declared = |name: &str, serves: &[&str]| Deployable {
            name: name.to_string(),
            at: Some(jadpo_diag::Span::new("app.jadpo", 0, 1)),
            serves: serves.iter().map(|api| api.to_string()).collect(),
            runs: jadpo_std::Runs::EVERYTHING,
            target: None,
        };
        let kiosk = declared("kiosk", &["main"]);
        let office = declared("office", &["staff"]);
        let ids = deployables(&[kiosk.clone(), office], "4b1d", "0.1.0", b"");
        assert_ne!(ids[0].build, ids[1].build);
        assert_eq!(ids[0].target, "native");
        // Serving another API is another build.
        let wider = declared("kiosk", &["main", "staff"]);
        assert_ne!(
            deployables(&[wider], "4b1d", "0.1.0", b"")[0].build,
            ids[0].build
        );
        // `app`, which nothing declares, digests the graph alone.
        let app = Deployable { at: None, ..kiosk };
        assert_eq!(
            deployables(&[app], "4b1d", "0.1.0", b"")[0].build,
            build_id("4b1d", "0.1.0", "native", b"")
        );
    }
}

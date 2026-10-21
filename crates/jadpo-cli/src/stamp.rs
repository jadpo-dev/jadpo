//! The build stamp (area 18, "Production builds"; docs/compiler-runtime.md
//! §2.8; chunks 8.1o, 8.1o2 and 8.1o3): what `jadpo build` puts in a
//! deployable and `jadpo check` writes in the manifest, so an event's
//! build id finds the manifest that explains it.
//!
//! A build id is `b_` and the first twelve hex digits of a SHA-256 over
//! the source it is built from, the compiler, the target and the package
//! lock ([`build_id`]). The source is the deployable's program as it is
//! compiled from what it reaches (chunk 8.1o3; [`program`]): the Rust its
//! callables become, as the text of its tokens, and its plan's bytes,
//! with the deployable's name and each store's migrations, which it
//! compares as it starts. So a change it doesn't reach leaves its id
//! alone, and two deployables of one app never share one. A change that
//! moves a callable it reaches to other lines is a change to it: its
//! frames name those lines.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use jadpo_check::deployables::Deployable;
use jadpo_graph::manifest::{DeployableStamp, Source};
use jadpo_plan::Plan;
use sha2::{Digest, Sha256};

/// Each store's migrations, by the store's connection, in the order they
/// apply: each one's name and digest.
pub type Stores = BTreeMap<String, Vec<(String, String)>>;

/// Area 18's build id: `b_` and the first twelve hex digits of a SHA-256
/// over the source's digest, the compiler, the target and the package
/// lock.
pub fn build_id(source: &str, compiler: &str, target: &str, lock: &[u8]) -> String {
    let mut hasher = Sha256::new();
    for part in [
        b"jadpo build id 2".as_slice(),
        source.as_bytes(),
        compiler.as_bytes(),
        target.as_bytes(),
        lock,
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("b_{}", &hex(&hasher.finalize())[..12])
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The migrations of each of `project`'s stores.
pub fn stores(db: &dyn jadpo_db::Db, project: jadpo_db::Project) -> Stores {
    jadpo_check::data::migrations::store_steps(jadpo_check::data::migrations::migrations(
        db, project,
    ))
    .into_iter()
    .map(|(store, migrations)| {
        let migrations = migrations
            .into_iter()
            .map(|migration| (migration.name, migration.digest))
            .collect();
        (store, migrations)
    })
    .collect()
}

/// A deployable's program: its plan, holding only what it reaches, which
/// records its build id.
pub struct Program {
    pub plan: Plan,
    pub build: String,
}

/// `deployable`'s program, one of `project`'s, which checked without
/// errors, and its build id. `Err` with why, which is Jadpo's own mistake:
/// its plan names what it doesn't hold, or a form isn't compiled.
pub fn program(
    db: &dyn jadpo_db::Db,
    project: jadpo_db::Project,
    deployable: &Deployable,
    compiler: &str,
    lock: &[u8],
    stores: &Stores,
) -> Result<Program, String> {
    let plan =
        Plan::for_deployable(db, project, deployable).map_err(|unkept| unkept.to_string())?;
    let written = jadpo_codegen::unprinted(&plan).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut part = |bytes: &[u8]| {
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    };
    part(b"deployable");
    part(deployable.name.as_bytes());
    // What a build with the `testing` feature holds isn't the
    // deployable's.
    for (path, bytes) in &written.files {
        if path.ends_with("plan-testing.bin") {
            continue;
        }
        part(path.as_bytes());
        part(bytes);
    }
    for (store, migrations) in stores {
        part(store.as_bytes());
        for (name, digest) in migrations {
            part(name.as_bytes());
            part(digest.as_bytes());
        }
    }
    let source = hex(&hasher.finalize());
    let build = build_id(&source, compiler, deployable.builds_for().name(), lock);
    Ok(Program {
        plan: plan.with_build(build.clone()),
        build,
    })
}

/// Each of the project's deployables as the manifest lists it: the
/// target `jadpo build` builds it for and its build id, which a project
/// with errors, which builds nothing, has none of.
pub fn deployables(
    db: &dyn jadpo_db::Db,
    project: jadpo_db::Project,
    errors: usize,
    compiler: &str,
    lock: &[u8],
) -> Vec<DeployableStamp> {
    let stores = (errors == 0).then(|| stores(db, project));
    jadpo_check::deployables::deployables(db, project)
        .list
        .iter()
        .map(|deployable| DeployableStamp {
            name: deployable.name.clone(),
            target: deployable.builds_for().name().to_string(),
            build: stores.as_ref().and_then(|stores| {
                program(db, project, deployable, compiler, lock, stores)
                    .ok()
                    .map(|program| program.build)
            }),
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
        assert!(
            id.starts_with("b_")
                && id.len() == 14
                && id[2..].chars().all(|c| c.is_ascii_hexdigit()),
            "{id}"
        );
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
}

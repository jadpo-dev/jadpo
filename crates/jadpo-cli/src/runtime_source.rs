//! The runtime's source this `jadpo` carries (roadmap chunk 9.2c;
//! docs/compiler-runtime.md §2.8, "Toolchain pinning"): the shared and
//! runtime crates a deployable compiles against, packed into the binary
//! by build.rs, with the lock and the toolchain file it pins, so `jadpo
//! build` never reads the folder `jadpo` was compiled in.
//!
//! [`written`] keeps them in `runtime/<version>-<copy>/` of Jadpo's cache,
//! `<copy>` a digest of where this `jadpo` is, so two copies never write
//! one folder and one copy keeps the same folder however often it is
//! rebuilt: a file whose bytes are unchanged keeps its time, so cargo
//! compiles again only what changed, and a file this `jadpo` doesn't
//! carry is removed. `.jadpo-runtime` there names the digest of what was
//! last written whole; the folder's lock is held while it is written.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The packed crates, deflated.
const PACKED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/runtime.deflate"));

/// The digest of the packed crates.
pub(crate) const DIGEST: &str = env!("JADPO_RUNTIME_DIGEST");

/// The runtime crates' lock, which a deployable's build keeps to.
pub(crate) const LOCK: &[u8] = include_bytes!("../../../Cargo.lock");

/// The toolchain file this `jadpo` pins, written into each deployable's
/// workspace.
pub(crate) const TOOLCHAIN: &str = include_str!("../../../rust-toolchain.toml");

/// The file naming the digest of what a folder holds.
const MARKER: &str = ".jadpo-runtime";

/// Each carried file by its path from the folder, in order.
pub(crate) fn files() -> &'static [(String, Vec<u8>)] {
    static FILES: OnceLock<Vec<(String, Vec<u8>)>> = OnceLock::new();
    FILES.get_or_init(|| {
        let packed = miniz_oxide::inflate::decompress_to_vec(PACKED).unwrap_or_default();
        let mut out = Vec::new();
        let mut rest = &packed[..];
        while rest.len() >= 4 {
            let (length, after) = rest.split_at(4);
            let length = u32::from_le_bytes(length.try_into().unwrap_or_default()) as usize;
            let (path, after) = after.split_at(length.min(after.len()));
            let (size, after) = after.split_at(8.min(after.len()));
            let size = u64::from_le_bytes(size.try_into().unwrap_or_default()) as usize;
            let (bytes, after) = after.split_at(size.min(after.len()));
            out.push((String::from_utf8_lossy(path).into_owned(), bytes.to_vec()));
            rest = after;
        }
        out
    })
}

/// The folder of this copy of `jadpo` in `cache`, Jadpo's cache folder:
/// `<version>-<copy>`, `<copy>` the first twelve hex digits of the
/// SHA-256 of where the running `jadpo` is.
pub(crate) fn copy_key() -> String {
    use sha2::{Digest, Sha256};
    let exe = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .unwrap_or_default();
    let digest: String = Sha256::digest(exe.as_os_str().as_encoded_bytes())
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{}-{digest}", crate::help::VERSION)
}

/// Writes the carried crates into `cache`'s `runtime/`, unless what is
/// there is already them: the folder holding them.
pub(crate) fn written(cache: &Path) -> std::io::Result<PathBuf> {
    let parent = cache.join("runtime");
    std::fs::create_dir_all(&parent)?;
    let key = copy_key();
    let folder = parent.join(&key);
    let lock = std::fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(parent.join(format!("{key}.lock")))?;
    lock.lock()?;
    if std::fs::read_to_string(folder.join(MARKER)).ok().as_deref() == Some(DIGEST) {
        return Ok(folder);
    }
    let carried = files();
    if carried.is_empty() {
        return Err(std::io::Error::other(
            "this jadpo carries no runtime source",
        ));
    }
    let _ = std::fs::remove_file(folder.join(MARKER));
    let mut kept = BTreeSet::new();
    for (path, bytes) in carried {
        let to = folder.join(path);
        kept.insert(to.clone());
        if std::fs::read(&to).ok().as_deref() == Some(&bytes[..]) {
            continue;
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&to, bytes)?;
    }
    removed_unless(&folder, &kept)?;
    std::fs::write(folder.join(MARKER), DIGEST)?;
    drop(lock);
    Ok(folder)
}

/// Removes every file under `folder` that isn't in `kept`.
fn removed_unless(folder: &Path, kept: &BTreeSet<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_dir() {
            removed_unless(&path, kept)?;
            if std::fs::read_dir(&path)?.next().is_none() {
                std::fs::remove_dir(&path)?;
            }
        } else if !kept.contains(&path) && path.file_name() != Some(MARKER.as_ref()) {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The crates `[workspace.metadata.layers]` lists as shared and
    /// runtime, which a generated program and its host depend on.
    fn layered() -> Vec<String> {
        let root = include_str!("../../../Cargo.toml");
        let layers = &root[root
            .find("[workspace.metadata.layers]")
            .expect("the layers")..];
        let mut out = Vec::new();
        for layer in ["shared = [", "runtime = ["] {
            let list = &layers[layers.find(layer).expect("a layer") + layer.len()..];
            let list = &list[..list.find(']').expect("a list")];
            out.extend(
                list.split(',')
                    .map(|name| name.trim().trim_matches('"').to_string())
                    .filter(|name| !name.is_empty()),
            );
        }
        out
    }

    #[test]
    fn every_shared_and_runtime_crate_is_carried_with_its_source_alone() {
        let carried = files();
        let paths: Vec<&str> = carried.iter().map(|(path, _)| path.as_str()).collect();
        for name in layered() {
            assert!(
                paths.contains(&format!("{name}/Cargo.toml").as_str()),
                "{name}"
            );
            assert!(
                paths.contains(&format!("{name}/src/lib.rs").as_str()),
                "{name}"
            );
        }
        for path in &paths {
            let crate_name = path.split('/').next().unwrap_or_default();
            assert!(
                *path == "Cargo.toml" || layered().iter().any(|name| name == crate_name),
                "{path} isn't a runtime crate's"
            );
            assert!(
                *path == "Cargo.toml" || path.ends_with("/Cargo.toml") || path.contains("/src/"),
                "{path}"
            );
        }
    }

    #[test]
    fn the_carried_manifests_name_nothing_outside_what_is_carried() {
        let manifest = |path: &str| {
            let (_, bytes) = files()
                .iter()
                .find(|(carried, _)| carried == path)
                .unwrap_or_else(|| panic!("{path}"));
            String::from_utf8(bytes.clone()).expect("UTF-8")
        };
        let workspace = manifest("Cargo.toml");
        assert!(
            workspace.contains("jadpo-runtime = { path = \"jadpo-runtime\" }"),
            "{workspace}"
        );
        assert!(!workspace.contains("crates/"), "{workspace}");
        assert!(!workspace.contains("jadpo-check"), "{workspace}");
        assert!(workspace.contains("[workspace.lints.rust]"), "{workspace}");
        assert!(!workspace.contains("[profile"), "{workspace}");
        for name in layered() {
            let own = manifest(&format!("{name}/Cargo.toml"));
            assert!(!own.contains("dev-dependencies"), "{name}: {own}");
            assert!(!own.contains("[[test]]"), "{name}: {own}");
            assert!(!own.contains("[[example]]"), "{name}: {own}");
        }
    }

    #[test]
    fn the_folder_is_written_once_and_kept_to_what_is_carried() {
        let cache =
            std::env::temp_dir().join(format!("jadpo-runtime-source-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cache);
        let folder = written(&cache).expect("written");
        let lib = folder.join("jadpo-runtime/src/lib.rs");
        assert!(lib.exists());
        let stray = folder.join("jadpo-runtime/src/stray.rs");
        std::fs::write(&stray, "").expect("a stray file");
        let before = std::fs::metadata(&lib)
            .and_then(|meta| meta.modified())
            .expect("a time");
        // Unchanged, it is left alone.
        assert_eq!(written(&cache).expect("written"), folder);
        assert!(stray.exists());
        // A folder not written whole is written again, keeping unchanged
        // files' times and removing what isn't carried.
        std::fs::remove_file(folder.join(MARKER)).expect("the marker");
        written(&cache).expect("written");
        assert!(!stray.exists());
        let after = std::fs::metadata(&lib)
            .and_then(|meta| meta.modified())
            .expect("a time");
        assert_eq!(before, after);
        let _ = std::fs::remove_dir_all(&cache);
    }
}

//! The package cache (docs/compiler-runtime.md §2.3; area 11, "The package
//! format"; chunk 7.1c2): an installed package's check, kept on disk by
//! its digest and the compiler, so an unchanged package costs nothing
//! after its first check.
//!
//! `~/.cache/jadpo/packages/<digest>/<compiler>` (under `XDG_CACHE_HOME`
//! when it is set) lists, one per line, the paths inside the package of
//! the source files this compiler checked with no problem at that digest.
//! `jadpo check` doesn't check those again, and adds each file it checks
//! cleanly. A package's code sees only its own names (area 11, "Chosen in
//! chunk 7.1"), so its check never depends on the app. The compiler is its
//! version and the size and modification time of the running `jadpo`, so
//! a rebuilt compiler never trusts an older one's check. Only a package
//! whose files match its digest in `jadpo.lock` is cached; a linked one,
//! which has no digest, never is. With neither variable set, or a cache
//! that can't be read or written, every file is checked, as without one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use jadpo_db::{Loaded, Source};
use jadpo_diag::Diagnostic;
use jadpo_term::Environment;

/// The folder the cache lives in, if the environment names a home for it.
fn folder(env: &Environment) -> Option<PathBuf> {
    Some(home(env)?.join("jadpo").join("packages"))
}

/// The user's cache folder, `$XDG_CACHE_HOME`, else `~/.cache`, if the
/// environment names either; Jadpo keeps its caches in its `jadpo/`.
pub(crate) fn home(env: &Environment) -> Option<PathBuf> {
    Some(match env.var("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
        Some(cache) => PathBuf::from(cache),
        None => PathBuf::from(env.var("HOME").filter(|v| !v.is_empty())?).join(".cache"),
    })
}

/// This compiler, as the cache tells compilers apart.
pub fn compiler() -> String {
    let built = std::env::current_exe()
        .and_then(std::fs::metadata)
        .map(|meta| {
            let modified = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |since| since.as_nanos());
            format!("{:x}-{modified:x}", meta.len())
        })
        .unwrap_or_default();
    format!("{}-{built}", crate::help::VERSION)
}

/// The installed packages whose files match their digest in the lock: each
/// folder with its digest.
fn held(loaded: &Loaded) -> BTreeMap<String, String> {
    let (db, project) = (&loaded.db, loaded.project);
    let Some(text) = project
        .records(db)
        .iter()
        .find(|record| record.path == jadpo_db::LOCK)
        .and_then(|record| record.text.as_ref().ok())
    else {
        return BTreeMap::new();
    };
    let Ok(lines) = jadpo_db::parse_lock(text) else {
        return BTreeMap::new();
    };
    let homes = jadpo_check::surface::homes(db, project);
    let mut out = BTreeMap::new();
    for line in lines {
        let digest = match &line.source {
            Source::Registry { digest, .. }
            | Source::Git { digest, .. }
            | Source::Path { digest, .. } => digest,
            Source::Link { .. } => continue,
        };
        let folder = line.folder();
        let Some(home) = homes.get(&folder) else {
            continue;
        };
        if jadpo_check::packages::digest_of(db, project, home).as_deref() == Ok(digest.as_str()) {
            out.insert(folder, digest.clone());
        }
    }
    out
}

/// Where one package's entry is.
fn entry(cache: &Path, digest: &str) -> PathBuf {
    let digest = digest.strip_prefix("sha256:").unwrap_or(digest);
    cache.join(digest).join(compiler())
}

/// The cached package files' paths in the project, read before a check.
fn read(cache: &Path, held: &BTreeMap<String, String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (folder, digest) in held {
        let Ok(text) = std::fs::read_to_string(entry(cache, digest)) else {
            continue;
        };
        out.extend(
            text.lines()
                .filter(|line| !line.is_empty())
                .map(|line| format!("{folder}/{line}")),
        );
    }
    out
}

/// Tells the project which installed package files the cache holds as
/// checked, so `jadpo check` skips them; returns what it found to keep.
pub fn apply(env: &Environment, loaded: &mut Loaded) -> Option<Held> {
    let cache = folder(env)?;
    let held = held(loaded);
    if held.is_empty() {
        return None;
    }
    let checked = read(&cache, &held);
    if !checked.is_empty() {
        loaded.set_checked(checked.clone());
    }
    Some(Held {
        cache,
        held,
        checked,
    })
}

/// The cache as a check found it.
pub struct Held {
    cache: PathBuf,
    held: BTreeMap<String, String>,
    checked: BTreeSet<String>,
}

impl Held {
    /// Adds each package file the check compiled with no problem.
    pub fn record(&self, loaded: &Loaded, diagnostics: &[Diagnostic]) {
        let (db, project) = (&loaded.db, loaded.project);
        let troubled: BTreeSet<&str> = diagnostics
            .iter()
            .flat_map(|d| d.primary.iter().chain(d.secondary.iter()))
            .map(|label| label.span.path.as_str())
            .collect();
        for (folder, digest) in &self.held {
            let prefix = format!("{folder}/");
            let mut clean: BTreeSet<String> = BTreeSet::new();
            for file in project.files(db) {
                let path = file.path(db);
                if let Some(inside) = path.strip_prefix(&prefix)
                    && !troubled.contains(path.as_str())
                {
                    clean.insert(inside.to_string());
                }
            }
            let known: BTreeSet<String> = self
                .checked
                .iter()
                .filter_map(|path| path.strip_prefix(&prefix))
                .map(str::to_string)
                .collect();
            if clean.is_subset(&known) {
                continue;
            }
            clean.extend(known);
            let text: String = clean.iter().map(|path| format!("{path}\n")).collect();
            let at = entry(&self.cache, digest);
            let Some(parent) = at.parent() else { continue };
            // Written beside and renamed, so a reader never sees half.
            let partial = parent.join(format!(".{}.{}", std::process::id(), compiler()));
            let _ = std::fs::create_dir_all(parent)
                .and_then(|()| std::fs::write(&partial, text))
                .and_then(|()| std::fs::rename(&partial, &at));
            let _ = std::fs::remove_file(&partial);
        }
    }
}

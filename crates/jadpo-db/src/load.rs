//! Finding and reading a project: the only I/O in the compiler crates.

use std::path::{Path, PathBuf};

use salsa::Durability;

use crate::places::{Ownership, locate};
use crate::{Database, File, Layout, Project};

/// The file that marks a project's root (area 14).
pub const PROJECT_FILE: &str = "app.jadpo";

/// The nearest folder at or above `start` that holds `app.jadpo`.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|folder| folder.join(PROJECT_FILE).is_file())
        .map(Path::to_path_buf)
}

/// A source file that could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unreadable {
    /// Relative to the root, like every path the compiler prints.
    pub path: String,
    pub error: String,
}

/// A project read into a fresh database.
#[derive(Debug)]
pub struct Loaded {
    pub db: Database,
    pub project: Project,
    /// The folder paths are relative to: the project root, or for loose
    /// files the folder they were found in.
    pub root: PathBuf,
    pub unreadable: Vec<Unreadable>,
}

/// Reads what `target` names. Inside a project (a folder at or above it
/// holds `app.jadpo`) that is the whole project; otherwise the file, or
/// every `.jadpo` file under the folder, as loose files.
pub fn load(target: &Path) -> Loaded {
    let folder = if target.is_dir() {
        target.to_path_buf()
    } else {
        target
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    };
    let (root, layout, sources) = match find_root(&folder) {
        Some(root) => {
            let sources = walk(&root, true);
            (root, Layout::Project, sources)
        }
        None if target.is_dir() => (folder.clone(), Layout::Loose, walk(&folder, false)),
        None => (folder, Layout::Loose, vec![target.to_path_buf()]),
    };
    let db = Database::default();
    let mut files = Vec::new();
    let mut other_files = Vec::new();
    let mut unreadable = Vec::new();
    for path in sources {
        let shown = display(&root, &path);
        if !shown.ends_with(".jadpo") {
            other_files.push(shown);
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let durability = match locate(&shown, layout).ownership {
                    Ownership::Domain(_) | Ownership::Loose => Durability::LOW,
                    Ownership::Shared | Ownership::Root => Durability::MEDIUM,
                };
                files.push(
                    File::builder(shown, text)
                        .path_durability(durability)
                        .text_durability(durability)
                        .new(&db),
                );
            }
            Err(error) => unreadable.push(Unreadable {
                path: shown,
                error: error.to_string(),
            }),
        }
    }
    let project = Project::new(&db, files, other_files, layout);
    Loaded {
        db,
        project,
        root,
        unreadable,
    }
}

/// A path as the reader sees it: relative to the root, with `/`.
fn display(root: &Path, file: &Path) -> String {
    let relative = file.strip_prefix(root).unwrap_or(file);
    relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Every file under `folder` in path order, leaving out hidden entries and
/// the generated `build/`. Outside `.jadpo` files, only a project's code
/// folders are listed, where templates and other assets sit beside code.
fn walk(folder: &Path, project: bool) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => {
                    if !(dir == folder && name == "build") {
                        pending.push(path);
                    }
                }
                Ok(_) if name.ends_with(".jadpo") => found.push(path),
                Ok(_) if project => {
                    let shown = display(folder, &path);
                    if shown.starts_with("domains/") || shown.starts_with("shared/") {
                        found.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    found.sort_by_key(|path| display(folder, path));
    found
}

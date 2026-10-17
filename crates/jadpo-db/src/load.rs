//! Finding and reading a project: the only I/O in the compiler crates.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use salsa::{Durability, Setter};

use crate::places::{Ownership, locate};
use crate::{
    Database, File, IDENTITIES, Layout, Project, Record, is_record, is_translation,
    parse_identities,
};

/// The file that marks a project's root (area 14).
pub const PROJECT_FILE: &str = "app.jadpo";

/// The nearest folder at or above `start` that holds `app.jadpo`.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|folder| folder.join(PROJECT_FILE).is_file())
        .map(Path::to_path_buf)
}

/// The folder every printed path is relative to (`root`), as the reader
/// reaches it from `cwd`, the folder the command ran in: `None` when `cwd`
/// is inside it, since a project's own paths work from anywhere in it;
/// otherwise the way the command line spelt it (`examples/07-testing`,
/// `../shop`, or an absolute path), so `<root>/<path>` opens the file and
/// a rerun command runs from `cwd`.
pub fn shown_root(cwd: &Path, root: &Path) -> Option<String> {
    let real = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if real(cwd).starts_with(real(root)) {
        return None;
    }
    let relative = root.strip_prefix(cwd).unwrap_or(root);
    let mut lead = String::new();
    let mut parts: Vec<String> = Vec::new();
    for part in relative.components() {
        match part {
            std::path::Component::Prefix(prefix) => {
                lead.push_str(&prefix.as_os_str().to_string_lossy());
            }
            std::path::Component::RootDir => lead.push('/'),
            std::path::Component::CurDir => {}
            other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
        }
    }
    let shown = format!("{lead}{}", parts.join("/"));
    (!shown.is_empty()).then_some(shown)
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
    load_into(target, Database::default())
}

/// [`load`] into a database of the caller's, such as one that records what
/// it executes.
pub fn load_into(target: &Path, mut db: Database) -> Loaded {
    let folder = if target.is_dir() {
        target.to_path_buf()
    } else {
        target
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    };
    let mut unreadable = Vec::new();
    let (root, layout, sources) = match find_root(&folder) {
        Some(root) => {
            let sources = walk(&root, true, &mut unreadable);
            (root, Layout::Project, sources)
        }
        None if target.is_dir() => {
            let sources = walk(&folder, false, &mut unreadable);
            (folder.clone(), Layout::Loose, sources)
        }
        None => (folder, Layout::Loose, vec![target.to_path_buf()]),
    };
    let mut files = Vec::new();
    let mut other_files = Vec::new();
    let mut records = Vec::new();
    let mut translations = Vec::new();
    for path in sources {
        let shown = display(&root, &path);
        if layout == Layout::Project && is_record(&shown) {
            records.push(read_record(&root, &shown));
            continue;
        }
        if layout == Layout::Project && is_translation(&shown) {
            translations.push(read_record(&root, &shown));
            continue;
        }
        if !shown.ends_with(".jadpo") {
            other_files.push(shown);
            continue;
        }
        // A pipe or a device named `.jadpo` would block or never end.
        if !std::fs::metadata(&path).is_ok_and(|meta| meta.is_file()) {
            unreadable.push(Unreadable {
                path: shown,
                error: "it isn't a regular file".to_string(),
            });
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let durability = durability(&shown, layout);
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
    // The identity registry (area 18): its lines are an input of their own,
    // and its text a record, for the edits that add a line to it.
    let identities = match identities(&records) {
        Ok(lines) => lines,
        Err(error) => {
            unreadable.push(Unreadable {
                path: IDENTITIES.to_string(),
                error,
            });
            BTreeMap::new()
        }
    };
    let project = Project::new(&db, files, other_files, layout, identities, records);
    if !translations.is_empty() {
        project.set_translations(&mut db).to(translations);
    }
    Loaded {
        db,
        project,
        root,
        unreadable,
    }
}

/// The identity registry's lines, from its record when the project has
/// one, or why they can't be read.
fn identities(records: &[Record]) -> Result<BTreeMap<String, String>, String> {
    let Some(record) = records.iter().find(|record| record.path == IDENTITIES) else {
        return Ok(BTreeMap::new());
    };
    let text = record.text.as_ref().map_err(Clone::clone)?;
    parse_identities(text).map_err(|error| format!("it isn't a JSON object of names: {error}"))
}

/// The record at `path`, read from `root`.
fn read_record(root: &Path, path: &str) -> Record {
    Record {
        path: path.to_string(),
        text: std::fs::read_to_string(root.join(path)).map_err(|error| error.to_string()),
    }
}

/// How rarely a file changes (docs/compiler-runtime.md §2.4): domain files
/// low, `shared/` and the root files medium, so a keystroke in a domain
/// never revalidates what only the others decide.
fn durability(path: &str, layout: Layout) -> Durability {
    match locate(path, layout).ownership {
        Ownership::Domain(_) | Ownership::Loose => Durability::LOW,
        Ownership::Shared | Ownership::Root => Durability::MEDIUM,
    }
}

impl Loaded {
    /// The source file at the project-relative `path`.
    pub fn file(&self, path: &str) -> Option<File> {
        self.project
            .files(&self.db)
            .iter()
            .copied()
            .find(|file| file.path(&self.db) == path)
    }

    /// An absolute path inside the root as the project names it: relative,
    /// with `/`. `None` outside the root.
    pub fn relative(&self, path: &Path) -> Option<String> {
        let relative = path.strip_prefix(&self.root).ok()?;
        Some(display(Path::new(""), relative))
    }

    /// Sets the text of the source file at `path`, adding it when it is
    /// new. Only `.jadpo` files outside `build/` are source; anything else
    /// is ignored. The database keeps every other result it can.
    pub fn set_text(&mut self, path: &str, text: String) {
        if !is_source(path) {
            return;
        }
        if let Some(file) = self.file(path) {
            if file.text(&self.db) != &text {
                file.set_text(&mut self.db).to(text);
            }
            return;
        }
        let layout = self.project.layout(&self.db);
        let durability = durability(path, layout);
        let file = File::builder(path.to_string(), text)
            .path_durability(durability)
            .text_durability(durability)
            .new(&self.db);
        let mut files = self.project.files(&self.db).clone();
        files.push(file);
        files.sort_by(|a, b| a.path(&self.db).cmp(b.path(&self.db)));
        self.project.set_files(&mut self.db).to(files);
    }

    /// Removes the source file at `path`, if the project has it.
    pub fn remove(&mut self, path: &str) {
        let files = self.project.files(&self.db).clone();
        let kept: Vec<File> = files
            .iter()
            .copied()
            .filter(|file| file.path(&self.db) != path)
            .collect();
        if kept.len() != files.len() {
            self.project.set_files(&mut self.db).to(kept);
        }
    }

    /// Reads `path` from disk again: its text now, or its removal when it
    /// is gone or unreadable.
    pub fn reload(&mut self, path: &str) {
        match std::fs::read_to_string(self.root.join(path)) {
            Ok(text) => self.set_text(path, text),
            Err(_) => self.remove(path),
        }
    }

    /// Whether the project holds `path`, project-relative: a source file,
    /// or in a project another file beside the code (a template).
    pub fn tracks(&self, path: &str) -> bool {
        tracked(path, self.project.layout(&self.db) == Layout::Project)
    }

    /// Brings `path` up to date with the disk after a change there, as a
    /// watcher sees it: a source file's text, added or removed, or another
    /// file's presence. A source file that exists but can't be read joins
    /// [`Loaded::unreadable`] until it can; one that is gone leaves.
    pub fn refresh(&mut self, path: &str) {
        if !self.tracks(path) {
            return;
        }
        self.unreadable.retain(|file| file.path != path);
        let full = self.root.join(path);
        if self.project.layout(&self.db) == Layout::Project && is_record(path) {
            let mut records = self.project.records(&self.db).clone();
            records.retain(|record| record.path != path);
            if full.is_file() {
                records.push(read_record(&self.root, path));
                records.sort_by(|a, b| a.path.cmp(&b.path));
            }
            if path == IDENTITIES {
                match identities(&records) {
                    Ok(lines) => {
                        if &lines != self.project.identities(&self.db) {
                            self.project.set_identities(&mut self.db).to(lines);
                        }
                    }
                    Err(error) => self.unreadable.push(Unreadable {
                        path: path.to_string(),
                        error,
                    }),
                }
            }
            if &records != self.project.records(&self.db) {
                self.project.set_records(&mut self.db).to(records);
            }
            return;
        }
        if self.project.layout(&self.db) == Layout::Project && is_translation(path) {
            let mut translations = self.project.translations(&self.db).clone();
            translations.retain(|record| record.path != path);
            if full.is_file() {
                translations.push(read_record(&self.root, path));
                translations.sort_by(|a, b| a.path.cmp(&b.path));
            }
            if &translations != self.project.translations(&self.db) {
                self.project.set_translations(&mut self.db).to(translations);
            }
            return;
        }
        if !is_source(path) {
            let mut others = self.project.other_files(&self.db).clone();
            let present = full.is_file();
            let known = others.iter().any(|other| other == path);
            if present != known {
                if present {
                    others.push(path.to_string());
                    others.sort();
                } else {
                    others.retain(|other| other != path);
                }
                self.project.set_other_files(&mut self.db).to(others);
            }
            return;
        }
        let read = if full.exists() && !std::fs::metadata(&full).is_ok_and(|meta| meta.is_file()) {
            Err("it isn't a regular file".to_string())
        } else {
            std::fs::read_to_string(&full).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    String::new()
                } else {
                    error.to_string()
                }
            })
        };
        match read {
            Ok(text) => self.set_text(path, text),
            Err(error) if error.is_empty() => self.remove(path),
            Err(error) => {
                self.remove(path);
                self.unreadable.push(Unreadable {
                    path: path.to_string(),
                    error,
                });
                self.unreadable.sort_by(|a, b| a.path.cmp(&b.path));
            }
        }
    }
}

/// Every file a project or a folder of loose files holds, relative to
/// `folder` with `/`, in path order: what [`load`] reads and a watcher
/// watches. Folders that can't be listed are left out.
pub fn files_under(folder: &Path, project: bool) -> Vec<String> {
    walk(folder, project, &mut Vec::new())
        .iter()
        .map(|path| display(folder, path))
        .collect()
}

/// Whether a project-relative path is one a project holds: a source file,
/// or in a project a file in a code folder.
fn tracked(path: &str, project: bool) -> bool {
    if is_source(path) {
        return true;
    }
    project
        && !path.split('/').any(|part| part.starts_with('.'))
        && (path.starts_with("domains/")
            || path.starts_with("shared/")
            || is_record(path)
            || is_translation(path))
}

/// Whether a project-relative path is a source file the project holds.
fn is_source(path: &str) -> bool {
    path.ends_with(".jadpo")
        && !path.starts_with("build/")
        && !path.split('/').any(|part| part.starts_with('.'))
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
fn walk(folder: &Path, project: bool, unreadable: &mut Vec<Unreadable>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            // A folder that can't be listed may hold source: checking
            // without it would pass a project with code missing.
            Err(error) => {
                let shown = display(folder, &dir);
                unreadable.push(Unreadable {
                    path: if shown.is_empty() {
                        ".".into()
                    } else {
                        format!("{shown}/")
                    },
                    error: error.to_string(),
                });
                continue;
            }
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
                    if shown.starts_with("domains/")
                        || shown.starts_with("shared/")
                        || is_record(&shown)
                        || is_translation(&shown)
                    {
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

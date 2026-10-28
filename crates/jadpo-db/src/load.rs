//! Finding and reading a project: the only I/O in the compiler crates.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use salsa::{Durability, Setter};

use crate::places::{Ownership, locate};
use crate::{
    Database, File, IDENTITIES, LEGACY_IDENTITIES, Layout, Project, Record, is_record,
    parse_identities, parse_legacy_identities,
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
    /// Each package `jadpo.lock` links to a live checkout (area 11,
    /// "Developing and publishing a package"): its folder in the project,
    /// `packages/acme_pricing/v2`, whose files are read from the checkout,
    /// and the checkout as the lock writes it, `../acme-pricing`.
    pub links: Vec<(String, String)>,
}

/// Reads what `target` names. Inside a project (a folder at or above it
/// holds `app.jadpo`) that is the whole project; otherwise the file, or
/// every `.jadpo` file under the folder, as loose files.
pub fn load(target: &Path) -> Loaded {
    load_into(target, Database::default())
}

/// [`load`] into a database of the caller's, such as one that records what
/// it executes.
pub fn load_into(target: &Path, db: Database) -> Loaded {
    read_into(target, db, true)
}

/// [`load`] with every installed package's resources compiled, whether or
/// not the app calls into them: for the commands that show or compare a
/// package whole (`jadpo package show`, `update`).
pub fn load_whole(target: &Path) -> Loaded {
    read_into(target, Database::default(), false)
}

/// Reads `target` into `db`; `lazily`, an installed package's resources
/// wait until the app calls into them ([`crate::lazy`]).
fn read_into(target: &Path, db: Database, lazily: bool) -> Loaded {
    let folder = if target.is_dir() {
        target.to_path_buf()
    } else {
        target
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    };
    let mut unreadable = Vec::new();
    let mut links = Vec::new();
    let (root, layout, sources) = match find_root(&folder) {
        Some(root) => {
            let mut sources = shown(&root, walk(&root, true, &mut unreadable));
            links = linked(&root, &mut sources, &mut unreadable);
            (root, Layout::Project, sources)
        }
        None if target.is_dir() => {
            let sources = shown(&folder, walk(&folder, false, &mut unreadable));
            (folder.clone(), Layout::Loose, sources)
        }
        None => {
            let sources = shown(&folder, vec![target.to_path_buf()]);
            (folder, Layout::Loose, sources)
        }
    };
    let mut files = Vec::new();
    let mut lazy = Vec::new();
    let mut other_files = Vec::new();
    let mut records = Vec::new();
    for (path, shown) in sources {
        // Loose files keep a prompt's text beside its type (area 21).
        let loose_prompt = layout == Layout::Loose && shown.ends_with(crate::PROMPT_EXTENSION);
        if (layout == Layout::Project && is_record(&shown)) || loose_prompt {
            records.push(read_record_at(&path, &shown));
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
                let waits = lazily && layout == Layout::Project && crate::lazy::is_lazy(&shown);
                let file = File::builder(shown, text)
                    .path_durability(durability)
                    .text_durability(durability)
                    .new(&db);
                if waits {
                    lazy.push(file);
                } else {
                    files.push(file);
                }
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
        Err((path, error)) => {
            unreadable.push(Unreadable { path, error });
            BTreeMap::new()
        }
    };
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let project = Project::builder(files, other_files, layout, identities, records)
        .name(name)
        .lazy(lazy)
        .new(&db);
    let mut loaded = Loaded {
        db,
        project,
        root,
        unreadable,
        links,
    };
    loaded.settle();
    loaded
}

/// Each path with how the project names it, relative to `root`.
fn shown(root: &Path, paths: Vec<PathBuf>) -> Vec<(PathBuf, String)> {
    paths
        .into_iter()
        .map(|path| {
            let shown = display(root, &path);
            (path, shown)
        })
        .collect()
}

/// Reads each package `jadpo.lock` links to from its live checkout
/// instead of its folder in `packages/` (area 11, "Developing and
/// publishing a package"), which stays as installed for `jadpo package
/// unlink`: the checkout's files take the folder's paths, so the package
/// is checked and run as the installed one would be. A checkout that isn't
/// there can't be read. Returns each link's folder and checkout.
fn linked(
    root: &Path,
    sources: &mut Vec<(PathBuf, String)>,
    unreadable: &mut Vec<Unreadable>,
) -> Vec<(String, String)> {
    let mut links = Vec::new();
    for line in link_lines(root) {
        let crate::Source::Link { path } = &line.source else {
            continue;
        };
        let folder = line.folder();
        let prefix = format!("{folder}/");
        sources.retain(|(_, shown)| !shown.starts_with(&prefix));
        let checkout = root.join(path);
        if let Some(missing) = missing_checkout(root, &line) {
            unreadable.push(missing);
            links.push((folder, path.clone()));
            continue;
        }
        for file in package_files(&checkout) {
            let relative = display(&checkout, &file);
            sources.push((file, format!("{prefix}{relative}")));
        }
        links.push((folder, path.clone()));
    }
    sources.sort_by(|a, b| a.1.cmp(&b.1));
    links
}

/// The lines of `root`'s `jadpo.lock` that link a package to a live
/// checkout. A lock that can't be read is the records' to report.
fn link_lines(root: &Path) -> Vec<crate::Locked> {
    let Ok(text) = std::fs::read_to_string(root.join(crate::LOCK)) else {
        return Vec::new();
    };
    let Ok(lines) = crate::parse_lock(&text) else {
        return Vec::new();
    };
    lines
        .into_iter()
        .filter(|line| matches!(line.source, crate::Source::Link { .. }))
        .collect()
}

/// Why the checkout `line` links to can't be read: it holds no index.
fn missing_checkout(root: &Path, line: &crate::Locked) -> Option<Unreadable> {
    let crate::Source::Link { path } = &line.source else {
        return None;
    };
    if root.join(path).join(crate::PACKAGE_INDEX).is_file() {
        return None;
    }
    Some(Unreadable {
        path: format!("{path}/"),
        error: format!(
            "`{}` links `{}` to it, and it holds no `{}`",
            crate::LOCK,
            line.package,
            crate::PACKAGE_INDEX
        ),
    })
}

/// Each package `root`'s `jadpo.lock` links to a live checkout: its folder
/// in the project, `packages/acme_pricing/v2`, and the checkout as the
/// lock writes it, `../acme-pricing`.
pub fn links(root: &Path) -> Vec<(String, String)> {
    link_lines(root)
        .iter()
        .filter_map(|line| match &line.source {
            crate::Source::Link { path } => Some((line.folder(), path.clone())),
            _ => None,
        })
        .collect()
}

/// Shows a linked package's problems and sources in its checkout: each
/// path under `packages/<name>/<version>/` as the same path under the
/// link's folder (area 11, "Developing and publishing a package").
pub fn show_in_checkouts(
    links: &[(String, String)],
    diagnostics: &mut [jadpo_diag::Diagnostic],
    sources: &mut BTreeMap<String, String>,
) {
    let moved = |path: &str| -> Option<String> {
        links.iter().find_map(|(folder, checkout)| {
            let rest = path.strip_prefix(&format!("{folder}/"))?;
            Some(format!("{checkout}/{rest}"))
        })
    };
    for d in diagnostics.iter_mut() {
        for label in d.primary.iter_mut().chain(d.secondary.iter_mut()) {
            if let Some(path) = moved(&label.span.path) {
                label.span.path = path;
            }
        }
    }
    let paths: Vec<String> = sources.keys().cloned().collect();
    for path in paths {
        if let Some(to) = moved(&path)
            && let Some(text) = sources.remove(&path)
        {
            sources.insert(to, text);
        }
    }
}

/// Every file a watcher of `root` watches, as the project names it, with
/// where it is on disk, in path order: the files [`load`] reads, so a
/// linked package's are its checkout's, at the package's folder, and the
/// folder's own installed copy isn't watched (area 11, "A package's names,
/// and watching a linked checkout").
pub fn watched_files(root: &Path, project: bool) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = files_under(root, project)
        .into_iter()
        .map(|path| {
            let file = root.join(&path);
            (path, file)
        })
        .collect();
    if !project {
        return found;
    }
    for (folder, path) in links(root) {
        let prefix = format!("{folder}/");
        found.retain(|(shown, _)| !shown.starts_with(&prefix));
        let checkout = root.join(path);
        for file in package_files(&checkout) {
            let shown = format!("{prefix}{}", display(&checkout, &file));
            if tracked(&shown, true) {
                found.push((shown, file));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// Every file of a package's own folder, leaving out hidden entries (its
/// `.git`) and a `build/` at its root, in path order: what installing
/// copies and what a link reads.
pub fn package_files(folder: &Path) -> Vec<PathBuf> {
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
                Ok(kind) if kind.is_file() => found.push(path),
                _ => {}
            }
        }
    }
    found.sort_by_key(|path| display(folder, path));
    found
}

/// The identity registry's lines, from its record when the project has
/// one, else from `database/identities.json`, where it was before chunk
/// 7.0, until `jadpo records update` rewrites it; or the file that can't
/// be read and why.
fn identities(records: &[Record]) -> Result<BTreeMap<String, String>, (String, String)> {
    let find = |path: &str| records.iter().find(|record| record.path == path);
    if let Some(record) = find(IDENTITIES) {
        // A registry that can't be read is `record.edited` or
        // `record.format_newer`, reported with every record's; until it is
        // put right every born name is the name now.
        return Ok(record
            .text
            .as_ref()
            .ok()
            .and_then(|text| parse_identities(text).ok())
            .unwrap_or_default());
    }
    let Some(record) = find(LEGACY_IDENTITIES) else {
        return Ok(BTreeMap::new());
    };
    let failed = |error: String| (LEGACY_IDENTITIES.to_string(), error);
    let text = record
        .text
        .as_ref()
        .map_err(|error| failed(error.clone()))?;
    parse_legacy_identities(text)
        .map_err(|error| failed(format!("it isn't a JSON object of names: {error}")))
}

/// The record the project names `path`, read from `file`.
fn read_record_at(file: &Path, path: &str) -> Record {
    Record {
        path: path.to_string(),
        text: std::fs::read_to_string(file).map_err(|error| error.to_string()),
    }
}

/// How rarely a file changes (docs/compiler-runtime.md §2.4): domain files
/// low, `shared/` and the root files medium, so a keystroke in a domain
/// never revalidates what only the others decide.
fn durability(path: &str, layout: Layout) -> Durability {
    match locate(path, layout).ownership {
        Ownership::Domain(_) | Ownership::Loose => Durability::LOW,
        Ownership::Shared | Ownership::Root => Durability::MEDIUM,
        // An installed package changes only when it is added or updated.
        Ownership::Package(home) if home.installed => Durability::HIGH,
        Ownership::Package(_) => Durability::MEDIUM,
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

    /// The installed package files the package cache holds as checked
    /// with no problem, which a check skips (chunk 7.1c2).
    pub fn set_checked(&mut self, checked: BTreeSet<String>) {
        if &checked != self.project.checked(&self.db) {
            self.project.set_checked(&mut self.db).to(checked);
        }
    }

    /// Compiles each lazily loaded resource the app now calls into
    /// ([`crate::lazy`]).
    fn settle(&mut self) {
        if let Some((files, waiting)) = crate::lazy::settle(&self.db, self.project) {
            self.project.set_files(&mut self.db).to(files);
            self.project.set_lazy(&mut self.db).to(waiting);
        }
    }

    /// Sets the text of the source file at `path`, adding it when it is
    /// new. Only `.jadpo` files outside `build/` are source; anything else
    /// is ignored. The database keeps every other result it can.
    pub fn set_text(&mut self, path: &str, text: String) {
        self.write_text(path, text);
        self.settle();
    }

    fn write_text(&mut self, path: &str, text: String) {
        if !is_source(path) {
            return;
        }
        let waiting = self
            .project
            .lazy(&self.db)
            .iter()
            .copied()
            .find(|file| file.path(&self.db) == path);
        if let Some(file) = self.file(path).or(waiting) {
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
        if layout == Layout::Project && crate::lazy::is_lazy(path) {
            let mut lazy = self.project.lazy(&self.db).clone();
            lazy.push(file);
            lazy.sort_by(|a, b| a.path(&self.db).cmp(b.path(&self.db)));
            self.project.set_lazy(&mut self.db).to(lazy);
            return;
        }
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
        let lazy = self.project.lazy(&self.db).clone();
        let waiting: Vec<File> = lazy
            .iter()
            .copied()
            .filter(|file| file.path(&self.db) != path)
            .collect();
        if waiting.len() != lazy.len() {
            self.project.set_lazy(&mut self.db).to(waiting);
        }
    }

    /// Reads `path` from disk again: its text now, or its removal when it
    /// is gone or unreadable.
    pub fn reload(&mut self, path: &str) {
        match std::fs::read_to_string(self.on_disk(path)) {
            Ok(text) => self.set_text(path, text),
            Err(_) => self.remove(path),
        }
    }

    /// Where the project-relative `path` is on disk: in a linked package's
    /// checkout for a path in its folder, and under the root otherwise.
    pub fn on_disk(&self, path: &str) -> PathBuf {
        for (folder, checkout) in &self.links {
            if let Some(rest) = path
                .strip_prefix(folder.as_str())
                .and_then(|rest| rest.strip_prefix('/'))
            {
                return self.root.join(checkout).join(rest);
            }
        }
        self.root.join(path)
    }

    /// Reads the lock's links again, after a change to it: a package
    /// linked, unlinked or linked elsewhere has every file of its folder
    /// read again from where it now lives.
    fn relink(&mut self) {
        let now = links(&self.root);
        if now == self.links {
            return;
        }
        let changed: BTreeSet<String> = self
            .links
            .iter()
            .chain(now.iter())
            .filter(|link| !(self.links.contains(link) && now.contains(link)))
            .map(|(folder, _)| folder.clone())
            .collect();
        // What each changed folder holds now, and held before.
        let mut paths: BTreeSet<String> = BTreeSet::new();
        let db = &self.db;
        for folder in &changed {
            let prefix = format!("{folder}/");
            let under = |path: &String| path.starts_with(&prefix);
            paths.extend(
                self.project
                    .files(db)
                    .iter()
                    .map(|file| file.path(db).clone())
                    .filter(under),
            );
            paths.extend(
                self.project
                    .records(db)
                    .iter()
                    .map(|record| record.path.clone())
                    .filter(under),
            );
            paths.extend(
                self.project
                    .other_files(db)
                    .iter()
                    .filter(|&path| under(path))
                    .cloned(),
            );
        }
        // A checkout that couldn't be read is read again too.
        let gone: Vec<String> = self
            .links
            .iter()
            .filter(|(folder, _)| changed.contains(folder))
            .map(|(_, checkout)| format!("{checkout}/"))
            .collect();
        self.unreadable.retain(|file| !gone.contains(&file.path));
        self.links = now;
        for line in link_lines(&self.root) {
            if changed.contains(&line.folder())
                && let Some(missing) = missing_checkout(&self.root, &line)
            {
                self.unreadable.push(missing);
            }
        }
        for folder in &changed {
            let disk = self.on_disk(&format!("{folder}/"));
            for file in package_files(&disk) {
                paths.insert(format!("{folder}/{}", display(&disk, &file)));
            }
        }
        for path in paths {
            self.refresh(&path);
        }
        self.unreadable.sort_by(|a, b| a.path.cmp(&b.path));
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
        let full = self.on_disk(path);
        if self.project.layout(&self.db) == Layout::Project && is_record(path) {
            let mut records = self.project.records(&self.db).clone();
            records.retain(|record| record.path != path);
            if full.is_file() {
                records.push(read_record_at(&full, path));
                records.sort_by(|a, b| a.path.cmp(&b.path));
            }
            if path == IDENTITIES || path == LEGACY_IDENTITIES {
                self.unreadable
                    .retain(|file| file.path != IDENTITIES && file.path != LEGACY_IDENTITIES);
                match identities(&records) {
                    Ok(lines) => {
                        if &lines != self.project.identities(&self.db) {
                            self.project.set_identities(&mut self.db).to(lines);
                        }
                    }
                    Err((path, error)) => self.unreadable.push(Unreadable { path, error }),
                }
            }
            if &records != self.project.records(&self.db) {
                self.project.set_records(&mut self.db).to(records);
            }
            if path == crate::LOCK {
                self.relink();
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
        && (path.starts_with("domains/") || path.starts_with("shared/") || is_record(path))
}

/// Whether a project-relative path is a source file the project holds.
fn is_source(path: &str) -> bool {
    path.ends_with(".jadpo")
        && !path.starts_with("build/")
        && !path.split('/').any(|part| part.starts_with('.'))
        && !crate::is_package_data(path)
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

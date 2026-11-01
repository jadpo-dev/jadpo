//! Finding and reading a project on disk: the only I/O in the compiler
//! crates. What the files make a project is decided in [`crate::files`],
//! which reads none.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use salsa::{Durability, Setter};

use crate::files::{
    FileSet, Role, assemble, identities, link_lines, links_in, missing_checkout, role,
};
use crate::places::{Ownership, locate};
use crate::{Database, File, IDENTITIES, LEGACY_IDENTITIES, Layout, Project, Record};

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
    let (root, files) = read(target);
    load_files_into(root, &files, db, lazily)
}

/// Reads what `target` names from disk, as [`load`] finds it: the folder
/// its paths are relative to, and its files. Inside a project that is
/// every file under the root a project can hold, and every file of each
/// checkout `jadpo.lock` links a package to, under the checkout's path as
/// the lock writes it; otherwise the file, or the files under the folder.
pub fn read(target: &Path) -> (PathBuf, FileSet) {
    let folder = if target.is_dir() {
        target.to_path_buf()
    } else {
        target
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    };
    let mut files = FileSet::default();
    let root = match find_root(&folder) {
        Some(root) => {
            let found = walk(&root, Layout::Project, &mut files);
            read_all(&mut files, &root, found);
            let lock = match files.files.get(crate::LOCK) {
                Some(Ok(text)) => text.clone(),
                _ => String::new(),
            };
            for (_, checkout) in links_in(&lock) {
                let at = root.join(&checkout);
                for file in package_files(&at) {
                    let shown = format!("{checkout}/{}", display(&at, &file));
                    files.insert(shown, read_file(&file, &at));
                }
            }
            root
        }
        None if target.is_dir() => {
            let found = walk(&folder, Layout::Loose, &mut files);
            read_all(&mut files, &folder, found);
            folder
        }
        None => {
            read_all(&mut files, &folder, vec![target.to_path_buf()]);
            folder
        }
    };
    (root, files)
}

/// Adds each of `paths` to `files`, by its path relative to `root`.
fn read_all(files: &mut FileSet, root: &Path, paths: Vec<PathBuf>) {
    for path in paths {
        files.insert(display(root, &path), read_file(&path, root));
    }
}

/// A file's text, or why it couldn't be read. A pipe or a device would
/// block or never end, so only a regular file is read; and a link only to
/// a file inside `within`, the folder being read, so a link a package or
/// an example holds can't make a check read a file from elsewhere on the
/// machine into its output or a build (security audit S1).
fn read_file(path: &Path, within: &Path) -> Result<String, String> {
    if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        let inside = match (std::fs::canonicalize(path), std::fs::canonicalize(within)) {
            (Ok(target), Ok(within)) => target.starts_with(within),
            _ => false,
        };
        if !inside {
            return Err(
                "it links to a file outside the project, which Jadpo doesn't read".to_string(),
            );
        }
    }
    if !std::fs::metadata(path).is_ok_and(|meta| meta.is_file()) {
        return Err("it isn't a regular file".to_string());
    }
    std::fs::read_to_string(path).map_err(|error| error.to_string())
}

/// Loads a set of files read from anywhere, a git commit as well as the
/// disk, by the same rules [`load`] uses ([`assemble`]). `root` is the
/// folder its paths are relative to; it names the application, and is
/// where [`Loaded::refresh`] and [`Loaded::reload`] read again.
pub fn load_files(root: PathBuf, files: &FileSet) -> Loaded {
    load_files_into(root, files, Database::default(), true)
}

fn load_files_into(root: PathBuf, files: &FileSet, db: Database, lazily: bool) -> Loaded {
    let assembled = assemble(files, lazily);
    let layout = assembled.layout;
    let input = |(path, text): (String, String)| {
        let durability = durability(&path, layout);
        File::builder(path, text)
            .path_durability(durability)
            .text_durability(durability)
            .new(&db)
    };
    let sources: Vec<File> = assembled.sources.into_iter().map(input).collect();
    let lazy: Vec<File> = assembled.lazy.into_iter().map(input).collect();
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let project = Project::builder(
        sources,
        assembled.other_files,
        layout,
        assembled.identities,
        assembled.records,
    )
    .name(name)
    .lazy(lazy)
    .folder(
        std::fs::canonicalize(&root)
            .unwrap_or_else(|_| root.clone())
            .to_string_lossy()
            .into_owned(),
    )
    .new(&db);
    let mut loaded = Loaded {
        db,
        project,
        root,
        unreadable: assembled.unreadable,
        links: assembled.links,
    };
    loaded.settle();
    loaded
}

/// Each package `root`'s `jadpo.lock` links to a live checkout: its folder
/// in the project, `packages/acme_pricing/v2`, and the checkout as the
/// lock writes it, `../acme-pricing`.
pub fn links(root: &Path) -> Vec<(String, String)> {
    links_in(&std::fs::read_to_string(root.join(crate::LOCK)).unwrap_or_default())
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
            if role(&shown, Layout::Project).is_some() {
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
    /// Sets the before side a change is compared with (chunk A8).
    pub fn set_before(&mut self, before: crate::Before) {
        if &before != self.project.before(&self.db) {
            self.project.set_before(&mut self.db).to(before);
        }
    }

    /// Sets the date rules triggered by time passing are judged at, in
    /// days since 1970-01-01 (area 1; chunk B9).
    pub fn set_judged_on(&mut self, day: Option<i32>) {
        if day != self.project.judged_on(&self.db) {
            self.project.set_judged_on(&mut self.db).to(day);
        }
    }

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
        if role(path, self.project.layout(&self.db)) != Some(Role::Source) {
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

    /// The folder `path` is read from: its linked package's checkout, or
    /// the root.
    fn within(&self, path: &str) -> PathBuf {
        for (folder, checkout) in &self.links {
            if path
                .strip_prefix(folder.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
            {
                return self.root.join(checkout);
            }
        }
        self.root.clone()
    }

    /// Reads the lock's links again, after a change to it: a package
    /// linked, unlinked or linked elsewhere has every file of its folder
    /// read again from where it now lives.
    fn relink(&mut self) {
        let now = links_in(&self.lock());
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
        for line in link_lines(&self.lock()) {
            let crate::Source::Link { path } = &line.source else {
                continue;
            };
            let index = self.root.join(path).join(crate::PACKAGE_INDEX);
            if changed.contains(&line.folder()) && !index.is_file() {
                self.unreadable.push(missing_checkout(&line, path));
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
        role(path, self.project.layout(&self.db)).is_some()
    }

    /// The text of the project's `jadpo.lock`, empty when it has none or
    /// it can't be read.
    fn lock(&self) -> String {
        self.project
            .records(&self.db)
            .iter()
            .find(|record| record.path == crate::LOCK)
            .and_then(|record| record.text.clone().ok())
            .unwrap_or_default()
    }

    /// Brings `path` up to date with the disk after a change there, as a
    /// watcher sees it: a source file's text, added or removed, or another
    /// file's presence. A source file that exists but can't be read joins
    /// [`Loaded::unreadable`] until it can; one that is gone leaves.
    pub fn refresh(&mut self, path: &str) {
        let Some(role) = role(path, self.project.layout(&self.db)) else {
            return;
        };
        let full = self.on_disk(path);
        let now = match role {
            // Another file is held by its presence alone.
            Role::Other => full.is_file().then(|| Ok(String::new())),
            Role::Source | Role::Record => match std::fs::metadata(&full) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                _ => match read_file(&full, &self.within(path)) {
                    Err(_) if !full.exists() => None,
                    read => Some(read),
                },
            },
        };
        self.apply(path, role, now);
        if path == crate::LOCK {
            self.relink();
        }
    }

    /// Sets what the project holds at `path` to `now`, its text, or why it
    /// can't be read, or `None` when it is gone, by `path`'s `role`.
    fn apply(&mut self, path: &str, role: Role, now: Option<Result<String, String>>) {
        self.unreadable.retain(|file| file.path != path);
        match role {
            Role::Record => {
                let mut records = self.project.records(&self.db).clone();
                records.retain(|record| record.path != path);
                if let Some(text) = now {
                    records.push(Record {
                        path: path.to_string(),
                        text,
                    });
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
                    self.unreadable.sort_by(|a, b| a.path.cmp(&b.path));
                }
                if &records != self.project.records(&self.db) {
                    self.project.set_records(&mut self.db).to(records);
                }
            }
            Role::Other => {
                let mut others = self.project.other_files(&self.db).clone();
                let present = now.is_some();
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
            }
            Role::Source => match now {
                Some(Ok(text)) => self.set_text(path, text),
                None => self.remove(path),
                Some(Err(error)) => {
                    self.remove(path);
                    self.unreadable.push(Unreadable {
                        path: path.to_string(),
                        error,
                    });
                    self.unreadable.sort_by(|a, b| a.path.cmp(&b.path));
                }
            },
        }
    }
}

/// Every file a project or a folder of loose files holds, relative to
/// `folder` with `/`, in path order: what [`load`] reads and a watcher
/// watches. Folders that can't be listed are left out.
pub fn files_under(folder: &Path, project: bool) -> Vec<String> {
    let layout = if project {
        Layout::Project
    } else {
        Layout::Loose
    };
    walk(folder, layout, &mut FileSet::default())
        .iter()
        .map(|path| display(folder, path))
        .collect()
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

/// Every file under `folder` a project of `layout` holds ([`role`]), in
/// path order. A folder that can't be listed is added to `files` as its
/// path with a trailing `/` and the error: it may hold source, and checking
/// without it would pass a project with code missing.
fn walk(folder: &Path, layout: Layout, files: &mut FileSet) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) => {
                files.insert(
                    format!("{}/", display(folder, &dir)),
                    Err(error.to_string()),
                );
                continue;
            }
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let shown = display(folder, &path);
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => {
                    if role(&format!("{shown}/x.jadpo"), layout).is_some() {
                        pending.push(path);
                    }
                }
                Ok(_) if role(&shown, layout).is_some() => found.push(path),
                _ => {}
            }
        }
    }
    found.sort_by_key(|path| display(folder, path));
    found
}

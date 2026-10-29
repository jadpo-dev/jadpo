//! A project as a set of files in memory, and the rules that make it a
//! project (docs/compiler-runtime.md §2.2, "Loading"): which files a
//! project holds, which are source and which records, which installed
//! package resources wait until the app calls into them, the identity
//! registry's lines, and which package folders a link reads from a live
//! checkout. Nothing here reads the disk, so a set read from disk
//! ([`crate::load`]) and one read from a git commit load by the same rules.

use std::collections::BTreeMap;

use crate::{IDENTITIES, LEGACY_IDENTITIES, Layout, Record, Unreadable};
use crate::{is_record, parse_identities, parse_legacy_identities};

/// A project's files as they were read: each by its path relative to the
/// folder they were read from, with `/`, holding its text or why it
/// couldn't be read. A folder that couldn't be listed is its path with a
/// trailing `/` and the error. A file of a package `jadpo.lock` links to a
/// live checkout is under the checkout's path as the lock writes it
/// (`../acme-pricing/package.jadpo`). The set may hold files a project
/// doesn't (hidden ones, `build/`, other files at its root): the rules
/// leave them out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileSet {
    pub files: BTreeMap<String, Result<String, String>>,
}

impl FileSet {
    /// A set of files that were all read.
    pub fn of<P: Into<String>, T: Into<String>>(files: impl IntoIterator<Item = (P, T)>) -> Self {
        FileSet {
            files: files
                .into_iter()
                .map(|(path, text)| (path.into(), Ok(text.into())))
                .collect(),
        }
    }

    /// Adds `path`'s text, or why it couldn't be read.
    pub fn insert(&mut self, path: impl Into<String>, text: Result<String, String>) {
        self.files.insert(path.into(), text);
    }
}

/// What a file is to a project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// A `.jadpo` file the compiler reads.
    Source,
    /// A record beside the source ([`Record`]).
    Record,
    /// Another file in a code folder, held by its path alone (an asset).
    Other,
}

/// What the file at the project-relative `path` is in a project of
/// `layout`, or `None` when the project doesn't hold it: anything hidden,
/// anything in the generated `build/`, and outside a project's code
/// folders and records anything that isn't source. Loose files keep a
/// prompt's text beside its type (area 21).
pub fn role(path: &str, layout: Layout) -> Option<Role> {
    let parts: Vec<&str> = path.split('/').collect();
    let inside = !path.is_empty()
        && !path.starts_with('/')
        && parts
            .iter()
            .all(|part| !part.is_empty() && !part.starts_with('.'));
    if !inside || parts[0] == "build" {
        return None;
    }
    match layout {
        Layout::Project if is_record(path) => Some(Role::Record),
        Layout::Loose if path.ends_with(crate::PROMPT_EXTENSION) => Some(Role::Record),
        _ if path.ends_with(".jadpo") => Some(Role::Source),
        Layout::Project if parts[0] == "domains" || parts[0] == "shared" => Some(Role::Other),
        _ => None,
    }
}

/// The layout of a set of files: a project when `app.jadpo` is at its root
/// (area 14), loose files otherwise.
pub fn layout_of(files: &FileSet) -> Layout {
    if files.files.contains_key(crate::PROJECT_FILE) {
        Layout::Project
    } else {
        Layout::Loose
    }
}

/// A set of files made a project by the rules: what [`crate::Loaded`] puts
/// in the database.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assembled {
    pub layout: Layout,
    /// The source files the compiler reads now, in path order.
    pub sources: Vec<(String, String)>,
    /// Installed packages' resource files waiting until the app calls into
    /// them ([`crate::lazy`]), in path order.
    pub lazy: Vec<(String, String)>,
    /// Every other file in a code folder, in path order.
    pub other_files: Vec<String>,
    pub records: Vec<Record>,
    /// The identity registry's lines (area 18).
    pub identities: BTreeMap<String, String>,
    /// What couldn't be read, in path order.
    pub unreadable: Vec<Unreadable>,
    /// Each package the lock links to a live checkout: its folder and the
    /// checkout as the lock writes it.
    pub links: Vec<(String, String)>,
}

/// Makes `files` a project by every rule above; `lazily`, an installed
/// package's resources wait until the app calls into them.
pub fn assemble(files: &FileSet, lazily: bool) -> Assembled {
    let layout = layout_of(files);
    let mut unreadable = Vec::new();
    let mut held: BTreeMap<String, Result<String, String>> = BTreeMap::new();
    for (path, text) in &files.files {
        match (path.strip_suffix('/'), text) {
            (Some(folder), Err(error)) => {
                if folder.is_empty() || role(&format!("{folder}/x.jadpo"), layout).is_some() {
                    unreadable.push(Unreadable {
                        path: if folder.is_empty() {
                            ".".to_string()
                        } else {
                            path.clone()
                        },
                        error: error.clone(),
                    });
                }
            }
            (Some(_), Ok(_)) => {}
            (None, _) => {
                held.insert(path.clone(), text.clone());
            }
        }
    }
    let links = if layout == Layout::Project {
        link(&mut held, &mut unreadable)
    } else {
        Vec::new()
    };
    let mut assembled = Assembled {
        layout,
        sources: Vec::new(),
        lazy: Vec::new(),
        other_files: Vec::new(),
        records: Vec::new(),
        identities: BTreeMap::new(),
        unreadable,
        links,
    };
    for (path, text) in held {
        match role(&path, layout) {
            Some(Role::Record) => assembled.records.push(Record { path, text }),
            Some(Role::Source) => match text {
                Ok(text) if lazily && layout == Layout::Project && crate::lazy::is_lazy(&path) => {
                    assembled.lazy.push((path, text));
                }
                Ok(text) => assembled.sources.push((path, text)),
                Err(error) => assembled.unreadable.push(Unreadable { path, error }),
            },
            Some(Role::Other) => assembled.other_files.push(path),
            None => {}
        }
    }
    // The identity registry (area 18): its lines are an input of their own,
    // and its text a record, for the edits that add a line to it.
    match identities(&assembled.records) {
        Ok(lines) => assembled.identities = lines,
        Err((path, error)) => assembled.unreadable.push(Unreadable { path, error }),
    }
    assembled.unreadable.sort_by(|a, b| a.path.cmp(&b.path));
    assembled
}

/// Reads each package `jadpo.lock` links to from its live checkout
/// instead of its folder in `packages/` (area 11, "Developing and
/// publishing a package"), which stays as installed for `jadpo package
/// unlink`: the checkout's files take the folder's paths, so the package
/// is checked and run as the installed one would be. A checkout holding no
/// index can't be read. Returns each link's folder and checkout.
fn link(
    held: &mut BTreeMap<String, Result<String, String>>,
    unreadable: &mut Vec<Unreadable>,
) -> Vec<(String, String)> {
    let lock = match held.get(crate::LOCK) {
        Some(Ok(text)) => text.clone(),
        _ => String::new(),
    };
    let lines = link_lines(&lock);
    let mut moved: BTreeMap<String, Result<String, String>> = BTreeMap::new();
    let mut links = Vec::new();
    for line in &lines {
        let crate::Source::Link { path } = &line.source else {
            continue;
        };
        let folder = line.folder();
        let prefix = format!("{folder}/");
        held.retain(|shown, _| !shown.starts_with(&prefix));
        links.push((folder.clone(), path.clone()));
        let checkout = format!("{path}/");
        if !held.contains_key(&format!("{checkout}{}", crate::PACKAGE_INDEX)) {
            unreadable.push(missing_checkout(line, path));
            continue;
        }
        for (file, text) in held.iter() {
            let Some(rest) = file.strip_prefix(&checkout) else {
                continue;
            };
            let first = rest.split('/').next().unwrap_or_default();
            if first == "build" || rest.split('/').any(|part| part.starts_with('.')) {
                continue;
            }
            moved.insert(format!("{prefix}{rest}"), text.clone());
        }
    }
    held.extend(moved);
    links
}

/// The lines of a `jadpo.lock`'s text that link a package to a live
/// checkout. A lock that can't be read is the records' to report.
pub(crate) fn link_lines(lock: &str) -> Vec<crate::Locked> {
    let Ok(lines) = crate::parse_lock(lock) else {
        return Vec::new();
    };
    lines
        .into_iter()
        .filter(|line| matches!(line.source, crate::Source::Link { .. }))
        .collect()
}

/// Each package a `jadpo.lock`'s text links to a live checkout: its folder
/// in the project, `packages/acme_pricing/v2`, and the checkout as the lock
/// writes it, `../acme-pricing`.
pub fn links_in(lock: &str) -> Vec<(String, String)> {
    link_lines(lock)
        .iter()
        .filter_map(|line| match &line.source {
            crate::Source::Link { path } => Some((line.folder(), path.clone())),
            _ => None,
        })
        .collect()
}

/// Why the checkout at `path` that `line` links to can't be read: it holds
/// no index.
pub(crate) fn missing_checkout(line: &crate::Locked, path: &str) -> Unreadable {
    Unreadable {
        path: format!("{path}/"),
        error: format!(
            "`{}` links `{}` to it, and it holds no `{}`",
            crate::LOCK,
            line.package,
            crate::PACKAGE_INDEX
        ),
    }
}

/// The identity registry's lines, from its record when the project has
/// one, else from `database/identities.json`, where it was before chunk
/// 7.0, until `jadpo records update` rewrites it; or the file that can't
/// be read and why.
pub(crate) fn identities(records: &[Record]) -> Result<BTreeMap<String, String>, (String, String)> {
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

//! Lazy loading of installed packages' resources (area 11, "The package
//! format"; docs/compiler-runtime.md §2.3; chunk 7.1c2). A package's index
//! lists no files: a file is named after what it declares, so the compiler
//! finds a resource by its name and compiles only the ones the app calls
//! into. A large provider costs only what is called.
//!
//! An installed package's `resources/<resource>.jadpo` is held in
//! [`Project::lazy`](crate::Project) until code outside every package
//! writes `<connection>.<resource>` for a connection to that package; then
//! [`crate::Loaded`] moves it to the files the compiler reads. The bytes
//! of every file still count towards the package's digest. The app's own
//! integrations in `shared/providers/` are the app's code, so they are
//! always compiled whole.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_syntax::SyntaxKind;

use crate::{Db, File, ItemKind, Project, item_tree, location, package_at, package_folder, parse};

/// Whether `path` is an installed package's resource file, which is
/// compiled only once the app calls into it.
pub fn is_lazy(path: &str) -> bool {
    let Some((home, rest)) = package_at(path) else {
        return false;
    };
    home.installed
        && matches!(rest.as_slice(), ["resources", file]
            if file.ends_with(".jadpo") && !file.ends_with(".test.jadpo"))
}

/// The resource files, by path, that the app's code calls into: each
/// `<connection>.<resource>` it writes, for a connection whose `package:`
/// is installed, names `resources/<resource>.jadpo` in that package's
/// folder. A file of that name that doesn't exist names nothing.
pub fn reached(db: &dyn Db, project: Project) -> BTreeSet<String> {
    let files = project.files(db);
    let mut connected: BTreeMap<String, String> = BTreeMap::new();
    for &file in files {
        if location(db, project, file).ownership.package().is_some() {
            continue;
        }
        for item in &item_tree(db, file).items {
            if item.kind != ItemKind::Singleton {
                continue;
            }
            for (connection, package) in &item.connected {
                connected.insert(connection.clone(), package_folder(package));
            }
        }
    }
    let mut out = BTreeSet::new();
    if connected.is_empty() {
        return out;
    }
    for &file in files {
        if location(db, project, file).ownership.package().is_some() {
            continue;
        }
        let tokens: Vec<_> = parse(db, file)
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| !token.kind().is_trivia())
            .collect();
        for window in tokens.windows(3) {
            let [connection, dot, resource] = window else {
                continue;
            };
            if dot.kind() != SyntaxKind::DOT {
                continue;
            }
            if let Some(folder) = connected.get(connection.text()) {
                out.insert(format!("{folder}/resources/{}.jadpo", resource.text()));
            }
        }
    }
    // A connected package's capability mappings call its own operations,
    // `call: <resource>.<operation>`, so those resources are reached too
    // (chunk 7.7c).
    let folders: BTreeSet<&String> = connected.values().collect();
    for &file in files {
        let path = file.path(db);
        let Some(folder) = folders
            .iter()
            .find(|folder| path.starts_with(&format!("{folder}/provides/")))
        else {
            continue;
        };
        let tokens: Vec<_> = parse(db, file)
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| !token.kind().is_trivia())
            .collect();
        for window in tokens.windows(4) {
            let [key, colon, resource, dot] = window else {
                continue;
            };
            if key.text() == "call"
                && colon.kind() == SyntaxKind::COLON
                && dot.kind() == SyntaxKind::DOT
            {
                out.insert(format!("{folder}/resources/{}.jadpo", resource.text()));
            }
        }
    }
    out
}

/// The project's lazy files the app reaches now, moved to the files the
/// compiler reads, or `None` when there are none. A resource once reached
/// stays compiled for the life of the database: one the app stops calling
/// costs only its own check.
pub(crate) fn settle(db: &dyn Db, project: Project) -> Option<(Vec<File>, Vec<File>)> {
    let lazy = project.lazy(db);
    if lazy.is_empty() {
        return None;
    }
    let reached = reached(db, project);
    let (now, waiting): (Vec<File>, Vec<File>) = lazy
        .iter()
        .copied()
        .partition(|file| reached.contains(file.path(db)));
    if now.is_empty() {
        return None;
    }
    let mut compiled = project.files(db).clone();
    compiled.extend(now);
    compiled.sort_by(|a, b| a.path(db).cmp(b.path(db)));
    Some((compiled, waiting))
}

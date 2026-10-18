//! The incremental database (docs/compiler-runtime.md §2.4): the project's
//! source files and layout as salsa inputs, and the queries every check
//! builds on.
//!
//! [`load`] finds a project by walking up to `app.jadpo` and reads its files;
//! this crate is the only compiler crate that does I/O. [`parse`] gives a
//! file's lossless tree, [`item_tree`] its declarations and signatures
//! without bodies, and [`symbols`] the project-wide name table built from
//! the item trees alone, so editing a body never invalidates another file's
//! resolution.

mod identities;
mod items;
mod load;
mod places;
mod table;
mod texts;

pub use identities::{IDENTITIES, parse_identities, write_identities};
pub use items::{Base, Item, ItemKind, ItemTree, Param, Signature, item_tree};
pub use items::{items_of, primary};
pub use load::{
    Loaded, PROJECT_FILE, Unreadable, files_under, find_root, load, load_into, shown_root,
};
pub use places::{
    Folder, Location, Ownership, Place, RootFile, Stray, locate, name_words, to_snake,
};
pub use table::{Contested, Symbol, Symbols, contested, domain_scope, identity_name, symbols};
pub use texts::source_texts;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use jadpo_syntax::Parse;

/// Whether the project's files follow the fixed layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layout {
    /// A project with `app.jadpo` at its root: every layout rule applies.
    Project,
    /// Files handed to the compiler on their own, without `app.jadpo` around
    /// them: one implicit domain, with the layout rules off (area 1).
    Loose,
}

/// One `.jadpo` file, by its project-relative path with `/` separators.
#[salsa::input(debug)]
pub struct File {
    #[returns(ref)]
    pub path: String,
    #[returns(ref)]
    pub text: String,
}

/// A committed record beside the source that isn't a `.jadpo` file (area
/// 18): a migration's `snapshot.json` and `postgres.sql`, an API's record,
/// `api/<api>.api` (area 7), the identity registry,
/// `database/identities.json`, and a prompt's text, `<name>.prompt`
/// (area 21), read whole.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Record {
    /// Project-relative, with `/`.
    pub path: String,
    /// Its text, or why it couldn't be read.
    pub text: Result<String, String>,
}

/// Whether a project-relative path is a record [`Record`] holds: a file in
/// a migration's folder that isn't source, an API's record, the identity
/// registry, a `translations/` folder's sources (area 18), or a prompt's
/// text in a `prompts/` folder (area 21).
pub fn is_record(path: &str) -> bool {
    if path == IDENTITIES {
        return true;
    }
    if is_prompt_text(path) {
        return true;
    }
    if let ["api", name] = path.split('/').collect::<Vec<_>>().as_slice() {
        return name.ends_with(".api");
    }
    matches!(
        path.split('/').collect::<Vec<_>>().as_slice(),
        [
            "database",
            "migrations",
            _,
            "snapshot.json" | "postgres.sql"
        ] | ["domains", _, "translations", "sources.jadpo_record"]
            | ["shared", "translations", "sources.jadpo_record"]
            | ["translations", "sources.jadpo_record"]
    )
}

/// The extension of a prompt's text (area 21).
pub const PROMPT_EXTENSION: &str = ".prompt";

/// Whether a project-relative path is a prompt's text, `<name>.prompt` in
/// a domain's `prompts/` folder or `shared/prompts/` (area 21).
pub fn is_prompt_text(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    let file = match parts.as_slice() {
        ["domains", _, "prompts", file] | ["shared", "prompts", file] => file,
        _ => return false,
    };
    file.len() > PROMPT_EXTENSION.len() && file.ends_with(PROMPT_EXTENSION)
}

/// The project: its source files in path order, the paths of every other
/// file inside it (templates and other assets), its layout, the identity
/// registry's lines (area 18): for each thing renamed since it was born,
/// `kind name-now` and its born name, as `database/identities.json` holds
/// them, and its migrations' records in path order. The registry and the
/// records are inputs like the files, so a rename re-keys exactly what it
/// touches and a new migration rechecks only what reads the migrations.
#[salsa::input(debug)]
pub struct Project {
    #[returns(ref)]
    pub files: Vec<File>,
    #[returns(ref)]
    pub other_files: Vec<String>,
    #[returns(copy)]
    pub layout: Layout,
    #[returns(ref)]
    pub identities: BTreeMap<String, String>,
    #[returns(ref)]
    pub records: Vec<Record>,
}

/// The database every compiler query runs against.
#[salsa::db]
pub trait Db: salsa::Database {}

#[salsa::db]
impl Db for Database {}

/// The one database implementation. `jadpo check` builds one per run; the
/// language server and watch mode keep one for their lifetime.
#[salsa::db]
#[derive(Clone, Default)]
pub struct Database {
    storage: salsa::Storage<Self>,
    executions: Option<Arc<Mutex<Vec<String>>>>,
}

#[salsa::db]
impl salsa::Database for Database {}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

impl Database {
    /// A database that records every query it executes, for tests of what
    /// an edit recomputes.
    pub fn recording() -> Database {
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&log);
        let storage = salsa::Storage::new(Some(Box::new(move |event: salsa::Event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind
                && let Ok(mut log) = sink.lock()
            {
                log.push(format!("{database_key:?}"));
            }
        })));
        Database {
            storage,
            executions: Some(log),
        }
    }

    /// The queries executed since the last call, as `name(Id(..))`.
    pub fn take_executions(&self) -> Vec<String> {
        self.executions
            .as_ref()
            .and_then(|log| log.lock().ok().map(|mut log| std::mem::take(&mut *log)))
            .unwrap_or_default()
    }
}

/// A file's lossless syntax tree and syntax errors.
#[salsa::tracked(returns(ref))]
pub fn parse(db: &dyn Db, file: File) -> Parse {
    jadpo_syntax::parse_file(file.text(db))
}

/// Where a file sits in the project's layout.
#[salsa::tracked(returns(ref))]
pub fn location(db: &dyn Db, project: Project, file: File) -> Location {
    locate(file.path(db), project.layout(db))
}

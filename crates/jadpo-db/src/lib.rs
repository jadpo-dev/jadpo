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

mod items;
mod load;
mod places;
mod table;

pub use items::{Base, Item, ItemKind, ItemTree, Param, Signature, item_tree};
pub use load::{Loaded, PROJECT_FILE, Unreadable, find_root, load, load_into};
pub use places::{Folder, Location, Ownership, Place, RootFile, Stray, locate};
pub use table::{Symbol, Symbols, symbols};

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

/// The project: its source files in path order, the paths of every other
/// file inside it (templates and other assets), its layout, and the
/// identity registry's lines (area 18): for each thing renamed since it was
/// born, `kind name-now` and its born name, as `database/identities.json`
/// holds them. The registry is an input like the files, so a rename it
/// records re-keys exactly what it touches.
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

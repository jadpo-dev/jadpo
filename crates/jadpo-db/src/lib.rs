//! The incremental database (docs/compiler-runtime.md §2.4): the project's
//! source files and layout as salsa inputs, and the queries every check
//! builds on.
//!
//! [`load`] finds a project by walking up to `app.jadpo` and reads its files
//! ([`read`]), the only I/O in the compiler crates; [`assemble`] makes a
//! set of files a project by every layout rule without reading anything,
//! so [`load_files`] loads one read from a git commit the same way. [`parse`] gives a
//! file's lossless tree, [`item_tree`] its declarations and signatures
//! without bodies, and [`symbols`] the project-wide name table built from
//! the item trees alone, so editing a body never invalidates another file's
//! resolution.

mod files;
mod identities;
mod items;
pub mod lazy;
mod load;
mod lock;
mod places;
pub mod record;
mod table;
mod texts;

pub use files::{Assembled, FileSet, Role, assemble, layout_of, links_in, role};
pub use identities::{
    IDENTITIES, LEGACY_IDENTITIES, parse_identities, parse_legacy_identities, write_identities,
};
pub use items::{Base, Item, ItemKind, ItemTree, Param, Signature, item_tree};
pub use items::{items_of, primary, seed_name};
pub use load::{
    Loaded, PROJECT_FILE, Unreadable, files_under, find_root, links, load, load_files, load_into,
    load_whole, package_files, read, show_in_checkouts, shown_root, watched_files,
};
pub use lock::{
    LOCK, Locked, Source, locked_jadpo, package_digest, package_folder, parse_lock, write_lock,
    write_pinned_lock,
};
pub use places::{
    Folder, Location, Ownership, PACKAGE_INDEX, PACKAGES, PROVIDER_FILE, PROVIDERS, PackageHome,
    Place, RootFile, Stray, is_package_data, is_version_label, locate, name_words, package_at,
    to_snake, to_upper_camel,
};
pub use table::{Contested, Symbol, Symbols, contested, domain_scope, identity_name, symbols};
pub use texts::source_texts;

use std::collections::{BTreeMap, BTreeSet};
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
/// 18): a migration's `snapshot.json` and `postgres.sql`, every
/// `.jadpo_record` ([`record::kind_of`]) and `jadpo.lock`, a record still
/// under its name from before chunk 7.0 (`api/<api>.api`,
/// `database/identities.json`), a prompt's text, `<name>.prompt`, and
/// its recordings, `recordings/<prompt>/<name>.json` (area 21), an
/// email's templates, `<name>.html` and `<name>.txt` (area
/// 16), and a package's files that aren't its source
/// ([`is_package_data`]), read whole for its digest.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Record {
    /// Project-relative, with `/`.
    pub path: String,
    /// Its text, or why it couldn't be read.
    pub text: Result<String, String>,
}

/// Whether a project-relative path is a record [`Record`] holds: a file in
/// a migration's folder that isn't source, a generated record of any kind,
/// one under its old name, a prompt's text in a `prompts/` folder (area
/// 21), or an email's template in an `emails/` folder (area 16).
pub fn is_record(path: &str) -> bool {
    if record::kind_of(path).is_some() || record::renamed(path).is_some() {
        return true;
    }
    if places::is_package_data(path) {
        return true;
    }
    if is_prompt_text(path) || is_prompt_recording(path) || is_email_template(path) {
        return true;
    }
    matches!(
        path.split('/').collect::<Vec<_>>().as_slice(),
        [
            "database",
            "migrations",
            _,
            "snapshot.json" | "postgres.sql"
        ]
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

/// Whether a project-relative path is a recording of a prompt's answer,
/// `recordings/<prompt>/<name>.json` in a domain's `prompts/` folder or
/// `shared/prompts/` (area 21, "Tests"; chunk 5.5v4).
pub fn is_prompt_recording(path: &str) -> bool {
    prompt_recording(path).is_some()
}

/// The prompts folder, the prompt's name and the recording's name of a
/// recording of a prompt's answer at `path` (chunk 5.5v4):
/// `("domains/support/prompts", "answer_question", "sesame")`.
pub fn prompt_recording(path: &str) -> Option<(String, &str, &str)> {
    let parts: Vec<&str> = path.split('/').collect();
    let (folder, prompt, file) = match parts.as_slice() {
        ["domains", domain, "prompts", "recordings", prompt, file] => {
            (format!("domains/{domain}/prompts"), *prompt, *file)
        }
        ["shared", "prompts", "recordings", prompt, file] => {
            ("shared/prompts".to_string(), *prompt, *file)
        }
        _ => return None,
    };
    let name = file.strip_suffix(".json").filter(|name| !name.is_empty())?;
    (!prompt.is_empty()).then_some((folder, prompt, name))
}

/// The extensions of an email's templates (area 16, "Email"): its HTML
/// body and its plain-text body.
pub const EMAIL_TEMPLATE_EXTENSIONS: [&str; 2] = [".html", ".txt"];

/// Whether a project-relative path is an email's template (area 16):
/// `<name>.html` or `<name>.txt` in a domain's `emails/` folder, or the
/// shared layout's in `shared/emails/`.
pub fn is_email_template(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    let file = match parts.as_slice() {
        ["domains", _, "emails", file] | ["shared", "emails", file] => file,
        _ => return false,
    };
    EMAIL_TEMPLATE_EXTENSIONS
        .iter()
        .any(|extension| file.len() > extension.len() && file.ends_with(extension))
}

/// The project: its source files in path order, the paths of every other
/// file inside it (assets), its layout, the identity
/// registry's lines (area 18): for each thing renamed since it was born,
/// `kind name-now` and its born name, as `database/identities.jadpo_record`
/// holds them, and its migrations' records in path order. The registry and the
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
    /// The application's name: its folder's, until `app.jadpo` gives one;
    /// empty where nothing named it, as in a test's project. It titles
    /// each API's OpenAPI document (area 7).
    #[default]
    #[returns(ref)]
    pub name: String,
    /// Installed packages' resource files that nothing the app writes
    /// reaches yet (area 11, "The package format"; chunk 7.1c2): read for
    /// the package's digest, but compiled only once the app calls into
    /// them, when [`load::Loaded`] moves them to `files`.
    #[default]
    #[returns(ref)]
    pub lazy: Vec<File>,
    /// The installed packages' source files already checked with no
    /// problem by this compiler at the package's digest, by path: the
    /// package cache (docs/compiler-runtime.md §2.3), whose files
    /// [`check`](../jadpo_check/fn.check.html) doesn't check again.
    #[default]
    #[returns(ref)]
    pub checked: BTreeSet<String>,
    /// The before side a change is compared with (area 18, "Comparing two
    /// builds"): what `jadpo check` reads from git, so acceptances can be
    /// judged (chunk A8). Not read for a program run, the editor or a
    /// test's project, which judge none.
    #[default]
    #[returns(ref)]
    pub before: Before,
    /// The date a rule triggered by time passing is judged at (area 1,
    /// "Rules triggered by time passing"), in days since 1970-01-01: today
    /// for `jadpo check`, the commit's date for `jadpo build`. `None` where
    /// nothing judges one, as for a program run, the editor or a test's
    /// project, so no flag is reported expired there.
    #[default]
    #[returns(copy)]
    pub judged_on: Option<i32>,
    /// The project's folder on this machine, its absolute path with links
    /// resolved: what `jadpo check` holds a disk store's `path:` outside
    /// (area 16, "The disk driver"; chunk B2). Empty where none is known,
    /// as in a test's project or the editor's.
    #[default]
    #[returns(ref)]
    pub folder: String,
}

/// The before side of a change (area 18, "Comparing two builds" and
/// "Acceptances"): the committed files at the base commit that
/// acceptances are judged against, or why there are none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Before {
    /// Not looked for: nothing is compared and no acceptance is judged.
    #[default]
    NotRead,
    /// Looked for and there is none, for the reason given ("not a git
    /// repository"): nothing is compared or judged, and `jadpo check`
    /// says so in one line.
    Nothing(String),
    /// The base commit isn't in this clone, such as a depth-1 checkout in
    /// CI: an error naming `git fetch`.
    Missing { base: String },
    /// The base commit's records and acceptances.
    At(BeforeSide),
}

/// The committed files at the base commit that a change is compared with.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct BeforeSide {
    /// How the base was found, for messages: `origin/main at 1a2b3c4`.
    pub described: String,
    /// The base commit's short hash, for a command that restores a file
    /// from it.
    pub commit: String,
    /// The project's records and acceptance files there, by
    /// project-relative path: `api/`, the event records, `access` and
    /// `network`, `jadpo.lock`, the identity registry and `acceptances/`.
    pub files: BTreeMap<String, String>,
    /// The project's domains there, by folder name.
    pub domains: BTreeSet<String>,
    /// The project's migrations there, by folder name: what a change
    /// doesn't add (chunk V3).
    pub migrations: BTreeSet<String>,
}

/// Whether a project-relative path is a domain's monitoring file,
/// `domains/<domain>/monitoring.jadpo`.
pub fn monitoring_file(path: &str) -> bool {
    path.strip_prefix("domains/")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(domain, file)| !domain.is_empty() && file == "monitoring.jadpo")
}

impl BeforeSide {
    /// The text of `path` on the before side, CRLF read as LF.
    pub fn text(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(String::as_str)
    }
}

/// Whether a project-relative path is one the before side holds (chunk
/// A8): a record, the lock, a file in `acceptances/`, or `app.jadpo`,
/// whose `connections:` block says whether a project with no network
/// record reached any outside host and holds the disk stores' floors; and
/// each domain's `monitoring.jadpo`, whose thresholds a change may loosen
/// (area 16; chunk B13).
pub fn compared_on_the_before_side(path: &str) -> bool {
    path == PROJECT_FILE
        || path.starts_with("acceptances/")
        || monitoring_file(path)
        || record::kind_of(path).is_some_and(|(kind, _)| {
            [
                record::API.word,
                record::EVENTS.word,
                record::ACCESS.word,
                record::NETWORK.word,
                record::LOCK.word,
                record::IDENTITIES.word,
            ]
            .contains(&kind.word)
        })
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

/// Where a file sits in the project's layout. A package held in two
/// versions side by side is written through its name and version
/// (`Postcodes.v3`, area 11, "Where packages live"), which the path alone
/// can't tell.
#[salsa::tracked(returns(ref))]
pub fn location(db: &dyn Db, project: Project, file: File) -> Location {
    let mut location = locate(file.path(db), project.layout(db));
    if let Ownership::Package(home) = &mut location.ownership
        && side_by_side(db, project).contains(&(home.name.clone(), home.installed))
    {
        home.through = home.versioned();
    }
    location
}

/// The packages the project holds in more than one version, each as its
/// folder's name and whether it is installed (area 11, "Where packages
/// live"): two versions of one package sit side by side while callers
/// move across, and their names carry the version. An installed package
/// and the app's own integration of one name are a clash instead
/// (`package.name_taken`), never versions of one package.
#[salsa::tracked(returns(ref))]
pub fn side_by_side(db: &dyn Db, project: Project) -> BTreeSet<(String, bool)> {
    let mut versions: BTreeMap<(String, bool), BTreeSet<String>> = BTreeMap::new();
    for file in project.files(db).iter().chain(project.lazy(db)) {
        if let Some((home, _)) = package_at(file.path(db)) {
            versions
                .entry((home.name, home.installed))
                .or_default()
                .insert(home.version);
        }
    }
    versions
        .into_iter()
        .filter(|(_, held)| held.len() > 1)
        .map(|(package, _)| package)
        .collect()
}

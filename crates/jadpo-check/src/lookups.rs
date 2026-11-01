//! Per-name lookups (docs/compiler-runtime.md §2.4). The project-wide tables
//! (symbols, types, signatures, failures) are rebuilt whenever any
//! declaration changes; a check that read a whole table would run again
//! with it. Each lookup here is a query keyed by one name that copies that
//! name's entry out of its table, so when the table is rebuilt and the
//! entry comes out equal, salsa's early cut-off stops there: a body, a
//! file's resolution or its naming rules depend only on the names they
//! read.
//!
//! [`Lookup`] gives a check those queries behind views ([`TypeView`],
//! [`NameView`], [`SigView`], [`FailureView`]). Suggestions on a mistake
//! ("did you mean") still read a whole table, through [`NameView::all`];
//! only code that already found a mistake does.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use jadpo_db::{Db, File, ItemKind, Ownership, Project, Symbol, Symbols, contested, symbols};
use jadpo_graph::Registry;

use crate::case::to_snake;
use crate::data::WriteRules;
use crate::failures::{FailureInfo, failure_table};
use crate::types::{Def, Sig, TypeView, callables, type_table};

/// A name, interned so it can key a query.
#[salsa::interned(debug)]
pub struct Name<'db> {
    #[returns(ref)]
    pub text: String,
}

/// Everything the symbol table says about one name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct NameFacts {
    /// The declarations called this, in path order.
    pub declarations: Vec<Symbol>,
    pub domain: bool,
    pub connection: bool,
    pub package: bool,
    /// For a package held in two versions side by side, the names the app
    /// writes it through, `Postcodes.v2` and `Postcodes.v3` (chunk
    /// 7.1c2); empty otherwise.
    pub versions: Vec<String>,
}

#[salsa::tracked(returns(ref))]
pub fn name_facts<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> NameFacts {
    let table = symbols(db, project);
    let text = name.text(db);
    NameFacts {
        declarations: table.declarations.get(text).cloned().unwrap_or_default(),
        domain: table.domains.contains(text),
        connection: table.connections.contains(text),
        package: table.packages.contains(text),
        versions: table.versions.get(text).cloned().unwrap_or_default(),
    }
}

/// The declarations called `name` that code in `domain` (empty outside
/// every domain) sees: its own private ones first, then the project-wide
/// ones; when there are none, every declaration of the name, so a use of
/// another domain's private name can be reported as one (area 1, "Files
/// and names").
#[salsa::tracked(returns(ref))]
pub fn in_scope<'db>(
    db: &'db dyn Db,
    project: Project,
    name: Name<'db>,
    domain: Name<'db>,
) -> Vec<Symbol> {
    let all = &name_facts(db, project, name).declarations;
    let domain = Some(domain.text(db).as_str()).filter(|domain| !domain.is_empty());
    // A package's names are seen outside it only written through it
    // (`Postcodes.UkPostcode`, area 11), never bare.
    let outside: Vec<&Symbol> = all
        .iter()
        .filter(|symbol| symbol.ownership.package().is_none())
        .collect();
    let mut seen: Vec<Symbol> = outside
        .iter()
        .filter(|symbol| symbol.scope.is_some() && symbol.in_scope_of(domain))
        .chain(outside.iter().filter(|symbol| symbol.scope.is_none()))
        .map(|symbol| (*symbol).clone())
        .collect();
    if seen.is_empty() {
        seen = outside.into_iter().cloned().collect();
    }
    seen
}

/// The functions and types declared in `domain` called `name`: what the
/// identity `<domain>.<name>` names (area 18), or the checker's tables'
/// name for a domain's contested private name ([`Symbol::canonical`]).
#[salsa::tracked(returns(ref))]
pub fn domain_declarations<'db>(
    db: &'db dyn Db,
    project: Project,
    name: Name<'db>,
    domain: Name<'db>,
) -> Vec<Symbol> {
    name_facts(db, project, name)
        .declarations
        .iter()
        .filter(|symbol| {
            symbol.ownership.scope() == Some(domain.text(db).as_str())
                && symbol.identity() != symbol.name
        })
        .cloned()
        .collect()
}

#[salsa::tracked(returns(ref))]
pub fn type_def<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> Option<Def> {
    type_table(db, project).def(name.text(db)).cloned()
}

/// A function's, action's, query's or fixture's signature.
#[salsa::tracked(returns(ref))]
pub fn signature<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> Option<Sig> {
    callables(db, project).plain.get(name.text(db)).cloned()
}

/// A library function's signature.
#[salsa::tracked(returns(ref))]
pub fn library_signature<'db>(
    db: &'db dyn Db,
    project: Project,
    library: Name<'db>,
    function: Name<'db>,
) -> Option<Sig> {
    callables(db, project)
        .library
        .get(&(library.text(db).clone(), function.text(db).clone()))
        .cloned()
}

/// A constant's type (area 3): one beside its use, or a library's.
#[salsa::tracked(returns(ref))]
pub fn constant_type<'db>(
    db: &'db dyn Db,
    project: Project,
    library: Option<Name<'db>>,
    name: Name<'db>,
) -> Option<crate::types::Ty> {
    let table = callables(db, project);
    match library {
        Some(library) => table
            .library_constants
            .get(&(library.text(db).clone(), name.text(db).clone()))
            .cloned(),
        None => table.constants.get(name.text(db)).cloned(),
    }
}

#[salsa::tracked(returns(ref))]
pub fn failure_info<'db>(
    db: &'db dyn Db,
    project: Project,
    name: Name<'db>,
) -> Option<FailureInfo> {
    failure_table(db, project).get(name.text(db)).cloned()
}

/// Every failure using the public code `code`, in file and source order.
#[salsa::tracked(returns(ref))]
pub fn failures_with_code<'db>(
    db: &'db dyn Db,
    project: Project,
    code: Name<'db>,
) -> Vec<FailureInfo> {
    failure_table(db, project).with_code(code.text(db)).to_vec()
}

/// Every test with the sentence `sentence`, as its file's path and
/// position, in path order (area 13).
#[salsa::tracked(returns(ref))]
pub fn tests_called<'db>(
    db: &'db dyn Db,
    project: Project,
    sentence: Name<'db>,
) -> Vec<(String, u32)> {
    let mut found = symbols(db, project)
        .tests
        .get(sentence.text(db))
        .cloned()
        .unwrap_or_default();
    found.sort();
    found
}

/// Every file by its project-relative path.
#[salsa::tracked(returns(ref))]
pub fn files_by_path(db: &dyn Db, project: Project) -> BTreeMap<String, File> {
    project
        .files(db)
        .iter()
        .map(|file| (file.path(db).clone(), *file))
        .collect()
}

/// The file at the project-relative path `path`: what a check that reads
/// one declaration's file depends on, rather than the list of every file.
#[salsa::tracked(returns(copy))]
pub fn file_at<'db>(db: &'db dyn Db, project: Project, path: Name<'db>) -> Option<File> {
    files_by_path(db, project).get(path.text(db)).copied()
}

/// Every declared name by the file name it folds to (`TodoList` and
/// `todo_list` both become `todo_list.jadpo`), built once per symbol table
/// rather than compared pairwise for each declaration.
#[salsa::tracked(returns(ref))]
pub fn folded_names(db: &dyn Db, project: Project) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, found) in &symbols(db, project).declarations {
        // A provided failure has no file of its own to clash with.
        if found.iter().all(|symbol| symbol.provided.is_some()) {
            continue;
        }
        out.entry(to_snake(name)).or_default().push(name.clone());
    }
    out
}

/// The declarations whose names fold to `folded`.
#[salsa::tracked(returns(ref))]
pub fn same_file_name<'db>(db: &'db dyn Db, project: Project, folded: Name<'db>) -> Vec<Symbol> {
    let table = symbols(db, project);
    folded_names(db, project)
        .get(folded.text(db))
        .into_iter()
        .flatten()
        .flat_map(|name| table.declarations.get(name).into_iter().flatten().cloned())
        .collect()
}

/// Reading the symbol table.
pub trait NameView {
    /// The declarations called `name` the reader sees, in path order: its
    /// own domain's private ones first, then the project-wide ones, or,
    /// when it sees none, every one (area 1, "Files and names"). `name` may
    /// also be a name as the checker's tables know it ([`Symbol::canonical`]).
    fn declared(&self, name: &str) -> &[Symbol];
    fn is_domain(&self, name: &str) -> bool;
    fn is_connection(&self, name: &str) -> bool;
    fn is_package(&self, name: &str) -> bool;
    /// The names a package held in two versions is written through,
    /// `Postcodes.v2` and `Postcodes.v3`; empty for any other name.
    fn versions(&self, name: &str) -> Vec<String>;
    /// The whole table: for suggestions on a mistake only, since reading it
    /// depends on every declaration.
    fn all(&self) -> &Symbols;
    /// The identity registry every key is made through.
    fn registry(&self) -> &Registry;

    /// Whether `name` is one of the resources of the provider package in
    /// `folder`, which the package's own actions call by its bare name
    /// (area 11; chunk 7.6).
    fn is_resource(&self, _folder: &str, _name: &str) -> bool {
        false
    }

    /// The name the checker's tables know `written` by, as this reader
    /// writes it ([`Symbol::canonical`]).
    fn canonical(&self, written: &str) -> String {
        written.to_string()
    }

    /// The declaration called `name`, if there is exactly one or the first.
    fn get(&self, name: &str) -> Option<&Symbol> {
        self.declared(name).first()
    }

    /// The library called `name`.
    fn library(&self, name: &str) -> Option<&Symbol> {
        self.declared(name)
            .iter()
            .find(|symbol| symbol.kind == ItemKind::Library)
    }

    /// How `head.rest[0]...` reaches a package (area 11, "Where packages
    /// live"; chunk 7.1c2): `Some(Some(via))` when `rest[0]` is a version
    /// to take into the package's name, `Postcodes.v3` (or one written
    /// needlessly, which reads as the package itself), `Some(None)` when a
    /// version is missing while two are held, and `None` when the name
    /// needs nothing. Resolution reports the version's mistakes.
    fn through_version(&self, head: &str, rest: &[String]) -> Option<Option<String>> {
        let versions = self.versions(head);
        let next = rest.first()?;
        if !versions.is_empty() {
            let via = format!("{head}.{next}");
            return Some(versions.contains(&via).then_some(via));
        }
        let needless = rest.len() > 1
            && jadpo_db::is_version_label(next)
            && self.declared(&format!("{head}.{next}")).is_empty();
        needless.then(|| Some(head.to_string()))
    }

    /// A dotted name whose first name is a package's, as the declaration
    /// it reaches (area 11; chunk 7.1e): `Postcodes.Round.morning` reaches
    /// `Postcodes.Round`, and `Postcodes.v3.Round.morning`, while two
    /// versions are held, `Postcodes.v3.Round`, with the number of names
    /// that took. `None` when the first name is no package's; `Some(None)`
    /// when the name stops at the package or misses its version, which
    /// resolution reports. A type's name and an enum's value are written
    /// through a package the same way.
    fn through_package(&self, names: &[String]) -> Option<Option<(String, usize)>> {
        let (head, rest) = names.split_first()?;
        if rest.is_empty() || !self.is_package(head) {
            return None;
        }
        let (via, at) = match self.through_version(head, rest) {
            Some(Some(via)) => (via, 1),
            Some(None) => return Some(None),
            None => (head.clone(), 0),
        };
        Some(
            rest.get(at)
                .map(|member| (format!("{via}.{member}"), at + 2)),
        )
    }

    /// The function `function` of library `library`.
    fn library_function(&self, library: &str, function: &str) -> Option<&Symbol> {
        self.declared(function).iter().find(|symbol| {
            symbol
                .library
                .as_ref()
                .is_some_and(|(ownership, _)| ownership == library)
        })
    }
}

/// Reading callables' signatures.
pub trait SigView {
    fn signature(&self, name: &str) -> Option<&Sig>;
    fn library_signature(&self, library: &str, function: &str) -> Option<&Sig>;
    /// The type of the constant `name`, or of library `library`'s.
    fn constant_type(&self, library: Option<&str>, name: &str) -> Option<&crate::types::Ty>;
}

/// Reading what writes can break (area 6).
pub trait WriteView {
    /// What a write to the entity called `entity` can break.
    fn write_rules(&self, entity: &str) -> Option<&WriteRules>;

    /// What erasing a record of the person entity called `person` does
    /// (area 15, "Erasure"; chunk P3).
    fn erasure(&self, person: &str) -> Option<&crate::erasure::Erasure>;
}

/// Reading declared failures.
pub trait FailureView {
    fn failure_info(&self, name: &str) -> Option<&FailureInfo>;

    fn is_declared_failure(&self, name: &str) -> bool {
        self.failure_info(name).is_some()
    }
}

/// The per-name lookups of one project, for one check. Made inside the
/// query that reads through it, so each lookup is that query's dependency.
/// Made for a file ([`Lookup::for_file`]) or an owner ([`Lookup::within`]),
/// it reads names as code there writes them, its domain's private names
/// first (area 1, "Files and names").
pub struct Lookup<'db> {
    db: &'db dyn Db,
    project: Project,
    /// The domain whose private names the reader sees, or the package
    /// whose names it sees bare.
    domain: Option<String>,
    /// Whether the reader is a package's own code, which sees only its
    /// package's names and the built-ins (area 11).
    in_package: bool,
    /// Lookups already made by this check, to save interning a name again.
    defs: RefCell<HashMap<String, Option<&'db Def>>>,
}

impl std::fmt::Debug for Lookup<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(std::any::type_name::<Self>())
            .finish_non_exhaustive()
    }
}

impl<'db> Lookup<'db> {
    /// The project's identity declarations (area 8), read when a check
    /// meets a caller.
    pub fn identity(&self) -> std::sync::Arc<crate::identity::Identity> {
        std::sync::Arc::clone(crate::identity::identity(self.db, self.project))
    }

    /// Lookups that read names as code outside every domain writes them.
    pub fn new(db: &'db dyn Db, project: Project) -> Lookup<'db> {
        Lookup {
            db,
            project,
            domain: None,
            in_package: false,
            defs: RefCell::new(HashMap::new()),
        }
    }

    /// Lookups that read names as code owned by `ownership` writes them.
    pub fn within(db: &'db dyn Db, project: Project, ownership: &Ownership) -> Lookup<'db> {
        Lookup {
            domain: ownership.scope().map(str::to_string),
            in_package: ownership.package().is_some(),
            ..Lookup::new(db, project)
        }
    }

    /// Lookups that read names as the code in `file` writes them.
    pub fn for_file(db: &'db dyn Db, project: Project, file: File) -> Lookup<'db> {
        let location = jadpo_db::location(db, project, file);
        Lookup::within(db, project, &location.ownership)
    }

    /// The domain whose private names this reader sees.
    pub fn domain(&self) -> Option<&str> {
        self.domain.as_deref()
    }

    /// The name the checker's tables know `written` by, as this reader
    /// writes it ([`Symbol::canonical`]): `written` may also be the
    /// identity of a function or type declared in a domain,
    /// `billing.Address` (area 18).
    pub fn canonical_name(&self, written: &str) -> String {
        let contested = contested(self.db, self.project);
        match self.identified(written) {
            Some((domain, name)) => contested.canonical(name, Some(domain)),
            None => contested.canonical(written, self.domain.as_deref()),
        }
    }

    /// The domain and name of `written` when it is the identity of a
    /// function or type declared in a domain, `billing.Address`, or a
    /// package's declaration written through it, `Postcodes.UkPostcode`.
    fn identified<'w>(&self, written: &'w str) -> Option<(&'w str, &'w str)> {
        let (domain, name) = written.split_once('.')?;
        if name.contains('.') {
            // A package held in two versions is written through its name
            // and version, `Postcodes.v3.UkPostcode` (chunk 7.1c2).
            let (package, name) = written.rsplit_once('.')?;
            if !package.contains('.') || !self.facts(package).package {
                return None;
            }
            return (!self.in_domain(name, package).is_empty()).then_some((package, name));
        }
        let facts = self.facts(domain);
        if !facts.domain && !facts.package {
            return None;
        }
        (!self.in_domain(name, domain).is_empty()).then_some((domain, name))
    }

    fn in_domain(&self, name: &str, domain: &str) -> &'db [Symbol] {
        domain_declarations(self.db, self.project, self.name(name), self.name(domain))
    }

    fn name(&self, text: &str) -> Name<'db> {
        Name::new(self.db, text.to_string())
    }

    pub fn facts(&self, name: &str) -> &'db NameFacts {
        name_facts(self.db, self.project, self.name(name))
    }

    /// The declarations whose names fold to the same file name as `folded`.
    pub fn same_file_name(&self, folded: &str) -> &'db [Symbol] {
        same_file_name(self.db, self.project, self.name(folded))
    }

    /// What a read of setting `name` needs to know of it (area 10).
    pub fn setting(&self, name: &str) -> Option<&'db crate::config::SettingFact> {
        crate::config::setting_fact(self.db, self.project, self.name(name)).as_ref()
    }

    /// The constant `name`, or library `library`'s, as the compiler worked
    /// it out (area 3).
    pub fn constant(
        &self,
        library: Option<&str>,
        name: &str,
    ) -> Option<&'db crate::constants::Evaluated> {
        let id = crate::constants::find(self, library, name)?;
        crate::constants::constant_table(self.db, self.project).get(&id)
    }

    pub fn failures_with_code(&self, code: &str) -> &'db [FailureInfo] {
        failures_with_code(self.db, self.project, self.name(code))
    }
}

impl TypeView for Lookup<'_> {
    fn def(&self, name: &str) -> Option<&Def> {
        if let Some(found) = self.defs.borrow().get(name) {
            return *found;
        }
        let canonical = self.canonical_name(name);
        let found = type_def(self.db, self.project, self.name(&canonical)).as_ref();
        self.defs.borrow_mut().insert(name.to_string(), found);
        found
    }

    fn canonical(&self, written: &str) -> String {
        self.canonical_name(written)
    }
}

impl NameView for Lookup<'_> {
    fn declared(&self, name: &str) -> &[Symbol] {
        if self.in_package {
            // A package is published apart from any app, so its code sees
            // only its own names (and the built-ins), bare.
            let own = self.domain.clone().unwrap_or_default();
            if let Some((package, rest)) = self.identified(name) {
                return if package == own {
                    self.in_domain(rest, package)
                } else {
                    &[]
                };
            }
            return self.in_domain(name, &own);
        }
        if let Some((domain, rest)) = self.identified(name) {
            return self.in_domain(rest, domain);
        }
        let all = &self.facts(name).declarations;
        if all.iter().all(|symbol| symbol.scope.is_none()) {
            return all;
        }
        let domain = self.domain.clone().unwrap_or_default();
        in_scope(self.db, self.project, self.name(name), self.name(&domain))
    }

    fn canonical(&self, written: &str) -> String {
        self.canonical_name(written)
    }

    fn is_domain(&self, name: &str) -> bool {
        !self.in_package && self.facts(name).domain
    }

    fn is_resource(&self, folder: &str, name: &str) -> bool {
        self.in_package && crate::package_code::is_resource(self.db, self.project, folder, name)
    }

    fn is_connection(&self, name: &str) -> bool {
        !self.in_package && self.facts(name).connection
    }

    fn is_package(&self, name: &str) -> bool {
        !self.in_package && self.facts(name).package
    }

    fn versions(&self, name: &str) -> Vec<String> {
        if self.in_package {
            return Vec::new();
        }
        self.facts(name).versions.clone()
    }

    fn all(&self) -> &Symbols {
        symbols(self.db, self.project)
    }

    fn library_function(&self, library: &str, function: &str) -> Option<&Symbol> {
        let library = self.canonical_name(library);
        // The library names its function exactly, a package's included,
        // which code outside the package sees only through its library.
        self.facts(function).declarations.iter().find(|symbol| {
            symbol
                .library
                .as_ref()
                .is_some_and(|(held_by, _)| *held_by == library)
        })
    }

    fn registry(&self) -> &Registry {
        crate::graph::registry(self.db, self.project)
    }
}

impl SigView for Lookup<'_> {
    fn signature(&self, name: &str) -> Option<&Sig> {
        let canonical = self.canonical_name(name);
        signature(self.db, self.project, self.name(&canonical)).as_ref()
    }

    fn library_signature(&self, library: &str, function: &str) -> Option<&Sig> {
        let library = self.canonical_name(library);
        library_signature(
            self.db,
            self.project,
            self.name(&library),
            self.name(function),
        )
        .as_ref()
    }

    fn constant_type(&self, library: Option<&str>, name: &str) -> Option<&crate::types::Ty> {
        let name = match library {
            Some(_) => name.to_string(),
            None => self.canonical_name(name),
        };
        constant_type(
            self.db,
            self.project,
            library.map(|library| self.name(library)),
            self.name(&name),
        )
        .as_ref()
    }
}

impl WriteView for Lookup<'_> {
    fn write_rules(&self, entity: &str) -> Option<&WriteRules> {
        crate::data::write_rules(self.db, self.project, self.name(entity)).as_ref()
    }

    fn erasure(&self, person: &str) -> Option<&crate::erasure::Erasure> {
        crate::erasure::erasures(self.db, self.project)
            .people
            .get(person)
    }
}

impl FailureView for Lookup<'_> {
    fn failure_info(&self, name: &str) -> Option<&FailureInfo> {
        let canonical = self.canonical_name(name);
        failure_info(self.db, self.project, self.name(&canonical)).as_ref()
    }
}

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

use jadpo_db::{Db, ItemKind, Project, Symbol, Symbols, symbols};
use jadpo_graph::Registry;

use crate::case::to_snake;
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
    }
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

/// Every declared name by the file name it folds to (`TodoList` and
/// `todo_list` both become `todo_list.jadpo`), built once per symbol table
/// rather than compared pairwise for each declaration.
#[salsa::tracked(returns(ref))]
pub fn folded_names(db: &dyn Db, project: Project) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in symbols(db, project).declarations.keys() {
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
    /// The declarations called `name`, in path order; more than one is a
    /// duplicate.
    fn declared(&self, name: &str) -> &[Symbol];
    fn is_domain(&self, name: &str) -> bool;
    fn is_connection(&self, name: &str) -> bool;
    fn is_package(&self, name: &str) -> bool;
    /// The whole table: for suggestions on a mistake only, since reading it
    /// depends on every declaration.
    fn all(&self) -> &Symbols;
    /// The identity registry every key is made through.
    fn registry(&self) -> &Registry;

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
pub struct Lookup<'db> {
    db: &'db dyn Db,
    project: Project,
    /// Lookups already made by this check, to save interning a name again.
    defs: RefCell<HashMap<String, Option<&'db Def>>>,
}

impl std::fmt::Debug for Lookup<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lookup").finish_non_exhaustive()
    }
}

impl<'db> Lookup<'db> {
    pub fn new(db: &'db dyn Db, project: Project) -> Lookup<'db> {
        Lookup {
            db,
            project,
            defs: RefCell::new(HashMap::new()),
        }
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

    pub fn failures_with_code(&self, code: &str) -> &'db [FailureInfo] {
        failures_with_code(self.db, self.project, self.name(code))
    }
}

impl TypeView for Lookup<'_> {
    fn def(&self, name: &str) -> Option<&Def> {
        if let Some(found) = self.defs.borrow().get(name) {
            return *found;
        }
        let found = type_def(self.db, self.project, self.name(name)).as_ref();
        self.defs.borrow_mut().insert(name.to_string(), found);
        found
    }
}

impl NameView for Lookup<'_> {
    fn declared(&self, name: &str) -> &[Symbol] {
        &self.facts(name).declarations
    }

    fn is_domain(&self, name: &str) -> bool {
        self.facts(name).domain
    }

    fn is_connection(&self, name: &str) -> bool {
        self.facts(name).connection
    }

    fn is_package(&self, name: &str) -> bool {
        self.facts(name).package
    }

    fn all(&self) -> &Symbols {
        symbols(self.db, self.project)
    }

    fn registry(&self) -> &Registry {
        crate::graph::registry(self.db, self.project)
    }
}

impl SigView for Lookup<'_> {
    fn signature(&self, name: &str) -> Option<&Sig> {
        signature(self.db, self.project, self.name(name)).as_ref()
    }

    fn library_signature(&self, library: &str, function: &str) -> Option<&Sig> {
        library_signature(
            self.db,
            self.project,
            self.name(library),
            self.name(function),
        )
        .as_ref()
    }
}

impl FailureView for Lookup<'_> {
    fn failure_info(&self, name: &str) -> Option<&FailureInfo> {
        failure_info(self.db, self.project, self.name(name)).as_ref()
    }
}

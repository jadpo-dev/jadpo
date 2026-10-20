//! The project-wide symbol table (docs/compiler-runtime.md §2.2). Names are
//! unique across the project and across kinds (area 1), except a private
//! function or type, which is unique only within its domain (area 1, "Files
//! and names", chunk 5.3d), so one table holds every declaration by name,
//! each with the scope its name is unique in; it is built from item trees
//! and paths only.

use std::collections::{BTreeMap, BTreeSet};

use crate::items::{Base, Item, ItemKind, item_tree};
use crate::places::{Ownership, Place};
use crate::{Db, Project, location};

/// One declaration in the table.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Symbol {
    pub name: String,
    pub kind: ItemKind,
    pub public: bool,
    /// The project-relative path of its file.
    pub path: String,
    /// Its position among the file's declarations.
    pub index: u32,
    /// For a library function: its library and its position in it.
    pub library: Option<(String, u32)>,
    pub ownership: Ownership,
    pub place: Place,
    pub test: bool,
    /// For an entity: its lifecycle's transitions (see [`Item::transitions`]).
    pub transitions: Vec<String>,
    /// For the not-found failure the compiler provides for an entity whose
    /// project declares none (area 6, question 6): that entity. It has no
    /// declaration of its own: its path is the entity's file and its index
    /// [`Symbol::PROVIDED`], which no declaration has.
    pub provided: Option<String>,
    /// The domain its name is unique in, for a private function or type of
    /// a domain ([`domain_scope`]); `None` for a name unique across the
    /// project.
    pub scope: Option<String>,
    /// The name the checker's tables know it by: its name, or, for a
    /// domain's private function or type whose name is also declared in
    /// another scope, `<domain>.<name>`, which no declaration can be called.
    pub canonical: String,
}

impl Symbol {
    /// The index of a provided declaration, which no file's declaration
    /// has.
    pub const PROVIDED: u32 = u32::MAX;

    /// Whether code owned by `from` may use this declaration: always within
    /// its own domain, from the root and loose files, and for `shared/` and
    /// root declarations; otherwise only when it is `public`.
    pub fn visible_from(&self, from: &Ownership) -> bool {
        match (&self.ownership, from) {
            (Ownership::Domain(a), Ownership::Domain(b)) => a == b || self.public,
            // A package's own code sees all of it; anything else only what
            // it makes `public` (area 11, "What is public").
            (Ownership::Package(theirs), Ownership::Package(ours)) => theirs == ours || self.public,
            (Ownership::Package(_), _) => self.public,
            _ => true,
        }
    }

    /// The name its stable identity is made from (area 18, question 8,
    /// amended 2026-10-16): `<domain>.<name>` for a function or type
    /// declared in a domain, public or not, and its name for everything
    /// else.
    pub fn identity(&self) -> String {
        identity_name(
            &self.name,
            &self.kind,
            self.library.is_some(),
            &self.ownership,
        )
    }

    /// Whether code written in `domain` (`None` outside every domain) sees
    /// this name without reaching into another domain's private names: a
    /// project-wide name, or one private to `domain`.
    pub fn in_scope_of(&self, domain: Option<&str>) -> bool {
        match &self.scope {
            None => true,
            Some(own) => domain == Some(own.as_str()),
        }
    }
}

/// The domain a declaration's name is unique in (area 1, "Files and names",
/// decided 2026-10-16): a private `function` or `type` of a domain, other
/// than a failure, an email or an event, whose names are seen outside the
/// code. Everything else is unique across the project: anything public, in
/// `shared/`, the root files or loose files, a library and its functions,
/// and every other kind. A type an API's record names stays unique across
/// the project too, which `semantic.duplicate_declaration` checks, since
/// only the routes know it.
pub fn domain_scope(
    kind: &ItemKind,
    public: bool,
    in_library: bool,
    ownership: &Ownership,
) -> Option<String> {
    // Every name a package declares is its own: bare inside it, written
    // through it outside (`Postcodes.UkPostcode`, area 11).
    if let Ownership::Package(home) = ownership {
        return Some(home.code.clone());
    }
    let Ownership::Domain(domain) = ownership else {
        return None;
    };
    let scoped = match kind {
        ItemKind::Function | ItemKind::Constant => !in_library,
        ItemKind::Type(
            Base::Failure | Base::EmailMessage | Base::Event | Base::Message | Base::Prompt,
        ) => false,
        ItemKind::Type(_) => true,
        _ => false,
    };
    (scoped && !public).then(|| domain.clone())
}

/// The name a declaration's stable identity is made from (area 18): the
/// domain and the name, `billing.Address`, for a function (not a library's)
/// or type declared in a domain, and the name for everything else. A
/// library's functions keep their library's name, `rounding.half_even`,
/// which promotion to `shared/` keeps.
pub fn identity_name(
    name: &str,
    kind: &ItemKind,
    in_library: bool,
    ownership: &Ownership,
) -> String {
    match (kind, ownership) {
        (_, Ownership::Package(home)) => format!("{}.{name}", home.code),
        (ItemKind::Type(_), Ownership::Domain(domain)) => format!("{domain}.{name}"),
        (ItemKind::Function | ItemKind::Constant, Ownership::Domain(domain)) if !in_library => {
            format!("{domain}.{name}")
        }
        _ => name.to_string(),
    }
}

/// Every name the project declares.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Symbols {
    /// Declarations by name, in path order; more than one is a duplicate.
    /// Library functions are here too, since their names are unique as well.
    pub declarations: BTreeMap<String, Vec<Symbol>>,
    /// Domain names, from the folders under `domains/`.
    pub domains: BTreeSet<String>,
    /// Connection names declared in `application { }` (area 11).
    pub connections: BTreeSet<String>,
    /// Package names the connections use, and every package the project
    /// holds, by its name in code.
    pub packages: BTreeSet<String>,
    /// The packages the project holds, by their name in code: installed in
    /// `packages/` and its own in `shared/providers/` (area 11).
    pub homes: BTreeMap<String, Vec<crate::places::PackageHome>>,
    /// Tests by their sentence, each as its file's path and position, in
    /// path order; more than one is a duplicate (area 13).
    pub tests: BTreeMap<String, Vec<(String, u32)>>,
}

/// The private names declared in more than one scope (a domain's, another
/// domain's or the project's), by domain and name: those whose
/// [`Symbol::canonical`] name is `<domain>.<name>`. Usually empty, and kept
/// apart from the whole table so what reads it isn't recomputed when an
/// unrelated declaration changes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Contested(pub BTreeSet<(String, String)>);

impl Contested {
    /// The name the checker's tables know the name `written`, as code in
    /// `domain` writes it, by: `<domain>.<written>` when it is one of that
    /// domain's contested private names, otherwise `written`.
    pub fn canonical(&self, written: &str, domain: Option<&str>) -> String {
        match domain {
            Some(domain) if self.0.contains(&(domain.to_string(), written.to_string())) => {
                format!("{domain}.{written}")
            }
            _ => written.to_string(),
        }
    }
}

/// The contested private names of `project` ([`Contested`]).
#[salsa::tracked(returns(ref))]
pub fn contested(db: &dyn Db, project: Project) -> Contested {
    let mut out = BTreeSet::new();
    for found in symbols(db, project).declarations.values() {
        for symbol in found {
            if let Some(domain) = &symbol.scope
                && symbol.canonical != symbol.name
            {
                out.insert((domain.clone(), symbol.name.clone()));
            }
        }
    }
    Contested(out)
}

impl Symbols {
    /// The declaration called `name`, if there is exactly one or the first.
    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.declarations.get(name).and_then(|found| found.first())
    }

    /// The declaration the checker's tables know as `canonical`: a
    /// project-wide name, or `<domain>.<name>` for a domain's contested
    /// private name.
    pub fn by_canonical(&self, canonical: &str) -> Option<&Symbol> {
        if let Some(found) = self.declarations.get(canonical) {
            return found.iter().find(|symbol| symbol.canonical == canonical);
        }
        let (domain, name) = canonical.split_once('.')?;
        self.declarations
            .get(name)?
            .iter()
            .find(|symbol| symbol.scope.as_deref() == Some(domain) && symbol.canonical == canonical)
    }

    /// The library called `name`.
    pub fn library(&self, name: &str) -> Option<&Symbol> {
        self.declarations
            .get(name)?
            .iter()
            .find(|symbol| symbol.kind == ItemKind::Library)
    }

    /// The function `function` of library `library`.
    pub fn library_function(&self, library: &str, function: &str) -> Option<&Symbol> {
        self.declarations.get(function)?.iter().find(|symbol| {
            symbol
                .library
                .as_ref()
                .is_some_and(|(ownership, _)| ownership == library)
        })
    }
}

/// The symbol table of `project`.
#[salsa::tracked(returns(ref))]
pub fn symbols(db: &dyn Db, project: Project) -> Symbols {
    let mut table = Symbols::default();
    for &file in project.files(db) {
        let path = file.path(db);
        let location = location(db, project, file);
        if let Some(domain) = location.ownership.domain() {
            table.domains.insert(domain.to_string());
        }
        if let Some(home) = location.ownership.package() {
            table.packages.insert(home.code.clone());
            let homes = table.homes.entry(home.code.clone()).or_default();
            if !homes.contains(home) {
                homes.push(home.clone());
            }
        }
        let symbol = |item: &Item, library: Option<(String, u32)>, index: u32| Symbol {
            scope: domain_scope(
                &item.kind,
                item.public,
                library.is_some(),
                &location.ownership,
            ),
            canonical: item.name.clone(),
            name: item.name.clone(),
            kind: item.kind.clone(),
            public: item.public,
            path: path.clone(),
            index,
            library,
            ownership: location.ownership.clone(),
            place: location.place.clone(),
            test: location.test,
            transitions: item.transitions.clone(),
            provided: None,
        };
        for item in &item_tree(db, file).items {
            match &item.kind {
                ItemKind::Test => table
                    .tests
                    .entry(item.name.clone())
                    .or_default()
                    .push((path.clone(), item.index)),
                ItemKind::Resource | ItemKind::Provide => {}
                ItemKind::Singleton => {
                    table.connections.extend(item.connections.iter().cloned());
                    table.packages.extend(item.packages.iter().cloned());
                }
                // A package's name is its folder's (`Postcodes` in
                // `packages/postcodes/v2/`), which its index has to give.
                ItemKind::Package => {}
                ItemKind::Library => {
                    // A package's library is known through its package,
                    // `Postcodes.uk`, so its functions say so.
                    let library = match location.ownership.package() {
                        Some(home) => format!("{}.{}", home.code, item.name),
                        None => item.name.clone(),
                    };
                    for function in &item.functions {
                        table
                            .declarations
                            .entry(function.name.clone())
                            .or_default()
                            .push(symbol(
                                function,
                                Some((library.clone(), function.index)),
                                item.index,
                            ));
                    }
                    table
                        .declarations
                        .entry(item.name.clone())
                        .or_default()
                        .push(symbol(item, None, item.index));
                }
                _ => table
                    .declarations
                    .entry(item.name.clone())
                    .or_default()
                    .push(symbol(item, None, item.index)),
            }
        }
    }
    // Every entity's not-found failure, `<Entity>NotFound`, is the one the
    // project declares, or else one the compiler provides with kind
    // `not_found`, public so any domain's callers can name it (area 6,
    // question 6).
    let entities: Vec<Symbol> = table
        .declarations
        .values()
        .filter_map(|found| found.iter().find(|symbol| symbol.kind == ItemKind::Entity))
        .cloned()
        .collect();
    for entity in entities {
        let name = format!("{}NotFound", entity.name);
        if table.declarations.contains_key(&name) {
            continue;
        }
        table.declarations.insert(
            name.clone(),
            vec![Symbol {
                name,
                kind: ItemKind::Type(Base::Failure),
                public: true,
                path: entity.path.clone(),
                index: Symbol::PROVIDED,
                library: None,
                ownership: entity.ownership.clone(),
                place: entity.place.clone(),
                test: false,
                transitions: Vec::new(),
                provided: Some(entity.name.clone()),
                scope: None,
                canonical: String::new(),
            }],
        );
    }
    // A private name declared in more than one scope is known to the
    // checker's tables by its domain and name, so each domain's reads its
    // own (area 1, "Files and names").
    for (name, found) in table.declarations.iter_mut() {
        let scopes: BTreeSet<Option<&String>> =
            found.iter().map(|symbol| symbol.scope.as_ref()).collect();
        let contested = scopes.len() > 1;
        for symbol in found.iter_mut() {
            symbol.canonical = match &symbol.scope {
                // A package's names are always known through it.
                Some(package) if symbol.ownership.package().is_some() => {
                    format!("{package}.{name}")
                }
                Some(domain) if contested => format!("{domain}.{name}"),
                _ => name.clone(),
            };
        }
    }
    table
}

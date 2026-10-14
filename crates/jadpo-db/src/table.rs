//! The project-wide symbol table (docs/compiler-runtime.md §2.2). Names are
//! unique across the project and across kinds (area 1), so one table holds
//! every declaration; it is built from item trees and paths only.

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
            _ => true,
        }
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
    /// Package names the connections use, and local provider packages.
    pub packages: BTreeSet<String>,
    /// Tests by their sentence, each as its file's path and position, in
    /// path order; more than one is a duplicate (area 13).
    pub tests: BTreeMap<String, Vec<(String, u32)>>,
}

impl Symbols {
    /// The declaration called `name`, if there is exactly one or the first.
    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.declarations.get(name).and_then(|found| found.first())
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
        let symbol = |item: &Item, library: Option<(String, u32)>, index: u32| Symbol {
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
                ItemKind::Package => {
                    let ownership = item.name.split('.').next().unwrap_or_default();
                    table.packages.insert(ownership.to_string());
                }
                ItemKind::Library => {
                    for function in &item.functions {
                        table
                            .declarations
                            .entry(function.name.clone())
                            .or_default()
                            .push(symbol(
                                function,
                                Some((item.name.clone(), function.index)),
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
            }],
        );
    }
    table
}

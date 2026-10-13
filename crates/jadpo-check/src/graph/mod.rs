//! The graph (docs/compiler-runtime.md §2.3), built from the checker's
//! queries: every domain, file and declaration of areas 1 to 4 with its
//! stable key, place, digest and `///` description; fields and variants;
//! signatures; and the `calls`, `uses` and `raises` edges between them.
//!
//! Each file's part is a query of its own ([`file_part`]), so an edit
//! rebuilds that file's nodes and the project's graph is only assembled
//! again ([`graph`]). Declarations later areas define (entities' sections,
//! routes, schedules, subscribers, queries' bodies) have their nodes, with
//! identity, place and digest; their contents come with their areas.

mod part;
mod paths;
mod similar;
mod text;

pub use part::{FilePart, file_part};
pub use similar::{FunctionShape, function_shapes, similar_problems};
pub use text::LineIndex;

use jadpo_db::{Db, ItemKind, Project, Symbols, symbols};
use jadpo_graph::{Builder, Graph, Key, Kind, Node, Registry};

/// The identity registry (area 18). `database/identities.json` is read
/// once `jadpo rename` and the domain commands, which write it, exist
/// (area 14); until then nothing has been renamed and every born name is
/// the name now. Every key is made through it.
pub fn registry() -> &'static Registry {
    static EMPTY: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();
    EMPTY.get_or_init(Registry::new)
}

/// The graph kind of a declaration.
pub fn kind_of(kind: &ItemKind) -> Option<Kind> {
    Some(match kind {
        ItemKind::Type(_) => Kind::Type,
        ItemKind::Function => Kind::Function,
        ItemKind::Action => Kind::Action,
        ItemKind::Query => Kind::Query,
        ItemKind::Fixture => Kind::Fixture,
        ItemKind::Entity => Kind::Entity,
        ItemKind::Library => Kind::Library,
        ItemKind::Routes => Kind::RouteGroup,
        ItemKind::Schedule => Kind::Schedule,
        ItemKind::Subscriber => Kind::Subscriber,
        ItemKind::Seed => Kind::Seed,
        ItemKind::Test => Kind::Test,
        // Packages, resources and provider mappings come with area 11, the
        // setting blocks with areas 10, 11 and 14.
        ItemKind::Package | ItemKind::Resource | ItemKind::Provide | ItemKind::Singleton => {
            return None;
        }
    })
}

/// The key of the project declaration called `name` (not a library
/// function), when it has a node.
pub fn declaration_key(symbols: &Symbols, name: &str) -> Option<Key> {
    let symbol = symbols
        .declarations
        .get(name)?
        .iter()
        .find(|symbol| symbol.library.is_none())?;
    let kind = kind_of(&symbol.kind)?;
    Some(registry().key(kind, name, None))
}

/// The key of function `function` of library `library`.
pub fn library_function_key(library: &str, function: &str) -> Key {
    let registry = registry();
    let born = registry.born(Kind::Library, library, None);
    registry.key(
        Kind::Function,
        &format!("{library}.{function}"),
        Some((library, &born)),
    )
}

/// The born name of the type or entity called `host` now.
fn born_host(host: &str) -> String {
    let registry = registry();
    let as_type = registry.born(Kind::Type, host, None);
    if as_type != host {
        return as_type;
    }
    registry.born(Kind::Entity, host, None)
}

/// The key of field `path` (`Note.body`, `Outcome.paid.receipt`) under the
/// type or entity `host`.
pub fn field_key(host: &str, path: &[String]) -> Key {
    let born = born_host(host);
    let now = format!("{host}.{}", path.join("."));
    registry().key(Kind::Field, &now, Some((host, &born)))
}

/// The key of variant `variant` of enum `host`.
pub fn variant_key(host: &str, variant: &str) -> Key {
    let born = born_host(host);
    registry().key(
        Kind::Variant,
        &format!("{host}.{variant}"),
        Some((host, &born)),
    )
}

/// The project's graph.
#[salsa::tracked(returns(ref))]
pub fn graph(db: &dyn Db, project: Project) -> Graph {
    let symbols = symbols(db, project);
    let mut builder = Builder::new();
    for domain in &symbols.domains {
        let mut node = Node::new(
            registry().key(Kind::Domain, domain, None),
            Kind::Domain,
            domain,
        );
        node.domain = Some(domain.clone());
        builder.node(node);
    }
    // In path order whatever order the files were given in, so the first
    // of two declarations with one name is always the same one.
    let mut files: Vec<_> = project.files(db).to_vec();
    files.sort_by(|a, b| a.path(db).cmp(b.path(db)));
    for file in files {
        let part = file_part(db, project, file);
        for node in &part.nodes {
            builder.node(node.clone());
        }
        for (from, kind, to) in &part.edges {
            builder.edge(from.clone(), *kind, to.clone());
        }
    }
    builder.build()
}

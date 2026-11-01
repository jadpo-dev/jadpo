//! Erasure at providers, as an app and its packages declare it (area 15,
//! "Erasure at providers"; area 11, "Erasing a person at a provider"; chunk
//! V6): a package's `erasure: { call: by: }` says which of its operations
//! erases a person and what that takes, and a connection's `erasure: { key:
//! }` names the field holding the person's key at that provider. Chunk P6
//! calls them; this reads and checks them, so a connection can be told to
//! be able to erase only on declarations that can be carried out.
//!
//! [`of_connection`] then says what becomes of what a connection receives,
//! and [`unerased`] lists each connection a person must accept that of
//! (`ErasureNotPossible`, read by [`crate::acceptances`]).

use std::collections::BTreeMap;

use jadpo_db::{Db, Ownership, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Decl};

use crate::ai_personal::Holder;
use crate::data::ConnectionKind;
use crate::lookups::Lookup;
use crate::personal_data::Sent;
use crate::tree::{self, name_text, node_span};
use crate::types::{Def, FieldsBase, Ty, TypeRef, TypeView, type_table};

/// The kind of change an acceptance names for a connection that can't erase
/// what it receives (area 18, "Acceptances").
pub const DATA_STAYS: &str = "ErasureNotPossible.data_stays";

/// A package's `erasure:`, read and checked.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Declared {
    /// The operation that erases a person, `customers.delete`.
    pub call: String,
    /// The type of the key it takes, as the checker knows it.
    pub by: Ty,
    /// `CustomerId`, as the package writes it.
    pub by_written: String,
}

/// Every package's `erasure:`, by its folder, and their problems.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Declarations {
    pub of: BTreeMap<String, Declared>,
    pub problems: Vec<Diagnostic>,
}

const REASON: &str = "`erasure:` says which of the package's operations erases a person, `call:`, and the type of the key it takes, `by:`, one field of its input.";

/// Every provider package's `erasure:`.
#[salsa::tracked(returns(ref))]
pub fn declared(db: &dyn Db, project: Project) -> Declarations {
    let mut out = Declarations::default();
    for &file in project.files(db) {
        let location = jadpo_db::location(db, project, file);
        let Ownership::Package(home) = &location.ownership else {
            continue;
        };
        let path = file.path(db).clone();
        if path != format!("{}/{}", home.folder, jadpo_db::PROVIDER_FILE) {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let Some(decl) = source.decls().find_map(|decl| match decl {
            Decl::SingletonDecl(decl)
                if decl
                    .keyword()
                    .is_some_and(|keyword| keyword.text() == "provider") =>
            {
                Some(decl)
            }
            _ => None,
        }) else {
            continue;
        };
        let Some((_, member)) = tree::members(decl.member_list())
            .into_iter()
            .find(|(key, _)| name_text(key).as_deref() == Some("erasure"))
        else {
            continue;
        };
        if let Some(read) = read(
            db,
            project,
            &location.ownership,
            &home.code,
            &home.folder,
            &path,
            &member,
            &mut out.problems,
        ) {
            out.of.insert(home.folder.clone(), read);
        }
    }
    out
}

fn problem(message: impl Into<String>, span: Span, label: &str) -> Diagnostic {
    Diagnostic::new(&rule::PACKAGE_PROVIDER_INVALID, message)
        .at(span, label)
        .reason(REASON)
}

const SHAPE: &str = "Write `erasure: { call: <resource>.<operation>  by: <Type> }`";

#[allow(clippy::too_many_arguments)]
fn read(
    db: &dyn Db,
    project: Project,
    ownership: &Ownership,
    package: &str,
    folder: &str,
    path: &str,
    member: &ast::Member,
    out: &mut Vec<Diagnostic>,
) -> Option<Declared> {
    let whole = node_span(path, member.syntax());
    if member.block().is_none() {
        out.push(problem("`erasure:` isn't a block", whole, "not a block").next(SHAPE));
        return None;
    }
    let (mut call, mut by) = (None, None);
    for (key, inner) in tree::members(member.block()) {
        match name_text(&key).as_deref() {
            Some("call") => call = inner.value(),
            Some("by") => by = inner.value(),
            other => out.push(
                problem(
                    format!("`erasure:` has no key `{}:`", other.unwrap_or("")),
                    node_span(path, key.syntax()),
                    "not one of its keys",
                )
                .next(format!("Remove `{}:`", other.unwrap_or(""))),
            ),
        }
    }
    let (Some(call), Some(by)) = (call, by) else {
        out.push(problem("`erasure:` needs `call:` and `by:`", whole, "not both").next(SHAPE));
        return None;
    };
    let call_span = node_span(path, call.syntax());
    let call = tree::written(call.syntax());
    let operations = crate::providers::operations(db, project);
    let Some(operation) = operations.of(folder).find(|op| op.full_name() == call) else {
        let names: Vec<String> = operations.of(folder).map(|op| op.full_name()).collect();
        let near = nearest(&call, names.iter().map(String::as_str));
        out.push(
            problem(
                format!("`erasure:` names `{call}`, which `{package}` doesn't have"),
                call_span,
                "no such operation",
            )
            .next(match near {
                Some(near) => format!("Did you mean `{near}`?"),
                None => "Name the operation of the package's `resources/` that erases a person"
                    .to_string(),
            }),
        );
        return None;
    };
    if !operation.changes {
        out.push(
            problem(
                format!("`erasure:` names `{call}`, which changes nothing"),
                call_span,
                "`changes: true` missing",
            )
            .next(format!(
                "Name the operation that erases a person, or give `{call}` `changes: true` if it does"
            )),
        );
        return None;
    }
    let by_span = node_span(path, by.syntax());
    let by_written = tree::written(by.syntax());
    let table = type_table(db, project);
    let contested = jadpo_db::contested(db, project);
    let canonical = |written: &str| contested.canonical(written, Some(package));
    let resolved = table.resolve(&TypeRef::from_expr(&by).scoped(&canonical));
    if resolved.has_error() {
        out.push(
            problem(
                format!("`erasure:`'s `by:` names `{by_written}`, which isn't a type"),
                by_span,
                "no such type",
            )
            .next("Name the type of the provider's key for a person, declared in the package's `types/`"),
        );
        return None;
    }
    if let Some(reason) = fills_input(db, project, ownership, operation, &resolved) {
        out.push(
            problem(
                format!("`erasure:` can't call `{call}` with only a `{by_written}`"),
                by_span,
                "not what it takes",
            )
            .reason(format!(
                "Erasing a person calls `{call}` with the key the connection names, and nothing else: {reason}."
            ))
            .next(format!(
                "Give `{call}`'s input one field of type `{by_written}`, and make any other it has optional"
            )),
        );
        return None;
    }
    Some(Declared {
        call,
        by: resolved,
        by_written,
    })
}

/// Why `operation` can't be called with only a key of type `by`, when it
/// can't.
fn fills_input(
    db: &dyn Db,
    project: Project,
    ownership: &Ownership,
    operation: &crate::providers::Operation,
    by: &Ty,
) -> Option<String> {
    let Some((input, written)) = &operation.input else {
        return Some("it has no `input:`".to_string());
    };
    let Ty::Named(name) = input else {
        return None;
    };
    let table = type_table(db, project);
    let Some(Def::Fields {
        base: FieldsBase::Object,
        fields,
        ..
    }) = table.def(name)
    else {
        return None;
    };
    let lookup = Lookup::within(db, project, ownership);
    let holding: Vec<&str> = fields
        .iter()
        .filter(|field| same_type(&lookup, &field.declared, by))
        .map(|field| field.name.as_str())
        .collect();
    let [key] = holding.as_slice() else {
        return Some(format!(
            "its input, `{written}`, has {} fields of that type, and must have one",
            holding.len()
        ));
    };
    fields
        .iter()
        .find(|field| {
            field.name != *key && !field.optional && !matches!(field.declared, Ty::Nullable(_))
        })
        .map(|other| format!("its input, `{written}`, needs `{}` too", other.name))
}

/// Whether a field declared `declared` holds a value of type `wanted`,
/// `none` aside: its type, or a field of that type it refines.
fn same_type(lookup: &Lookup<'_>, declared: &Ty, wanted: &Ty) -> bool {
    let mut ty = declared.strip_nullable().clone();
    for _ in 0..8 {
        if ty == *wanted {
            return true;
        }
        let Ty::Field(host, path) = &ty else {
            return false;
        };
        let Some(def) = lookup.field(host, path) else {
            return false;
        };
        ty = def.declared.strip_nullable().clone();
    }
    false
}

/// The keys connections name that can't be used: what isn't a field of a
/// person entity holding what the package's erasure takes.
pub fn key_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let declarations = declared(db, project);
    let persons = crate::persons::persons(db, project);
    let lookup = Lookup::new(db, project);
    for connection in &crate::connections::package_connections(db, project).connections {
        let Some(key) = &connection.erasure_key else {
            continue;
        };
        let name = &connection.name;
        let shown = format!("{}.{}", key.entity, key.field);
        let problem = |message: String, label: &str| {
            Diagnostic::new(&rule::CONNECTION_VALUE_INVALID, message)
                .at(Span::new(key.at.0.clone(), key.at.1, key.at.2), label)
        };
        if !persons.people.contains_key(&key.entity) {
            let named: Vec<String> = persons.people.keys().map(|p| format!("`{p}`")).collect();
            out.push(
                problem(
                    format!("`{name}`'s `erasure:` key `{shown}` isn't on a person"),
                    "no person entity",
                )
                .reason("The key is copied from the person's own record as they are erased, so it is a field of an entity marked `person: true`.")
                .next(if named.is_empty() {
                    "Mark the entity `person: true`, or name a field of one that is".to_string()
                } else {
                    format!("Name a field of {}", crate::or_list(&named))
                }),
            );
            continue;
        }
        let Some(field) = lookup.field(&key.entity, std::slice::from_ref(&key.field)) else {
            out.push(
                problem(
                    format!("`{name}`'s `erasure:` key `{shown}` isn't a field"),
                    "no such field",
                )
                .reason("The key is the field of the person's record holding what the provider knows them by.")
                .next(format!(
                    "Add the field to `{}`, or name the one that holds the provider's key",
                    key.entity
                )),
            );
            continue;
        };
        let Some(erasure) = declarations.of.get(&connection.folder) else {
            continue;
        };
        if !same_type(&lookup, &field.declared, &erasure.by) {
            out.push(
                problem(
                    format!(
                        "`{name}`'s `erasure:` key `{shown}` isn't a `{}`",
                        erasure.by_written
                    ),
                    "another type",
                )
                .reason(format!(
                    "`{}`'s `erasure:` calls `{}` with a `{}`, so the field holding the key is of that type.",
                    connection.package, erasure.call, erasure.by_written
                ))
                .next(format!(
                    "Declare `{shown}` as `{}.{}`",
                    connection.package.split('.').next().unwrap_or_default(),
                    erasure.by_written
                )),
            );
        }
    }
    out
}

/// Why what a connection receives stays there when a person is erased.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Stays {
    /// It reaches its server with a driver rather than a package, so no
    /// `erasure:` can be declared for it.
    NoPackage,
    /// Its package declares no `erasure:`.
    NotDeclared { package: String },
    /// Its package does, and the connection names no key.
    NoKey { package: String },
    /// Its key is on one person entity, and it also receives the data of
    /// another, which erasing that one can't ask the provider to erase.
    OtherPerson {
        key: String,
        field: String,
        person: String,
    },
}

impl Stays {
    /// Why, in words: `` `AcmePay.v1` declares no `erasure:` ``.
    pub fn said(&self) -> String {
        match self {
            Stays::NoPackage => {
                "it is reached with a driver, so it has no package to declare `erasure:`"
                    .to_string()
            }
            Stays::NotDeclared { package } => format!("`{package}` declares no `erasure:`"),
            Stays::NoKey { package } => format!(
                "`{package}` declares `erasure:`, and the connection names no `erasure: {{ key: }}`"
            ),
            Stays::OtherPerson { key, field, person } => format!(
                "its `erasure:` key `{key}` can't erase `{field}`, which belongs to `{person}`"
            ),
        }
    }
}

/// What becomes of what a connection receives when a person is erased.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Erasure {
    /// The provider erases it: its package's call, with the field holding
    /// their key.
    Declared {
        call: String,
        key: String,
    },
    /// A store of files on the server's own disk: no other company holds
    /// it, and a file goes with the records holding it.
    OwnStore,
    Stays(Stays),
}

/// What becomes of what `sent` takes through `connection`.
pub fn of_connection(db: &dyn Db, project: Project, connection: &str, sent: &[&Sent]) -> Erasure {
    if crate::data::connections(db, project).kind(connection) == Some(ConnectionKind::Files) {
        return Erasure::OwnStore;
    }
    let Some(read) = crate::connections::package_connections(db, project)
        .connections
        .iter()
        .find(|read| read.name == connection)
    else {
        return Erasure::Stays(Stays::NoPackage);
    };
    let Some(declared) = declared(db, project).of.get(&read.folder) else {
        return Erasure::Stays(Stays::NotDeclared {
            package: read.package.clone(),
        });
    };
    let Some(key) = &read.erasure_key else {
        return Erasure::Stays(Stays::NoKey {
            package: read.package.clone(),
        });
    };
    let persons = crate::persons::persons(db, project);
    let shown = format!("{}.{}", key.entity, key.field);
    for sent in sent {
        let Holder::Field { host, field } = &sent.holder else {
            continue;
        };
        let owners: Vec<&String> = persons
            .people
            .iter()
            .filter(|(name, person)| *name == host || person.records.contains(host))
            .map(|(name, _)| name)
            .collect();
        if let Some(first) = owners.first()
            && !owners.iter().any(|owner| **owner == key.entity)
        {
            return Erasure::Stays(Stays::OtherPerson {
                key: shown,
                field: format!("{host}.{field}"),
                person: (*first).clone(),
            });
        }
    }
    Erasure::Declared {
        call: declared.call.clone(),
        key: shown,
    }
}

/// A connection whose copy of personal data stays there when a person is
/// erased.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Unerased {
    pub connection: String,
    pub stays: Stays,
    /// What it receives, each once, in order.
    pub holders: Vec<Holder>,
}

impl Unerased {
    /// The change in plain words, as the approval report says it.
    pub fn said(&self) -> String {
        const MOST: usize = 3;
        let mut named: Vec<String> = self
            .holders
            .iter()
            .take(MOST)
            .map(|holder| match holder {
                Holder::Field { host, field } => format!("`{host}.{field}`"),
                Holder::Type(ty) => format!("a `{ty}`"),
            })
            .collect();
        if self.holders.len() > MOST {
            named.push(format!("{} more", self.holders.len() - MOST));
        }
        let held = match named.as_slice() {
            [one] => one.clone(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
            [] => "personal data".to_string(),
        };
        format!(
            "connection `{}` keeps {held} after a person is erased: {}",
            self.connection,
            self.stays.said()
        )
    }
}

/// Each connection that receives personal data and can't erase it, by name.
#[salsa::tracked(returns(ref))]
pub fn unerased(db: &dyn Db, project: Project) -> Vec<Unerased> {
    let mut by_connection: BTreeMap<&str, Vec<&Sent>> = BTreeMap::new();
    for sent in crate::personal_data::sent(db, project) {
        by_connection
            .entry(&sent.connection)
            .or_default()
            .push(sent);
    }
    let mut out = Vec::new();
    for (connection, sent) in by_connection {
        if let Erasure::Stays(stays) = of_connection(db, project, connection, &sent) {
            let mut holders: Vec<Holder> = Vec::new();
            for row in &sent {
                if !holders.contains(&row.holder) {
                    holders.push(row.holder.clone());
                }
            }
            out.push(Unerased {
                connection: connection.to_string(),
                stays,
                holders,
            });
        }
    }
    out
}

/// Where the connection called `name` is written in `application { }`.
pub fn connection_span(db: &dyn Db, project: Project, name: &str) -> Option<Span> {
    for found in crate::config::singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("connections") {
                continue;
            }
            for (connection, _) in tree::members(member.block()) {
                if name_text(&connection).as_deref() == Some(name) {
                    return Some(node_span(&found.path, connection.syntax()));
                }
            }
        }
    }
    None
}

/// Whether what leaves through each connection can be worked out: every
/// file parses, every body checks and the connections read. A body with
/// errors may hold calls nothing can see.
pub fn clean(db: &dyn Db, project: Project) -> bool {
    project.files(db).iter().all(|file| {
        !jadpo_db::parse(db, *file).has_errors()
            && crate::types::body_problems(db, project, *file).is_empty()
    }) && crate::connections::package_connections(db, project)
        .problems
        .is_empty()
        && declared(db, project).problems.is_empty()
}

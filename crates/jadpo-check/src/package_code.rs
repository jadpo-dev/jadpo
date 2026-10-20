//! What package code may do (area 11, "What package code may do"; chunk
//! 7.6): a provider package's actions call only its own API.
//!
//! A package action, in the package's `actions/`, calls its own
//! operations by their bare names (`customers.create(input)`), public or
//! not, in any order, passing answers between them, and uses its own pure
//! functions. It calls no other action, of its package or anyone's: what
//! one action does in order is one way through it, which is what a
//! multi-step call's record replays. The app calls a `public` one through
//! its connection, `stripe.start_subscription(input)`, from an action of
//! a domain the connection lists, mapping its package failures at the call
//! as an operation's; its changing calls are steps of the calling action
//! ([`crate::types::steps`]). A package's resources and actions share the
//! connection's namespace, so one name for both is refused here.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, Folder, ItemKind, Ownership, Place, Project, item_tree, location};
use jadpo_diag::{Diagnostic, rule};

use crate::tree;

/// One of a provider package's actions.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PackageAction {
    /// The package's folder, `shared/providers/acme_pay/v1`.
    pub folder: String,
    /// Its name as the package writes it, `start_subscription`.
    pub name: String,
    /// The name the checker's tables know it by,
    /// `AcmePay.start_subscription`.
    pub canonical: String,
    pub public: bool,
}

/// Each provider package's resources, by the package's folder.
#[salsa::tracked(returns(ref))]
pub fn resources(db: &dyn Db, project: Project) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for &file in project.files(db) {
        let location = location(db, project, file);
        let (Ownership::Package(home), Place::Folder(Folder::Resources)) =
            (&location.ownership, &location.place)
        else {
            continue;
        };
        for item in &item_tree(db, file).items {
            if item.kind == ItemKind::Resource {
                out.entry(home.folder.clone())
                    .or_default()
                    .insert(item.name.clone());
            }
        }
    }
    out
}

/// Whether `name` is one of the resources of the package in `folder`.
pub fn is_resource(db: &dyn Db, project: Project, folder: &str, name: &str) -> bool {
    resources(db, project)
        .get(folder)
        .is_some_and(|names| names.contains(name))
}

/// Every provider package's actions.
#[salsa::tracked(returns(ref))]
pub fn actions(db: &dyn Db, project: Project) -> Vec<PackageAction> {
    let mut out: Vec<PackageAction> = jadpo_db::symbols(db, project)
        .declarations
        .values()
        .flatten()
        .filter(|symbol| symbol.kind == ItemKind::Action)
        .filter_map(|symbol| {
            let home = symbol.ownership.package()?;
            Some(PackageAction {
                folder: home.folder.clone(),
                name: symbol.name.clone(),
                canonical: symbol.canonical.clone(),
                public: symbol.public,
            })
        })
        .collect();
    out.sort_by(|a, b| (&a.folder, &a.name).cmp(&(&b.folder, &b.name)));
    out
}

/// The action `name` of the package in `folder`.
pub fn action<'a>(
    db: &'a dyn Db,
    project: Project,
    folder: &str,
    name: &str,
) -> Option<&'a PackageAction> {
    actions(db, project)
        .iter()
        .find(|action| action.folder == folder && action.name == name)
}

/// The problems with the package actions `file` declares: one named as
/// one of its package's resources.
#[salsa::tracked(returns(ref))]
pub fn action_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let location = location(db, project, file);
    let (Ownership::Package(home), Place::Folder(Folder::Actions)) =
        (&location.ownership, &location.place)
    else {
        return Vec::new();
    };
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let path = file.path(db);
    let mut out = Vec::new();
    for item in &item_tree(db, file).items {
        if item.kind != ItemKind::Action || !is_resource(db, project, &home.folder, &item.name) {
            continue;
        }
        let name = &item.name;
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_ACTION_INVALID,
                format!(
                    "`{name}` is both a resource and an action of `{}`",
                    home.versioned()
                ),
            )
            .at(
                tree::name_span(&source, path, item.index, None),
                "a resource's name",
            )
            .reason(format!(
                "A package's resources and actions share its connection's namespace, so `connection.{name}` would name both."
            ))
            .next(format!("Rename the action `{name}`")),
        );
    }
    out
}

/// The event `emit` names, as written: `SubscriptionRenewed` for `emit
/// SubscriptionRenewed { ... }`.
pub fn emitted_name(emit: &jadpo_syntax::ast::EmitStmt) -> Option<String> {
    use jadpo_syntax::ast::{AstNode, Expr};
    let expr = emit.expr()?;
    Some(match &expr {
        Expr::RecordExpr(record) => tree::written(record.expr()?.syntax()),
        other => tree::written(other.syntax()),
    })
}

/// The events each provider package's actions emit, by the package's
/// folder, as written (chunk 7.6b): read from the source alone, so the
/// webhook events a package declares are told from its own without
/// checking a body. An event a package's action emits is the package's
/// own, and needs no `wire_name:`.
#[salsa::tracked(returns(ref))]
pub fn emitted(db: &dyn Db, project: Project) -> BTreeMap<String, BTreeSet<String>> {
    use jadpo_syntax::ast::{AstNode, EmitStmt};
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for &file in project.files(db) {
        let location = location(db, project, file);
        let (Ownership::Package(home), Place::Folder(Folder::Actions)) =
            (&location.ownership, &location.place)
        else {
            continue;
        };
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        for emit in source.syntax().descendants().filter_map(EmitStmt::cast) {
            if let Some(name) = emitted_name(&emit) {
                out.entry(home.folder.clone()).or_default().insert(name);
            }
        }
    }
    out
}

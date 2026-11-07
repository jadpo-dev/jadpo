//! The scoped review of an imported package and the operations still
//! unconfirmed (area 11, "Mapping imported types to standard types",
//! layers 4 and 5; chunk 7.2e2).
//!
//! For each of the app's own integrations in `shared/providers/`: the
//! operations the app calls through its connections, each confirmed or
//! not by a recording in the package's `recordings/`; and the package's
//! own types those operations reach, each reviewed or not by the agent's
//! `reviewed:` list: in `imported:` for a package that was imported, in
//! `provider { }` itself for one written by hand. The scope grows as the app
//! calls something new, since it is worked out from the calls each time.
//! `jadpo check` and the import's summary show it, and `jadpo check`
//! writes it to `build/report/providers.md` and `.json` for `jadpo
//! approval`. The publishing bar's `reviewed` row reads every public
//! type of the package instead ([`public_review`]): any of them reaches
//! an app that installs it.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::body::Callee;
use jadpo_syntax::ast::{self, AstNode, Decl, Expr};

use crate::tree::{self, name_text, node_span};
use crate::types::{Def, Ty, TypeView, callables_of, type_table, typed_body};

/// The keys of `provider.jadpo`'s `imported:`.
pub const IMPORTED_KEYS: [&str; 3] = ["file", "digest", "reviewed"];

/// One own integration: what the app calls of it, and what those calls
/// reach.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProviderReview {
    /// `Tidepay.v2024_06_20`.
    pub package: String,
    /// `shared/providers/tidepay/v2024_06_20`.
    pub folder: String,
    /// Whether it was imported, so its types are reviewed.
    pub imported: bool,
    /// Each call the app makes, by the call as written after its
    /// connection, in name order.
    pub calls: Vec<Called>,
    /// The package's types the calls reach, by name, each reviewed or
    /// not.
    pub types: Vec<(String, bool)>,
}

impl ProviderReview {
    /// The types still to review.
    pub fn outstanding(&self) -> impl Iterator<Item = &str> {
        self.types
            .iter()
            .filter(|(_, reviewed)| !reviewed)
            .map(|(name, _)| name.as_str())
    }

    /// The calls no recording confirms.
    pub fn unconfirmed(&self) -> impl Iterator<Item = &Called> {
        self.calls.iter().filter(|call| !call.confirmed)
    }
}

/// One operation the app calls.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Called {
    /// As the app writes it: `payments.charges.create`.
    pub call: String,
    /// The connection: `payments`.
    pub connection: String,
    /// The operation: `charges.create`.
    pub operation: String,
    /// Whether `recordings/<operation>.json` confirms it.
    pub confirmed: bool,
}

/// The calls out through a connection each non-test file's bodies make:
/// the connection and `resource.operation`.
#[salsa::tracked(returns(ref))]
fn provider_calls(db: &dyn Db, project: Project, file: File) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    if file.path(db).ends_with(".test.jadpo") {
        return out;
    }
    for callable in callables_of(db, file) {
        for (_, callee) in typed_body(db, project, callable).body.calls() {
            if let Callee::Provider {
                connection,
                operation,
            } = callee
            {
                out.insert((connection.clone(), operation.clone()));
            }
        }
    }
    out
}

/// Every own integration with a `provider.jadpo`, by folder: what the app
/// calls of it, and what those calls reach.
#[salsa::tracked(returns(ref))]
pub fn review(db: &dyn Db, project: Project) -> Vec<ProviderReview> {
    let homes = crate::surface::homes(db, project);
    let connections = &crate::connections::package_connections(db, project).connections;
    let operations = crate::providers::operations(db, project);
    let table = type_table(db, project);
    let contested = jadpo_db::contested(db, project);
    let mut calls: BTreeSet<(String, String)> = BTreeSet::new();
    for &file in project.files(db) {
        calls.extend(provider_calls(db, project, file).iter().cloned());
    }
    let recordings: BTreeSet<&str> = project
        .records(db)
        .iter()
        .map(|record| record.path.as_str())
        .collect();
    let mut out = Vec::new();
    for home in homes.values().filter(|home| !home.installed) {
        let folder = &home.folder;
        let Some(provider) = provider_file(db, project, folder) else {
            continue;
        };
        let reviewed = reviewed_of(db, provider);
        // The package's own types, by the name the tables know each by.
        let own: BTreeMap<String, String> = type_names(db, project, folder)
            .into_iter()
            .map(|(name, _)| (contested.canonical(&name, Some(&home.code)), name))
            .collect();
        let mut called = Vec::new();
        let mut starts: Vec<Ty> = Vec::new();
        for connection in connections.iter().filter(|c| c.folder == *folder) {
            for (_, operation) in calls.iter().filter(|(c, _)| *c == connection.name) {
                let recording = format!("{folder}/recordings/{operation}.json");
                called.push(Called {
                    call: format!("{}.{operation}", connection.name),
                    connection: connection.name.clone(),
                    operation: operation.clone(),
                    confirmed: recordings.contains(recording.as_str()),
                });
                let Some((resource, name)) = operation.split_once('.') else {
                    continue;
                };
                if let Some(found) = operations.find(folder, resource, name) {
                    starts.extend(found.input.iter().map(|(ty, _)| ty.clone()));
                    starts.push(found.output.clone());
                    starts.extend(found.decode.iter().map(|(ty, _)| ty.clone()));
                }
            }
        }
        let types = reached(table, &own, starts)
            .into_iter()
            .map(|name| {
                let done = reviewed.names.iter().any(|(written, _)| *written == name);
                (name, done)
            })
            .collect();
        out.push(ProviderReview {
            package: home.versioned(),
            folder: folder.clone(),
            imported: reviewed.imported,
            calls: called,
            types,
        });
    }
    out
}

/// The package's own types `starts` reach, through fields, list items,
/// payload variants and refinements, by the name the package writes.
fn reached(
    table: &crate::types::TypeTable,
    own: &BTreeMap<String, String>,
    starts: Vec<Ty>,
) -> BTreeSet<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut queue = starts;
    while let Some(ty) = queue.pop() {
        match ty {
            Ty::Named(name) | Ty::Field(name, _) => {
                if !seen.insert(name.clone()) {
                    continue;
                }
                match table.def(&name) {
                    Some(Def::Fields { fields, .. }) => {
                        queue.extend(fields.iter().map(|field| field.declared.clone()));
                    }
                    Some(Def::Enum { variants }) => {
                        for variant in variants {
                            queue.extend(
                                variant
                                    .fields
                                    .iter()
                                    .flatten()
                                    .map(|field| field.declared.clone()),
                            );
                        }
                    }
                    Some(Def::Scalar { parent, .. }) => queue.push(parent.clone()),
                    _ => {}
                }
            }
            Ty::List(inner) | Ty::Set(inner) | Ty::Nullable(inner) => queue.push(*inner),
            Ty::Map(key, value) => {
                queue.push(*key);
                queue.push(*value);
            }
            _ => {}
        }
    }
    seen.into_iter()
        .filter_map(|name| own.get(&name).cloned())
        .collect()
}

/// The package in `folder`'s `provider.jadpo`.
fn provider_file(db: &dyn Db, project: Project, folder: &str) -> Option<File> {
    let path = format!("{folder}/{}", jadpo_db::PROVIDER_FILE);
    project
        .files(db)
        .iter()
        .copied()
        .find(|file| *file.path(db) == path)
}

/// The types declared in the package in `folder`'s `types/`, each with
/// where it is.
fn type_names(db: &dyn Db, project: Project, folder: &str) -> Vec<(String, Span)> {
    let prefix = format!("{folder}/types/");
    let mut out = Vec::new();
    for &file in project.files(db) {
        let path = file.path(db);
        if !path.starts_with(&prefix) {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        for decl in source.decls() {
            if let Decl::TypeDecl(decl) = decl
                && let Some(name) = decl.name().as_ref().and_then(name_text)
            {
                out.push((name, node_span(path, decl.syntax())));
            }
        }
    }
    out
}

/// `provider { }`'s member `key`.
fn provider_member(source: &ast::SourceFile, key: &str) -> Option<ast::Member> {
    source.decls().find_map(|decl| match decl {
        Decl::SingletonDecl(decl)
            if decl
                .keyword()
                .is_some_and(|keyword| keyword.text() == "provider") =>
        {
            tree::members(decl.member_list())
                .into_iter()
                .find(|(name, _)| name_text(name).as_deref() == Some(key))
                .map(|(_, member)| member)
        }
        _ => None,
    })
}

/// A package's `reviewed:` list and where it is written.
#[derive(Default)]
struct Reviewed {
    /// Whether the package gives `imported:`, which then holds the list.
    imported: bool,
    /// Each name listed and its entry.
    names: Vec<(String, ast::Expr)>,
}

/// The package's `reviewed:` list: `imported:`'s for a package that was
/// imported, `provider { }`'s own for one written by hand.
fn reviewed_of(db: &dyn Db, provider: File) -> Reviewed {
    let Some(source) = tree::source(db, provider) else {
        return Reviewed::default();
    };
    let imported = provider_member(&source, "imported");
    let list = match &imported {
        Some(imported) => tree::members(imported.block())
            .into_iter()
            .find(|(name, _)| name_text(name).as_deref() == Some(REVIEWED))
            .map(|(_, member)| member),
        None => provider_member(&source, REVIEWED),
    };
    let mut names = Vec::new();
    if let Some(Expr::ListExpr(list)) = list.and_then(|member| member.value()) {
        for entry in list.exprs() {
            names.push((tree::written(entry.syntax()), entry));
        }
    }
    Reviewed {
        imported: imported.is_some(),
        names,
    }
}

/// A provider package's public types against its `reviewed:` list, for
/// the publishing bar's `reviewed` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicReview {
    /// How many public types the package has.
    pub public: usize,
    /// Whether it was imported, so its list is `imported:`'s.
    pub imported: bool,
    /// Those not reviewed, by the name the package writes, in name order.
    pub unreviewed: Vec<String>,
    /// `publish.type_unreviewed` on each of them.
    pub problems: Vec<Diagnostic>,
}

/// Every public type in the package in `folder`'s `types/` that its
/// `reviewed:` doesn't list: what the publishing bar's `reviewed` row
/// holds a provider package to, since any of them reaches an app that
/// installs it. `None` for a package with no `provider.jadpo`, a library,
/// which has nothing to review against.
pub fn public_review(db: &dyn Db, project: Project, folder: &str) -> Option<PublicReview> {
    let provider = provider_file(db, project, folder)?;
    let reviewed = reviewed_of(db, provider);
    let place = if reviewed.imported {
        "in `imported:` in `provider.jadpo`"
    } else {
        "in `provider.jadpo`"
    };
    let prefix = format!("{folder}/types/");
    let mut public = 0;
    let mut found: Vec<(String, Diagnostic)> = Vec::new();
    for &file in project.files(db) {
        let path = file.path(db);
        if !path.starts_with(&prefix) {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        for decl in source.decls() {
            let Decl::TypeDecl(decl) = decl else { continue };
            let Some(written) = decl.name() else { continue };
            let Some(name) = name_text(&written) else {
                continue;
            };
            if decl.visibility().is_none() {
                continue;
            }
            public += 1;
            if reviewed.names.iter().any(|(listed, _)| *listed == name) {
                continue;
            }
            let problem = Diagnostic::new(
                &rule::PUBLISH_TYPE_UNREVIEWED,
                format!("Public type `{name}` isn't reviewed"),
            )
            .at(node_span(path, written.syntax()), "not in `reviewed:`")
            .reason("A provider package is published once an agent has gone through each of its public types against the provider's documentation: what each field is, which standard type fits it, and which hold personal data, which names alone can't tell. `reviewed:` records each type gone through.")
            .next(format!(
                "Go through `{name}` against the provider's documentation, then add it to `reviewed:` {place}"
            ));
            found.push((name, problem));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    let (unreviewed, problems) = found.into_iter().unzip();
    Some(PublicReview {
        public,
        imported: reviewed.imported,
        unreviewed,
        problems,
    })
}

/// `reviewed:`, the list of the package's types the agent has gone
/// through: in `imported:` for an imported package, in `provider { }`
/// itself for one written by hand.
pub const REVIEWED: &str = "reviewed";

fn invalid(message: String, span: Span, label: &str) -> Diagnostic {
    Diagnostic::new(&rule::PACKAGE_PROVIDER_INVALID, message).at(span, label)
}

/// The problems with `provider.jadpo`'s `imported:` in the package in
/// `folder`: a key it doesn't take, and its `reviewed:`'s
/// ([`reviewed_problems`]).
pub fn imported_problems(
    db: &dyn Db,
    project: Project,
    folder: &str,
    path: &str,
    member: &ast::Member,
    out: &mut Vec<Diagnostic>,
) {
    let reason = "`imported:` gives the description the package was imported from, `file:` and `digest:`, and `reviewed:`, the package's types the agent has gone through against the provider's documentation, each named once.";
    for (name, inner) in tree::members(member.block()) {
        let key = name_text(&name).unwrap_or_default();
        if !IMPORTED_KEYS.contains(&key.as_str()) {
            out.push(
                invalid(
                    format!("`imported:` has no key `{key}:`"),
                    node_span(path, name.syntax()),
                    "not one of its keys",
                )
                .reason(reason)
                .next(format!("Remove `{key}:`")),
            );
            continue;
        }
        if key == REVIEWED {
            list_problems(db, project, folder, path, &inner, reason, out);
        }
    }
}

/// The problems with a hand-written package's own `reviewed:` in
/// `provider { }`: one beside `imported:`, whose list it belongs in, and
/// a list that isn't one of the package's types once each
/// (`package.provider_invalid`).
pub fn reviewed_problems(
    db: &dyn Db,
    project: Project,
    folder: &str,
    path: &str,
    member: &ast::Member,
    imported: bool,
    out: &mut Vec<Diagnostic>,
) {
    if imported {
        out.push(
            invalid(
                "`reviewed:` is beside `imported:`".to_string(),
                node_span(path, member.syntax()),
                "outside `imported:`",
            )
            .reason("An imported package lists its reviewed types in `imported:`, beside the description they were reviewed against; `reviewed:` in `provider { }` itself is a hand-written package's.")
            .next("Move the list into `imported:` as its `reviewed:`"),
        );
        return;
    }
    let reason = "`reviewed:` lists the package's types the agent has gone through against the provider's documentation, each named once.";
    list_problems(db, project, folder, path, member, reason, out);
}

/// A `reviewed:` that isn't a list, and an entry that isn't one of the
/// package's types or is listed twice.
fn list_problems(
    db: &dyn Db,
    project: Project,
    folder: &str,
    path: &str,
    inner: &ast::Member,
    reason: &str,
    out: &mut Vec<Diagnostic>,
) {
    let Some(Expr::ListExpr(list)) = inner.value() else {
        out.push(
            invalid(
                "`reviewed:` isn't a list".to_string(),
                node_span(path, inner.syntax()),
                "not a list",
            )
            .reason(reason)
            .next("Write the reviewed types in brackets: `reviewed: [<Type>]`"),
        );
        return;
    };
    let types: Vec<String> = type_names(db, project, folder)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for entry in list.exprs() {
        let written = tree::written(entry.syntax());
        let span = node_span(path, entry.syntax());
        if !types.contains(&written) {
            let near = nearest(&written, types.iter().map(String::as_str));
            let problem = invalid(
                format!("`{written}` in `reviewed:` isn't one of the package's types"),
                span.clone(),
                "not a type of the package",
            )
            .reason(format!(
                "{reason} The package declares no type `{written}` in its `types/`."
            ));
            out.push(match near {
                Some(near) => problem
                    .next(format!("Use `{near}` instead of `{written}`"))
                    .edit(span, near),
                None => problem.next(format!("Remove `{written}` from `reviewed:`")),
            });
            continue;
        }
        if !seen.insert(written.clone()) {
            out.push(
                invalid(
                    format!("`{written}` is in `reviewed:` twice"),
                    span,
                    "listed again",
                )
                .reason(reason)
                .next(format!("Remove the second `{written}`")),
            );
        }
    }
}

/// The provider report, `build/report/providers.md` and its `.json` twin,
/// for `jadpo approval` to show what ships unconfirmed or unreviewed:
/// each own integration's calls, confirmed or not, and the types they
/// reach, reviewed or not. `None` when the app has no
/// integration of its own.
pub fn report(db: &dyn Db, project: Project) -> Option<(String, jadpo_graph::json::Value)> {
    use jadpo_graph::json::Value;
    let reviews = review(db, project);
    if reviews.is_empty() {
        return None;
    }
    let mut markdown = String::from(
        "# Providers\n\n\
         The operations the app calls through its own integrations, whether a recording \
         confirms each, and the package's types they reach, reviewed or not. Written by \
         `jadpo check`.\n",
    );
    let mut packages = Vec::new();
    for review in reviews {
        markdown.push_str(&format!("\n## `{}`\n\n", review.package));
        if review.calls.is_empty() {
            markdown.push_str("The app calls none of its operations.\n");
        } else {
            markdown.push_str("| Call | Confirmed |\n|---|---|\n");
            for call in &review.calls {
                markdown.push_str(&format!(
                    "| `{}` | {} |\n",
                    call.call,
                    if call.confirmed { "yes" } else { "no" }
                ));
            }
        }
        if !review.types.is_empty() {
            markdown.push_str("\n| Type | Reviewed |\n|---|---|\n");
            for (name, reviewed) in &review.types {
                markdown.push_str(&format!(
                    "| `{name}` | {} |\n",
                    if *reviewed { "yes" } else { "no" }
                ));
            }
        }
        let mut entry = Value::object();
        entry.set("package", review.package.as_str());
        entry.set("folder", review.folder.as_str());
        entry.set("imported", review.imported);
        entry.set(
            "calls",
            review
                .calls
                .iter()
                .map(|call| {
                    let mut item = Value::object();
                    item.set("call", call.call.as_str());
                    item.set("connection", call.connection.as_str());
                    item.set("operation", call.operation.as_str());
                    item.set("confirmed", call.confirmed);
                    item
                })
                .collect::<Vec<_>>(),
        );
        entry.set(
            "types",
            review
                .types
                .iter()
                .map(|(name, reviewed)| {
                    let mut item = Value::object();
                    item.set("name", name.as_str());
                    item.set("reviewed", *reviewed);
                    item
                })
                .collect::<Vec<_>>(),
        );
        packages.push(entry);
    }
    let mut json = Value::object();
    json.set("providers", packages);
    Some((markdown, json))
}

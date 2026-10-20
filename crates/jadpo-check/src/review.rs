//! The scoped review of an imported package and the operations still
//! unconfirmed (area 11, "Mapping imported types to standard types",
//! layers 4 and 5; chunk 7.2e2).
//!
//! For each of the app's own integrations in `shared/providers/`: the
//! operations the app calls through its connections, each confirmed or
//! not by a recording in the package's `recordings/`; and, for one that
//! was imported (its `provider.jadpo` gives `imported:`), the package's
//! own types those operations reach, each reviewed or not by the
//! agent's `reviewed:` list in `imported:`. The scope grows as the app
//! calls something new, since it is worked out from the calls each time.
//! `jadpo check` and the import's summary show it, and `jadpo check`
//! writes it to `build/report/providers.md` and `.json` for `jadpo
//! approval`.

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
    /// not; empty for a package that wasn't imported.
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
        let reviewed = imported_of(db, provider);
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
        let types = match &reviewed {
            Some(reviewed) => reached(table, &own, starts)
                .into_iter()
                .map(|name| {
                    let done = reviewed.iter().any(|(written, _)| *written == name);
                    (name, done)
                })
                .collect(),
            None => Vec::new(),
        };
        out.push(ProviderReview {
            package: home.versioned(),
            folder: folder.clone(),
            imported: reviewed.is_some(),
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

/// `imported:`'s `reviewed:` list, each name and its entry; `None` when
/// the package gives no `imported:`.
fn imported_of(db: &dyn Db, provider: File) -> Option<Vec<(String, ast::Expr)>> {
    let source = tree::source(db, provider)?;
    let imported = provider_member(&source, "imported")?;
    let mut out = Vec::new();
    for (name, member) in tree::members(imported.block()) {
        if name_text(&name).as_deref() != Some("reviewed") {
            continue;
        }
        if let Some(Expr::ListExpr(list)) = member.value() {
            for entry in list.exprs() {
                out.push((tree::written(entry.syntax()), entry));
            }
        }
    }
    Some(out)
}

/// The problems with `provider.jadpo`'s `imported:` in the package in
/// `folder`: a key it doesn't take, a `reviewed:` that isn't a list, and
/// an entry that isn't one of the package's types or is listed twice
/// (`package.provider_invalid`).
pub fn imported_problems(
    db: &dyn Db,
    project: Project,
    folder: &str,
    path: &str,
    member: &ast::Member,
    out: &mut Vec<Diagnostic>,
) {
    let invalid = |message: String, span: Span, label: &str| -> Diagnostic {
        Diagnostic::new(&rule::PACKAGE_PROVIDER_INVALID, message).at(span, label)
    };
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
        if key != "reviewed" {
            continue;
        }
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
            continue;
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
}

/// The provider report, `build/report/providers.md` and its `.json` twin,
/// for `jadpo approval` to show what ships unconfirmed or unreviewed:
/// each own integration's calls, confirmed or not, and the imported
/// types they reach, reviewed or not. `None` when the app has no
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
         confirms each, and the imported types they reach, reviewed or not. Written by \
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
        if review.imported && !review.types.is_empty() {
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

//! The publishing bar (area 11, "What publishing requires"; chunk 7.1b):
//! what `jadpo package check` holds a package to before it is tagged, on
//! top of compiling with no errors. Every public type, failure, library
//! and library function has a `///` description ([`undescribed`]), and the
//! version agrees with how the surface changed since the last release
//! ([`version`]): a surface that only adds is a minor release, one whose
//! descriptions or files alone moved is a patch, and one that removes or
//! changes what an app may rely on is refused within its major version, as
//! Elm's package manager refuses it.
//!
//! The bar applies only to what is published: the app's own integrations
//! and libraries skip it.

use jadpo_db::record::{self as format, Entry};
use jadpo_db::{Db, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, rule};
use jadpo_syntax::ast::{AstNode, Decl};

use crate::graph::text::doc;
use crate::tree;

/// Each public declaration of the package in `folder` that has no `///`
/// description: its types, failures, libraries and each function of a
/// public library. In path order, then by position.
pub fn undescribed(db: &dyn Db, project: Project, folder: &str) -> Vec<Diagnostic> {
    let prefix = format!("{folder}/");
    let mut out = Vec::new();
    for &file in project.files(db) {
        let path = file.path(db);
        if !path.starts_with(&prefix) || path.ends_with(".test.jadpo") {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        for item in &item_tree(db, file).items {
            if !item.public {
                continue;
            }
            let Some(decl) = tree::decl_at(&source, item.index) else {
                continue;
            };
            let what = match &item.kind {
                ItemKind::Type(base) if *base == jadpo_db::Base::Failure => "Public failure",
                ItemKind::Type(_) => "Public type",
                ItemKind::Library => "Public library",
                _ => continue,
            };
            if doc(decl.syntax()).is_none() {
                out.push(missing(
                    tree::name_span(&source, path, item.index, None),
                    format!("{what} `{}` has no description", item.name),
                    &format!("public {} {}", keyword(&item.kind), item.name),
                ));
            }
            let Decl::LibraryDecl(library) = &decl else {
                continue;
            };
            let members = tree::library_members(library);
            for (i, function) in item.functions.iter().enumerate() {
                if function.kind != ItemKind::Function {
                    continue;
                }
                let Some(member) = members.get(i) else {
                    continue;
                };
                if doc(member.syntax()).is_none() {
                    out.push(missing(
                        tree::name_span(&source, path, item.index, Some(i as u32)),
                        format!(
                            "Function `{}.{}` has no description",
                            item.name, function.name
                        ),
                        &format!("function {}", function.name),
                    ));
                }
            }
        }
    }
    out.sort_by(|a, b| {
        let at = |d: &Diagnostic| {
            d.primary
                .as_ref()
                .map(|label| (label.span.path.clone(), label.span.start))
        };
        at(a).cmp(&at(b))
    });
    out
}

fn keyword(kind: &ItemKind) -> &'static str {
    match kind {
        ItemKind::Library => "library",
        _ => "type",
    }
}

fn missing(span: jadpo_diag::Span, message: String, above: &str) -> Diagnostic {
    Diagnostic::new(&rule::PUBLISH_DESCRIPTION_MISSING, message)
        .at(span, "no `///` description")
        .reason("Every public type, failure, library and library function of a published package has a `///` description: it is what `jadpo package show` and the registry's page say of it, and what an agent reads before it opens the file.")
        .next(format!("Add a `///` line above `{above}` saying what it is for"))
}

/// The version a package's surface takes, and what changed since the last
/// release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bump {
    /// `2.1.0`.
    pub version: String,
    /// The last release's version, when there is one.
    pub since: Option<String>,
    /// What was added or changed without breaking an app, as the change's
    /// word and the entry's heading: `("added", "type OutwardCode")`.
    pub changes: Vec<(String, String)>,
}

/// The version the surface `now` takes, given the package's version label
/// (`v2`, `v2024_06_20`) and the last release's version and surface. A
/// first release is the major version its label names (`v2` is 2.0.0, `v0`
/// is 0.1.0, a dated label 1.0.0); after one, only new entries is the next
/// minor version and nothing but descriptions and files the next patch.
/// What would break an app that uses the last release (an entry removed,
/// or one whose facts changed: a new failure, a field retyped or added) is
/// refused, each as `("removed" | "changed", heading)`.
pub fn version(
    label: &str,
    previous: Option<(&str, &str)>,
    now: &str,
) -> Result<Bump, Vec<(String, String)>> {
    let Some((since, before)) = previous else {
        let digits = label.strip_prefix('v').unwrap_or(label);
        let version = match digits.parse::<u64>() {
            Ok(0) => "0.1.0".to_string(),
            Ok(major) => format!("{major}.0.0"),
            Err(_) => "1.0.0".to_string(),
        };
        return Ok(Bump {
            version,
            since: None,
            changes: Vec::new(),
        });
    };
    let read =
        |text: &str| format::read(&format::normalise(text), &format::SURFACE).unwrap_or_default();
    let (before, now) = (read(before), read(now));
    let mut breaking = Vec::new();
    let mut additions = Vec::new();
    for entry in &now {
        match before.iter().find(|old| old.heading == entry.heading) {
            None => additions.push(("added".to_string(), entry.heading.clone())),
            Some(old) => {
                let (old_facts, new_facts) = (significant(old), significant(entry));
                if old_facts == new_facts {
                    continue;
                }
                let licence = |facts: &[String]| {
                    facts
                        .iter()
                        .find(|fact| fact.starts_with("licence "))
                        .cloned()
                };
                let package = entry.heading.starts_with("package ");
                if package && licence(&old_facts) == licence(&new_facts) {
                    additions.push(("changed".to_string(), entry.heading.clone()));
                } else {
                    breaking.push(("changed".to_string(), entry.heading.clone()));
                }
            }
        }
    }
    for old in &before {
        if !now.iter().any(|entry| entry.heading == old.heading) {
            breaking.push(("removed".to_string(), old.heading.clone()));
        }
    }
    // A type's heading names its base too; a change says only what it is.
    let named = |changes: Vec<(String, String)>| -> Vec<(String, String)> {
        changes
            .into_iter()
            .map(|(word, heading)| {
                let name = heading.split(" = ").next().unwrap_or(&heading).to_string();
                (word, name)
            })
            .collect()
    };
    if !breaking.is_empty() {
        return Err(named(breaking));
    }
    let additions = named(additions);
    let parts: Vec<u64> = since
        .split(['-', '+'])
        .next()
        .unwrap_or(since)
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect();
    let (major, minor, patch) = (
        parts.first().copied().unwrap_or(0),
        parts.get(1).copied().unwrap_or(0),
        parts.get(2).copied().unwrap_or(0),
    );
    let version = if additions.is_empty() {
        format!("{major}.{minor}.{}", patch + 1)
    } else {
        format!("{major}.{}.0", minor + 1)
    };
    Ok(Bump {
        version,
        since: Some(since.to_string()),
        changes: additions,
    })
}

/// An entry's lines an app relies on: everything but its description, its
/// file, where it is confirmed from and, for the package, its version and
/// guide.
fn significant(entry: &Entry) -> Vec<String> {
    const LOOSE: [&str; 6] = [
        "///",
        "file ",
        "confirmed ",
        "source ",
        "version ",
        "guide ",
    ];
    let mut out = Vec::new();
    for member in &entry.members {
        if LOOSE.iter().any(|loose| member.line.starts_with(loose)) {
            continue;
        }
        out.push(member.line.clone());
        out.extend(member.nested.iter().map(|nested| format!("  {nested}")));
    }
    out
}

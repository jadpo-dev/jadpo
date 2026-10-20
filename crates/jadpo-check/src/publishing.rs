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

/// How a package's surface changed from `before` to `now`, entry by
/// entry, as an upgrade's review lists it (area 18, "Package upgrades"):
/// `("added" | "changed" | "removed", heading)` in the record's order, a
/// type's heading without its base, and the package's own entry left out.
/// Descriptions, files and versions alone aren't changes.
pub fn changes(before: &str, now: &str) -> Vec<(String, String)> {
    let read =
        |text: &str| format::read(&format::normalise(text), &format::SURFACE).unwrap_or_default();
    let (before, now) = (read(before), read(now));
    let name = |heading: &str| heading.split(" = ").next().unwrap_or(heading).to_string();
    let mut out = Vec::new();
    for entry in &now {
        if entry.heading.starts_with("package ") {
            continue;
        }
        match before
            .iter()
            .find(|old| name(&old.heading) == name(&entry.heading))
        {
            None => out.push(("added".to_string(), name(&entry.heading))),
            Some(old) if significant(old) != significant(entry) || old.heading != entry.heading => {
                out.push(("changed".to_string(), name(&entry.heading)));
            }
            Some(_) => {}
        }
    }
    for old in &before {
        if !old.heading.starts_with("package ")
            && !now
                .iter()
                .any(|entry| name(&entry.heading) == name(&old.heading))
        {
            out.push(("removed".to_string(), name(&old.heading)));
        }
    }
    out
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

/// An example in a public declaration's description (area 11, question
/// 27): a fenced block from ```` ```jadpo ```` to ```` ``` ````, holding
/// statements as a test's body does, run as a test of the package by
/// `jadpo package check`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Example {
    /// The declaration as the app writes it after the package's name:
    /// `uk.parse`, `UkPostcode`.
    pub declaration: String,
    /// 1 for the first example in its description.
    pub number: usize,
    /// Each statement's line, from 1, in the declaration's file, and its
    /// text without the `///` and the one space after it.
    pub lines: Vec<(u32, String)>,
}

impl Example {
    /// The test it runs as: `example of uk.parse`, then `example 2 of
    /// uk.parse`.
    pub fn name(&self) -> String {
        if self.number == 1 {
            format!("example of {}", self.declaration)
        } else {
            format!("example {} of {}", self.number, self.declaration)
        }
    }
}

/// The examples in the description above line `line` (from 1) of `text`,
/// the declaration `declaration`: its `///` lines, read upwards past any
/// `decline` lines between them and the declaration.
pub fn examples(text: &str, declaration: &str, line: u32) -> Vec<Example> {
    let lines: Vec<&str> = text.lines().collect();
    let mut doc: Vec<(u32, &str)> = Vec::new();
    let mut at = line as usize;
    while at > 1 {
        at -= 1;
        let trimmed = lines.get(at - 1).map_or("", |l| l.trim_start());
        if let Some(rest) = trimmed.strip_prefix("///") {
            doc.push((at as u32, rest.strip_prefix(' ').unwrap_or(rest)));
        } else if trimmed.starts_with("decline ") && doc.is_empty() {
            continue;
        } else {
            break;
        }
    }
    doc.reverse();
    let mut out: Vec<Example> = Vec::new();
    let mut open: Option<Vec<(u32, String)>> = None;
    for (number, content) in doc {
        let fence = content.trim();
        match &mut open {
            None if fence.starts_with("```") => {
                if fence.trim_start_matches('`').trim() == "jadpo" {
                    open = Some(Vec::new());
                }
            }
            None => {}
            Some(_) if fence == "```" => {
                let body = open.take().unwrap_or_default();
                if !body.is_empty() {
                    out.push(Example {
                        declaration: declaration.to_string(),
                        number: out.len() + 1,
                        lines: body,
                    });
                }
            }
            Some(body) => body.push((number, content.to_string())),
        }
    }
    out
}

/// Whether one of `texts` (examples and test files) asserts that a call
/// to `call` (`uk.parse`) fails with `failure`: a line holding `call(`
/// and `fails <failure>`.
pub fn shows_failure(texts: &[&str], call: &str, failure: &str) -> bool {
    let opening = format!("{call}(");
    let fails = format!("fails {failure}");
    texts.iter().any(|text| {
        text.lines().any(|line| {
            line.contains(&opening)
                && line.match_indices(&fails).any(|(at, _)| {
                    !line[at + fails.len()..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
                })
        })
    })
}

/// The heading of the changelog's file, a package's `CHANGELOG.md`.
pub const CHANGELOG: &str = "CHANGELOG.md";

/// A package's changelog with its newest version's section written
/// (area 11, question 28).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changelog {
    pub text: String,
    /// Whether the section has the author's text beneath the list.
    pub why: bool,
    /// The section heading's line, from 1.
    pub line: u32,
}

/// One change of a version, as its changelog lists it: `added type
/// \`OutwardCode\``, from `("added", "type OutwardCode")`.
pub fn change_item(word: &str, heading: &str) -> String {
    match heading.split_once(' ') {
        Some((kind, name)) => format!("{word} {kind} `{name}`"),
        None => format!("{word} `{heading}`"),
    }
}

/// `old` (the changelog as it is, if there is one) with a `## <version>`
/// section at the top holding `changes` as a list, and the author's text
/// beneath it kept: the top section is rewritten while its version isn't
/// one of `released`, so a change made before the tag rewrites the list;
/// otherwise a new section goes above it. Released sections are kept as
/// they are.
pub fn changelog(
    old: Option<&str>,
    version: &str,
    changes: &[String],
    released: &[String],
) -> Changelog {
    let old = old.unwrap_or("").replace("\r\n", "\n");
    let lines: Vec<&str> = old.lines().collect();
    let first = lines.iter().position(|line| line.starts_with("## "));
    let mut preamble: Vec<String> = lines[..first.unwrap_or(lines.len())]
        .iter()
        .map(|line| line.to_string())
        .collect();
    while preamble.last().is_some_and(|line| line.trim().is_empty()) {
        preamble.pop();
    }
    if preamble.is_empty() {
        preamble.push("# Changelog".to_string());
    }
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    for line in lines.iter().skip(first.unwrap_or(lines.len())) {
        match line.strip_prefix("## ") {
            Some(heading) => sections.push((heading.trim().to_string(), Vec::new())),
            None => {
                if let Some((_, body)) = sections.last_mut() {
                    body.push(line.to_string());
                }
            }
        }
    }
    let replace = sections.first().is_some_and(|(heading, _)| {
        heading == version || !released.iter().any(|released| released == heading)
    });
    let why: Vec<String> = if replace {
        let (_, body) = sections.remove(0);
        why_text(&body)
    } else {
        Vec::new()
    };
    let mut body: Vec<String> = vec![String::new()];
    body.extend(changes.iter().map(|change| format!("- {change}")));
    if !why.is_empty() {
        body.push(String::new());
        body.extend(why.iter().cloned());
    }
    sections.insert(0, (version.to_string(), body));
    let mut out: Vec<String> = preamble;
    let mut line = 0;
    for (index, (heading, body)) in sections.iter().enumerate() {
        out.push(String::new());
        if index == 0 {
            line = out.len() + 1;
        }
        out.push(format!("## {heading}"));
        let mut body = body.clone();
        while body.last().is_some_and(|line| line.trim().is_empty()) {
            body.pop();
        }
        out.extend(body);
    }
    Changelog {
        text: format!("{}\n", out.join("\n")),
        why: why.iter().any(|line| !line.trim().is_empty()),
        line: line as u32,
    }
}

/// Whether the section for `version` in `text` has the author's text
/// beneath its list.
pub fn says_why(text: &str, version: &str) -> bool {
    let mut body: Vec<String> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            if inside {
                break;
            }
            inside = heading.trim() == version;
            continue;
        }
        if inside {
            body.push(line.to_string());
        }
    }
    why_text(&body).iter().any(|line| !line.trim().is_empty())
}

/// A section's text beneath the list Jadpo writes, the first list under
/// its heading.
fn why_text(body: &[String]) -> Vec<String> {
    let mut rest = body
        .iter()
        .skip_while(|line| line.trim().is_empty())
        .peekable();
    while rest.peek().is_some_and(|line| line.starts_with("- ")) {
        rest.next();
    }
    let mut why: Vec<String> = rest
        .skip_while(|line| line.trim().is_empty())
        .cloned()
        .collect();
    while why.last().is_some_and(|line| line.trim().is_empty()) {
        why.pop();
    }
    why
}

#[cfg(test)]
mod tests {
    use super::*;

    const UK: &str = "public library uk {
    /// Reads a postcode.
    ///
    /// ```jadpo
    /// var postcode: UkPostcode = attempt uk.parse(\"sw1a 1aa\")
    /// assert postcode == \"SW1A 1AA\"
    /// ```
    ///
    /// ```text
    /// not run
    /// ```
    ///
    /// ```jadpo
    /// assert uk.parse(\"x\") fails InvalidPostcode
    /// ```
    function parse(written: Text) fails InvalidPostcode -> UkPostcode {
";

    #[test]
    fn examples_are_the_jadpo_fences_in_a_description() {
        let found = examples(UK, "uk.parse", 16);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name(), "example of uk.parse");
        assert_eq!(
            found[0].lines,
            vec![
                (
                    5,
                    "var postcode: UkPostcode = attempt uk.parse(\"sw1a 1aa\")".to_string()
                ),
                (6, "assert postcode == \"SW1A 1AA\"".to_string()),
            ]
        );
        assert_eq!(found[1].name(), "example 2 of uk.parse");
        assert_eq!(found[1].lines[0].0, 14);
        assert!(shows_failure(&[UK], "uk.parse", "InvalidPostcode"));
        assert!(!shows_failure(&[UK], "uk.parse", "Invalid"));
        assert!(!shows_failure(&[UK], "uk.tidy", "InvalidPostcode"));
    }

    #[test]
    fn the_changelog_keeps_the_authors_why_beneath_the_list() {
        let first = changelog(None, "2.0.0", &["first release".into()], &[]);
        assert_eq!(first.text, "# Changelog\n\n## 2.0.0\n\n- first release\n");
        assert!(!first.why);
        assert_eq!(first.line, 3);
        let written = format!("{}\nWhy it exists.\n", first.text);
        assert!(says_why(&written, "2.0.0"));
        let again = changelog(Some(&written), "2.0.0", &["first release".into()], &[]);
        assert_eq!(again.text, written);
        assert!(again.why);
        let next = changelog(
            Some(&written),
            "2.1.0",
            &[change_item("added", "type OutwardCode")],
            &["2.0.0".into()],
        );
        assert_eq!(
            next.text,
            "# Changelog\n\n## 2.1.0\n\n- added type `OutwardCode`\n\n## 2.0.0\n\n- first release\n\nWhy it exists.\n"
        );
        assert!(!next.why);
        // Before the tag, a patch that became a minor version is rewritten.
        let patch = changelog(Some(&written), "2.0.1", &[], &["2.0.0".into()]);
        let minor = changelog(
            Some(
                &patch
                    .text
                    .replace("## 2.0.1\n", "## 2.0.1\n\nFixes a description.\n"),
            ),
            "2.1.0",
            &[change_item("added", "type OutwardCode")],
            &["2.0.0".into()],
        );
        assert!(minor.text.starts_with("# Changelog\n\n## 2.1.0\n\n- added type `OutwardCode`\n\nFixes a description.\n\n## 2.0.0\n"), "{}", minor.text);
        assert!(minor.why);
    }
}

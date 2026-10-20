//! `jadpo package import openapi <file>` (area 11, "Where packages come
//! from"; chunk 7.2): a published description read once, offline, into
//! the app's own integration in `shared/providers/<name>/<version>/`, in
//! the package format.
//!
//! The screen, designed from the CLI design language: a status line
//! saying what was written where, with the package's name and what it
//! holds counted on the grid; the changes a second import made, when it
//! merged into files already there; then what is left to check, grouped
//! by what to do, each group's operations and fields listed under it,
//! for the agent; then the footer and the one next command, `jadpo
//! check`. `--json` gives the files, the counts and every finding.

mod merge;
mod model;
mod notation;
mod openapi;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Block, ChangeMark, Fact, Json, Outcome, Screen};

use crate::fmt::project_not_found;
use crate::run::{Call, Reply};

/// What `jadpo package` does.
pub const ACTIONS: &[crate::table::Value] = &[crate::table::Value {
    name: "import",
    description: "Read a published description into the app's own integration",
}];

/// The formats `jadpo package import` takes.
pub const FORMATS: &[crate::table::Value] = &[crate::table::Value {
    name: "openapi",
    description: "An OpenAPI 3.0 or 3.1 document, as JSON",
}];

/// The formats area 11 decides that aren't built yet.
const PLANNED: [&str; 4] = ["smithy", "postman", "har", "dts"];

/// The width findings are wrapped to, inside the screen's gutter.
const WIDTH: usize = 76;

pub fn package(call: &Call) -> Reply {
    match call.arguments.first().map(String::as_str) {
        Some("import") => import(call),
        Some(other) => Reply::failed(arguments(
            format!("`{other}` isn't something `jadpo package` does"),
            "`jadpo package import openapi <file>` reads an OpenAPI document into the app's own integration.",
        )),
        None => Reply::failed(arguments(
            "`jadpo package` needs to know what to do".to_string(),
            "`jadpo package import openapi <file>` reads an OpenAPI document into the app's own integration.",
        )),
    }
}

fn arguments(message: String, reason: &str) -> Diagnostic {
    Diagnostic::new(&rule::CLI_PACKAGE_ARGUMENTS, message)
        .reason(reason)
        .next("Run `jadpo package import openapi <file>`")
}

fn import(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let (Some(format), Some(file)) = (call.arguments.get(1), call.arguments.get(2)) else {
        return Reply::failed(arguments(
            "`jadpo package import` needs a format and a file".to_string(),
            "It reads a published description once: `jadpo package import openapi <file>`.",
        ));
    };
    if let Some(extra) = call.arguments.get(3) {
        return Reply::failed(arguments(
            format!("`jadpo package import` takes one file, not `{extra}` as well"),
            "It reads one description into one package.",
        ));
    }
    if PLANNED.contains(&format.as_str()) {
        return Reply::failed(
            Diagnostic::new(
                &rule::TOOLCHAIN_NOT_BUILT_YET,
                format!("Importing `{format}` isn't built yet"),
            )
            .reason("Area 11 decides importers for Smithy, Postman collections, HAR captures and TypeScript definitions; this compiler reads OpenAPI only.")
            .next("Import the provider's OpenAPI document, or write the package from its documentation"),
        );
    }
    if format != "openapi" {
        return Reply::failed(arguments(
            format!("`{format}` isn't a format `jadpo package import` reads"),
            "It reads `openapi`: an OpenAPI 3.0 or 3.1 document, as JSON.",
        ));
    }
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_not_found("package import");
    };
    let path = cwd.join(file);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::CLI_PACKAGE_UNREADABLE,
                    format!("`{file}` can't be read"),
                )
                .reason(format!("Reading it failed: {error}."))
                .next("Give the path of the OpenAPI document, from this folder"),
            );
        }
    };
    let document: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(document) => document,
        Err(error) => {
            let yaml = !bytes.trim_ascii_start().starts_with(b"{");
            return Reply::failed(
                Diagnostic::new(
                    &rule::CLI_PACKAGE_UNREADABLE,
                    format!("`{file}` isn't JSON"),
                )
                .reason(if yaml {
                    "The importer reads an OpenAPI document as JSON; YAML isn't read yet.".to_string()
                } else {
                    format!("The importer reads an OpenAPI document as JSON, and this one stops at line {}: {error}.", error.line())
                })
                .next(if yaml {
                    "Convert the document to JSON, then import that"
                } else {
                    "Give the OpenAPI document as JSON"
                }),
            );
        }
    };
    let (name, version) = match call.value("--as") {
        Some(written) => match named(written) {
            Some(named) => named,
            None => {
                return Reply::failed(
                    Diagnostic::new(
                        &rule::CLI_PACKAGE_ARGUMENTS,
                        format!("`{written}` isn't a package's name and version"),
                    )
                    .reason("A package is named in `UpperCamelCase` with its version: `Tidepool.v1`, `Stripe.v2024_06_20`.")
                    .next("Give `--as` a name and version, such as `--as Tidepool.v1`"),
                );
            }
        },
        None => derived(&document),
    };
    let shown_file = relative(&root, &path);
    let digest = digest(&bytes);
    let package = match openapi::read(&document, &name, &version, &shown_file, &digest) {
        Ok(package) => package,
        Err(openapi::Invalid(why)) => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::CLI_PACKAGE_NOT_OPENAPI,
                    format!("`{file}` isn't an OpenAPI document the importer reads"),
                )
                .reason(format!(
                    "The importer reads OpenAPI 3.0 and 3.1, and {why}."
                ))
                .next("Give the provider's OpenAPI 3 document"),
            );
        }
    };
    let folder = package.folder();
    let mut written: Vec<(String, &'static str)> = Vec::new();
    let mut added: Vec<String> = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    for (path, text) in package.files() {
        let full = root.join(&folder).join(&path);
        let text = jadpo_fmt::format_file(&text).unwrap_or(text);
        let (text, change) = match std::fs::read_to_string(&full) {
            Ok(existing) => match merge::merge(&existing, &text) {
                Some(merged) if merged.text == existing => (existing, "unchanged"),
                Some(merged) => {
                    added.extend(merged.added);
                    kept.extend(merged.kept);
                    (merged.text, "merged")
                }
                // A file that doesn't read is left as it is.
                None => (existing, "unreadable"),
            },
            Err(_) => (text, "written"),
        };
        if matches!(change, "written" | "merged")
            && let Err(error) = save(&full, &text)
        {
            return Reply::failed(
                Diagnostic::new(
                    &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                    format!("`{folder}/{path}` couldn't be written"),
                )
                .reason(format!("Writing it failed: {error}."))
                .next("Check that the folder can be written, then import again"),
            );
        }
        written.push((format!("{folder}/{path}"), change));
    }
    let unreadable: Vec<&String> = written
        .iter()
        .filter(|(_, change)| *change == "unreadable")
        .map(|(path, _)| path)
        .collect();
    let mut findings = package.findings.clone();
    for path in &unreadable {
        findings.push(model::Finding {
            at: (*path).clone(),
            next: "Left as it is, since it doesn't read as Jadpo: fix it, then import again to merge into it".to_string(),
        });
    }
    report(
        &package,
        &shown_file,
        &folder,
        &written,
        &added,
        &kept,
        &findings,
    )
}

fn report(
    package: &model::Package,
    file: &str,
    folder: &str,
    written: &[(String, &'static str)],
    added: &[String],
    kept: &[String],
    findings: &[model::Finding],
) -> Reply {
    let first = written.iter().all(|(_, change)| *change == "written");
    let operations: usize = package
        .resources
        .values()
        .map(|resource| resource.operations.len())
        .sum();
    let changing = package
        .resources
        .values()
        .flat_map(|resource| resource.operations.values())
        .filter(|operation| operation.changes)
        .count();
    let resources: Vec<&str> = package.resources.keys().map(String::as_str).collect();
    let mut rows = vec![
        (
            "package".to_string(),
            format!("{}.{}", package.name, package.version),
        ),
        (
            "read".to_string(),
            if package.exact {
                "exactly, from its x-jadpo fields".to_string()
            } else {
                "from its schemas".to_string()
            },
        ),
    ];
    if !resources.is_empty() {
        rows.push(("resources".to_string(), resources.join(", ")));
    }
    rows.push((
        "operations".to_string(),
        if changing == 0 {
            operations.to_string()
        } else {
            format!(
                "{operations}, {changing} {} something",
                if changing == 1 { "changes" } else { "change" }
            )
        },
    ));
    rows.push(("types".to_string(), package.types.len().to_string()));
    if !package.failures.is_empty() {
        rows.push(("failures".to_string(), package.failures.len().to_string()));
    }
    let mut screen = Screen::new();
    screen.push(Block::Status {
        outcome: Outcome::Success,
        text: format!(
            "{} {file} into {folder}",
            if first { "Imported" } else { "Merged" }
        ),
        rows,
    });
    if !added.is_empty() || !kept.is_empty() {
        let mut groups = Vec::new();
        if !added.is_empty() {
            groups.push((
                "Added".to_string(),
                added
                    .iter()
                    .map(|what| (ChangeMark::Added, vec![what.clone()]))
                    .collect(),
            ));
        }
        if !kept.is_empty() {
            groups.push((
                "Kept, not in the description".to_string(),
                kept.iter()
                    .map(|what| (ChangeMark::Changed, vec![what.clone()]))
                    .collect(),
            ));
        }
        screen.push(Block::Changes {
            note: Some("compared with the files already there; corrections were kept".to_string()),
            groups,
        });
    }
    // What is left to check, grouped by what to do.
    let mut grouped: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut order: Vec<&str> = Vec::new();
    for finding in findings {
        if !grouped.contains_key(finding.next.as_str()) {
            order.push(&finding.next);
        }
        grouped.entry(&finding.next).or_default().push(&finding.at);
    }
    if !order.is_empty() {
        let mut lines = Vec::new();
        for (i, next) in order.iter().enumerate() {
            if i > 0 {
                lines.push(String::new());
            }
            lines.extend(wrap(&format!("{next} (agent)"), WIDTH));
            lines.extend(
                wrap(&grouped[next].join(", "), WIDTH - 2)
                    .into_iter()
                    .map(|line| format!("  {line}")),
            );
        }
        screen.push(Block::Section {
            heading: "To check".to_string(),
            lines,
        });
    }
    let count =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut facts = vec![
        Fact::plain(count(operations, "operation", "operations")),
        Fact::plain(count(package.types.len(), "type", "types")),
    ];
    if !package.failures.is_empty() {
        facts.push(Fact::plain(count(
            package.failures.len(),
            "failure",
            "failures",
        )));
    }
    if !findings.is_empty() {
        facts.push(Fact::plain(format!("{} to check", findings.len())));
    }
    screen.push(Block::summary(
        Outcome::Success,
        if first { "imported" } else { "merged" },
        facts,
    ));
    screen.push(Block::hint(
        "jadpo check",
        "checks the package with the app",
    ));
    let data = Json::object([
        (
            "package",
            Json::from(format!("{}.{}", package.name, package.version)),
        ),
        ("folder", Json::from(folder)),
        ("exact", Json::from(package.exact)),
        (
            "files",
            Json::array(written.iter().map(|(path, change)| {
                Json::object([
                    ("path", Json::from(path.as_str())),
                    ("change", Json::from(*change)),
                ])
            })),
        ),
        ("operations", Json::from(operations)),
        ("types", Json::from(package.types.len())),
        ("failures", Json::from(package.failures.len())),
        (
            "added",
            Json::array(added.iter().map(|what| Json::from(what.as_str()))),
        ),
        (
            "kept",
            Json::array(kept.iter().map(|what| Json::from(what.as_str()))),
        ),
        (
            "findings",
            Json::array(findings.iter().map(|finding| {
                Json::object([
                    ("at", Json::from(finding.at.as_str())),
                    ("next", Json::from(finding.next.as_str())),
                    ("decides", Json::from("agent")),
                ])
            })),
        ),
    ]);
    Reply::screen(screen, data)
}

/// `Tidepool.v1` as its name and version.
fn named(written: &str) -> Option<(String, String)> {
    let (name, version) = written.split_once('.')?;
    let name_ok = name.starts_with(|c: char| c.is_ascii_uppercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric());
    let version_ok = version.strip_prefix('v').is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .split('_')
                .all(|group| !group.is_empty() && group.chars().all(|c| c.is_ascii_digit()))
    });
    (name_ok && version_ok).then(|| (name.to_string(), version.to_string()))
}

/// The name and version a document gives: its title in `UpperCamelCase`,
/// and its version's major number, or its date as `v2024_06_20`; `v1`
/// when it gives neither, as a Jadpo application's, whose version is a
/// hash of its record.
fn derived(document: &serde_json::Value) -> (String, String) {
    let title = document
        .pointer("/info/title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Provider");
    let mut name = model::camel(title);
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        name = format!("Provider{name}");
    }
    let version = document
        .pointer("/info/version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let version = version.strip_prefix('v').unwrap_or(version);
    let parts: Vec<&str> = version.split(['-', '.']).collect();
    let digits = |part: &&str| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit());
    let label = match parts.as_slice() {
        [year, month, day] if year.len() == 4 && parts.iter().all(digits) => {
            format!("v{year}_{month}_{day}")
        }
        [major, ..] if digits(major) => {
            let major = major.trim_start_matches('0');
            format!("v{}", if major.is_empty() { "0" } else { major })
        }
        _ => "v1".to_string(),
    };
    (name, label)
}

/// `path` from the project's root when it is inside it, else as given.
fn relative(root: &Path, path: &Path) -> String {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    path.strip_prefix(&root)
        .map(|inside| inside.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
}

/// `sha256:` and the SHA-256 of the file's bytes.
fn digest(bytes: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(bytes);
    format!(
        "sha256:{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

/// Writes `text` through a temporary file, so a reader never sees half of
/// it.
fn save(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    let temporary = path.with_file_name(name);
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path)
}

/// `text` in lines of at most `width` characters, long words whole.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_package_is_named_with_its_version() {
        assert_eq!(
            named("AcmeCouriers.v1"),
            Some(("AcmeCouriers".to_string(), "v1".to_string()))
        );
        assert_eq!(
            named("Stripe.v2024_06_20").map(|(_, version)| version),
            Some("v2024_06_20".to_string())
        );
        assert_eq!(named("acme.v1"), None);
        assert_eq!(named("Acme.1"), None);
        assert_eq!(named("Acme"), None);
    }

    #[test]
    fn a_documents_version_gives_the_packages() {
        let document = |version: &str| serde_json::json!({ "info": { "title": "acme couriers", "version": version } });
        assert_eq!(
            derived(&document("1.4.2")),
            ("AcmeCouriers".to_string(), "v1".to_string())
        );
        assert_eq!(derived(&document("2024-06-20")).1, "v2024_06_20");
        assert_eq!(derived(&document("v3")).1, "v3");
        assert_eq!(derived(&document("6a2987f054a4")).1, "v1");
    }
}

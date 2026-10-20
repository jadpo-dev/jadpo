//! `jadpo package` (area 11, "The surface" and "Developing and publishing
//! a package"; area 14; chunks 7.1b and 7.1b2): `show` prints a package's
//! description, licence, version, kind and public surface, or one
//! declaration in full, or its guide; `check` runs the publishing bar in a
//! package's own repository, its examples as tests, and writes its
//! `surface.jadpo_record` and its changelog's new section. Installing
//! (`add`, `update`, `remove`, `link`, `unlink`) is in [`crate::install`].
//!
//! `show` answers what the package is first, its facts on the grid, then a
//! list per section and a footer counting them, and the one command that
//! shows a declaration in full:
//!
//! ```text
//! UK postcodes: reading and normalising them.
//!   package   Postcodes.v2
//!   kind      library
//!   version   2.1.0
//!   source    path ../postcodes
//!   …
//!
//! Functions
//!   uk.parse         (written: WrittenPostcode) fails InvalidPostcode -> UkPostcode
//!
//! ✓ Postcodes.v2  1 function · 2 types · 1 failure
//! → jadpo package show postcodes uk.parse  shows one in full, with its file
//! ```
//!
//! `check` prints the package's problems at their places in its folder,
//! then the bar as a list, a row per item with its state as a symbol and a
//! word, then what it wrote and the footer:
//!
//! ```text
//! Publishing bar
//! ✓ compiles   passed           no errors
//! – examples   not checked yet  waits for how a description's example is written
//! ✓ version    passed           2.1.0, 1 added since 2.0.0
//!
//! ✓ Wrote surface.jadpo_record
//! ✓ package check passed  Postcodes.v2 2.1.0 · 5 passed · 2 not checked yet
//! → git tag v2.1.0  tags the release once surface.jadpo_record is committed
//! ```
//!
//! The package is checked placed as the app's own integration in a scratch
//! app (the golden rule: a package compiles as the app's own code does),
//! and every path is shown relative to the package's folder.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use jadpo_check::publishing::{self, Bump};
use jadpo_check::surface::{self, Declaration, Surface, What, first_paragraph};
use jadpo_db::{Locked, Source};
use jadpo_diag::{Diagnostic, nearest, rule};
use jadpo_term::{Block, Fact, Json, Outcome, Row, RowState, Screen};

use crate::fmt::project_needed;
use crate::run::{Call, Reply, either, quoted};
use crate::table::Value;

/// What `jadpo package` does today.
pub const ACTIONS: &[Value] = &[
    Value {
        name: "show",
        description: "Show a package's surface, one declaration, or its guide",
    },
    Value {
        name: "check",
        description: "Run the publishing bar and write surface.jadpo_record",
    },
    Value {
        name: "add",
        description: "Install a package from a folder or a git repository's tag",
    },
    Value {
        name: "update",
        description: "Move an installed package to its newest release",
    },
    Value {
        name: "remove",
        description: "Remove an installed package the app no longer uses",
    },
    Value {
        name: "link",
        description: "Compile the app against a package's live checkout",
    },
    Value {
        name: "unlink",
        description: "Go back to the installed version of a linked package",
    },
];

/// What the language decides `jadpo package` does, not built yet, and the
/// work that brings each.
const LATER: &[(&str, &str)] = &[
    ("new", "package repositories"),
    ("import", "the OpenAPI importer"),
    ("record", "recordings"),
    ("conform", "capabilities"),
    ("publish", "the hosted registry"),
    ("recipe", "recipes"),
];

pub fn package(call: &Call) -> Reply {
    let names: Vec<&str> = ACTIONS.iter().map(|action| action.name).collect();
    let action = call.arguments.first().map(String::as_str);
    if call.flag("--guide") && action != Some("show") {
        return Reply::failed(arguments(
            "`--guide` goes with `jadpo package show`".to_string(),
            "`--guide` prints a package's `GUIDE.md`, which only `show` does.".to_string(),
            "Run `jadpo package show <name> --guide`".to_string(),
        ));
    }
    match action {
        None => Reply::failed(arguments(
            "`jadpo package` needs something to do".to_string(),
            "`jadpo package show <name>` prints a package's surface, and `jadpo package check` runs the publishing bar in a package's own folder.".to_string(),
            "Run `jadpo package show <name>` or `jadpo package check`".to_string(),
        )),
        Some("show") => show(call),
        Some("check") => check(call),
        Some("add") => crate::install::add(call),
        Some("update") => crate::install::update(call),
        Some("remove") => crate::install::remove(call),
        Some("link") => crate::install::link(call),
        Some("unlink") => crate::install::unlink(call),
        Some(later) if LATER.iter().any(|(name, _)| *name == later) => {
            let comes = LATER
                .iter()
                .find(|(name, _)| *name == later)
                .map_or("", |(_, comes)| comes);
            let diagnostic = Diagnostic::new(
                &rule::TOOLCHAIN_NOT_BUILT_YET,
                format!("`jadpo package {later}` isn't built yet"),
            )
            .reason(format!(
                "Area 11 decides `jadpo package {later}`, and it comes with {comes}."
            ))
            .next("Run `jadpo package show <name>` or `jadpo package check` meanwhile")
            .help_at("11-integrations.html#developing-and-publishing-a-package");
            Reply::failed(diagnostic)
        }
        Some(other) => {
            let reason = format!("`jadpo package` can {}.", either(&quoted(&names)));
            Reply::failed(match nearest(other, names.iter().copied()) {
                Some(near) => arguments(
                    format!("`{other}` isn't something `jadpo package` does"),
                    format!("{reason} Did you mean `{near}`?"),
                    format!("Use `jadpo package {near}`"),
                ),
                None => arguments(
                    format!("`{other}` isn't something `jadpo package` does"),
                    reason,
                    "Run `jadpo package show <name>` or `jadpo package check`".to_string(),
                ),
            })
        }
    }
}

pub(crate) fn arguments(message: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_PACKAGE_ARGUMENTS, message)
        .reason(reason)
        .next(next)
}

pub(crate) fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// `a`, `a and b`, `a, b and c`.
pub(crate) fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

// --- show ------------------------------------------------------------------

fn show(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_needed("package show");
    };
    let loaded = jadpo_db::load(&root);
    let (db, project) = (&loaded.db, loaded.project);
    let homes = surface::homes(db, project);
    let folders: Vec<String> = homes.values().map(|home| home.name.clone()).collect();
    let Some(wanted) = call.arguments.get(1) else {
        let reason = if folders.is_empty() {
            "The project holds no package: none in `packages/` or `shared/providers/`.".to_string()
        } else {
            format!(
                "Give the package by its folder's name; the project holds {}.",
                and_list(&quoted(&folders))
            )
        };
        return Reply::failed(arguments(
            "`jadpo package show` needs a package".to_string(),
            reason,
            "Run `jadpo package show <name>`".to_string(),
        ));
    };
    let found = homes
        .values()
        .find(|home| *wanted == home.name || *wanted == home.code || *wanted == home.versioned());
    let Some(home) = found else {
        let diagnostic = |reason: String, next: String| {
            arguments(
                format!("`{wanted}` isn't a package in this project"),
                reason,
                next,
            )
        };
        if folders.is_empty() {
            return Reply::failed(diagnostic(
                "The project holds no package: none in `packages/` or `shared/providers/`."
                    .to_string(),
                "Install one with `jadpo package add`, or write the app's own in `shared/providers/<name>/<version>/`".to_string(),
            ));
        }
        let holds = format!("The project holds {}.", and_list(&quoted(&folders)));
        return Reply::failed(match nearest(wanted, folders.iter().map(String::as_str)) {
            Some(near) => diagnostic(
                format!("{holds} Did you mean `{near}`?"),
                format!("Use `jadpo package show {near}`"),
            ),
            None => diagnostic(holds, "Run `jadpo package show <name>`".to_string()),
        });
    };
    let Some(surface) = surface::surface(db, project, &home.folder) else {
        return Reply::failed(arguments(
            format!("`{wanted}` isn't a package in this project"),
            "Its folder holds no package.".to_string(),
            "Run `jadpo package show <name>`".to_string(),
        ));
    };
    let locked = lock_line(&loaded, &surface.package);
    if call.flag("--guide") {
        if call.arguments.len() > 2 {
            return Reply::failed(arguments(
                "`--guide` prints the package's guide, not a declaration's".to_string(),
                "A package's `GUIDE.md` says how to use the whole package.".to_string(),
                format!("Run `jadpo package show {} --guide`", home.name),
            ));
        }
        return guide(&loaded, &surface, &home.name);
    }
    match call.arguments.get(2) {
        Some(name) => declaration(&surface, &home.name, name),
        None => overview(&surface, &home.name, locked.as_ref()),
    }
}

/// The package's line in the lock, when it is installed.
fn lock_line(loaded: &jadpo_db::Loaded, package: &str) -> Option<Locked> {
    let record = loaded
        .project
        .records(&loaded.db)
        .iter()
        .find(|record| record.path == jadpo_db::LOCK)?;
    let text = record.text.as_ref().ok()?;
    jadpo_db::parse_lock(text)
        .ok()?
        .into_iter()
        .find(|line| line.package == package)
}

/// The version a lock line holds, and where it came from, in words.
fn source_words(surface: &Surface, locked: Option<&Locked>) -> (Option<String>, String) {
    if !surface.installed {
        return (None, "the app's own".to_string());
    }
    match locked.map(|locked| &locked.source) {
        Some(Source::Path { version, path, .. }) => (Some(version.clone()), format!("path {path}")),
        Some(Source::Git {
            version,
            repository,
            tag,
            ..
        }) => (Some(version.clone()), format!("git {repository} tag {tag}")),
        Some(Source::Registry { version, .. }) => {
            (Some(version.clone()), "the registry".to_string())
        }
        Some(Source::Link { path }) => (None, format!("a link to {path}")),
        None => (None, "not in jadpo.lock".to_string()),
    }
}

/// A declaration's one line in a list.
fn summary(declaration: &Declaration) -> String {
    match &declaration.what {
        What::Operation {
            method,
            path,
            input,
            output,
            ..
        } => {
            let mut out = format!("{method} {path}");
            match (input, output) {
                (Some(input), Some(output)) => out.push_str(&format!("  {input} -> {output}")),
                (None, Some(output)) => out.push_str(&format!("  -> {output}")),
                (Some(input), None) => out.push_str(&format!("  {input}")),
                (None, None) => {}
            }
            out
        }
        What::Function {
            params,
            result,
            fails,
        } => {
            let params: Vec<String> = params.iter().map(|(n, t)| format!("{n}: {t}")).collect();
            let mut out = format!("({})", params.join(", "));
            if !fails.is_empty() {
                out.push_str(&format!(" fails {}", fails.join(", ")));
            }
            if let Some(result) = result {
                out.push_str(&format!(" -> {result}"));
            }
            out
        }
        What::Type {
            base,
            rules,
            fields,
            variants,
        } => {
            let inner: Vec<String> = if !variants.is_empty() {
                variants.iter().map(|(name, _)| name.clone()).collect()
            } else if !fields.is_empty() {
                fields.iter().map(|(n, t)| format!("{n}: {t}")).collect()
            } else {
                rules.iter().map(|(n, v)| format!("{n}: {v}")).collect()
            };
            if inner.is_empty() {
                base.clone()
            } else {
                format!("{base} {{ {} }}", inner.join(", "))
            }
        }
        What::Failure { kind, code, .. } => kind.clone().unwrap_or_else(|| code.clone()),
        What::Recipe => declaration.file.clone(),
    }
}

/// The sections a package's surface lists, in the record's order.
const SECTIONS: [(&str, &str, &str); 5] = [
    ("operation", "Operations", "operation"),
    ("function", "Functions", "function"),
    ("type", "Types", "type"),
    ("failure", "Failures", "failure"),
    ("recipe", "Recipes", "recipe"),
];

fn overview(surface: &Surface, folder_name: &str, locked: Option<&Locked>) -> Reply {
    let (version, source) = source_words(surface, locked);
    let headline = surface
        .description
        .as_deref()
        .and_then(first_paragraph)
        .unwrap_or_else(|| surface.package.clone());
    let mut fields: Vec<(String, String)> = vec![
        ("package".into(), surface.package.clone()),
        ("kind".into(), surface.kind.to_string()),
    ];
    if let Some(version) = &version {
        fields.push(("version".into(), version.clone()));
    }
    fields.push(("source".into(), source.clone()));
    if let Some(licence) = &surface.licence {
        fields.push(("licence".into(), licence.clone()));
    }
    if let Some(jadpo) = &surface.jadpo {
        fields.push(("jadpo".into(), jadpo.clone()));
    }
    fields.push(("folder".into(), surface.folder.clone()));
    if surface.guide {
        fields.push(("guide".into(), surface::GUIDE.to_string()));
    }
    let mut screen = Screen::new().with(Block::Answer { headline, fields });
    if let Some(provider) = &surface.provider {
        let rows = provider
            .facts
            .iter()
            .map(|fact| match fact.rsplit_once(' ') {
                Some((name, value)) => Row::new(name, value),
                None => Row::new(fact.as_str(), ""),
            })
            .collect();
        screen.push(Block::list("Provider", rows));
    }
    let mut counts = Vec::new();
    for (word, heading, noun) in SECTIONS {
        let rows: Vec<Row> = surface
            .declarations
            .iter()
            .filter(|d| d.what.word() == word)
            .map(|d| Row::new(d.name.as_str(), summary(d)))
            .collect();
        if rows.is_empty() {
            continue;
        }
        counts.push(Fact::plain(plural(rows.len(), noun, &format!("{noun}s"))));
        screen.push(Block::list(heading, rows));
    }
    if counts.is_empty() {
        counts.push(Fact::plain("nothing public yet"));
    }
    screen.push(Block::summary(
        Outcome::Success,
        surface.package.clone(),
        counts,
    ));
    if let Some(first) = surface
        .declarations
        .iter()
        .find(|d| !matches!(d.what, What::Recipe))
    {
        screen.push(Block::hint(
            format!("jadpo package show {folder_name} {}", first.name),
            "shows one in full, with its file",
        ));
    }
    let data = Json::object([
        (
            "package",
            Json::object([
                ("name", Json::from(surface.package.as_str())),
                ("kind", Json::from(surface.kind)),
                ("version", version.as_deref().map_or(Json::Null, Json::from)),
                ("source", Json::from(source.as_str())),
                (
                    "licence",
                    surface.licence.as_deref().map_or(Json::Null, Json::from),
                ),
                (
                    "jadpo",
                    surface.jadpo.as_deref().map_or(Json::Null, Json::from),
                ),
                ("folder", Json::from(surface.folder.as_str())),
                ("guide", surface.guide.into()),
                (
                    "description",
                    surface
                        .description
                        .as_deref()
                        .map_or(Json::Null, Json::from),
                ),
            ]),
        ),
        (
            "provider",
            surface.provider.as_ref().map_or(Json::Null, |provider| {
                Json::object([(
                    "facts",
                    Json::array(provider.facts.iter().map(String::as_str)),
                )])
            }),
        ),
        (
            "declarations",
            Json::array(
                surface
                    .declarations
                    .iter()
                    .map(|d| declaration_json(&surface.folder, d)),
            ),
        ),
    ]);
    Reply::screen(screen, data)
}

fn declaration_json(folder: &str, declaration: &Declaration) -> Json {
    Json::object([
        ("name", Json::from(declaration.name.as_str())),
        ("kind", Json::from(declaration.what.word())),
        (
            "description",
            declaration
                .description
                .as_deref()
                .map_or(Json::Null, Json::from),
        ),
        ("file", Json::from(format!("{folder}/{}", declaration.file))),
        (
            "line",
            declaration
                .line
                .map_or(Json::Null, |line| Json::from(line as usize)),
        ),
        (
            "facts",
            Json::array(
                declaration
                    .facts()
                    .into_iter()
                    .map(|member| Json::from(member.line)),
            ),
        ),
    ])
}

fn declaration(surface: &Surface, folder_name: &str, name: &str) -> Reply {
    let Some(found) = surface.declaration(name) else {
        let names: Vec<String> = surface
            .declarations
            .iter()
            .map(|d| d.name.clone())
            .collect();
        let holds = if names.len() <= 10 {
            format!("Its public surface holds {}.", and_list(&quoted(&names)))
        } else {
            format!(
                "Its public surface holds {}.",
                plural(names.len(), "declaration", "declarations")
            )
        };
        let message = format!("`{}` has no public `{name}`", surface.package);
        return Reply::failed(match nearest(name, names.iter().map(String::as_str)) {
            Some(near) => arguments(
                message,
                format!("{holds} Did you mean `{near}`?"),
                format!("Use `jadpo package show {folder_name} {near}`"),
            ),
            None => arguments(
                message,
                holds,
                format!("Run `jadpo package show {folder_name}` to see them all"),
            ),
        });
    };
    let headline = found
        .description
        .as_deref()
        .and_then(first_paragraph)
        .unwrap_or_else(|| found.name.clone());
    let file = match found.line {
        Some(line) => format!("{}/{}:{line}", surface.folder, found.file),
        None => format!("{}/{}", surface.folder, found.file),
    };
    let mut fields: Vec<(String, String)> = vec![
        ("name".into(), found.name.clone()),
        ("kind".into(), found.what.word().to_string()),
        ("package".into(), surface.package.clone()),
        ("file".into(), file),
    ];
    let mut lists: Vec<Block> = Vec::new();
    match &found.what {
        What::Operation {
            method,
            path,
            input,
            output,
            output_at,
            changes,
            idempotent,
            fails,
            source,
            confirmed,
        } => {
            fields.push(("request".into(), format!("{method} {path}")));
            if let Some(input) = input {
                fields.push(("input".into(), input.clone()));
            }
            if let Some(output) = output {
                fields.push(("output".into(), output.clone()));
            }
            if let Some(at) = output_at {
                fields.push(("output at".into(), at.clone()));
            }
            fields.push(("changes".into(), answer(*changes)));
            fields.push((
                "repeats".into(),
                if *idempotent {
                    "safely".into()
                } else {
                    "not safely".into()
                },
            ));
            if !fails.is_empty() {
                fields.push(("fails".into(), fails.join(", ")));
            }
            if let Some((how, docs)) = source {
                fields.push(("source".into(), how.clone()));
                if let Some(docs) = docs {
                    fields.push(("docs".into(), docs.clone()));
                }
            }
            fields.push((
                "checked".into(),
                confirmed.clone().map_or_else(
                    || "against no recording yet".to_string(),
                    |path| format!("against {path}"),
                ),
            ));
        }
        What::Function {
            params,
            result,
            fails,
        } => {
            let params: Vec<String> = params.iter().map(|(n, t)| format!("{n}: {t}")).collect();
            fields.push((
                "takes".into(),
                if params.is_empty() {
                    "nothing".to_string()
                } else {
                    params.join(", ")
                },
            ));
            fields.push((
                "returns".into(),
                result.clone().unwrap_or_else(|| "nothing".to_string()),
            ));
            if !fails.is_empty() {
                fields.push(("fails".into(), fails.join(", ")));
            }
        }
        What::Type {
            base,
            rules,
            fields: own,
            variants,
        } => {
            fields.push(("built on".into(), base.clone()));
            if !rules.is_empty() {
                let rules: Vec<String> = rules.iter().map(|(n, v)| format!("{n}: {v}")).collect();
                fields.push(("rules".into(), rules.join(", ")));
            }
            if !own.is_empty() {
                lists.push(Block::list(
                    "Fields",
                    own.iter()
                        .map(|(n, t)| Row::new(n.as_str(), t.as_str()))
                        .collect(),
                ));
            }
            if !variants.is_empty() {
                lists.push(Block::list(
                    "Variants",
                    variants
                        .iter()
                        .map(|(n, fields)| {
                            let inner: Vec<String> =
                                fields.iter().map(|(f, t)| format!("{f}: {t}")).collect();
                            Row::new(
                                n.as_str(),
                                if inner.is_empty() {
                                    String::new()
                                } else {
                                    format!("{{ {} }}", inner.join(", "))
                                },
                            )
                        })
                        .collect(),
                ));
            }
        }
        What::Failure {
            kind,
            code,
            message,
            fields: own,
        } => {
            if let Some(kind) = kind {
                fields.push(("failure".into(), kind.clone()));
            }
            fields.push(("code".into(), code.clone()));
            if let Some(message) = message {
                fields.push(("message".into(), message.clone()));
            }
            if !own.is_empty() {
                lists.push(Block::list(
                    "Fields",
                    own.iter()
                        .map(|(n, t)| Row::new(n.as_str(), t.as_str()))
                        .collect(),
                ));
            }
        }
        What::Recipe => {}
    }
    let mut screen = Screen::new().with(Block::Answer { headline, fields });
    for list in lists {
        screen.push(list);
    }
    // A description longer than its first paragraph is printed whole.
    if let Some(description) = &found.description {
        let paragraphs: Vec<String> = description
            .split("\n\n")
            .map(|paragraph| {
                paragraph
                    .lines()
                    .map(str::trim)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|paragraph| !paragraph.is_empty())
            .collect();
        if paragraphs.len() > 1 {
            let mut lines = Vec::new();
            for (i, paragraph) in paragraphs.iter().enumerate() {
                if i > 0 {
                    lines.push(String::new());
                }
                lines.extend(wrap(paragraph, 76));
            }
            screen.push(Block::section("Description", lines));
        }
    }
    Reply::screen(
        screen,
        Json::object([
            ("package", Json::from(surface.package.as_str())),
            ("declaration", declaration_json(&surface.folder, found)),
        ]),
    )
}

fn answer(yes: bool) -> String {
    if yes { "yes" } else { "no" }.to_string()
}

/// Words to lines of at most `width` characters, a long word whole.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
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

fn guide(loaded: &jadpo_db::Loaded, surface: &Surface, folder_name: &str) -> Reply {
    let path = format!("{}/{}", surface.folder, surface::GUIDE);
    let text = loaded
        .project
        .records(&loaded.db)
        .iter()
        .find(|record| record.path == path)
        .and_then(|record| record.text.clone().ok());
    match text {
        Some(text) => Reply::raw(
            text.clone(),
            Json::object([
                ("package", Json::from(surface.package.as_str())),
                ("path", Json::from(path.as_str())),
                ("guide", Json::from(text.as_str())),
            ]),
        ),
        None => Reply::failed(arguments(
            format!("`{}` has no guide", surface.package),
            format!(
                "A package's `{}` says how to use it well; `{path}` isn't there.",
                surface::GUIDE
            ),
            format!("Run `jadpo package show {folder_name}` for its surface"),
        )),
    }
}

// --- check -----------------------------------------------------------------

/// One row of the publishing bar.
struct Item {
    name: &'static str,
    state: RowState,
    detail: String,
}

impl Item {
    fn passed(name: &'static str, detail: impl Into<String>) -> Item {
        Item {
            name,
            state: RowState::Done,
            detail: detail.into(),
        }
    }

    fn failed(name: &'static str, detail: impl Into<String>) -> Item {
        Item {
            name,
            state: RowState::Failed,
            detail: detail.into(),
        }
    }

    fn later(name: &'static str, detail: impl Into<String>) -> Item {
        Item {
            name,
            state: RowState::Skipped,
            detail: detail.into(),
        }
    }

    fn word(&self) -> &'static str {
        match self.state {
            RowState::Done => "passed",
            RowState::Failed => "failed",
            RowState::Skipped => "not checked yet",
        }
    }
}

/// A scratch app the package is checked in, removed when dropped.
pub(crate) struct Scratch(pub PathBuf);

impl Scratch {
    pub(crate) fn new() -> std::io::Result<Scratch> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos());
        // Two scratch apps made in the same tick by one process (tests run
        // side by side) still get folders of their own.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("jadpo-package-{}-{nanos}-{n}", std::process::id()));
        std::fs::create_dir_all(&path)?;
        Ok(Scratch(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The package's name in code from its index's text: `Postcodes.v2`.
pub(crate) fn index_name(text: &str) -> Option<String> {
    use jadpo_syntax::ast::{self, AstNode};
    let parse = jadpo_syntax::parse_file(text);
    let source = ast::SourceFile::cast(parse.syntax())?;
    source.decls().find_map(|decl| match decl {
        ast::Decl::PackageDecl(decl) => {
            let path = decl.path()?;
            Some(
                path.syntax()
                    .descendants_with_tokens()
                    .filter_map(|element| element.into_token())
                    .filter(|token| !token.kind().is_trivia())
                    .map(|token| token.text().to_string())
                    .collect(),
            )
        }
        _ => None,
    })
}

/// Every file under `folder`, relative, leaving out hidden entries and a
/// `build/` at its root.
fn package_files(folder: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => {
                    if !(dir == folder && name == "build") {
                        pending.push(path);
                    }
                }
                Ok(kind) if kind.is_file() => {
                    if let Ok(relative) = path.strip_prefix(folder) {
                        out.push(
                            relative
                                .components()
                                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                                .collect::<Vec<_>>()
                                .join("/"),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    out.sort();
    out
}

fn no_package() -> Reply {
    Reply::failed(arguments(
        "There is no package here".to_string(),
        format!(
            "`jadpo package check` runs in a package's own folder, the one holding `{}`: its description, licence and the earliest Jadpo version it needs.",
            jadpo_db::PACKAGE_INDEX
        ),
        "Run it in the package's folder, or give the folder: `jadpo package check <folder>`"
            .to_string(),
    ))
}

/// A package's folder compiled as the app's own integration in a scratch
/// app (the golden rule: a package compiles as the app's own code does).
pub(crate) struct Compiled {
    pub scratch: Scratch,
    pub loaded: jadpo_db::Loaded,
    /// Where the package sits in the scratch app,
    /// `shared/providers/postcodes/v2`.
    pub mount: String,
    /// Its name and version label, `Postcodes.v2`.
    pub package: String,
    pub code: String,
    pub label: String,
    /// Its problems, paths still in the scratch app.
    pub problems: Vec<Diagnostic>,
}

impl Compiled {
    /// A path in the scratch app as the package's folder names it.
    pub fn relative(&self, path: &str) -> String {
        let prefix = format!("{}/", self.mount);
        path.strip_prefix(&prefix).unwrap_or(path).to_string()
    }

    /// The package's source texts, by path in its folder.
    pub fn sources(&self) -> BTreeMap<String, String> {
        let prefix = format!("{}/", self.mount);
        jadpo_db::source_texts(&self.loaded.db, self.loaded.project)
            .into_iter()
            .filter_map(|(path, text)| Some((path.strip_prefix(&prefix)?.to_string(), text)))
            .collect()
    }

    /// The package's surface, when it has one.
    pub fn surface(&self) -> Option<Surface> {
        surface::surface(&self.loaded.db, self.loaded.project, &self.mount)
    }
}

/// Compiles the package in `folder`; `Err(None)` when there is no package
/// there, `Err(Some(reply))` when the scratch app couldn't be written.
pub(crate) fn compile(folder: &Path, command: &str) -> Result<Compiled, Option<Reply>> {
    let index = std::fs::read_to_string(folder.join(jadpo_db::PACKAGE_INDEX)).map_err(|_| None)?;
    let package = index_name(&index).ok_or(None)?;
    let (code, label) = package.rsplit_once('.').ok_or(None)?;
    if !jadpo_db::is_version_label(label) || code.is_empty() {
        return Err(None);
    }
    let (code, label) = (code.to_string(), label.to_string());
    let mount = format!(
        "shared/{}/{}/{label}",
        jadpo_db::PROVIDERS,
        jadpo_db::to_snake(&code)
    );
    let scratch = Scratch::new().map_err(|error| {
        Some(write_failed(
            command,
            "a scratch folder",
            &error.to_string(),
        ))
    })?;
    let copied = std::fs::write(scratch.0.join("app.jadpo"), "application {}\n").and_then(|()| {
        for file in package_files(folder) {
            let to = scratch.0.join(&mount).join(&file);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(folder.join(&file), to)?;
        }
        Ok(())
    });
    if let Err(error) = copied {
        return Err(Some(write_failed(
            command,
            "the scratch app the package is checked in",
            &error.to_string(),
        )));
    }
    let loaded = jadpo_db::load(&scratch.0);
    let prefix = format!("{mount}/");
    let relative = |path: &str| path.strip_prefix(&prefix).unwrap_or(path).to_string();
    let mut problems: Vec<Diagnostic> = loaded
        .unreadable
        .iter()
        .map(|file| {
            Diagnostic::new(
                &rule::TOOLCHAIN_SOURCE_READ_FAILED,
                format!("`{}` couldn't be read", relative(&file.path)),
            )
            .reason(format!("Reading it failed: {}.", file.error))
            .next(format!(
                "Check it can be read, then run `jadpo {command}` again"
            ))
        })
        .collect();
    problems.extend(
        jadpo_check::check(&loaded.db, loaded.project)
            .into_iter()
            // The curated packages take the names Jadpo reserves for them.
            .filter(|d| d.rule.id != rule::PACKAGE_RESERVED_NAME.id),
    );
    Ok(Compiled {
        scratch,
        loaded,
        mount,
        package,
        code,
        label,
        problems,
    })
}

/// Shows each problem at its place in the package's folder.
pub(crate) fn in_package(compiled: &Compiled, diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics
        .into_iter()
        .map(|mut d| {
            if let Some(label) = &mut d.primary {
                label.span.path = compiled.relative(&label.span.path);
            }
            for label in &mut d.secondary {
                label.span.path = compiled.relative(&label.span.path);
            }
            d
        })
        .collect()
}

/// Shows a linked package's problems and sources in its checkout: each
/// path under `packages/<name>/<version>/` as the same path under the
/// link's folder (area 11, "Developing and publishing a package").
pub(crate) fn show_in_checkouts(
    links: &[(String, String)],
    diagnostics: &mut [Diagnostic],
    sources: &mut BTreeMap<String, String>,
) {
    let moved = |path: &str| -> Option<String> {
        links.iter().find_map(|(folder, checkout)| {
            let rest = path.strip_prefix(&format!("{folder}/"))?;
            Some(format!("{checkout}/{rest}"))
        })
    };
    for d in diagnostics.iter_mut() {
        for label in d.primary.iter_mut().chain(d.secondary.iter_mut()) {
            if let Some(path) = moved(&label.span.path) {
                label.span.path = path;
            }
        }
    }
    let paths: Vec<String> = sources.keys().cloned().collect();
    for path in paths {
        if let Some(to) = moved(&path)
            && let Some(text) = sources.remove(&path)
        {
            sources.insert(to, text);
        }
    }
}

fn check(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    if call.arguments.len() > 2 {
        return Reply::failed(arguments(
            format!(
                "`jadpo package check` takes one folder, not `{}`",
                call.arguments[2]
            ),
            "It checks the package in one folder.".to_string(),
            "Run `jadpo package check [<folder>]`".to_string(),
        ));
    }
    let folder = match call.arguments.get(1) {
        Some(argument) => cwd.join(argument),
        None => cwd.clone(),
    };
    let compiled = match compile(&folder, "package check") {
        Ok(compiled) => compiled,
        Err(Some(reply)) => return reply,
        Err(None) => return no_package(),
    };
    let (package, code, label, mount) = (
        compiled.package.clone(),
        compiled.code.clone(),
        compiled.label.clone(),
        compiled.mount.clone(),
    );
    let shown = jadpo_db::shown_root(&cwd, &folder);
    let (db, project) = (&compiled.loaded.db, compiled.loaded.project);
    let compile = compiled.problems.clone();
    let described = publishing::undescribed(db, project, &mount);
    let found = compiled.surface();
    let mut sources = compiled.sources();

    let mut items: Vec<Item> = Vec::new();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let compiled_cleanly = compile.is_empty();
    items.push(if compiled_cleanly {
        Item::passed("compiles", "no errors")
    } else {
        Item::failed("compiles", plural(compile.len(), "error", "errors"))
    });
    diagnostics.extend(in_package(&compiled, compile));
    items.push(if described.is_empty() {
        Item::passed("described", "every public declaration")
    } else {
        Item::failed(
            "described",
            format!("{} no description", plural(described.len(), "has", "have")),
        )
    });
    diagnostics.extend(in_package(&compiled, described));
    let mut test_blocks: Vec<Block> = Vec::new();
    match (&found, compiled_cleanly) {
        (Some(surface), true) => {
            let ran = examples_and_tests(&compiled, surface, &sources);
            items.push(ran.examples);
            items.push(ran.tests);
            diagnostics.extend(ran.problems);
            test_blocks = ran.blocks;
        }
        _ => {
            items.push(Item::later("examples", "the package has errors"));
            items.push(Item::later("tests", "the package has errors"));
        }
    }
    let data_files: Vec<String> = package_files(&folder);
    if found.as_ref().is_some_and(|s| s.kind == "provider")
        && data_files.iter().any(|f| f.starts_with("recordings/"))
    {
        items.push(Item::later("decoders", "decoding comes with the importer"));
    }
    if data_files.iter().any(|f| f.starts_with("recipes/")) {
        items.push(Item::later("recipes", "recipes in `sample/` come later"));
    }

    // The version, from the last release's surface.
    let git = Git::of(&folder);
    let releases = git
        .as_ref()
        .map(|git| git.releases(&package))
        .unwrap_or_default();
    let mut version: Option<String> = None;
    let mut hint: Option<String> = None;
    // A new version's changes, as its changelog lists them; `None` when
    // the version is one already released.
    let mut listed: Option<Vec<String>> = None;
    match (&found, compiled_cleanly) {
        (Some(surface), true) => {
            let now = surface.text(None);
            let outcome = match releases.first() {
                Some(last)
                    if git
                        .as_ref()
                        .is_some_and(|git| git.unchanged_since(&last.tag)) =>
                {
                    Ok((
                        last.version.clone(),
                        format!("{}, released as {}", last.version, last.tag),
                        None,
                    ))
                }
                Some(last) => {
                    publishing::version(label.as_str(), Some((&last.version, &last.surface)), &now)
                        .map(|bump| {
                            let list = if bump.changes.is_empty() {
                                vec!["no change to what it offers".to_string()]
                            } else {
                                bump.changes
                                    .iter()
                                    .map(|(word, heading)| publishing::change_item(word, heading))
                                    .collect()
                            };
                            (bump.version.clone(), bump_words(&bump), Some(list))
                        })
                }
                None => publishing::version(label.as_str(), None, &now).map(|bump| {
                    (
                        bump.version.clone(),
                        format!("{}, the first release", bump.version),
                        Some(vec!["first release".to_string()]),
                    )
                }),
            };
            match outcome {
                Ok((next, words, list)) => {
                    items.push(Item::passed("version", words));
                    if list.is_some() && git.is_some() {
                        hint = Some(next.clone());
                    }
                    listed = list;
                    version = Some(next);
                }
                Err(breaking) => {
                    let since = releases
                        .first()
                        .map(|last| last.version.clone())
                        .unwrap_or_default();
                    items.push(Item::failed(
                        "version",
                        format!(
                            "{} since {since}",
                            plural(breaking.len(), "breaking change", "breaking changes")
                        ),
                    ));
                    diagnostics.push(breaking_change(&package, &code, &label, &since, &breaking));
                }
            }
        }
        _ => {
            items.push(Item::later("version", "the package has errors"));
        }
    }

    // The changelog: the new version's list written from the surface, and
    // the author's why beneath it.
    let log_path = folder.join(publishing::CHANGELOG);
    let old_log = std::fs::read_to_string(&log_path).ok();
    let mut wrote_log = false;
    match (&version, &listed) {
        (Some(version), Some(list)) => {
            let released: Vec<String> = releases.iter().map(|r| r.version.clone()).collect();
            let log = publishing::changelog(old_log.as_deref(), version, list, &released);
            if old_log.as_deref() != Some(log.text.as_str()) {
                if let Err(error) = std::fs::write(&log_path, &log.text) {
                    return write_failed(
                        "package check",
                        publishing::CHANGELOG,
                        &error.to_string(),
                    );
                }
                wrote_log = true;
            }
            if log.why {
                items.push(Item::passed("changelog", format!("{version} says why")));
            } else {
                items.push(Item::failed(
                    "changelog",
                    format!("{version} doesn't say why"),
                ));
                diagnostics.push(changelog_missing(&log.text, log.line, version));
                sources.insert(publishing::CHANGELOG.to_string(), log.text.clone());
            }
        }
        (Some(version), None) => {
            let why = old_log
                .as_deref()
                .is_some_and(|text| publishing::says_why(text, version));
            items.push(Item::passed(
                "changelog",
                if why {
                    format!("{version} says why")
                } else {
                    format!("{version} is released")
                },
            ));
        }
        (None, _) => items.push(Item::later(
            "changelog",
            if compiled_cleanly && found.is_some() {
                "waits for an allowed version"
            } else {
                "the package has errors"
            },
        )),
    }
    let licence = found.as_ref().and_then(|s| s.licence.clone());
    let jadpo = found.as_ref().and_then(|s| s.jadpo.clone());
    items.push(match (licence, jadpo) {
        (Some(licence), Some(jadpo)) => {
            Item::passed("licence", format!("{licence}, from Jadpo {jadpo}"))
        }
        _ => Item::failed("licence", "the index doesn't say"),
    });

    let mut written: Option<(String, bool)> = None;
    if let (Some(surface), Some(version)) = (&found, &version) {
        let text = surface.text(Some(version));
        let path = folder.join(jadpo_db::record::SURFACE_FILE);
        let same = std::fs::read_to_string(&path).is_ok_and(|old| old == text);
        if !same && let Err(error) = std::fs::write(&path, &text) {
            return write_failed(
                "package check",
                jadpo_db::record::SURFACE_FILE,
                &error.to_string(),
            );
        }
        written = Some((in_folder(&shown, jadpo_db::record::SURFACE_FILE), !same));
    }

    let mut screen = Screen::new();
    for diagnostic in &diagnostics {
        screen.push(Block::diagnostic_from(
            diagnostic,
            &sources,
            shown.as_deref(),
        ));
    }
    for block in test_blocks {
        screen.push(block);
    }
    let rows: Vec<Row> = items
        .iter()
        .map(|item| {
            Row::new(item.name, item.word())
                .state(item.state)
                .extra([item.detail.clone()])
        })
        .collect();
    screen.push(Block::list("Publishing bar", rows));
    let mut wrote: Vec<String> = Vec::new();
    if let Some((path, true)) = &written {
        wrote.push(path.clone());
    }
    if wrote_log {
        wrote.push(in_folder(&shown, publishing::CHANGELOG));
    }
    if !wrote.is_empty() {
        screen.push(Block::Status {
            outcome: Outcome::Success,
            text: format!("Wrote {}", and_list(&wrote)),
            rows: Vec::new(),
        });
    }
    let count = |state: RowState| items.iter().filter(|item| item.state == state).count();
    let (failed, passed, later) = (
        count(RowState::Failed),
        count(RowState::Done),
        count(RowState::Skipped),
    );
    let mut facts = Vec::new();
    if failed == 0 {
        facts.push(Fact::plain(match &version {
            Some(version) => format!("{package} {version}"),
            None => package.clone(),
        }));
    } else {
        facts.push(Fact::problem(format!("{failed} failed")));
    }
    facts.push(Fact::plain(format!("{passed} passed")));
    if later > 0 {
        facts.push(Fact::plain(format!("{later} not checked yet")));
    }
    let (outcome, headline) = if failed == 0 {
        (Outcome::Success, "package check passed")
    } else {
        (Outcome::Failure, "package check failed")
    };
    screen.push(Block::summary(outcome, headline, facts));
    if failed == 0
        && let Some(version) = &hint
    {
        screen.push(Block::hint(
            format!("git tag v{version}"),
            format!(
                "tags the release once {} is committed",
                jadpo_db::record::SURFACE_FILE
            ),
        ));
    }
    let file_json = |path: Option<(String, bool)>| match path {
        Some((path, wrote)) => Json::object([
            ("path", Json::from(path.as_str())),
            ("written", wrote.into()),
        ]),
        None => Json::Null,
    };
    let data = Json::object([
        ("package", Json::from(package.as_str())),
        ("version", version.as_deref().map_or(Json::Null, Json::from)),
        (
            "bar",
            Json::array(items.iter().map(|item| {
                Json::object([
                    ("item", Json::from(item.name)),
                    (
                        "state",
                        Json::from(match item.state {
                            RowState::Done => "passed",
                            RowState::Failed => "failed",
                            RowState::Skipped => "not_checked",
                        }),
                    ),
                    ("detail", Json::from(item.detail.as_str())),
                ])
            })),
        ),
        ("surface", file_json(written)),
        (
            "changelog",
            file_json(
                (version.is_some() && listed.is_some())
                    .then(|| (in_folder(&shown, publishing::CHANGELOG), wrote_log)),
            ),
        ),
    ]);
    Reply::Report {
        screen,
        data,
        diagnostics,
        sources,
    }
}

/// `file` in the package's folder, as the reader reaches it.
fn in_folder(shown: &Option<String>, file: &str) -> String {
    match shown {
        Some(root) => format!("{root}/{file}"),
        None => file.to_string(),
    }
}

/// `publish.changelog_missing` at the new version's heading.
fn changelog_missing(text: &str, line: u32, version: &str) -> Diagnostic {
    let (start, end) = line_span(text, line, 0);
    Diagnostic::new(
        &rule::PUBLISH_CHANGELOG_MISSING,
        format!("{} doesn't say why {version} is released", publishing::CHANGELOG),
    )
    .at(
        jadpo_diag::Span::new(publishing::CHANGELOG, start, end),
        "no why beneath the list",
    )
    .reason("`jadpo package check` writes what changed from the surface; the author writes why, beneath it. A release reviewers can't tell the purpose of is one they can't judge.")
    .next(format!(
        "Write why {version} is released under `## {version}` in {}, after the list",
        publishing::CHANGELOG
    ))
}

/// The byte range of line `line` (from 1) of `text`, from `skip` bytes in
/// and without its trailing space.
fn line_span(text: &str, line: u32, skip: usize) -> (u32, u32) {
    let mut offset = 0usize;
    for (at, content) in text.split_inclusive('\n').enumerate() {
        if at + 1 == line as usize {
            let trimmed = content.trim_end();
            let start = offset + skip.min(trimmed.len());
            return (start as u32, (offset + trimmed.len()) as u32);
        }
        offset += content.len();
    }
    (offset as u32, offset as u32)
}

/// What running a package's examples and tests gave.
struct Ran {
    examples: Item,
    tests: Item,
    problems: Vec<Diagnostic>,
    blocks: Vec<Block>,
}

/// One example placed as a test in the scratch app.
struct Placed {
    example: publishing::Example,
    /// The declaration's file, in the package's folder.
    file: String,
    /// The test file it is in, in the scratch app.
    test_file: String,
    /// The `test` line and its first statement's line there.
    test_line: u32,
    first: u32,
}

/// Writes each example into the test file beside its declaration's file
/// (the package's own tests first, then the examples), leaving out those
/// in `dropped`.
fn place(
    compiled: &Compiled,
    found: &[(String, publishing::Example)],
    sources: &BTreeMap<String, String>,
    dropped: &[String],
) -> std::io::Result<Vec<Placed>> {
    // Every test file an example goes in is written again, even when all
    // of its examples are left out, so none stays from the last pass.
    let mut by_file: BTreeMap<String, Vec<&(String, publishing::Example)>> = BTreeMap::new();
    for entry in found {
        let test = format!(
            "{}.test.jadpo",
            entry.0.strip_suffix(".jadpo").unwrap_or(&entry.0)
        );
        let entries = by_file.entry(test).or_default();
        if !dropped.contains(&entry.1.name()) {
            entries.push(entry);
        }
    }
    let mut placed = Vec::new();
    for (test, entries) in by_file {
        let mut text = sources.get(&test).cloned().unwrap_or_default();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        if entries.is_empty() && text.is_empty() {
            let _ = std::fs::remove_file(compiled.scratch.0.join(&compiled.mount).join(&test));
            continue;
        }
        for (file, example) in entries {
            if !text.is_empty() {
                text.push('\n');
            }
            let test_line = text.matches('\n').count() as u32 + 1;
            text.push_str(&format!("test \"{}\" {{\n", example.name()));
            for (_, statement) in &example.lines {
                text.push_str(&format!("    {statement}\n"));
            }
            text.push_str("}\n");
            placed.push(Placed {
                example: example.clone(),
                file: file.clone(),
                test_file: format!("{}/{test}", compiled.mount),
                test_line,
                first: test_line + 1,
            });
        }
        std::fs::write(compiled.scratch.0.join(&compiled.mount).join(&test), text)?;
    }
    Ok(placed)
}

/// The placed example a line of a test file in the scratch app is in.
fn placed_at<'a>(placed: &'a [Placed], file: &str, line: u32) -> Option<&'a Placed> {
    placed.iter().find(|p| {
        p.test_file == file && line >= p.test_line && line <= p.first + p.example.lines.len() as u32
    })
}

/// The span of an example's statement on line `line` of the scratch's
/// test file, in its declaration's file: past the `///` and its space.
fn statement_span(
    placed: &Placed,
    line: u32,
    sources: &BTreeMap<String, String>,
) -> jadpo_diag::Span {
    let index = line.saturating_sub(placed.first) as usize;
    let original = placed
        .example
        .lines
        .get(index)
        .or(placed.example.lines.first())
        .map_or(1, |(line, _)| *line);
    let text = sources.get(&placed.file).map_or("", String::as_str);
    let content = text.lines().nth(original as usize - 1).unwrap_or("");
    let skip = content.find("///").map_or(0, |at| {
        at + 3 + usize::from(content[at + 3..].starts_with(' '))
    });
    let (start, end) = line_span(text, original, skip);
    jadpo_diag::Span::new(&placed.file, start, end)
}

/// Runs the package's examples and its own tests in the scratch app: the
/// bar's two rows, the problems they found, and each failed test's report.
fn examples_and_tests(
    compiled: &Compiled,
    surface: &Surface,
    sources: &BTreeMap<String, String>,
) -> Ran {
    let mut problems: Vec<Diagnostic> = Vec::new();
    let mut found: Vec<(String, publishing::Example)> = Vec::new();
    for declaration in &surface.declarations {
        let (Some(line), Some(text)) = (declaration.line, sources.get(&declaration.file)) else {
            continue;
        };
        for example in publishing::examples(text, &declaration.name, line) {
            found.push((declaration.file.clone(), example));
        }
    }
    // Every function of a public library has an example, and each failure
    // it can raise is shown by an example or a test.
    let shown_in: Vec<String> = found
        .iter()
        .map(|(_, example)| {
            example
                .lines
                .iter()
                .map(|(_, line)| line.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .chain(
            sources
                .iter()
                .filter(|(path, _)| path.ends_with(".test.jadpo"))
                .map(|(_, text)| text.clone()),
        )
        .collect();
    let texts: Vec<&str> = shown_in.iter().map(String::as_str).collect();
    let (mut without, mut unshown) = (0, 0);
    for declaration in &surface.declarations {
        let What::Function { fails, .. } = &declaration.what else {
            continue;
        };
        let span = name_span(declaration, sources);
        if !found
            .iter()
            .any(|(_, example)| example.owner == declaration.name)
        {
            without += 1;
            problems.push(
                Diagnostic::new(
                    &rule::PUBLISH_EXAMPLE_MISSING,
                    format!("Function `{}` has no example", declaration.name),
                )
                .at(span.clone(), "no example in its description")
                .reason("Every function of a published library shows how it is called with an example in its description, a ```jadpo block run as a test, so what the registry's page and `jadpo package show` say of it always runs.")
                .next(format!(
                    "Add a ```jadpo block to `{}`'s `///` description, holding statements as a test does: `assert {}(...) == ...`",
                    declaration.name, declaration.name
                )),
            );
        }
        for failure in fails {
            if !publishing::shows_failure(&texts, &declaration.name, failure) {
                unshown += 1;
                problems.push(
                    Diagnostic::new(
                        &rule::PUBLISH_EXAMPLE_MISSING,
                        format!(
                            "`{}` can fail with `{failure}`, which no example or test shows",
                            declaration.name
                        ),
                    )
                    .at(span.clone(), format!("fails {failure}"))
                    .reason("A published function shows each failure it can raise, so an app knows when it happens: an example or a test asserts it with `fails`.")
                    .next(format!(
                        "Add `assert {}(...) fails {failure}` to its example or to a test beside it",
                        declaration.name
                    )),
                );
            }
        }
    }

    // Place the examples as tests; one that doesn't compile is shown at
    // its place and left out, so the rest still run.
    let mut dropped: Vec<String> = Vec::new();
    let mut failed_examples = 0;
    let mut placed: Vec<Placed> = Vec::new();
    for _ in 0..2 {
        placed = match place(compiled, &found, sources, &dropped) {
            Ok(placed) => placed,
            Err(error) => {
                return Ran {
                    examples: Item::failed("examples", "couldn't be placed"),
                    tests: Item::later("tests", "the examples couldn't be placed"),
                    problems: vec![
                        Diagnostic::new(
                            &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                            "`jadpo package check` couldn't write the examples as tests",
                        )
                        .reason(format!("Writing them failed: {error}."))
                        .next("Check that the temporary folder can be written, then run `jadpo package check` again"),
                    ],
                    blocks: Vec::new(),
                };
            }
        };
        let loaded = jadpo_db::load(&compiled.scratch.0);
        let mut broken = false;
        for diagnostic in jadpo_check::check(&loaded.db, loaded.project) {
            let Some(label) = &diagnostic.primary else {
                continue;
            };
            let text = std::fs::read_to_string(compiled.scratch.0.join(&label.span.path))
                .unwrap_or_default();
            let line = text[..(label.span.start as usize).min(text.len())]
                .matches('\n')
                .count() as u32
                + 1;
            let Some(at) = placed_at(&placed, &label.span.path, line) else {
                continue;
            };
            let name = at.example.name();
            if dropped.contains(&name) {
                continue;
            }
            dropped.push(name);
            broken = true;
            failed_examples += 1;
            problems.push(
                Diagnostic::new(
                    &rule::PUBLISH_EXAMPLE_FAILED,
                    format!("Example of `{}` doesn't compile", at.example.owner),
                )
                .at(statement_span(at, line, sources), "doesn't compile")
                .reason(format!(
                    "Run as a test of the package, it gives: {} An example runs as a test, so a description always shows what the code does.",
                    sentence(&diagnostic.message)
                ))
                .next("Correct the example, writing statements as a test's body does"),
            );
        }
        if !broken {
            break;
        }
    }

    let ran = jadpo_test::run(jadpo_test::Options {
        cwd: compiled.scratch.0.clone(),
        ..jadpo_test::Options::default()
    });
    let run = match ran {
        jadpo_test::Ran::Done(run) => Some(run),
        jadpo_test::Ran::Refused { .. } | jadpo_test::Ran::NoProject => {
            return Ran {
                examples: if failed_examples > 0 {
                    Item::failed(
                        "examples",
                        format!(
                            "{failed_examples} of {} failed",
                            plural(found.len(), "example", "examples")
                        ),
                    )
                } else {
                    Item::later("examples", "the package has errors")
                },
                tests: Item::later("tests", "the package has errors"),
                problems,
                blocks: Vec::new(),
            };
        }
        _ => None,
    };
    let mut own_total = 0;
    let mut own_failed = 0;
    let mut blocks: Vec<Block> = Vec::new();
    if let Some(mut run) = run {
        let mut own: Vec<jadpo_test::TestResult> = Vec::new();
        for result in std::mem::take(&mut run.results) {
            match placed_at(&placed, &result.test.file, result.test.line) {
                Some(at) => {
                    if let Some(failure) = &result.failure {
                        failed_examples += 1;
                        let (label, reason) = what_failed(failure);
                        problems.push(
                            Diagnostic::new(
                                &rule::PUBLISH_EXAMPLE_FAILED,
                                format!("Example of `{}` failed", at.example.owner),
                            )
                            .at(statement_span(at, failure.location.line, sources), label)
                            .reason(format!(
                                "{reason} An example runs as a test, so a description always shows what the code does."
                            ))
                            .next("Correct the example, or the code if the example says what it should do"),
                        );
                    }
                }
                None => own.push(result),
            }
        }
        for result in &mut own {
            result.test.file = compiled.relative(&result.test.file);
        }
        own_total = own.len();
        own_failed = own.iter().filter(|r| r.failure.is_some()).count();
        run.results = own;
        if own_failed > 0 {
            blocks = jadpo_test::screen(&run).into_blocks();
            blocks.pop();
            problems.push(
                Diagnostic::new(
                    &rule::TEST_FAILED,
                    format!("{own_failed} of {} failed", plural(own_total, "test", "tests")),
                )
                .reason("A package is published only when its own tests pass; each failed test's report is below.")
                .next("Fix each failure, then run `jadpo package check` again"),
            );
        }
    }
    let tests = if own_total == 0 {
        Item::passed("tests", "no tests")
    } else if own_failed == 0 {
        Item::passed("tests", plural(own_total, "test", "tests"))
    } else {
        Item::failed(
            "tests",
            format!("{own_failed} of {}", plural(own_total, "test", "tests")),
        )
    };
    let total = found.len();
    let mut wrong: Vec<String> = Vec::new();
    if failed_examples > 0 {
        wrong.push(format!(
            "{failed_examples} of {} failed",
            plural(total, "example", "examples")
        ));
    }
    if without > 0 {
        wrong.push(format!(
            "{} no example",
            plural(without, "function has", "functions have")
        ));
    }
    if unshown > 0 {
        wrong.push(format!(
            "{} not shown",
            plural(unshown, "failure", "failures")
        ));
    }
    let any_fails = surface
        .declarations
        .iter()
        .any(|d| matches!(&d.what, What::Function { fails, .. } if !fails.is_empty()));
    let examples = if !wrong.is_empty() {
        Item::failed("examples", wrong.join(", "))
    } else if total == 0 {
        Item::passed("examples", "no functions to show")
    } else if any_fails {
        Item::passed(
            "examples",
            format!(
                "{}, every failure shown",
                plural(total, "example", "examples")
            ),
        )
    } else {
        Item::passed("examples", plural(total, "example", "examples"))
    };
    Ran {
        examples,
        tests,
        problems,
        blocks,
    }
}

/// A message as a sentence ending in a full stop.
fn sentence(message: &str) -> String {
    if message.ends_with('.') {
        message.to_string()
    } else {
        format!("{message}.")
    }
}

/// The caret's label and the reason for an example's failed test.
fn what_failed(failure: &jadpo_test::TestFailure) -> (String, String) {
    match &failure.detail {
        jadpo_test::Detail::Assertion(assertion) => {
            if let Some(fails) = &assertion.fails {
                let instead = match (&assertion.returned, &assertion.instead) {
                    (Some(returned), _) => format!("it returned {returned}"),
                    (_, Some(other)) => format!("it failed with `{}`", other.name),
                    _ => "it didn't".to_string(),
                };
                (
                    format!("doesn't fail with `{fails}`"),
                    format!(
                        "Run as a test of the package, the call was to fail with `{fails}`, and {instead}."
                    ),
                )
            } else {
                let reason = match (&assertion.expected, &assertion.actual) {
                    (Some(expected), Some(actual)) => format!(
                        "Run as a test of the package, it expected {expected} and got {actual}."
                    ),
                    _ => "Run as a test of the package, it doesn't hold.".to_string(),
                };
                ("this doesn't hold".to_string(), reason)
            }
        }
        jadpo_test::Detail::Failure(info) => (
            format!("fails with `{}`", info.name),
            format!(
                "Run as a test of the package, it failed with `{}`, which nothing in the example expects.",
                info.name
            ),
        ),
        jadpo_test::Detail::Fault(info) => (
            "faults".to_string(),
            format!(
                "Run as a test of the package, it faulted: {}.",
                info.subject
            ),
        ),
    }
}

/// The span of a declaration's name on its line, in the package's folder.
fn name_span(declaration: &Declaration, sources: &BTreeMap<String, String>) -> jadpo_diag::Span {
    let text = sources.get(&declaration.file).map_or("", String::as_str);
    let line = declaration.line.unwrap_or(1);
    let content = text.lines().nth(line as usize - 1).unwrap_or("");
    let name = declaration
        .name
        .rsplit('.')
        .next()
        .unwrap_or(&declaration.name);
    let skip = content
        .match_indices(name)
        .find(|(at, _)| {
            let before = content[..*at].chars().next_back();
            let after = content[at + name.len()..].chars().next();
            !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
                && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
        .map_or(0, |(at, _)| at);
    let (start, _) = line_span(text, line, skip);
    jadpo_diag::Span::new(&declaration.file, start, start + name.len() as u32)
}

pub(crate) fn write_failed(command: &str, what: &str, error: &str) -> Reply {
    Reply::failed(
        Diagnostic::new(
            &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
            format!("`jadpo {command}` couldn't write {what}"),
        )
        .reason(format!("Writing it failed: {error}."))
        .next(format!(
            "Check that the folder can be written, then run `jadpo {command}` again"
        )),
    )
}

/// The version row's words for a new release.
fn bump_words(bump: &Bump) -> String {
    let since = bump.since.as_deref().unwrap_or("");
    if bump.changes.is_empty() {
        return format!("{}, a patch on {since}", bump.version);
    }
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (word, _) in &bump.changes {
        *counts.entry(word.as_str()).or_default() += 1;
    }
    let words: Vec<String> = counts
        .iter()
        .map(|(word, count)| format!("{count} {word}"))
        .collect();
    format!("{}, {} since {since}", bump.version, words.join(", "))
}

fn breaking_change(
    package: &str,
    code: &str,
    label: &str,
    since: &str,
    breaking: &[(String, String)],
) -> Diagnostic {
    let quote = |items: Vec<&String>| -> Vec<String> {
        items.into_iter().map(|item| format!("`{item}`")).collect()
    };
    let changed = quote(
        breaking
            .iter()
            .filter(|(word, _)| word == "changed")
            .map(|(_, heading)| heading)
            .collect(),
    );
    let removed = quote(
        breaking
            .iter()
            .filter(|(word, _)| word == "removed")
            .map(|(_, heading)| heading)
            .collect(),
    );
    let mut parts = Vec::new();
    if !changed.is_empty() {
        parts.push(format!("{} changed", and_list(&changed)));
    }
    if !removed.is_empty() {
        parts.push(format!(
            "{} {} removed",
            and_list(&removed),
            if removed.len() == 1 { "was" } else { "were" }
        ));
    }
    let next_major = label
        .strip_prefix('v')
        .and_then(|digits| digits.parse::<u64>().ok())
        .map(|major| format!("`{code}.v{}`", major + 1));
    let next = match next_major {
        Some(next) => format!(
            "Put back what changed, or publish the change as {next} in a folder of its own"
        ),
        None => "Put back what changed, or publish the change as the package for the API's next version, in a folder of its own".to_string(),
    };
    Diagnostic::new(
        &rule::PUBLISH_BREAKING_CHANGE,
        format!("`{package}` changed in ways that break the apps that use it"),
    )
    .reason(format!(
        "Since {since}, {}, which an app using {since} may rely on. A breaking change is a new major version: a new package beside this one.",
        parts.join(" and ")
    ))
    .next(next)
}

/// The package's repository, when its folder is in one.
pub(crate) struct Git {
    folder: PathBuf,
    /// The folder's path in the repository: `""` at its root, or
    /// `packages/postcodes/`.
    prefix: String,
}

/// A release: a `v<version>` tag whose surface names the package.
pub(crate) struct Release {
    pub tag: String,
    pub version: String,
    pub surface: String,
}

impl Git {
    pub(crate) fn of(folder: &Path) -> Option<Git> {
        let prefix = git(folder, &["rev-parse", "--show-prefix"])?;
        Some(Git {
            folder: folder.to_path_buf(),
            prefix: prefix.trim().to_string(),
        })
    }

    /// The package's releases, newest first.
    pub(crate) fn releases(&self, package: &str) -> Vec<Release> {
        let Some(tags) = git(&self.folder, &["tag", "--list", "v*"]) else {
            return Vec::new();
        };
        let heading = format!("package {package}");
        let mut out: Vec<(Vec<u64>, Release)> = Vec::new();
        for tag in tags.lines().map(str::trim) {
            let Some(order) = semver(tag) else { continue };
            let spec = format!("{tag}:{}{}", self.prefix, jadpo_db::record::SURFACE_FILE);
            let Some(surface) = git(&self.folder, &["show", &spec]) else {
                continue;
            };
            if !surface.lines().any(|line| line == heading) {
                continue;
            }
            out.push((
                order,
                Release {
                    tag: tag.to_string(),
                    version: tag[1..].to_string(),
                    surface,
                },
            ));
        }
        out.sort_by(|a, b| b.0.cmp(&a.0));
        out.into_iter().map(|(_, release)| release).collect()
    }

    /// Whether nothing in the package's folder changed since `tag`.
    fn unchanged_since(&self, tag: &str) -> bool {
        let same = Command::new("git")
            .args(["diff", "--quiet", tag, "--", "."])
            .current_dir(&self.folder)
            .status()
            .is_ok_and(|status| status.success());
        let untracked = git(
            &self.folder,
            &["ls-files", "--others", "--exclude-standard", "--", "."],
        );
        same && untracked.is_some_and(|files| files.trim().is_empty())
    }
}

/// `v2.1.0` as numbers to order by; a pre-release orders before its
/// release.
pub(crate) fn semver(tag: &str) -> Option<Vec<u64>> {
    let version = tag.strip_prefix('v')?;
    let (core, pre) = match version.split_once('-') {
        Some((core, _)) => (core, 0),
        None => (version, 1),
    };
    let parts: Option<Vec<u64>> = core.split('.').map(|part| part.parse().ok()).collect();
    let mut parts = parts?;
    if parts.len() != 3 {
        return None;
    }
    parts.push(pre);
    Some(parts)
}

/// What `git <args>` prints in `folder`, when it succeeds.
pub(crate) fn git(folder: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(folder)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

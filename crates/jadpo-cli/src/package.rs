//! `jadpo package` (area 11, "The surface" and "Developing and publishing
//! a package"; area 14; chunk 7.1b): `show` prints a package's
//! description, licence, version, kind and public surface, or one
//! declaration in full, or its guide; `check` runs the publishing bar in a
//! package's own repository and writes its `surface.jadpo_record`.
//! Installing (`add`, `update`, `remove`, `link`, `unlink`) comes with
//! chunk 7.1b2.
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
];

/// What the language decides `jadpo package` does, not built yet, and the
/// work that brings each.
const LATER: &[(&str, &str)] = &[
    ("add", "installing packages"),
    ("update", "installing packages"),
    ("remove", "installing packages"),
    ("link", "installing packages"),
    ("unlink", "installing packages"),
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

fn arguments(message: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_PACKAGE_ARGUMENTS, message)
        .reason(reason)
        .next(next)
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// `a`, `a and b`, `a, b and c`.
fn and_list(items: &[String]) -> String {
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
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> std::io::Result<Scratch> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "jadpo-package-check-{}-{nanos}",
            std::process::id()
        ));
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
fn index_name(text: &str) -> Option<String> {
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
    let Ok(index) = std::fs::read_to_string(folder.join(jadpo_db::PACKAGE_INDEX)) else {
        return no_package();
    };
    let Some(package) = index_name(&index) else {
        return no_package();
    };
    let Some((code, label)) = package.rsplit_once('.') else {
        return no_package();
    };
    if !jadpo_db::is_version_label(label) || code.is_empty() {
        return no_package();
    }
    let mount = format!(
        "shared/{}/{}/{label}",
        jadpo_db::PROVIDERS,
        jadpo_db::to_snake(code)
    );
    let scratch = match Scratch::new() {
        Ok(scratch) => scratch,
        Err(error) => return write_failed("a scratch folder", &error.to_string()),
    };
    let copied = std::fs::write(scratch.0.join("app.jadpo"), "application {}\n").and_then(|()| {
        for file in package_files(&folder) {
            let to = scratch.0.join(&mount).join(&file);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(folder.join(&file), to)?;
        }
        Ok(())
    });
    if let Err(error) = copied {
        return write_failed(
            "the scratch app the package is checked in",
            &error.to_string(),
        );
    }
    let shown = jadpo_db::shown_root(&cwd, &folder);
    let prefix = format!("{mount}/");
    let relative = |path: &str| path.strip_prefix(&prefix).unwrap_or(path).to_string();

    let loaded = jadpo_db::load(&scratch.0);
    let (db, project) = (&loaded.db, loaded.project);
    let mut compile: Vec<Diagnostic> = loaded
        .unreadable
        .iter()
        .map(|file| {
            Diagnostic::new(
                &rule::TOOLCHAIN_SOURCE_READ_FAILED,
                format!("`{}` couldn't be read", relative(&file.path)),
            )
            .reason(format!("Reading it failed: {}.", file.error))
            .next("Check it can be read, then run `jadpo package check` again")
        })
        .collect();
    compile.extend(
        jadpo_check::check(db, project)
            .into_iter()
            // The curated packages take the names Jadpo reserves for them.
            .filter(|d| d.rule.id != rule::PACKAGE_RESERVED_NAME.id),
    );
    let described = publishing::undescribed(db, project, &mount);
    let found = surface::surface(db, project, &mount);
    let sources: BTreeMap<String, String> = jadpo_db::source_texts(db, project)
        .into_iter()
        .filter_map(|(path, text)| Some((path.strip_prefix(&prefix)?.to_string(), text)))
        .collect();

    let mut items: Vec<Item> = Vec::new();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let compiled = compile.is_empty();
    items.push(if compiled {
        Item::passed("compiles", "no errors")
    } else {
        Item::failed("compiles", plural(compile.len(), "error", "errors"))
    });
    diagnostics.extend(compile);
    items.push(if described.is_empty() {
        Item::passed("described", "every public declaration")
    } else {
        Item::failed(
            "described",
            format!("{} no description", plural(described.len(), "has", "have")),
        )
    });
    diagnostics.extend(described);
    items.push(Item::later(
        "examples",
        "waits for how examples are written",
    ));
    let mut test_blocks: Vec<Block> = Vec::new();
    items.push(if compiled {
        let (item, problem, blocks) = run_tests(&scratch.0, &prefix);
        diagnostics.extend(problem);
        test_blocks = blocks;
        item
    } else {
        Item::later("tests", "the package has errors")
    });
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
    let mut written: Option<(String, bool)> = None;
    let mut version: Option<String> = None;
    let mut hint: Option<String> = None;
    match (&found, compiled) {
        (Some(surface), true) => {
            let now = surface.text(None);
            let releases = git
                .as_ref()
                .map(|git| git.releases(&package))
                .unwrap_or_default();
            let outcome = match releases.first() {
                Some(last)
                    if git
                        .as_ref()
                        .is_some_and(|git| git.unchanged_since(&last.tag)) =>
                {
                    Ok((
                        last.version.clone(),
                        format!("{}, released as {}", last.version, last.tag),
                        false,
                    ))
                }
                Some(last) => {
                    publishing::version(label, Some((&last.version, &last.surface)), &now)
                        .map(|bump| (bump.version.clone(), bump_words(&bump), true))
                }
                None => publishing::version(label, None, &now).map(|bump| {
                    (
                        bump.version.clone(),
                        format!("{}, the first release", bump.version),
                        true,
                    )
                }),
            };
            match outcome {
                Ok((next, words, new)) => {
                    items.push(Item::passed("version", words));
                    if new && git.is_some() {
                        hint = Some(next.clone());
                    }
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
                    diagnostics.push(breaking_change(&package, code, label, &since, &breaking));
                }
            }
        }
        _ => items.push(Item::later("version", "the package has errors")),
    }
    items.push(Item::later("changelog", "waits for the changelog's form"));
    let licence = found.as_ref().and_then(|s| s.licence.clone());
    let jadpo = found.as_ref().and_then(|s| s.jadpo.clone());
    items.push(match (licence, jadpo) {
        (Some(licence), Some(jadpo)) => {
            Item::passed("licence", format!("{licence}, from Jadpo {jadpo}"))
        }
        _ => Item::failed("licence", "the index doesn't say"),
    });

    if let (Some(surface), Some(version)) = (&found, &version) {
        let text = surface.text(Some(version));
        let path = folder.join(jadpo_db::record::SURFACE_FILE);
        let same = std::fs::read_to_string(&path).is_ok_and(|old| old == text);
        if !same && let Err(error) = std::fs::write(&path, &text) {
            return write_failed(jadpo_db::record::SURFACE_FILE, &error.to_string());
        }
        let shown_path = match &shown {
            Some(root) => format!("{root}/{}", jadpo_db::record::SURFACE_FILE),
            None => jadpo_db::record::SURFACE_FILE.to_string(),
        };
        written = Some((shown_path, !same));
    }

    // Each problem at its place in the package's folder.
    let diagnostics: Vec<Diagnostic> = diagnostics
        .into_iter()
        .map(|mut d| {
            if let Some(label) = &mut d.primary {
                label.span.path = relative(&label.span.path);
            }
            for label in &mut d.secondary {
                label.span.path = relative(&label.span.path);
            }
            d
        })
        .collect();
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
    if let Some((path, true)) = &written {
        screen.push(Block::Status {
            outcome: Outcome::Success,
            text: format!("Wrote {path}"),
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
        (
            "surface",
            match &written {
                Some((path, wrote)) => Json::object([
                    ("path", Json::from(path.as_str())),
                    ("written", (*wrote).into()),
                ]),
                None => Json::Null,
            },
        ),
    ]);
    Reply::Report {
        screen,
        data,
        diagnostics,
        sources,
    }
}

fn write_failed(what: &str, error: &str) -> Reply {
    Reply::failed(
        Diagnostic::new(
            &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
            format!("`jadpo package check` couldn't write {what}"),
        )
        .reason(format!("Writing it failed: {error}."))
        .next("Check that the folder can be written, then run `jadpo package check` again"),
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

/// Runs the package's tests in the scratch app: the bar's row, the
/// problem a failure gives, and each failed test's report.
fn run_tests(root: &Path, prefix: &str) -> (Item, Option<Diagnostic>, Vec<Block>) {
    let ran = jadpo_test::run(jadpo_test::Options {
        cwd: root.to_path_buf(),
        ..jadpo_test::Options::default()
    });
    match ran {
        jadpo_test::Ran::Done(mut run) => {
            for result in &mut run.results {
                if let Some(relative) = result.test.file.strip_prefix(prefix) {
                    result.test.file = relative.to_string();
                }
            }
            let (total, failed) = (run.results.len(), run.failed());
            if failed == 0 {
                return (
                    Item::passed("tests", plural(total, "test", "tests")),
                    None,
                    Vec::new(),
                );
            }
            let mut blocks = jadpo_test::screen(&run).into_blocks();
            blocks.pop();
            (
                Item::failed("tests", format!("{failed} of {}", plural(total, "test", "tests"))),
                Some(
                    Diagnostic::new(
                        &rule::TEST_FAILED,
                        format!("{failed} of {} failed", plural(total, "test", "tests")),
                    )
                    .reason("A package is published only when its own tests pass; each failed test's report is below.")
                    .next("Fix each failure, then run `jadpo package check` again"),
                ),
                blocks,
            )
        }
        jadpo_test::Ran::Refused { .. } | jadpo_test::Ran::NoProject => (
            Item::later("tests", "the package has errors"),
            None,
            Vec::new(),
        ),
        _ => (Item::passed("tests", "no tests"), None, Vec::new()),
    }
}

/// The package's repository, when its folder is in one.
struct Git {
    folder: PathBuf,
    /// The folder's path in the repository: `""` at its root, or
    /// `packages/postcodes/`.
    prefix: String,
}

/// A release: a `v<version>` tag whose surface names the package.
struct Release {
    tag: String,
    version: String,
    surface: String,
}

impl Git {
    fn of(folder: &Path) -> Option<Git> {
        let prefix = git(folder, &["rev-parse", "--show-prefix"])?;
        Some(Git {
            folder: folder.to_path_buf(),
            prefix: prefix.trim().to_string(),
        })
    }

    /// The package's releases, newest first.
    fn releases(&self, package: &str) -> Vec<Release> {
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
fn semver(tag: &str) -> Option<Vec<u64>> {
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
fn git(folder: &Path, args: &[&str]) -> Option<String> {
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

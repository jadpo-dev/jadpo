//! Packages (area 11, revised 2026-10-16; chunk 7.1): the package format,
//! the two kinds, where packages live, their names, and the lock.
//!
//! A package is a folder laid out as `shared/` is: installed ones in
//! `packages/<name>/<version>/`, the app's own integrations in
//! `shared/providers/<name>/<version>/`. Its index, `package.jadpo`, holds
//! one `package Name.vN { licence: ... jadpo: ... }` with a `///`
//! description; a provider package adds `provider.jadpo`, one `provider {
//! }` with a fixed set of keys. The kind follows from what is inside: a
//! package with `resources/` is a provider and needs `provider.jadpo`,
//! one without is a library.
//!
//! [`package_problems`] reads every package the project holds and the
//! lock: each installed package has its line in `jadpo.lock` and matches
//! its digest there, and each line has its package. A package's name joins
//! the names of the app's domains and libraries, so the app can't already
//! use it, and the app's own integration can't take a provider's reserved
//! brand name. Names inside a package are bare and its own (`jadpo-db`'s
//! symbol table scopes them to it); the app writes them through it.

use std::collections::BTreeMap;

use jadpo_db::{
    Db, File, Folder, ItemKind, Locked, Ownership, PackageHome, Place, Project, Source, item_tree,
    location, package_digest, parse_lock, symbols,
};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_syntax::ast::{self, AstNode, Decl};

use crate::case::to_snake;
use crate::tree::{self, file_span, node_span};

/// The keys every package index has, and the only ones (area 11, "The
/// index: `package.jadpo`").
pub const INDEX_KEYS: [&str; 2] = ["licence", "jadpo"];

/// The licences a package may declare (area 11): `Licence.<name>`.
pub const LICENCES: [&str; 8] = [
    "mit",
    "apache_2_0",
    "bsd_2_clause",
    "bsd_3_clause",
    "isc",
    "mpl_2_0",
    "unlicense",
    "proprietary",
];

/// The licence only a package installed from git may have.
pub const PROPRIETARY: &str = "proprietary";

/// The keys `provider.jadpo`'s `provider { }` may hold (area 11, "How the
/// API is reached").
pub const PROVIDER_KEYS: [&str; 15] = [
    "protocol",
    "hosts",
    "runs",
    "credential",
    "oauth",
    "idempotency",
    "error_code",
    "webhooks",
    "polls",
    "erasure",
    "max_concurrent",
    "settings",
    "imported",
    "description",
    "aws",
];

/// How a provider's bodies are sent: `Protocol.<name>`.
pub const PROTOCOLS: [&str; 4] = ["json_http", "form_http", "multipart_http", "aws_json"];

/// The keys of `hosts:`.
pub const HOST_KEYS: [&str; 2] = ["live", "sandbox"];

/// The keys of `webhooks:` (area 11, "Receiving from outside"; chunk
/// 7.7): how the provider signs them, the header one scheme reads, and
/// where each event's id, type and time are in the body; and
/// `handshake:`, how the provider checks an address first (chunk 7.7b).
pub const WEBHOOK_KEYS: [&str; 6] = [
    "check",
    "header",
    "event_id",
    "event_type",
    "occurred_at",
    "handshake",
];

/// The keys of `idempotency:`.
pub const IDEMPOTENCY_KEYS: [&str; 7] = [
    "header",
    "window",
    "max_length",
    "in_use",
    "reused",
    "new_key_on",
    "replays_failures",
];

/// The keys of `aws:` (chunk 7.7c): the service an AWS API's requests are
/// signed for, and the prefix its operations are named under in
/// `X-Amz-Target`.
pub const AWS_KEYS: [&str; 2] = ["service", "target"];

/// The keys of `runs:`.
pub const RUNS_KEYS: [&str; 3] = ["image", "port", "asks"];

/// Provider brand names Jadpo reserves for its curated packages (area 11,
/// "Names"), as folders: the app's own integration can't take one, so
/// `Stripe` in code always means the official package.
pub const RESERVED: [&str; 3] = ["stripe", "twilio", "github"];

/// One package the project holds: where it is, and its files.
struct Package {
    home: PackageHome,
    /// Its source files, by path.
    files: Vec<File>,
}

/// The packages the project holds, by folder.
fn packages(db: &dyn Db, project: Project) -> BTreeMap<String, Package> {
    let mut out: BTreeMap<String, Package> = BTreeMap::new();
    // A resource the app doesn't call into yet is still the package's
    // (chunk 7.1c2), though it isn't compiled.
    for &file in project.files(db).iter().chain(project.lazy(db)) {
        if let Ownership::Package(home) = &location(db, project, file).ownership {
            out.entry(home.folder.clone())
                .or_insert_with(|| Package {
                    home: home.clone(),
                    files: Vec::new(),
                })
                .files
                .push(file);
        }
    }
    for record in project.records(db) {
        if let Some((home, _)) = jadpo_db::package_at(&record.path) {
            out.entry(home.folder.clone()).or_insert_with(|| Package {
                home,
                files: Vec::new(),
            });
        }
    }
    out
}

/// Every problem with the project's packages and its lock.
#[salsa::tracked(returns(ref))]
pub fn package_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let all = packages(db, project);
    let lock = lock(db, project);
    for package in all.values() {
        let index = index(db, package, &mut out);
        provider(db, project, package, &mut out);
        names(db, project, package, &mut out);
        if package.home.installed {
            locked(
                db,
                project,
                package,
                index.as_ref(),
                lock.as_ref(),
                &mut out,
            );
        }
    }
    if let Some((path, locked)) = &lock {
        for line in locked {
            if matches!(line.source, Source::Link { .. }) {
                continue;
            }
            let installed = all
                .values()
                .any(|package| package.home.installed && package.home.versioned() == line.package);
            if !installed {
                out.push(not_installed(path, line));
            }
        }
    }
    out
}

/// Each package `jadpo.lock` links to a live checkout, refused where CI
/// and production build (area 11, "Developing and publishing a package"):
/// a link compiles the app against a folder on one machine, so it must
/// never ship. `jadpo check` and `jadpo test` add these when the `CI`
/// variable is set, and `jadpo build` will always.
pub fn links_refused(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let Some(record) = project
        .records(db)
        .iter()
        .find(|record| record.path == jadpo_db::LOCK)
    else {
        return Vec::new();
    };
    let Ok(text) = record.text.as_ref() else {
        return Vec::new();
    };
    let Ok(lines) = parse_lock(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in lines {
        let Source::Link { path } = &line.source else {
            continue;
        };
        let written = line.line();
        let mut offset = 0usize;
        let mut span = file_span(&record.path);
        for text_line in text.split_inclusive('\n') {
            if text_line.trim_end() == written {
                let end = offset + text_line.trim_end().len();
                span = Span::new(&record.path, offset as u32, end as u32);
                break;
            }
            offset += text_line.len();
        }
        let name = folder_of(&line.package);
        let name = name.rsplit('/').nth(1).unwrap_or(&line.package).to_string();
        out.push(
            Diagnostic::new(
                &rule::INSTALL_LINK_REFUSED,
                format!("`{}` is linked to `{path}`", line.package),
            )
            .at(span, "a live checkout")
            .reason("A link compiles the app against a checkout on one machine, so CI (the `CI` variable is set) and production builds refuse it, and a link never ships.")
            .next(format!(
                "Run `jadpo package unlink {name}`, then commit `{}`",
                jadpo_db::LOCK
            )),
        );
    }
    out
}

/// The lock's lines, when the project has one that reads; one that
/// doesn't is the records' to report.
fn lock(db: &dyn Db, project: Project) -> Option<(String, Vec<Locked>)> {
    let record = project
        .records(db)
        .iter()
        .find(|record| record.path == jadpo_db::LOCK)?;
    let text = record.text.as_ref().ok()?;
    Some((record.path.clone(), parse_lock(text).ok()?))
}

/// What a package's index says, once read.
struct Index {
    /// The licence's name, `mit`.
    licence: Option<String>,
    /// Where the licence is written, for a problem with it.
    licence_span: Option<Span>,
}

fn index_path(home: &PackageHome) -> String {
    format!("{}/{}", home.folder, jadpo_db::PACKAGE_INDEX)
}

/// The package's index: present, one `package` declaration naming the
/// package its folder holds, a description, and exactly `licence:` and
/// `jadpo:`.
fn index(db: &dyn Db, package: &Package, out: &mut Vec<Diagnostic>) -> Option<Index> {
    let home = &package.home;
    let path = index_path(home);
    let Some(file) = package
        .files
        .iter()
        .copied()
        .find(|file| *file.path(db) == path)
    else {
        let at = package
            .files
            .first()
            .map_or_else(|| home.folder.clone(), |file| file.path(db).clone());
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_INDEX_INVALID,
                format!("Package `{}` has no index", home.versioned()),
            )
            .at(file_span(&at), "")
            .reason(format!(
                "Every package has `{}` at its root: its description, its licence and the earliest Jadpo version it needs. It is how a package is known and installed.",
                jadpo_db::PACKAGE_INDEX
            ))
            .next(format!(
                "Add `{path}` holding `/// <what it does>` and `package {} {{ licence: Licence.mit  jadpo: \"1.0\" }}`",
                home.versioned()
            )),
        );
        return None;
    };
    let source = tree::source(db, file)?;
    let decls: Vec<ast::PackageDecl> = source
        .decls()
        .filter_map(|decl| match decl {
            Decl::PackageDecl(decl) => Some(decl),
            _ => None,
        })
        .collect();
    let Some(decl) = decls.first() else {
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_INDEX_INVALID,
                format!("`{path}` declares no package"),
            )
            .at(file_span(&path), "")
            .reason("A package's index holds its one `package` declaration.")
            .next(format!(
                "Write `package {} {{ licence: Licence.mit  jadpo: \"1.0\" }}` here, with a `///` description",
                home.versioned()
            )),
        );
        return None;
    };
    for extra in decls.iter().skip(1) {
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_INDEX_INVALID,
                format!("`{path}` declares a second package"),
            )
            .at(node_span(&path, extra.syntax()), "a second `package`")
            .reason("A package's index holds its one `package` declaration; one folder is one package at one version.")
            .next("Remove this declaration"),
        );
    }
    let written = decl
        .path()
        .map(|path| tree::written(path.syntax()))
        .unwrap_or_default();
    let name_span = decl.path().map_or_else(
        || node_span(&path, decl.syntax()),
        |p| node_span(&path, p.syntax()),
    );
    if written != home.versioned() {
        out.push(name_mismatch(name_span.clone(), &written, home));
    }
    if crate::graph::text::doc(decl.syntax()).is_none() {
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_INDEX_INVALID,
                format!("Package `{written}` has no description"),
            )
            .at(name_span.clone(), "no `///` description")
            .reason("A package's index starts with a `///` description of what the package is for: it heads `jadpo package show` and the registry's page.")
            .next("Add a `///` line above `package` saying what the package does"),
        );
    }
    let mut licence = None;
    let mut licence_span = None;
    let mut seen: Vec<String> = Vec::new();
    for (name, member) in tree::members(decl.member_list()) {
        let key = tree::name_text(&name).unwrap_or_default();
        let span = node_span(&path, member.syntax());
        if !INDEX_KEYS.contains(&key.as_str()) {
            let (reason, next) = if key == "requires" {
                (
                    "Packages have no dependencies: a package that needs another's code carries a copy, so installing one is one folder and one line in the lock.".to_string(),
                    "Remove `requires:`, and copy what the package needs into it".to_string(),
                )
            } else {
                (
                    format!(
                        "Every package's index is the same: a `///` description and two keys, `licence:` and `jadpo:`, and nothing else; `{key}:` isn't one of them. Files are found by name, so the index lists none."
                    ),
                    format!("Remove `{key}:`"),
                )
            };
            out.push(
                Diagnostic::new(
                    &rule::PACKAGE_INDEX_INVALID,
                    format!("A package's index can't hold `{key}:`"),
                )
                .at(span.clone(), "not an index key")
                .reason(reason)
                .next(next)
                .edit(tree::removal(&path, &member), ""),
            );
            continue;
        }
        if seen.contains(&key) {
            continue;
        }
        seen.push(key.clone());
        match key.as_str() {
            "licence" => {
                let value = member.value().map(|value| tree::written(value.syntax()));
                match value.as_deref().and_then(|value| value.strip_prefix("Licence.")) {
                    Some(name) if LICENCES.contains(&name) => {
                        licence = Some(name.to_string());
                        licence_span = Some(span);
                    }
                    _ => out.push(bad_value(
                        span,
                        "licence",
                        value.as_deref().unwrap_or(""),
                        &format!(
                            "A package's licence is one of {}.",
                            crate::and_list(
                                &LICENCES
                                    .iter()
                                    .map(|name| format!("`Licence.{name}`"))
                                    .collect::<Vec<_>>()
                            )
                        ),
                        "Write `licence: Licence.mit`, or the licence the package is published under",
                    )),
                }
            }
            _ => {
                let value = member.value().map(|value| tree::written(value.syntax()));
                let version = value
                    .as_deref()
                    .and_then(|value| value.strip_prefix('"')?.strip_suffix('"'));
                if !version.is_some_and(is_language_version) {
                    out.push(bad_value(
                        span,
                        "jadpo",
                        value.as_deref().unwrap_or(""),
                        "`jadpo:` is the earliest Jadpo version the package needs, its major and minor numbers as text.",
                        "Write `jadpo: \"1.0\"`, or the earliest version the package needs",
                    ));
                }
            }
        }
    }
    let span = node_span(&path, decl.syntax());
    for key in INDEX_KEYS {
        if !seen.iter().any(|seen| seen == key) {
            out.push(
                Diagnostic::new(
                    &rule::PACKAGE_INDEX_INVALID,
                    format!("Package `{written}` doesn't give `{key}:`"),
                )
                .at(span.clone(), format!("no `{key}:`"))
                .reason(if key == "licence" {
                    "Every package declares its licence, from a fixed list, so an app knows on what terms it uses the code.".to_string()
                } else {
                    "Every package gives the earliest Jadpo version it needs, so an older Jadpo refuses it rather than misreading it.".to_string()
                })
                .next(if key == "licence" {
                    "Add `licence: Licence.mit`, or the licence the package is published under"
                        .to_string()
                } else {
                    "Add `jadpo: \"1.0\"`, or the earliest version the package needs".to_string()
                }),
            );
        }
    }
    Some(Index {
        licence,
        licence_span,
    })
}

/// `"1.0"`: a major and a minor number.
fn is_language_version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_digit())
                && (*part == "0" || !part.starts_with('0'))
        })
}

fn bad_value(span: Span, key: &str, written: &str, reason: &str, next: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::PACKAGE_INDEX_INVALID,
        format!("`{written}` isn't a package's `{key}:`"),
    )
    .at(span, "not one Jadpo reads")
    .reason(reason)
    .next(next)
}

fn name_mismatch(span: Span, written: &str, home: &PackageHome) -> Diagnostic {
    Diagnostic::new(
        &rule::PACKAGE_NAME_MISMATCH,
        format!(
            "`{written}` is in the folder of `{}`",
            home.versioned()
        ),
    )
    .at(span.clone(), format!("the folder says `{}`", home.versioned()))
    .reason(format!(
        "A package's folder is its name in `lower_snake_case` and its version, `{}/{}`, so its name in code is `{}`, the folder's in `UpperCamelCase`, and the index gives the same.",
        home.name,
        home.version,
        home.versioned()
    ))
    .next(format!("Write `package {}`", home.versioned()))
    .edit(span, home.versioned())
}

/// `provider.jadpo`: one `provider { }` with the fixed keys, `protocol:`
/// among them, and where the API is from one place; and the kind: a
/// package with resources has one, and a provider carries no
/// translations.
fn provider(db: &dyn Db, project: Project, package: &Package, out: &mut Vec<Diagnostic>) {
    let home = &package.home;
    let provider_path = format!("{}/{}", home.folder, jadpo_db::PROVIDER_FILE);
    let provider_file = package
        .files
        .iter()
        .copied()
        .find(|file| *file.path(db) == provider_path);
    let in_folder = |folder: Folder| {
        package
            .files
            .iter()
            .copied()
            .filter(move |file| location(db, project, *file).place == Place::Folder(folder))
    };
    if provider_file.is_none()
        && let Some(file) = in_folder(Folder::Resources).next()
    {
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_PROVIDER_MISSING,
                format!(
                    "Package `{}` has resources but no `{}`",
                    home.versioned(),
                    jadpo_db::PROVIDER_FILE
                ),
            )
            .at(file_span(file.path(db)), "")
            .reason(format!(
                "A package with `resources/` describes an API, so it says how the API is reached in `{}`: its protocol, hosts and credential. A package without resources is a library.",
                jadpo_db::PROVIDER_FILE
            ))
            .next(format!(
                "Add `{provider_path}` with `provider {{ protocol: Protocol.json_http ... }}`, or move the resources out if this is a library"
            )),
        );
    }
    let Some(file) = provider_file else {
        return;
    };
    for translations in in_folder(Folder::Translations) {
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_PROVIDER_TRANSLATIONS,
                format!(
                    "Provider package `{}` carries translations",
                    home.versioned()
                ),
            )
            .at(file_span(translations.path(db)), "")
            .reason("A provider package shows no text of its own: its failures are always mapped to the app's own, which the app words. Only a library package carries translations, for its own messages and failures.")
            .next("Remove the package's `translations/`, and word what users see in the app's own failures"),
        );
    }
    let path = provider_path;
    let Some(source) = tree::source(db, file) else {
        return;
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
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_PROVIDER_INVALID,
                format!("`{path}` declares no `provider`"),
            )
            .at(file_span(&path), "")
            .reason("A provider package's `provider.jadpo` holds one `provider { }`: how its API is reached and called.")
            .next("Write `provider { protocol: Protocol.json_http ... }` here"),
        );
        return;
    };
    let mut seen: BTreeMap<String, Span> = BTreeMap::new();
    for (name, member) in tree::members(decl.member_list()) {
        let key = tree::name_text(&name).unwrap_or_default();
        let span = node_span(&path, member.syntax());
        if !PROVIDER_KEYS.contains(&key.as_str()) {
            let near = jadpo_diag::nearest(&key, PROVIDER_KEYS.iter().copied());
            let mut problem = Diagnostic::new(
                &rule::PACKAGE_PROVIDER_INVALID,
                format!("`provider` has no key `{key}:`"),
            )
            .at(node_span(&path, name.syntax()), "not a provider key")
            .reason(format!(
                "`provider {{ }}` has one fixed set of keys: {}.{}",
                crate::and_list(
                    &PROVIDER_KEYS
                        .iter()
                        .map(|key| format!("`{key}:`"))
                        .collect::<Vec<_>>()
                ),
                near.map_or_else(String::new, |near| format!(" Did you mean `{near}:`?"))
            ));
            problem = match near {
                Some(near) => problem
                    .next(format!("Use `{near}:` instead of `{key}:`"))
                    .edit(node_span(&path, name.syntax()), near),
                None => problem.next(format!("Remove `{key}:`")),
            };
            out.push(problem);
            continue;
        }
        seen.entry(key.clone()).or_insert(span.clone());
        match key.as_str() {
            "protocol" => {
                let value = member
                    .value()
                    .map(|value| tree::written(value.syntax()))
                    .unwrap_or_default();
                let known = value
                    .strip_prefix("Protocol.")
                    .is_some_and(|name| PROTOCOLS.contains(&name));
                if !known {
                    out.push(
                        Diagnostic::new(
                            &rule::PACKAGE_PROVIDER_INVALID,
                            format!("`{value}` isn't a protocol"),
                        )
                        .at(span, "not a protocol")
                        .reason(format!(
                            "`protocol:` says how bodies are sent: {}.",
                            crate::and_list(
                                &PROTOCOLS
                                    .iter()
                                    .map(|name| format!("`Protocol.{name}`"))
                                    .collect::<Vec<_>>()
                            )
                        ))
                        .next("Write `protocol: Protocol.json_http`, or the protocol the API uses"),
                    );
                }
            }
            "hosts" => nested_keys(&path, &key, &member, &HOST_KEYS, out),
            "idempotency" => {
                nested_keys(&path, &key, &member, &IDEMPOTENCY_KEYS, out);
                let has_window = tree::members(member.block())
                    .iter()
                    .any(|(name, _)| tree::name_text(name).as_deref() == Some("window"));
                if member.block().is_some() && !has_window {
                    out.push(
                        Diagnostic::new(
                            &rule::PACKAGE_PROVIDER_INVALID,
                            "`idempotency:` doesn't give `window:`",
                        )
                        .at(span, "no `window:`")
                        .reason("`window:` is how long the provider keeps an idempotency key; a retry after it would make the change again, so it is required.")
                        .next("Add `window:` with the provider's documented time, such as `window: 24h`"),
                    );
                }
            }
            "runs" => nested_keys(&path, &key, &member, &RUNS_KEYS, out),
            "aws" => nested_keys(&path, &key, &member, &AWS_KEYS, out),
            "webhooks" => webhooks(&path, &member, out),
            "settings" => crate::connections::provider_settings(&path, &member, out),
            _ => {}
        }
    }
    if !seen.contains_key("protocol") {
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_PROVIDER_INVALID,
                "`provider` doesn't give `protocol:`",
            )
            .at(node_span(&path, decl.syntax()), "no `protocol:`")
            .reason("`protocol:` says how the API's bodies are sent, and is required.")
            .next("Add `protocol: Protocol.json_http`, or the protocol the API uses"),
        );
    }
    aws_needs(&path, &decl, out);
    if let (Some(hosts), Some(_)) = (seen.get("hosts"), seen.get("runs")) {
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_PROVIDER_INVALID,
                "`provider` gives both `hosts:` and `runs:`",
            )
            .at(hosts.clone(), "a fixed host")
            .reason("Where an API is comes from exactly one place: `hosts:` when it is on the internet, `runs:` when Jadpo runs it in a container, or neither when the app gives `host:` on the connection.")
            .next("Keep `hosts:` or `runs:`, and remove the other"),
        );
    }
}

/// What an AWS API needs of `aws:` (chunk 7.7c): `target:` for
/// `Protocol.aws_json`, which names each operation under it, and
/// `service:` for `CredentialScheme.aws_sigv4`, which signs for it.
fn aws_needs(path: &str, decl: &ast::SingletonDecl, out: &mut Vec<Diagnostic>) {
    let members = tree::members(decl.member_list());
    let written = |key: &str| {
        members
            .iter()
            .find(|(name, _)| tree::name_text(name).as_deref() == Some(key))
            .and_then(|(_, member)| member.value())
            .map(|value| tree::written(value.syntax()))
    };
    let aws: Vec<(String, Option<String>)> = members
        .iter()
        .find(|(name, _)| tree::name_text(name).as_deref() == Some("aws"))
        .map(|(_, member)| {
            tree::members(member.block())
                .iter()
                .filter_map(|(key, inner)| {
                    let text = match inner.value().as_ref().map(crate::types::Const::from_expr) {
                        Some(crate::types::Const::Str(text)) if !text.is_empty() => Some(text),
                        _ => None,
                    };
                    Some((tree::name_text(key)?, text))
                })
                .collect()
        })
        .unwrap_or_default();
    let has = |key: &str| aws.iter().any(|(name, text)| name == key && text.is_some());
    let needs = [
        (
            written("protocol").as_deref() == Some("Protocol.aws_json"),
            "target",
            "`Protocol.aws_json` names each operation in `X-Amz-Target` under the API's prefix, `aws: { target: }`, such as `\"AmazonSQS\"`.",
        ),
        (
            written("credential").as_deref() == Some("CredentialScheme.aws_sigv4"),
            "service",
            "`CredentialScheme.aws_sigv4` signs each request for the API's service, `aws: { service: }`, such as `\"sqs\"`.",
        ),
    ];
    for (needed, key, reason) in needs {
        if needed && !has(key) {
            out.push(
                Diagnostic::new(
                    &rule::PACKAGE_PROVIDER_INVALID,
                    format!("`provider` doesn't give `aws: {{ {key}: }}`"),
                )
                .at(
                    node_span(path, decl.syntax()),
                    format!("no `aws: {{ {key}: }}`"),
                )
                .reason(reason)
                .next(format!(
                    "Add `aws: {{ {key}: \"...\" }}` with the API's own name"
                )),
            );
        }
    }
}

/// `webhooks:` (chunk 7.7): `check:` one of the built-in signature
/// schemes, `header:` for the one that reads a header the provider names
/// and only for it, `event_id:` and `event_type:` given, and each place in
/// the body a dotted path; `handshake:` one of the built-in handshakes,
/// signed as its provider's scheme signs it (chunk 7.7b).
fn webhooks(path: &str, member: &ast::Member, out: &mut Vec<Diagnostic>) {
    use jadpo_std::provider::SignatureScheme;
    let span = node_span(path, member.syntax());
    let Some(block) = member.block() else {
        out.push(
            Diagnostic::new(&rule::PACKAGE_PROVIDER_INVALID, "`webhooks:` isn't a block")
                .at(span, "not a block")
                .reason("`webhooks:` says how the provider signs its webhooks, `check:`, and where each event's id and type are in its body, `event_id:` and `event_type:`.")
                .next("Write `webhooks: { check: SignatureScheme.stripe  event_id: \"id\"  event_type: \"type\" }`"),
        );
        return;
    };
    nested_keys(path, "webhooks", member, &WEBHOOK_KEYS, out);
    let members = tree::members(Some(block));
    let find = |key: &str| {
        members
            .iter()
            .find(|(name, _)| tree::name_text(name).as_deref() == Some(key))
            .map(|(_, inner)| inner)
    };
    let value_span = |inner: &ast::Member| {
        inner.value().map_or_else(
            || node_span(path, inner.syntax()),
            |value| node_span(path, value.syntax()),
        )
    };
    let schemes: Vec<String> = SignatureScheme::BUILT
        .iter()
        .map(|name| format!("`SignatureScheme.{name}`"))
        .collect();
    let scheme = match find("check") {
        None => {
            out.push(
                Diagnostic::new(&rule::PACKAGE_PROVIDER_INVALID, "`webhooks:` doesn't give `check:`")
                    .at(span.clone(), "no `check:`")
                    .reason(format!(
                        "`check:` names how the provider signs its webhooks, which the runtime checks before anything in one is kept: {}.",
                        crate::or_list(&schemes)
                    ))
                    .next("Add `check:` with the provider's scheme, such as `check: SignatureScheme.standard_webhooks`"),
            );
            None
        }
        Some(inner) => {
            let written = inner
                .value()
                .map(|value| tree::written(value.syntax()))
                .unwrap_or_default();
            let name = written.strip_prefix("SignatureScheme.").unwrap_or_default();
            if SignatureScheme::BUILT.contains(&name) {
                Some(name.to_string())
            } else {
                out.push(
                    Diagnostic::new(
                        &rule::PACKAGE_PROVIDER_INVALID,
                        format!("`{written}` isn't a signature scheme"),
                    )
                    .at(value_span(inner), "not a signature scheme")
                    .reason(format!(
                        "`check:` names one of the built-in signature schemes: {}.",
                        crate::or_list(&schemes)
                    ))
                    .next("Name the scheme the provider signs with, such as `SignatureScheme.standard_webhooks`"),
                );
                None
            }
        }
    };
    match (scheme.as_deref(), find("header")) {
        (Some("hmac_sha256"), None) => out.push(
            Diagnostic::new(&rule::PACKAGE_PROVIDER_INVALID, "`webhooks:` doesn't give `header:`")
                .at(span.clone(), "no `header:`")
                .reason("`SignatureScheme.hmac_sha256` reads the signature from the header the provider names, which `header:` gives.")
                .next("Add `header:` with the provider's header, such as `header: \"X-Signature\"`"),
        ),
        (Some("hmac_sha256"), Some(inner)) => {
            let fits = matches!(
                inner.value().as_ref().map(crate::types::Const::from_expr),
                Some(crate::types::Const::Str(text)) if !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            );
            if !fits {
                out.push(
                    Diagnostic::new(&rule::PACKAGE_PROVIDER_INVALID, "`header:` isn't a header's name")
                        .at(value_span(inner), "not a header's name")
                        .reason("`header:` names the header the signature comes in, as text of letters, digits and dashes.")
                        .next("Write the header's name as text, such as `header: \"X-Signature\"`"),
                );
            }
        }
        (Some(other), Some(inner)) => out.push(
            Diagnostic::new(
                &rule::PACKAGE_PROVIDER_INVALID,
                format!("`SignatureScheme.{other}` takes no `header:`"),
            )
            .at(value_span(inner), "not read")
            .reason(format!(
                "Only `SignatureScheme.hmac_sha256` reads a header the provider names; `SignatureScheme.{other}` knows where its signature is."
            ))
            .next("Remove `header:`"),
        ),
        _ => {}
    }
    for (key, required) in [
        ("event_id", true),
        ("event_type", true),
        ("occurred_at", false),
    ] {
        match find(key) {
            None if required => out.push(
                Diagnostic::new(
                    &rule::PACKAGE_PROVIDER_INVALID,
                    format!("`webhooks:` doesn't give `{key}:`"),
                )
                .at(span.clone(), format!("no `{key}:`"))
                .reason(
                    "`event_id:` says where each webhook's event id is in its body, by which repeats are dropped, and `event_type:` where its type is, which names the event it carries.",
                )
                .next(format!("Add `{key}:` with the place in the body, such as `{key}: \"{}\"`", if key == "event_id" { "id" } else { "type" })),
            ),
            None => {}
            Some(inner) => {
                let fits = matches!(
                    inner.value().as_ref().map(crate::types::Const::from_expr),
                    Some(crate::types::Const::Str(text)) if !text.is_empty() && text.split('.').all(|part| !part.is_empty())
                );
                if !fits {
                    out.push(
                        Diagnostic::new(
                            &rule::PACKAGE_PROVIDER_INVALID,
                            format!("`{key}:` isn't a place in the body"),
                        )
                        .at(value_span(inner), "not a place")
                        .reason(format!(
                            "`{key}:` says where in a webhook's JSON body the value is, as its members' names from the root joined by dots, such as `\"data.id\"`."
                        ))
                        .next(format!("Write `{key}:` as text, such as `{key}: \"id\"`")),
                    );
                }
            }
        }
    }
    if let Some(inner) = find("handshake") {
        use jadpo_std::provider::WebhookHandshake;
        let written = inner
            .value()
            .map(|value| tree::written(value.syntax()))
            .unwrap_or_default();
        let name = written
            .strip_prefix("WebhookHandshake.")
            .unwrap_or_default();
        let handshakes: Vec<String> = WebhookHandshake::NAMED
            .iter()
            .map(|name| format!("`WebhookHandshake.{name}`"))
            .collect();
        match WebhookHandshake::from_name(name) {
            None => out.push(
                Diagnostic::new(
                    &rule::PACKAGE_PROVIDER_INVALID,
                    format!("`{written}` isn't a webhook handshake"),
                )
                .at(value_span(inner), "not a handshake")
                .reason(format!(
                    "`handshake:` names how the provider checks an address before sending to it, one of the built-in rules the runtime answers: {}.",
                    crate::or_list(&handshakes)
                ))
                .next("Name the provider's handshake, such as `WebhookHandshake.slack`"),
            ),
            // SNS's confirmation and Slack's challenge are signed as their
            // provider's events are, and checked before they are answered.
            Some(handshake) => {
                if let (Some(needed), Some(scheme)) = (handshake.scheme(), scheme.as_deref())
                    && needed != scheme
                {
                    out.push(
                        Diagnostic::new(
                            &rule::PACKAGE_PROVIDER_INVALID,
                            format!("`WebhookHandshake.{name}` needs `SignatureScheme.{needed}`"),
                        )
                        .at(value_span(inner), "signed otherwise")
                        .reason(format!(
                            "The provider signs its handshake as it signs its webhooks, with `SignatureScheme.{needed}`, and the runtime checks that signature before it answers."
                        ))
                        .next(format!("Write `check: SignatureScheme.{needed}`")),
                    );
                }
            }
        }
    }
}

/// A block's keys, each one of `keys`.
fn nested_keys(
    path: &str,
    key: &str,
    member: &ast::Member,
    keys: &[&str],
    out: &mut Vec<Diagnostic>,
) {
    for (name, _) in tree::members(member.block()) {
        let inner = tree::name_text(&name).unwrap_or_default();
        if keys.contains(&inner.as_str()) {
            continue;
        }
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_PROVIDER_INVALID,
                format!("`{key}:` has no key `{inner}:`"),
            )
            .at(node_span(path, name.syntax()), "not one of its keys")
            .reason(format!(
                "`{key}:` holds {}.",
                crate::and_list(
                    &keys
                        .iter()
                        .map(|key| format!("`{key}:`"))
                        .collect::<Vec<_>>()
                )
            ))
            .next(format!("Remove `{inner}:`")),
        );
    }
}

/// A package's name against the app's (area 11, "Names"): its own
/// integration can't take a reserved brand name, and no package's name
/// can be a domain's, a library's or a declaration's of the app, or
/// another package's.
fn names(db: &dyn Db, project: Project, package: &Package, out: &mut Vec<Diagnostic>) {
    let home = &package.home;
    let Some(first) = package.files.first() else {
        return;
    };
    let at = package
        .files
        .iter()
        .find(|file| *file.path(db) == index_path(home))
        .unwrap_or(first)
        .path(db)
        .clone();
    let span = file_span(&at);
    let folded = home.name.replace('_', "");
    if !home.installed && RESERVED.contains(&folded.as_str()) {
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_RESERVED_NAME,
                format!("`{}` is a reserved provider name", home.code),
            )
            .at(span.clone(), "")
            .reason(format!(
                "Jadpo reserves the names of the providers it curates packages for, so `{}` in code always means the official package. The app's own integration takes a name of its own.",
                home.code
            ))
            .next(format!(
                "Rename the folder `{}` to a name of the app's own, such as `{}_api`, and the index with it",
                home.folder, home.name
            )),
        );
    }
    let table = symbols(db, project);
    let mut taken: Vec<String> = Vec::new();
    if table.domains.contains(&home.name) {
        taken.push(format!("domain `{}`", home.name));
    }
    for (name, found) in &table.declarations {
        if *name != home.code && to_snake(name) != home.name {
            continue;
        }
        for symbol in found {
            if symbol.ownership.package().is_some() || symbol.provided.is_some() {
                continue;
            }
            let is_library = symbol.kind == ItemKind::Library && symbol.library.is_none();
            if *name == home.code || is_library {
                taken.push(format!(
                    "{} `{name}` in `{}`",
                    symbol.kind.word(),
                    symbol.path
                ));
            }
        }
    }
    // While two versions are held, a library package's text is worded
    // under its folder's name and version (`postcodes_v3: { }`), which no
    // domain can share either (chunk 7.1c3).
    if home.through != home.code
        && table.domains.contains(&home.wording_key())
        && package
            .files
            .iter()
            .any(|file| location(db, project, *file).place == Place::Folder(Folder::Translations))
    {
        taken.push(format!(
            "domain `{}`, the key its wording takes while two versions are held",
            home.wording_key()
        ));
    }
    // Two versions of one package, both installed or both the app's own,
    // sit side by side with names carrying the version (chunk 7.1c2); an
    // installed package and the app's own of one name are a clash.
    if let Some(homes) = table.homes.get(&home.code)
        && let Some(other) = homes
            .iter()
            .find(|other| other.folder < home.folder && other.installed != home.installed)
    {
        taken.push(format!(
            "package `{}` in `{}`",
            other.versioned(),
            other.folder
        ));
    }
    if taken.is_empty() {
        return;
    }
    out.push(
        Diagnostic::new(
            &rule::PACKAGE_NAME_TAKEN,
            format!("Package `{}`'s name is already taken", home.code),
        )
        .at(span, "")
        .reason(format!(
            "A package's name joins the names of the app's domains and libraries, which are unique among themselves, and two packages never share a name in code, so `{}.` means one thing; it is already {}.",
            home.code,
            crate::and_list(&taken)
        ))
        .next(if home.installed {
            "Rename the app's own declaration with `jadpo rename` first, then install the package".to_string()
        } else {
            "Rename one of them".to_string()
        }),
    );
}

/// An installed package against the lock: its line, its digest and, for a
/// proprietary package, a git source.
fn locked(
    db: &dyn Db,
    project: Project,
    package: &Package,
    index: Option<&Index>,
    lock: Option<&(String, Vec<Locked>)>,
    out: &mut Vec<Diagnostic>,
) {
    let home = &package.home;
    let at = index_path(home);
    let at = if package.files.iter().any(|file| *file.path(db) == at) {
        at
    } else {
        package
            .files
            .first()
            .map_or_else(|| home.folder.clone(), |file| file.path(db).clone())
    };
    let line =
        lock.and_then(|(_, lines)| lines.iter().find(|line| line.package == home.versioned()));
    let Some(line) = line else {
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_NOT_LOCKED,
                format!("Package `{}` isn't in `{}`", home.versioned(), jadpo_db::LOCK),
            )
            .at(file_span(&at), "")
            .reason(format!(
                "`{}/` holds only installed packages, each with its line in `{}`: its version, digest and source. A package with no line was copied in rather than installed, so nothing says where it came from.",
                jadpo_db::PACKAGES,
                jadpo_db::LOCK
            ))
            .next(format!(
                "Install it with `jadpo package add`, or, if it is the app's own code, move it to `shared/{}/{}/{}/`",
                jadpo_db::PROVIDERS,
                home.name,
                home.version
            )),
        );
        return;
    };
    let digest = match &line.source {
        Source::Registry { digest, .. }
        | Source::Git { digest, .. }
        | Source::Path { digest, .. } => digest,
        Source::Link { .. } => return,
    };
    let now = digest_of(db, project, home);
    if now.as_deref() != Ok(digest.as_str()) {
        let found = match &now {
            Ok(now) => format!("its files now give `{now}`"),
            Err(path) => format!("`{path}` can't be read"),
        };
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_DIGEST_MISMATCH,
                format!(
                    "Package `{}` differs from what was installed",
                    home.versioned()
                ),
            )
            .at(file_span(&at), "")
            .reason(format!(
                "An installed package is read-only: `{}` locks it with digest `{digest}`, and {found}, so a file in `{}/` was changed, added or removed since it was installed.",
                jadpo_db::LOCK,
                home.folder
            ))
            .next(format!(
                "Restore `{}/` from version control, or install the package again with `jadpo package add`; change the app's own code instead",
                home.folder
            )),
        );
    }
    if let Some(Index {
        licence: Some(licence),
        licence_span: Some(span),
    }) = index
        && licence == PROPRIETARY
        && !matches!(line.source, Source::Git { .. })
    {
        out.push(
            Diagnostic::new(
                &rule::PACKAGE_LICENCE_NOT_ALLOWED,
                format!(
                    "Package `{}` is proprietary but wasn't installed from git",
                    home.versioned()
                ),
            )
            .at(span.clone(), "proprietary")
            .reason("Only a package installed from git, a company's private package, may be `Licence.proprietary`; the registry and other sources carry packages others may use.")
            .next("Install the package from its git repository, or publish it under one of the open licences"),
        );
    }
}

/// An installed package's digest from the files it holds now, or the path
/// of one that can't be read.
pub fn digest_of(db: &dyn Db, project: Project, home: &PackageHome) -> Result<String, String> {
    let prefix = format!("{}/", home.folder);
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for &file in project.files(db).iter().chain(project.lazy(db)) {
        if let Some(relative) = file.path(db).strip_prefix(&prefix) {
            files.push((relative.to_string(), file.text(db).clone().into_bytes()));
        }
    }
    for record in project.records(db) {
        if let Some(relative) = record.path.strip_prefix(&prefix) {
            match &record.text {
                Ok(text) => files.push((relative.to_string(), text.clone().into_bytes())),
                Err(_) => return Err(record.path.clone()),
            }
        }
    }
    let borrowed: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    Ok(package_digest(&borrowed))
}

fn not_installed(path: &str, line: &Locked) -> Diagnostic {
    let folder = folder_of(&line.package);
    Diagnostic::new(
        &rule::PACKAGE_NOT_INSTALLED,
        format!("`{}` names `{}`, which isn't installed", jadpo_db::LOCK, line.package),
    )
    .at(file_span(path), "")
    .reason(format!(
        "Each line of the lock is a package in `{}/`; `{}` has none at `{folder}/`, so the lock and the folder disagree, as removing the folder by hand leaves them.",
        jadpo_db::PACKAGES,
        line.package
    ))
    .next(format!(
        "Restore `{folder}/` from version control, or remove the package with `jadpo package remove`"
    ))
}

/// `packages/<name>/<version>` for `Name.vN`.
fn folder_of(package: &str) -> String {
    match package.rsplit_once('.') {
        Some((name, version)) => format!("{}/{}/{version}", jadpo_db::PACKAGES, to_snake(name)),
        None => format!("{}/{package}", jadpo_db::PACKAGES),
    }
}

/// The package declarations of a file that the first part of chunk 7.1
/// doesn't run: a package's subscribers (chunk 7.6) and its capability
/// mappings (chunk 7.3c). Its events are read since chunk 7.7, each a
/// webhook's event the app subscribes to, and its actions since chunk 7.6.
#[salsa::tracked(returns(ref))]
pub fn unbuilt_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let location = location(db, project, file);
    let Ownership::Package(home) = &location.ownership else {
        return Vec::new();
    };
    let what = match location.place {
        Place::Folder(Folder::Subscribers) => "A package's subscribers",
        // `receiving`'s is read since chunk 7.7c ([`crate::receiving`]).
        Place::Folder(Folder::Provides) if location.stem == "receiving" => return Vec::new(),
        Place::Folder(Folder::Provides) => "A package's capability mappings",
        _ => return Vec::new(),
    };
    let path = file.path(db);
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    item_tree(db, file)
        .items
        .iter()
        .map(|item| {
            Diagnostic::new(
                &rule::TOOLCHAIN_NOT_BUILT_YET,
                format!("{what} aren't built yet"),
            )
            .at(
                tree::name_span(&source, path, item.index, None),
                "declared, not built",
            )
            .reason(format!(
                "The language declares {} in `{}`, but this compiler can't run them yet.",
                what.to_lowercase(),
                home.versioned()
            ))
            .next(format!("Do without `{}` for now", item.name))
        })
        .collect()
}

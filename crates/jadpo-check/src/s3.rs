//! The S3 driver's settings (area 16, "The S3 driver"; area 11,
//! "Connecting in `app.jadpo`"): each connection with `driver: Driver.s3`,
//! its `host:`, `region:`, `bucket:` and `credential: { key_id: secret: }`.

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_syntax::ast;
use jadpo_syntax::ast::AstNode;

use crate::config::config;
use crate::data::ConnectionKind;
use crate::emails::{application, literal_text, reads};
use crate::lookups::Lookup;
use crate::tree::{self, name_text, node_span};
use crate::types::{Prim, TypeView};

/// One of a connection's settings, written out or read from a setting.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum S3Text {
    Setting(String),
    Written(String),
}

/// A connection with `driver: Driver.s3`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct S3Connection {
    pub name: String,
    pub host: Option<S3Text>,
    pub region: Option<S3Text>,
    pub bucket: Option<S3Text>,
    /// The settings `credential: { key_id: secret: }` reads.
    pub key_id: Option<String>,
    pub secret: Option<String>,
}

/// Every S3 connection, and what is wrong with them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct S3Connections {
    pub list: Vec<S3Connection>,
    pub problems: Vec<Diagnostic>,
}

/// The settings a connection with `driver: Driver.s3` must give, by what
/// each says.
const REQUIRED: [(&str, &str); 3] = [
    ("host", "the service's address, `https://…`"),
    ("region", "the region it signs for, `auto` on Cloudflare R2"),
    ("bucket", "the bucket its files are kept in"),
];

/// Each connection with `driver: Driver.s3`.
#[salsa::tracked(returns(ref))]
pub fn s3_connections(db: &dyn Db, project: Project) -> S3Connections {
    let mut out = S3Connections::default();
    let lookup = Lookup::new(db, project);
    for (path, key, member) in application(db, project) {
        if name_text(&key).as_deref() != Some("connections") {
            continue;
        }
        for (name, entry) in tree::members(member.block()) {
            let Some(connection) = name_text(&name) else {
                continue;
            };
            if crate::data::connection_kind(entry.block()) != ConnectionKind::Files
                || crate::data::files_driver(entry.block()) != Some(jadpo_std::Driver::S3)
            {
                continue;
            }
            out.list.push(read_one(
                db,
                project,
                &lookup,
                &path,
                (&connection, &name, entry.block()),
                &mut out.problems,
            ));
        }
    }
    out.list.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// One connection's settings, its problems added to `problems`.
fn read_one(
    db: &dyn Db,
    project: Project,
    lookup: &Lookup<'_>,
    path: &str,
    (connection, name, block): (&str, &ast::Name, Option<ast::MemberList>),
    problems: &mut Vec<Diagnostic>,
) -> S3Connection {
    let mut found = S3Connection {
        name: connection.to_string(),
        host: None,
        region: None,
        bucket: None,
        key_id: None,
        secret: None,
    };
    for (setting, option) in tree::members(block.clone()) {
        let word = name_text(&setting).unwrap_or_default();
        let span = option.value().map_or_else(
            || node_span(path, option.syntax()),
            |value| node_span(path, value.syntax()),
        );
        match word.as_str() {
            "driver" => {}
            "host" | "region" | "bucket" => {
                let read = text_setting(db, project, lookup, connection, &word, &option, span, problems);
                match word.as_str() {
                    "host" => found.host = read,
                    "region" => found.region = read,
                    _ => found.bucket = read,
                }
            }
            "credential" => read_credential(db, project, path, connection, &option, span, &mut found, problems),
            other => problems.push(
                Diagnostic::new(
                    &rule::CONNECTION_KEY_INVALID,
                    format!("A connection with `driver: Driver.s3` takes no `{other}:`"),
                )
                .at(node_span(path, setting.syntax()), "not an S3 setting")
                .reason("A connection to an S3-compatible service gives `host:`, `region:`, `bucket:` and `credential: { key_id: secret: }`.")
                .next(format!("Remove `{other}:`")),
            ),
        }
    }
    let written: Vec<String> = tree::members(block.clone())
        .iter()
        .filter_map(|(setting, _)| name_text(setting))
        .collect();
    for (key, says) in REQUIRED {
        if !written.iter().any(|name| name == key) {
            problems.push(
                Diagnostic::new(
                    &rule::CONNECTION_KEY_MISSING,
                    format!("`{connection}` doesn't say its `{key}:`"),
                )
                .at(node_span(path, name.syntax()), format!("no `{key}:`"))
                .reason(format!(
                    "A connection with `driver: Driver.s3` gives `{key}:`, {says}."
                ))
                .next(format!("Add `{key}:`")),
            );
        }
    }
    if !written.iter().any(|name| name == "credential") {
        problems.push(
            Diagnostic::new(
                &rule::CONNECTION_KEY_MISSING,
                format!("`{connection}` doesn't say how it signs in"),
            )
            .at(node_span(path, name.syntax()), "no `credential:`")
            .reason("A connection with `driver: Driver.s3` gives `credential: { key_id: config.<name>  secret: config.<name> }`, each read from a setting, the secret a `secret: true` one.")
            .next("Add `credential: { key_id: config.files_key_id  secret: config.files_secret }`"),
        );
    }
    found
}

/// A `host:`, `region:` or `bucket:`: text written out, or a setting built
/// on `Text`, `config.<name>`, read from each deployment's environment.
#[allow(clippy::too_many_arguments)]
fn text_setting(
    db: &dyn Db,
    project: Project,
    lookup: &Lookup<'_>,
    connection: &str,
    word: &str,
    option: &ast::Member,
    span: Span,
    problems: &mut Vec<Diagnostic>,
) -> Option<S3Text> {
    let invalid = |title: String, span: Span, label: String| {
        Diagnostic::new(&rule::CONNECTION_VALUE_INVALID, title).at(span, label)
    };
    let value = option.value();
    match (
        value.as_ref().and_then(reads),
        value.as_ref().and_then(literal_text),
    ) {
        (Some(field), _) => match config(db, project).setting(&field) {
            Some(found)
                if lookup.base_prim(&found.ty) == Some(Prim::Text)
                    || (word == "host" && lookup.base_prim(&found.ty) == Some(Prim::Url)) =>
            {
                Some(S3Text::Setting(field))
            }
            Some(found) => {
                problems.push(
                    invalid(
                        format!("`{connection}`'s `{word}:` must be text"),
                        span,
                        format!("`{field}` is `{}`", found.ty),
                    )
                    .reason(format!(
                        "`{word}:` is text written out, or a setting built on `Text`, `config.<name>`, read from each deployment's environment."
                    ))
                    .next(format!("Declare `{field}: Text` in `config.jadpo`")),
                );
                None
            }
            None => None,
        },
        (None, Some(text)) if word == "host" && !host_shaped(&text) => {
            problems.push(
                invalid(
                    format!("`{connection}`'s `host:` must be an address"),
                    span,
                    "not an address".to_string(),
                )
                .reason("`host:` is the address of the S3-compatible service: `https://` and its name, with nothing after it, or `http://` and this machine for a local MinIO.")
                .next(format!("Write `host: \"https://example.r2.cloudflarestorage.com\"`, or `host: config.{connection}_host`")),
            );
            None
        }
        (None, Some(text)) if !text.is_empty() => Some(S3Text::Written(text)),
        _ => {
            problems.push(
                invalid(
                    format!("`{connection}`'s `{word}:` must be text"),
                    span,
                    "not text".to_string(),
                )
                .reason(format!(
                    "`{word}:` is text written out, or a setting built on `Text`, `config.<name>`, read from each deployment's environment."
                ))
                .next(format!("Write `{word}: config.{connection}_{word}`")),
            );
            None
        }
    }
}

/// Whether `text` is an address `host:` may give: `https://` and a name,
/// or `http://` and this machine, with no path.
fn host_shaped(text: &str) -> bool {
    let rest = text.strip_prefix("https://").or_else(|| {
        text.strip_prefix("http://").filter(|rest| {
            let host = rest.split(':').next().unwrap_or_default();
            matches!(host, "localhost" | "127.0.0.1")
        })
    });
    rest.is_some_and(|rest| {
        !rest.is_empty()
            && !rest.contains('/')
            && !rest.chars().any(|c| c.is_control() || c.is_whitespace())
    })
}

/// `credential: { key_id: secret: }`, each a setting, the secret a
/// `secret: true` one.
#[allow(clippy::too_many_arguments)]
fn read_credential(
    db: &dyn Db,
    project: Project,
    path: &str,
    connection: &str,
    option: &ast::Member,
    span: Span,
    found: &mut S3Connection,
    problems: &mut Vec<Diagnostic>,
) {
    let Some(block) = option.block() else {
        problems.push(
            Diagnostic::new(
                &rule::CONNECTION_VALUE_INVALID,
                format!("`{connection}`'s `credential:` holds no block"),
            )
            .at(span, "not a block")
            .reason("An S3 credential is a key id and a secret access key, `credential: { key_id: config.<name>  secret: config.<name> }`, each read from a setting.")
            .next("Write `credential: { key_id: config.files_key_id  secret: config.files_secret }`"),
        );
        return;
    };
    for (part, value) in tree::members(Some(block)) {
        let part_name = name_text(&part).unwrap_or_default();
        let part_span = value.value().map_or_else(
            || node_span(path, value.syntax()),
            |value| node_span(path, value.syntax()),
        );
        if part_name != "key_id" && part_name != "secret" {
            problems.push(
                Diagnostic::new(
                    &rule::CONNECTION_KEY_INVALID,
                    format!("`{connection}`'s credential takes no `{part_name}:`"),
                )
                .at(node_span(path, part.syntax()), "not a part of the credential")
                .reason("An S3 credential is a key id and a secret access key: `key_id:` and `secret:`, each read from a setting.")
                .next(format!("Remove `{part_name}:`")),
            );
            continue;
        }
        let Some(field) = value.value().as_ref().and_then(reads) else {
            problems.push(
                Diagnostic::new(
                    &rule::CONNECTION_VALUE_INVALID,
                    format!("`{connection}`'s `{part_name}:` must be a setting"),
                )
                .at(part_span, "written in source")
                .reason("A credential is read from the deployment's environment, so each part is a setting, `config.<name>`, and the secret access key a `secret: true` one.")
                .next(format!("Declare `files_{part_name}: Text` in `config.jadpo`, and write `{part_name}: config.files_{part_name}`")),
            );
            continue;
        };
        let Some(setting) = config(db, project).setting(&field) else {
            continue;
        };
        if part_name == "secret" && !setting.secret {
            problems.push(
                Diagnostic::new(
                    &rule::CONNECTION_VALUE_INVALID,
                    format!("`{connection}`'s secret must be a secret setting"),
                )
                .at(part_span, format!("`{field}` isn't secret"))
                .reason("A secret access key is read from the deployment's environment and never seen by code, so it is a `secret: true` setting.")
                .next(format!("Add `secret: true` to `{field}` in `config.jadpo`")),
            );
            continue;
        }
        if part_name == "key_id" {
            found.key_id = Some(field);
        } else {
            found.secret = Some(field);
        }
    }
    if found.key_id.is_none() || found.secret.is_none() {
        let wrote_both = option.block().map_or(0, |block| {
            tree::members(Some(block))
                .iter()
                .filter(|(part, _)| matches!(name_text(part).as_deref(), Some("key_id" | "secret")))
                .count()
        }) == 2;
        if !wrote_both {
            problems.push(
                Diagnostic::new(
                    &rule::CONNECTION_KEY_MISSING,
                    format!("`{connection}`'s credential needs a key id and a secret"),
                )
                .at(span, "one part given")
                .reason("An S3 credential is a key id and a secret access key, each read from a setting; one without the other can't sign anything.")
                .next("Give both `key_id:` and `secret:`"),
            );
        }
    }
}

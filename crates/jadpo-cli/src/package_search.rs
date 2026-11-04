//! `jadpo package search` (area 11, "Search"; chunk R5): asks the registry
//! for what answers a need, the standard library's first.
//!
//! ```text
//! Standard library
//!   text.upper                Its account mentions postcode.
//!
//! Packages
//!   @acme/postcodes uk.parse  Its account mentions postcode.
//!
//! ✓ 2 found  1 in the standard library · 1 in a package
//!
//! → jadpo package add @acme/postcodes  installs the first package found
//! ```
//!
//! `--from` and `--to` read a type from the app, and what is sent is its
//! shape: standard types and their constraints, with no name or source of
//! the app's in it.

use std::path::PathBuf;

use jadpo_diag::nearest;
use jadpo_registry::protocol::{PackageKind, Request, SearchAnswer};
use jadpo_term::{Block, Fact, Json, Outcome, Row, Screen};

use crate::package::arguments;
use crate::registry::{Asked, Client};
use crate::run::{Call, Reply};

pub(crate) fn search(call: &Call) -> Reply {
    let words = call.arguments[1..].join(" ");
    if words.trim().is_empty() {
        return Reply::failed(arguments(
            "`jadpo package search` needs some words".to_string(),
            "It finds packages and standard-library functions by what they do, in your own words."
                .to_string(),
            "Run `jadpo package search <words>`".to_string(),
        ));
    }
    let kind = match call.value("--kind") {
        None => None,
        Some(word) => match PackageKind::parse(word) {
            Some(kind) => Some(kind),
            None => {
                return Reply::failed(arguments(
                    format!("`{word}` isn't a kind of package"),
                    "A package is a `library` or a `provider`.".to_string(),
                    "Use `--kind library` or `--kind provider`".to_string(),
                ));
            }
        },
    };
    let (from, to) = match (shape(call, "--from"), shape(call, "--to")) {
        (Ok(from), Ok(to)) => (from, to),
        (Err(reply), _) | (_, Err(reply)) => return reply,
    };
    let client = Client::of(&call.environment);
    let answer: SearchAnswer = match client.ask(&Request::Search {
        q: words.trim().to_string(),
        from,
        to,
        kind,
    }) {
        Ok(answer) => answer,
        Err(asked) => return Asked::reply(asked),
    };
    let row = |result: &jadpo_registry::protocol::SearchResult| {
        Row::new(
            match &result.package {
                Some(package) => format!("{package} {}", result.declaration),
                None => result.declaration.clone(),
            },
            result.why.clone(),
        )
    };
    let standard: Vec<Row> = answer
        .results
        .iter()
        .filter(|result| result.package.is_none())
        .map(row)
        .collect();
    let packaged: Vec<Row> = answer
        .results
        .iter()
        .filter(|result| result.package.is_some())
        .map(row)
        .collect();
    let (standards, packages) = (standard.len(), packaged.len());
    let mut screen = Screen::new();
    if standards > 0 {
        screen.push(Block::list("Standard library", standard));
    }
    if packages > 0 {
        screen.push(Block::list("Packages", packaged));
    }
    if answer.results.is_empty() {
        screen.push(Block::summary(
            Outcome::Success,
            "nothing found",
            [Fact::plain(format!("for \"{}\"", words.trim()))],
        ));
    } else {
        let mut facts = Vec::new();
        if standards > 0 {
            facts.push(Fact::plain(format!("{standards} in the standard library")));
        }
        if packages > 0 {
            facts.push(Fact::plain(if packages == 1 {
                "1 in a package".to_string()
            } else {
                format!("{packages} in packages")
            }));
        }
        screen.push(Block::summary(
            Outcome::Success,
            format!("{} found", answer.results.len()),
            facts,
        ));
    }
    if let Some(first) = answer.results.iter().find_map(|r| r.package.as_ref()) {
        screen.push(Block::hint(
            format!("jadpo package add {first}"),
            "installs the first package found",
        ));
    }
    Reply::screen(
        screen,
        Json::object([(
            "results",
            Json::array(answer.results.iter().map(|result| {
                Json::object([
                    (
                        "package",
                        result.package.as_ref().map_or(Json::Null, |package| {
                            Json::from(package.to_string().as_str())
                        }),
                    ),
                    ("declaration", Json::from(result.declaration.as_str())),
                    ("why", Json::from(result.why.as_str())),
                ])
            })),
        )]),
    )
}

/// What `flag` sends: the type it names reduced to its shape, or a standard
/// type as it is written.
fn shape(call: &Call, flag: &str) -> Result<Option<String>, Reply> {
    let Some(written) = call.value(flag) else {
        return Ok(None);
    };
    let inner = written
        .trim_end_matches('?')
        .trim_start_matches("List<")
        .trim_start_matches("Set<")
        .trim_end_matches('>');
    if jadpo_std::Builtin::from_name(inner).is_some() {
        return Ok(Some(written.to_string()));
    }
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let loaded = jadpo_db::find_root(&cwd).map(|root| jadpo_db::load(&root));
    let (found, names) = match &loaded {
        Some(loaded) => (
            jadpo_check::release::type_shape(&loaded.db, loaded.project, written),
            jadpo_check::release::type_names(&loaded.db, loaded.project),
        ),
        None => (None, Vec::new()),
    };
    if let Some(found) = found {
        return Ok(Some(found));
    }
    let message = format!("`{written}` isn't a type here");
    let reason = "`--from` and `--to` read a standard type, or a type of the app in this folder, and send its shape.".to_string();
    Err(Reply::failed(
        match nearest(written, names.iter().map(String::as_str)) {
            Some(near) => arguments(
                message,
                format!("{reason} Did you mean `{near}`?"),
                format!("Use `{flag} {near}`"),
            ),
            None => arguments(message, reason, format!("Give a type with `{flag}`")),
        },
    ))
}

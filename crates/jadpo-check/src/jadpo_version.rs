//! The Jadpo version an application needs (area 1, "The root"; area 14,
//! "Scaffold"): `application { jadpo: "0.1" }`, the earliest version as its
//! major and minor numbers in text, as a package's index gives its own
//! (area 11). A release that only fixes something never changes what a
//! project can use, so the patch number is never written. Left out, nothing
//! is checked. A project needing a newer Jadpo is refused with that error
//! alone ([`crate::check`]): what a later version added would only read as
//! mistakes here.

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, rule};
use jadpo_syntax::ast::AstNode;

use crate::config::singletons;
use crate::tree::{self, node_span};

/// This Jadpo's major and minor numbers.
fn this_version() -> (u64, u64) {
    let mut parts = env!("CARGO_PKG_VERSION")
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0));
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0))
}

/// `"0.1"`'s numbers: a major and a minor number, neither with a leading
/// zero.
pub fn parse_version(text: &str) -> Option<(u64, u64)> {
    let (major, minor) = text.split_once('.')?;
    let number = |part: &str| {
        (!part.is_empty()
            && part.bytes().all(|b| b.is_ascii_digit())
            && (part == "0" || !part.starts_with('0')))
        .then(|| part.parse::<u64>().ok())
        .flatten()
    };
    Some((number(major)?, number(minor)?))
}

/// Each application's `jadpo:` that isn't a version, and the one that needs
/// a newer Jadpo than this.
#[salsa::tracked(returns(ref))]
pub fn version_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for found in singletons(db, project, "application") {
        let Some((key, member)) = tree::members(found.list.clone())
            .into_iter()
            .find(|(key, _)| tree::name_text(key).as_deref() == Some("jadpo"))
        else {
            continue;
        };
        let value = member.value();
        let written = value
            .as_ref()
            .map(|value| tree::written(value.syntax()))
            .unwrap_or_default();
        let span = value.as_ref().map_or_else(
            || node_span(&found.path, key.syntax()),
            |value| node_span(&found.path, value.syntax()),
        );
        let version = written
            .strip_prefix('"')
            .and_then(|text| text.strip_suffix('"'))
            .and_then(parse_version);
        match version {
            None => out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_VERSION_INVALID,
                    format!("`{written}` isn't a Jadpo version"),
                )
                .at(span, "not a major and minor number as text")
                .reason("`jadpo:` is the earliest Jadpo version the project needs: its major and minor numbers as text, such as `\"0.1\"`. A release that only fixes something never changes what a project can use, so the patch number is left out.")
                .next("Write the version as its major and minor numbers in quotes, such as `jadpo: \"0.1\"`"),
            ),
            Some((major, minor)) if (major, minor) > this_version() => out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_VERSION_NEWER,
                    format!("This project needs Jadpo {major}.{minor} or later"),
                )
                .at(span, "newer than this Jadpo")
                .reason("`jadpo:` is the earliest Jadpo version the project needs. This Jadpo is older, so it stops here rather than misread what a later version added.")
                .next(format!(
                    "Upgrade Jadpo to {major}.{minor} or later (`jadpo version` shows this one), then run the command again"
                )),
            ),
            Some(_) => {}
        }
    }
    out
}

/// Whether `diagnostic` refuses the whole project.
pub fn refuses(diagnostic: &Diagnostic) -> bool {
    diagnostic.rule.id == rule::TOOLCHAIN_VERSION_NEWER.id
}

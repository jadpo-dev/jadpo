//! The Jadpo release line an application is written for (area 1, "The
//! root"; area 14, "Which jadpo runs"; roadmap chunks D3 and D14):
//! `application { jadpo: "0.1" }`, the line's major and minor numbers in
//! text, as a package's index gives its own (area 11). It is a line, not a
//! minimum: while Jadpo is at 0.x a minor release may break an app, so a
//! `jadpo` on another line refuses the project with that error alone
//! ([`crate::check`]), `toolchain.version_newer` for a later line and
//! `toolchain.version_older` for an earlier one, rather than compile it.
//! Left out, nothing is checked. The installed `jadpo` normally hands such
//! a project to the line's own release before it is checked at all
//! (jadpo-cli's launcher); these errors are what a `jadpo` that can't hand
//! it on says.

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_syntax::ast::{self, AstNode};

use crate::config::singletons;
use crate::tree::{self, node_span};

/// This Jadpo's major and minor numbers.
pub fn this_line() -> (u64, u64) {
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

/// An application's `jadpo:` as written: its value with the quotes, and
/// where that is in the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declared {
    pub written: String,
    pub start: u32,
    pub end: u32,
}

impl Declared {
    /// The line it names, when it is one.
    pub fn line(&self) -> Option<(u64, u64)> {
        self.written
            .strip_prefix('"')
            .and_then(|text| text.strip_suffix('"'))
            .and_then(parse_version)
    }
}

/// The `jadpo:` of the `application { }` in `text`, an `app.jadpo`, read
/// without the rest of the project: what the launcher reads to choose the
/// release that runs a command.
pub fn declared(text: &str) -> Option<Declared> {
    let source = ast::SourceFile::cast(jadpo_syntax::parse_file(text).syntax())?;
    source.decls().find_map(|decl| {
        let ast::Decl::SingletonDecl(decl) = decl else {
            return None;
        };
        if decl.keyword()?.text() != "application" {
            return None;
        }
        let (_, member) = tree::members(decl.member_list())
            .into_iter()
            .find(|(key, _)| tree::name_text(key).as_deref() == Some("jadpo"))?;
        let value = member.value()?;
        let range = value.syntax().text_range();
        Some(Declared {
            written: tree::written(value.syntax()),
            start: u32::from(range.start()),
            end: u32::from(range.end()),
        })
    })
}

/// Why the launcher couldn't run the release a project needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unfetched {
    /// GitHub couldn't be reached, or didn't answer.
    Unreachable,
    /// No such release, or no release of the line, is published.
    Unreleased,
    /// GitHub's API refused the request: its limit of 60 requests an hour
    /// from one address without signing in is used up.
    Limited,
    /// The download failed its SHA-256 or attestation check.
    Unverified,
    /// There is no cache folder to keep it in.
    Uncached,
}

/// The refusal of a project that needs another release than this `jadpo`,
/// `wanted`: its line (`"0.2"`), or the exact release `jadpo.lock` pins
/// (`"0.1.2"`) when `pinned`. `later` says whether it is later than this
/// one, `at` points at what asks for it, and `fetch` is why the launcher
/// couldn't run it instead, when it tried: what kind of failure, and the
/// failure in words, a sentence.
pub fn refusal(
    wanted: &str,
    later: bool,
    pinned: bool,
    at: Option<Span>,
    fetch: Option<(Unfetched, &str)>,
) -> Diagnostic {
    let (rule, label) = if later {
        (&rule::TOOLCHAIN_VERSION_NEWER, "later than this jadpo")
    } else {
        (&rule::TOOLCHAIN_VERSION_OLDER, "earlier than this jadpo")
    };
    // What failed comes first; why it had to be that release after.
    let failed =
        fetch.map(|(_, why)| format!("Jadpo {wanted} couldn't be fetched to run it: {why} "));
    let why = if pinned {
        format!(
            "`jadpo.lock` pins Jadpo {wanted}, so every run of this project uses that release, and this jadpo is another (`jadpo --version` shows it)."
        )
    } else if later {
        format!(
            "`jadpo:` names the release line the project is written for, {wanted}, which is later than this jadpo's (`jadpo --version` shows it), so it stops here rather than misread what a later version added."
        )
    } else {
        format!(
            "`jadpo:` names the release line the project is written for, {wanted}, which is earlier than this jadpo's (`jadpo --version` shows it). A 0.x minor release may break an app, so this jadpo doesn't compile the project itself."
        )
    };
    let next = match fetch.map(|(kind, _)| kind) {
        None => format!(
            "Run it with Jadpo {wanted}, which the installed `jadpo` fetches unless `JADPO_TOOLCHAIN` is `local`"
        ),
        Some(Unfetched::Unreachable) => {
            "Run the command again once this machine can reach github.com".to_string()
        }
        Some(Unfetched::Limited) => {
            "Run the command again in an hour, when GitHub's limit has reset; nothing was fetched".to_string()
        }
        Some(Unfetched::Unreleased) if pinned => {
            "Run `jadpo toolchain unpin`, or pin a published release".to_string()
        }
        Some(Unfetched::Unreleased) => {
            "Name a published line in `jadpo:`; https://github.com/jadpo-dev/jadpo/releases lists them".to_string()
        }
        Some(Unfetched::Unverified) => {
            "Run the command again to fetch it afresh; if it fails the same way, install the newest jadpo, whose trusted root may be newer, and if that fails too, report it at https://github.com/jadpo-dev/jadpo/security/policy".to_string()
        }
        Some(Unfetched::Uncached) => {
            "Set `XDG_CACHE_HOME` to a folder jadpo may keep releases in, then run the command again".to_string()
        }
    };
    let diagnostic = Diagnostic::new(rule, format!("This project needs Jadpo {wanted}"))
        .reason(format!("{}{why}", failed.unwrap_or_default()))
        .next(next);
    match at {
        Some(span) => diagnostic.at(span, label),
        None => diagnostic,
    }
}

/// Each application's `jadpo:` that isn't a version, the one on another
/// line than this Jadpo's, and a `jadpo.lock` pin on another line than
/// the one the application names.
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
                .reason("`jadpo:` names the Jadpo release line the project is written for: its major and minor numbers as text, such as `\"0.1\"`. A release that only fixes something stays on its line, so the patch number is left out.")
                .next("Write the line as its major and minor numbers in quotes, such as `jadpo: \"0.1\"`"),
            ),
            Some(line) if line != this_line() => out.push(refusal(
                &format!("{}.{}", line.0, line.1),
                line > this_line(),
                false,
                Some(span),
                None,
            )),
            Some(line) => out.extend(pin_problem(db, project, line)),
        }
    }
    out
}

/// A `jadpo.lock` pin on another line than `line`, the application's: the
/// line was changed after the pin was written.
fn pin_problem(db: &dyn Db, project: Project, line: (u64, u64)) -> Option<Diagnostic> {
    let record = project
        .records(db)
        .iter()
        .find(|record| record.path == jadpo_db::LOCK)?;
    let text = record.text.as_ref().ok()?;
    let (version, at) = jadpo_db::locked_jadpo(text)?;
    if release_line(&version)? == line {
        return None;
    }
    let offset: usize = text.split_inclusive('\n').take(at).map(str::len).sum();
    let length = text.lines().nth(at).map_or(0, str::len);
    Some(
        Diagnostic::new(
            &rule::RECORD_OUT_OF_DATE,
            format!(
                "`{}` pins Jadpo {version}, which isn't on line {}.{}",
                jadpo_db::LOCK,
                line.0,
                line.1
            ),
        )
        .at(
            Span::new(&record.path, offset as u32, (offset + length) as u32),
            "another line than `jadpo:` names",
        )
        .reason(format!(
            "`jadpo.lock` pins the exact release the project runs with, which has to be on the line `app.jadpo` names, {}.{}. The line changed after the pin was written.",
            line.0, line.1
        ))
        .next("Run `jadpo toolchain pin` to pin the release this project now runs with"),
    )
}

/// The line of a release, `0.1` for `0.1.2`.
pub fn release_line(version: &str) -> Option<(u64, u64)> {
    let mut parts = version.split('.');
    parse_version(&format!("{}.{}", parts.next()?, parts.next()?))
}

/// Whether `diagnostic` refuses the whole project.
pub fn refuses(diagnostic: &Diagnostic) -> bool {
    diagnostic.rule.id == rule::TOOLCHAIN_VERSION_NEWER.id
        || diagnostic.rule.id == rule::TOOLCHAIN_VERSION_OLDER.id
}

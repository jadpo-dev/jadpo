//! Your own company's repositories (area 11, "Installing safely"; chunk
//! V1): `application { packages: { own_sources: [...] } }`. A package
//! locked from a repository under one of them needs no
//! `PackageChange.git_source`, so this is what an agent would forge to let
//! a stranger's code in without a person. Two things keep it honest:
//! adding or widening an entry needs `PolicyChange.own_source_added`
//! (`acceptances::compare`), and a repository is under an entry only by
//! its exact host and whole path segments, read here.
//!
//! The reading fails closed: an address that can't be told apart from
//! someone else's is under nothing, and only costs a person's acceptance.
//! So nothing is unescaped or resolved, an account git would sign in as
//! (other than ssh's `git`) is refused, and a port is part of the server.
//! A host has no case, as DNS has none; a path keeps its case, since many
//! servers tell `Acme` from `acme`.

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Expr};

use crate::config::singletons;
use crate::tree::{self, node_span};

/// The one setting `packages:` holds.
const OWN_SOURCES: &str = "own_sources";

/// How `packages:` is written, for an error's next step.
const SHAPE: &str = "`packages: { own_sources: [\"github.com/acme/\"] }`";

/// The section, read: the entries that are sound, and the problems with
/// the rest. An entry refused covers nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnSources {
    pub entries: Vec<String>,
    pub problems: Vec<Diagnostic>,
}

/// `packages:` of `app.jadpo`'s `application { }`.
#[salsa::tracked(returns(ref))]
pub fn own_sources(db: &dyn Db, project: Project) -> OwnSources {
    let mut out = OwnSources::default();
    let mut first: Option<Span> = None;
    for found in singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if tree::name_text(&key).as_deref() != Some("packages") {
                continue;
            }
            let at = node_span(&found.path, key.syntax());
            if let Some(first) = &first {
                out.problems.push(
                    invalid("`packages:` is declared twice", at, "declared again")
                        .also(first.clone(), "first declared here")
                        .reason("An application has one list of its own repositories, so a reviewer reads all of it in one place.")
                        .next("Keep one `packages:` and remove the other"),
                );
                continue;
            }
            first = Some(at.clone());
            read_section(&found.path, &at, &member, &mut out);
        }
    }
    out
}

/// The problems in the project's `packages:`.
pub fn problems(db: &dyn Db, project: Project) -> &[Diagnostic] {
    &own_sources(db, project).problems
}

fn read_section(path: &str, at: &Span, member: &ast::Member, out: &mut OwnSources) {
    let Some(block) = member.block() else {
        out.problems.push(
            invalid("`packages:` isn't a block", at.clone(), "not a block")
                .reason("`packages:` holds the settings for where the app's packages come from.")
                .next(format!("Write {SHAPE}")),
        );
        return;
    };
    let mut seen: Option<Span> = None;
    for (key, member) in tree::members(Some(block)) {
        let written = tree::name_text(&key).unwrap_or_default();
        let key_at = node_span(path, key.syntax());
        if written != OWN_SOURCES {
            out.problems.push(unknown(&written, key_at));
            continue;
        }
        if let Some(seen) = &seen {
            out.problems.push(
                invalid("`own_sources:` is declared twice", key_at, "declared again")
                    .also(seen.clone(), "first declared here")
                    .reason("An application has one list of its own repositories, so a reviewer reads all of it in one place.")
                    .next("Keep one `own_sources:` and put every entry in it"),
            );
            continue;
        }
        seen = Some(key_at.clone());
        read_list(path, &key_at, &member, out);
    }
}

fn read_list(path: &str, key_at: &Span, member: &ast::Member, out: &mut OwnSources) {
    let value = member.value();
    let Some(Expr::ListExpr(list)) = &value else {
        let at = value
            .as_ref()
            .map_or_else(|| key_at.clone(), |v| node_span(path, v.syntax()));
        out.problems.push(
            invalid("`own_sources:` isn't a list", at, "not a list")
                .reason("`own_sources:` lists each of your company's git locations, a host and a path ending in `/`.")
                .next(format!("Write {SHAPE}")),
        );
        return;
    };
    let mut seen: Vec<String> = Vec::new();
    for item in list.exprs() {
        let at = node_span(path, item.syntax());
        let crate::types::Const::Str(entry) = crate::types::Const::from_expr(&item) else {
            out.problems.push(
                invalid("`own_sources:` lists something other than text", at, "not text")
                    .reason("Each entry is written out, a host and a path ending in `/`, so a reviewer reads exactly what is trusted.")
                    .next(format!("Write {SHAPE}")),
            );
            continue;
        };
        if seen.contains(&entry) {
            out.problems.push(
                invalid(format!("`{entry}` is listed twice"), at, "listed again")
                    .reason("Each git location is listed once, so the list says exactly what is trusted.")
                    .next(format!("Remove the second `{entry}`")),
            );
            continue;
        }
        seen.push(entry.clone());
        match refusal(&entry) {
            None => out.entries.push(entry),
            Some(refused) => out.problems.push(refused.diagnostic(&entry, at)),
        }
    }
}

fn invalid(message: impl Into<String>, at: Span, label: &str) -> Diagnostic {
    Diagnostic::new(&rule::PACKAGE_OWN_SOURCE_INVALID, message).at(at, label)
}

/// A setting `packages:` doesn't hold, with the edit to `own_sources` when
/// it is a near miss: a list misspelt would otherwise look kept.
fn unknown(written: &str, at: Span) -> Diagnostic {
    let problem = invalid(
        format!("`{written}` isn't something `packages:` holds"),
        at.clone(),
        "not a setting",
    )
    .reason("`packages:` holds only `own_sources:`, your company's git locations.");
    match nearest(written, [OWN_SOURCES]) {
        Some(near) => problem
            .next(format!("Use `{near}` instead of `{written}`"))
            .edit(at, near),
        None => problem.next(format!("Remove `{written}` from `packages:`")),
    }
}

/// Why an entry is refused, and the entry it should be when that is plain.
struct Refusal {
    reason: String,
    next: String,
    instead: Option<String>,
}

impl Refusal {
    fn new(reason: impl Into<String>, next: impl Into<String>) -> Refusal {
        Refusal {
            reason: reason.into(),
            next: next.into(),
            instead: None,
        }
    }

    /// Refused, with the entry to write instead.
    fn instead(reason: impl Into<String>, entry: String) -> Refusal {
        Refusal {
            reason: reason.into(),
            next: format!("Write `{entry}`"),
            instead: Some(entry),
        }
    }

    fn diagnostic(self, entry: &str, at: Span) -> Diagnostic {
        let problem = invalid(
            format!("`{entry}` isn't a host and a path ending in `/`"),
            at.clone(),
            "not a git location",
        )
        .reason(self.reason)
        .next(self.next);
        match self.instead {
            Some(instead) => problem.edit(at, format!("\"{instead}\"")),
            None => problem,
        }
    }
}

const HOST_AND_PATH: &str =
    "An entry is a host and a path ending in `/`, such as `github.com/acme/`";

/// Why `entry` can't be an entry, if it can't.
fn refusal(entry: &str) -> Option<Refusal> {
    if let Some((scheme, rest)) = entry.split_once("://") {
        let rest = rest.strip_prefix("git@").unwrap_or(rest);
        let reason = format!(
            "{HOST_AND_PATH}, with no scheme: `{scheme}://` is how a repository is fetched, not whose it is."
        );
        return Some(if rest.contains(['@', ':']) || rest.is_empty() {
            Refusal::new(reason, "Write the host and the path alone")
        } else {
            Refusal::instead(reason, rest.to_string())
        });
    }
    if entry.contains('@') {
        return Some(Refusal::new(
            format!(
                "{HOST_AND_PATH}, with no account: an account before the host, or in it, could make the address read as another server's."
            ),
            "Write the host and the path alone",
        ));
    }
    let Some(folder) = entry.strip_suffix('/') else {
        let reason = format!(
            "{HOST_AND_PATH}. An entry ends in `/` so that what it covers reads plainly: `github.com/acme/` covers `github.com/acme/postcodes`, and never `github.com/acme-evil/postcodes`."
        );
        return Some(if entry.is_empty() {
            Refusal::new(reason, format!("Write {SHAPE}"))
        } else {
            Refusal::instead(reason, format!("{entry}/"))
        });
    };
    let (authority, path) = folder.split_once('/').unwrap_or((folder, ""));
    if authority.chars().any(|c| c.is_ascii_uppercase()) && server(authority).is_some() {
        let reason = format!(
            "{HOST_AND_PATH}, its host in lower case: a host's name has no case, so one spelling keeps the list and its acceptances exact."
        );
        return Some(Refusal::instead(
            reason,
            format!("{}/", join(&authority.to_ascii_lowercase(), path)),
        ));
    }
    if server(authority).is_none() {
        return Some(Refusal::new(
            format!(
                "{HOST_AND_PATH}: `{authority}` isn't a host, which has letters, digits, `-` and at least one `.` between its labels, and may end in a port such as `:8443`."
            ),
            "Write the git server's host first",
        ));
    }
    if path.is_empty() {
        return Some(Refusal::new(
            format!(
                "{HOST_AND_PATH}: `{authority}/` alone would let in every repository on the host, a stranger's among them."
            ),
            "Name the company's folder on the host, such as `github.com/acme/`",
        ));
    }
    if let Some(bad) = path.split('/').find(|segment| !is_segment(segment)) {
        return Some(Refusal::new(
            format!(
                "{HOST_AND_PATH}: its path has `{bad}`. Each part of a path has letters, digits, `-`, `_`, `.` and `~`, and isn't only dots, so an escape or a `..` can't make an address read as another folder's."
            ),
            "Write the folder's path plainly",
        ));
    }
    if path
        .rsplit('/')
        .next()
        .is_some_and(|last| last.ends_with(".git"))
    {
        return Some(Refusal::new(
            format!(
                "{HOST_AND_PATH}: this one ends at one repository, and an entry covers the repositories below it."
            ),
            "Name the folder that holds the repository",
        ));
    }
    None
}

fn join(authority: &str, path: &str) -> String {
    if path.is_empty() {
        authority.to_string()
    } else {
        format!("{authority}/{path}")
    }
}

/// A git server: its lower-case host and any port, or `None` when
/// `authority` isn't one. Letters, digits and `-` in labels between dots,
/// and a whole port with no leading zero; so no account, no escape and no
/// character that only looks like a letter.
fn server(authority: &str) -> Option<String> {
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    let labels: Vec<&str> = host.split('.').collect();
    let plain = labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        });
    let port_plain = port.is_none_or(|port| {
        !port.starts_with('0')
            && port.chars().all(|c| c.is_ascii_digit())
            && port.parse::<u16>().is_ok()
    });
    (plain && port_plain).then(|| authority.to_ascii_lowercase())
}

/// A part of a path that means only itself.
fn is_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment.chars().any(|c| c != '.')
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~'))
}

/// An address reduced to what decides whose it is.
struct Place<'a> {
    server: String,
    path: Vec<&'a str>,
}

/// Where a lock's repository is, as `jadpo package add` and git read it:
/// `host/path`, `https://`, `ssh://` with no account or `git`, and `git@host:path`.
/// `None` for anything else: another scheme, another account, a folder on
/// this machine, or an address that isn't plain.
fn repository(written: &str) -> Option<Place<'_>> {
    let (server_and_path, scp) = if let Some(rest) = written.strip_prefix("https://") {
        (rest, false)
    } else if let Some(rest) = written.strip_prefix("ssh://") {
        (rest.strip_prefix("git@").unwrap_or(rest), false)
    } else if written.contains("://") {
        return None;
    } else if let Some(rest) = written.strip_prefix("git@") {
        (rest, true)
    } else {
        (written, false)
    };
    let (authority, path) = if scp {
        // `git@host:path`: the colon ends the host, and no port is possible.
        let (authority, path) = server_and_path.split_once(':')?;
        if authority.contains(':') {
            return None;
        }
        (authority, path)
    } else {
        server_and_path.split_once('/')?
    };
    let server = server(authority)?;
    let mut path: Vec<&str> = path.split('/').collect();
    let last = path.last_mut()?;
    *last = last.strip_suffix(".git").unwrap_or(last);
    path.iter()
        .all(|segment| is_segment(segment))
        .then_some(Place { server, path })
}

/// A sound entry as its server and folder.
fn folder(entry: &str) -> Option<Place<'_>> {
    let folder = entry.strip_suffix('/')?;
    let (authority, path) = folder.split_once('/')?;
    Some(Place {
        server: server(authority)?,
        path: path.split('/').collect(),
    })
}

/// The entry of `entries` that `written`, a lock's repository, is under.
pub fn covering<'a>(entries: &'a [String], written: &str) -> Option<&'a str> {
    let repository = repository(written)?;
    entries
        .iter()
        .find(|entry| {
            folder(entry).is_some_and(|folder| {
                folder.server == repository.server
                    && repository.path.len() > folder.path.len()
                    && repository.path.starts_with(&folder.path)
            })
        })
        .map(String::as_str)
}

/// Whether `entry` lets in nothing `entries` doesn't: it is one of them,
/// or below one. An entry that adds nothing widens nothing.
pub fn covers(entries: &[String], entry: &str) -> bool {
    let Some(new) = folder(entry) else {
        return false;
    };
    entries.iter().any(|was| {
        folder(was).is_some_and(|was| was.server == new.server && new.path.starts_with(&was.path))
    })
}

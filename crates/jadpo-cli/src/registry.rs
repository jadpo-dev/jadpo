//! The `jadpo` tool's side of the registry (area 11, "The registry
//! protocol" and "Installing and updating from the registry"; chunk R5):
//! asking it, checking what it answers against its public log, and which
//! release an `add` or an `update` takes.
//!
//! Nothing it answers is trusted because it answered. A release is taken
//! only when the head it gives extends the last one this project saw, the
//! log proves the release is in that head, and the archive's files have the
//! digest the log holds; the commands then verify the files as they verify a
//! package from anywhere ([`crate::install`]), and show facts worked out from
//! those files rather than the ones it sent.
//!
//! The registry is `https://jadpo.dev/registry/v1/`. `JADPO_REGISTRY` names a
//! stand-in, as `JADPO_RUST_DIST_SERVER` does for Rust's: HTTPS, or plain HTTP
//! on this machine only.

use std::path::{Path, PathBuf};

use jadpo_diag::{Decider, Diagnostic, rule};
use jadpo_registry::archive;
use jadpo_registry::log::{Hash, LogEntry, check_extends, check_included};
use jadpo_registry::names::{Digest, ScopedName, Version};
use jadpo_registry::protocol::{
    self, AdvisoryAnswer, AdvisoryQuery, Change, ChangeEntry, ConsistencyProof, InclusionProof,
    Installed, LogHead, Package, Release, ReleaseSummary, Request,
};
use jadpo_term::{Block, ChangeMark, Environment, Row};
use serde::de::DeserializeOwned;

use crate::download;
use crate::package::Scratch;
use crate::run::{Call, Reply};

/// What names a stand-in for the registry.
pub(crate) const VARIABLE: &str = "JADPO_REGISTRY";

/// Where the last head seen is kept, beside the project's other local state.
const HEAD: &str = ".jadpo/registry_head";

/// How old a release has to be before `update` takes it unasked.
const WAIT: i64 = 3 * DAY;

const DAY: i64 = 86_400;

/// The most a JSON answer may be.
const LONGEST_ANSWER: usize = 16 << 20;

/// The most an archive may be: twice what the verifier lets a release come
/// to, so a registry can't make a client hold more than a release could be,
/// and one that raises its limit doesn't strand this one at once.
const LONGEST_ARCHIVE: usize = 2 * jadpo_verifier::files::SIZE_LIMIT as usize;

// --- asking ----------------------------------------------------------------

/// Why a request got no answer of the kind it asks for.
pub(crate) enum Asked {
    /// A failure the table names, by its code.
    Failure(String),
    /// Nothing that can be read: not reached, or not the protocol.
    Unreachable(Box<Diagnostic>),
}

impl Asked {
    /// As a reply, with no failure the asker can do anything else with.
    pub(crate) fn reply(self) -> Reply {
        match self {
            Asked::Unreachable(diagnostic) => Reply::Failed(diagnostic),
            Asked::Failure(code) => {
                Reply::Failed(Box::new(unreachable(format!("It answered `{code}`."))))
            }
        }
    }
}

/// `registry.unreachable`, saying why.
pub(crate) fn unreachable(reason: String) -> Diagnostic {
    Diagnostic::new(
        &rule::REGISTRY_UNREACHABLE,
        "The registry couldn't be read".to_string(),
    )
    .reason(reason)
    .next_for(
        "Check your connection, then run the command again",
        Decider::Human,
    )
}

/// The registry this command talks to.
pub(crate) struct Client {
    /// The protocol's address, ending `/registry/v1/`.
    address: String,
}

impl Client {
    pub(crate) fn of(environment: &Environment) -> Client {
        Client {
            address: environment
                .var(VARIABLE)
                .unwrap_or(protocol::PUBLIC)
                .to_string(),
        }
    }

    /// The URL of a request of the table.
    fn url(&self, request: &Request) -> Result<String, Asked> {
        let Some(origin) = self.address.strip_suffix(protocol::BASE) else {
            return Err(Asked::Unreachable(Box::new(unreachable(format!(
                "`{}` isn't a registry's address, which ends `{}`.",
                self.address,
                protocol::BASE
            )))));
        };
        Ok(format!("{origin}{}", request.target()))
    }

    fn send(&self, request: &Request, most: usize) -> Result<download::Answer, Asked> {
        let url = self.url(request)?;
        let body = request.body();
        download::send(
            request.method().as_str(),
            &url,
            body.as_deref().map(str::as_bytes),
            most,
        )
        .map_err(|why| Asked::Unreachable(Box::new(unreachable(why))))
    }

    /// The answer to a request of the table, held to the shape the table
    /// gives it.
    pub(crate) fn ask<T: DeserializeOwned>(&self, request: &Request) -> Result<T, Asked> {
        let answer = self.send(request, LONGEST_ANSWER)?;
        let endpoint = request.endpoint();
        let unread = |why: String| {
            Asked::Unreachable(Box::new(unreachable(format!(
                "Its answer to `{} {}` can't be read: {why}.",
                request.method(),
                request.target()
            ))))
        };
        if answer.status != endpoint.status {
            return Err(failure_of(&answer).map_or_else(
                || unread(format!("it answered {}", answer.status)),
                Asked::Failure,
            ));
        }
        let json: serde_json::Value =
            serde_json::from_slice(&answer.body).map_err(|error| unread(error.to_string()))?;
        protocol::check_answer(endpoint, &json).map_err(unread)?;
        serde_json::from_value(json).map_err(|error| unread(error.to_string()))
    }

    /// A release's archive.
    fn archive(&self, digest: &Digest) -> Result<Vec<u8>, Asked> {
        let request = Request::Archive {
            digest: digest.clone(),
        };
        let answer = self.send(&request, LONGEST_ARCHIVE)?;
        if answer.status == 200 {
            return Ok(answer.body);
        }
        Err(failure_of(&answer).map_or_else(
            || {
                Asked::Unreachable(Box::new(unreachable(format!(
                    "It answered {} to the archive `{digest}`.",
                    answer.status
                ))))
            },
            Asked::Failure,
        ))
    }
}

/// The code in a failure envelope (area 4, "What the client sees").
fn failure_of(answer: &download::Answer) -> Option<String> {
    let json: serde_json::Value = serde_json::from_slice(&answer.body).ok()?;
    json["error"]["code"].as_str().map(str::to_string)
}

// --- the log ---------------------------------------------------------------

/// `install.not_proved`.
fn not_proved(message: String, reason: String) -> Reply {
    Reply::Failed(Box::new(
        Diagnostic::new(&rule::INSTALL_NOT_PROVED, message)
            .reason(reason)
            .next_for(
                "Install nothing from it until you know why: report it to Jadpo, with this output",
                Decider::Human,
            ),
    ))
}

/// The head kept in the project, when there is one.
fn kept_head(root: &Path) -> Result<Option<LogHead>, Reply> {
    let Ok(text) = std::fs::read_to_string(root.join(HEAD)) else {
        return Ok(None);
    };
    let read = text.trim().split_once(' ').and_then(|(size, hash)| {
        Some(LogHead {
            size: size.parse().ok()?,
            root: Hash::from_hex(hash)?,
        })
    });
    match read {
        Some(head) => Ok(Some(head)),
        None => Err(not_proved(
            format!("`{HEAD}` can't be read"),
            "It holds the last head of the registry's log this project saw, as its size and root hash, so a registry can't quietly start another log.".to_string(),
        )),
    }
}

fn keep_head(root: &Path, head: &LogHead, command: &str) -> Result<(), Reply> {
    let path = root.join(HEAD);
    let staged = root.join(format!("{HEAD}.writing"));
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&staged, format!("{} {}\n", head.size, head.root.to_hex())))
        .and_then(|()| std::fs::rename(&staged, &path));
    written.map_err(|error| {
        crate::package::write_failed(command, &format!("`{HEAD}`"), &error.to_string())
    })
}

/// Where a release sits in the log it was proved against.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Logged {
    pub(crate) index: u64,
    pub(crate) size: u64,
}

/// Proves `entry` is in the registry's log, whose head has to extend the last
/// one this project saw, and keeps the new head.
fn prove(root: &Path, client: &Client, entry: &LogEntry, command: &str) -> Result<Logged, Reply> {
    let release = format!("`{}` {}", entry.package, entry.version);
    let refused = |reason: String| {
        not_proved(
            format!("The registry's public log doesn't prove {release}"),
            reason,
        )
    };
    let head: LogHead = client.ask(&Request::LogHead).map_err(Asked::reply)?;
    if let Some(seen) = kept_head(root)? {
        if head.size < seen.size {
            return Err(refused(format!(
                "The log has {} entries now, and this project has seen it with {}: a log only ever grows.",
                head.size, seen.size
            )));
        }
        if head.size == seen.size {
            if head.root != seen.root {
                return Err(refused(format!(
                    "Its {} entries have another root than this project saw, so its history has changed.",
                    head.size
                )));
            }
        } else {
            let proof: ConsistencyProof = client
                .ask(&Request::LogConsistency {
                    from: seen.size,
                    to: head.size,
                })
                .map_err(|asked| match asked {
                    Asked::Failure(code) => {
                        refused(format!("It answered `{code}` when asked to prove its head extends the one this project saw."))
                    }
                    other => other.reply(),
                })?;
            check_extends(&seen, &head, &proof).map_err(|why| {
                refused(format!(
                    "Its head doesn't extend the one this project saw: {why}."
                ))
            })?;
        }
    }
    let proof: InclusionProof = client
        .ask(&Request::LogProof {
            digest: entry.digest.clone(),
            size: head.size,
        })
        .map_err(|asked| match asked {
            Asked::Failure(code) => refused(format!(
                "It answered `{code}` when asked to prove the release is in the log."
            )),
            other => other.reply(),
        })?;
    check_included(&head, entry, &proof).map_err(|why| refused(format!("{why}.")))?;
    keep_head(root, &head, command)?;
    Ok(Logged {
        index: proof.index,
        size: head.size,
    })
}

// --- fetching a release ----------------------------------------------------

/// A release fetched and proved, in a scratch folder, ready to verify.
pub(crate) struct Fetched {
    pub(crate) _scratch: Scratch,
    pub(crate) folder: PathBuf,
    pub(crate) name: ScopedName,
    pub(crate) package: Package,
    pub(crate) release: Release,
    pub(crate) logged: Logged,
}

impl Fetched {
    /// Its release tag, which its surface's version has to be.
    pub(crate) fn tag(&self) -> String {
        format!("v{}", self.release.version)
    }

    /// Checks what was verified from its files is what the registry said it
    /// is.
    pub(crate) fn holds(
        &self,
        package: &str,
        version: &str,
        digest: &str,
        surface: &str,
    ) -> Result<(), Reply> {
        let refused = |reason: String| {
            not_proved(
                format!(
                    "`{}` {} isn't what the registry says it is",
                    self.name, self.release.version
                ),
                reason,
            )
        };
        let (code, _) = package.rsplit_once('.').unwrap_or((package, ""));
        if code != self.package.code_name {
            return Err(refused(format!(
                "The registry calls it `{}` in code, and its files say `{package}`.",
                self.package.code_name
            )));
        }
        let scope = self.name.publisher_segment();
        match crate::install::surface_publisher(surface) {
            Some(named) if named == scope => {}
            Some(named) => {
                return Err(refused(format!(
                    "Its name says `{scope}`, and its files say `{named}`."
                )));
            }
            None => {
                return Err(refused(format!(
                    "Its name says `{scope}`, and its files name no publisher."
                )));
            }
        }
        if version != self.release.version.as_str() {
            return Err(refused(format!(
                "The registry says {}, and its files say {version}.",
                self.release.version
            )));
        }
        if digest != self.release.digest.as_str() {
            return Err(refused(format!(
                "The log holds `{}`, and the files give `{digest}`.",
                self.release.digest
            )));
        }
        if jadpo_db::record::normalise(&self.release.surface) != surface {
            return Err(refused(
                "The surface the registry shows isn't the one its files carry.".to_string(),
            ));
        }
        Ok(())
    }
}

/// `install.not_in_registry`.
fn not_in_registry(message: String, reason: String, next: String) -> Reply {
    Reply::Failed(Box::new(
        Diagnostic::new(&rule::INSTALL_NOT_IN_REGISTRY, message)
            .reason(reason)
            .next_for(next, Decider::Human),
    ))
}

/// Fetches and proves one release of `package`: its details, then the log,
/// then its files, whose digest has to be the one the log holds.
pub(crate) fn fetch(
    root: &Path,
    client: &Client,
    name: &ScopedName,
    package: Package,
    version: &Version,
    command: &str,
) -> Result<Fetched, Reply> {
    let summary = package
        .releases
        .iter()
        .find(|release| &release.version == version);
    let release: Release = client
        .ask(&Request::Release {
            package: name.clone(),
            version: version.clone(),
        })
        .map_err(|asked| match asked {
            Asked::Failure(code) if code == protocol::RELEASE_NOT_FOUND.code => {
                no_release(name, &package, version)
            }
            other => other.reply(),
        })?;
    let refused = |reason: String| {
        not_proved(
            format!("The registry's answers about `{name}` {version} don't agree"),
            reason,
        )
    };
    if &release.package != name || &release.version != version {
        return Err(refused(format!(
            "Asked for this release, it answered with `{}` {}.",
            release.package, release.version
        )));
    }
    if summary.is_some_and(|summary| summary.digest != release.digest) {
        return Err(refused(
            "The package's list and the release give different digests.".to_string(),
        ));
    }
    let logged = prove(
        root,
        client,
        &LogEntry {
            package: name.clone(),
            version: version.clone(),
            digest: release.digest.clone(),
        },
        command,
    )?;
    let bytes = client
        .archive(&release.digest)
        .map_err(|asked| match asked {
            Asked::Failure(code) => {
                refused(format!("It answered `{code}` for the release's archive."))
            }
            other => other.reply(),
        })?;
    let swapped = |reason: String| {
        not_proved(
            format!("The archive of `{name}` {version} isn't the release the log holds"),
            reason,
        )
    };
    let files = archive::unpack(&bytes).map_err(|why| swapped(format!("{why}.")))?;
    let view: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    let digest = Digest::of_files(&view);
    if digest != release.digest {
        return Err(swapped(format!(
            "The log holds `{}`, and its files give `{digest}`.",
            release.digest
        )));
    }
    let failed = |error: std::io::Error| {
        crate::package::write_failed(command, "a scratch folder", &error.to_string())
    };
    let scratch = Scratch::new().map_err(failed)?;
    let folder = scratch.0.join("release");
    for (path, bytes) in &files {
        // `archive::unpack` allows only plain relative paths inside a package.
        let to = folder.join(path);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(failed)?;
        }
        std::fs::write(&to, bytes).map_err(failed)?;
    }
    Ok(Fetched {
        _scratch: scratch,
        folder,
        name: name.clone(),
        package,
        release,
        logged,
    })
}

fn no_release(name: &ScopedName, package: &Package, version: &Version) -> Reply {
    let releases: Vec<String> = package
        .releases
        .iter()
        .map(|release| format!("`{}`", release.version))
        .collect();
    not_in_registry(
        format!("`{name}` has no release {version}"),
        if releases.is_empty() {
            "It has no releases yet.".to_string()
        } else {
            format!("Its releases are {}.", crate::package::and_list(&releases))
        },
        "Give one of its releases with `--to`".to_string(),
    )
}

fn package_of(client: &Client, name: &ScopedName, written: &str) -> Result<Package, Reply> {
    client
        .ask(&Request::Package {
            package: name.clone(),
        })
        .map_err(|asked| match asked {
            Asked::Failure(code) if code == protocol::PACKAGE_NOT_FOUND.code => {
                if written.starts_with('@') {
                    not_in_registry(
                        format!("The registry has no package `{name}`"),
                        "`jadpo package add` installs that exact name or stops: it never picks one that looks like it, since the name decides whose code your app runs.".to_string(),
                        "Check the name, or look for it with `jadpo package search`".to_string(),
                    )
                } else {
                    not_in_registry(
                        format!("Jadpo has no official package `{written}`"),
                        format!(
                            "A name with no publisher means Jadpo's own, `{name}`, and another publisher's is never chosen for you."
                        ),
                        format!("Look for one with `jadpo package search {written}`, then add it by its whole name"),
                    )
                }
            }
            other => other.reply(),
        })
}

// --- add -------------------------------------------------------------------

/// `written` as a registry name: `@acme/postcodes`, or a bare name for
/// Jadpo's own. `None` for anything else that isn't one.
pub(crate) fn name_to_add(written: &str) -> Option<Result<ScopedName, Reply>> {
    if let Some((scoped, version)) = written
        .strip_prefix('@')
        .and_then(|rest| rest.split_once('@'))
    {
        return Some(Err(Reply::failed(crate::package::arguments(
            format!("`{written}` isn't a registry name"),
            "A registry name is `@publisher/name`, and a release is named on its own.".to_string(),
            format!(
                "Run `jadpo package add @{scoped} --to {}`",
                version.trim_start_matches('v')
            ),
        ))));
    }
    if written.starts_with('@') {
        return Some(ScopedName::parse(written).map_err(|_| {
            Reply::failed(crate::package::arguments(
                format!("`{written}` isn't a registry name"),
                "A registry name is `@publisher/name`: each part lower-case letters and digits, starting with a letter, with single `-`s between them.".to_string(),
                "Give the package's whole name: `@<publisher>/<name>`".to_string(),
            ))
        }));
    }
    if written.contains(['/', '\\', '.']) {
        return None;
    }
    Some(ScopedName::parse(&format!("@jadpo/{written}")).map_err(|_| {
        Reply::failed(crate::package::arguments(
            format!("`{written}` isn't a package's folder or a registry name"),
            "A name with no publisher means Jadpo's own, in lower-case letters, digits and single `-`s.".to_string(),
            "Give the package's whole name: `@<publisher>/<name>`".to_string(),
        ))
    }))
}

/// `yanked` means new installs refuse it, and apps that have it keep it.
fn yanked(name: &ScopedName, version: &Version) -> Reply {
    Reply::Failed(Box::new(
        Diagnostic::new(
            &rule::INSTALL_YANKED,
            format!("`{name}` {version} has been yanked"),
        )
        .reason(
            "A releaser withdrew it: apps that have it keep working, and nothing installs it anew."
                .to_string(),
        )
        .next_for(
            "Take another release: leave out `--to` for the newest",
            Decider::Human,
        ),
    ))
}

/// The release `add` installs, fetched and proved: the one `--to` names, or
/// the newest that isn't yanked or a pre-release.
pub(crate) fn for_add(
    call: &Call,
    root: &Path,
    name: &ScopedName,
    written: &str,
) -> Result<Fetched, Reply> {
    let client = Client::of(&call.environment);
    let package = package_of(&client, name, written)?;
    let version = match call.value("--to") {
        Some(to) => {
            let version = wanted_version(to)?;
            match package.releases.iter().find(|r| r.version == version) {
                None => return Err(no_release(name, &package, &version)),
                Some(found) if found.yanked => return Err(yanked(name, &version)),
                Some(found) => found.version.clone(),
            }
        }
        None => {
            let newest = package
                .releases
                .iter()
                .filter(|r| !r.yanked && !is_pre_release(&r.version))
                .max_by(|a, b| a.version.cmp(&b.version));
            match newest {
                Some(found) => found.version.clone(),
                None if package.releases.iter().all(|r| r.yanked)
                    && !package.releases.is_empty() =>
                {
                    let last = package.releases.last().map(|r| r.version.clone());
                    return Err(yanked(name, &last.unwrap_or_else(version_zero)));
                }
                None => {
                    return Err(not_in_registry(
                        format!("`{name}` has no full release"),
                        if package.releases.is_empty() {
                            "It has no releases yet.".to_string()
                        } else {
                            "It has only pre-releases, which are taken only when named.".to_string()
                        },
                        "Name a pre-release with `--to`, or wait for a release".to_string(),
                    ));
                }
            }
        }
    };
    fetch(root, &client, name, package, &version, "package add")
}

fn version_zero() -> Version {
    Version::parse("0.0.0").expect("a version")
}

fn wanted_version(to: &str) -> Result<Version, Reply> {
    Version::parse(to.trim_start_matches('v')).map_err(|_| {
        Reply::failed(crate::package::arguments(
            format!("`{to}` isn't a version"),
            "`--to` names a release, such as `2.1.0`.".to_string(),
            "Give a release's version with `--to`".to_string(),
        ))
    })
}

fn is_pre_release(version: &Version) -> bool {
    version.as_str().contains('-')
}

fn major(version: &Version) -> &str {
    version.as_str().split('.').next().unwrap_or("")
}

/// The rows `add` adds about where a package comes from.
pub(crate) fn origin_rows(fetched: &Fetched, now: i64) -> Vec<(String, String)> {
    let publisher = &fetched.package.publisher;
    let standing = if publisher.official {
        "official"
    } else if publisher.verified {
        "verified"
    } else {
        "not verified"
    };
    let mut rows = vec![(
        "publisher".to_string(),
        format!("{}, {standing}", publisher.handle),
    )];
    if let Some(first) = fetched.package.releases.first() {
        rows.push((
            "age".to_string(),
            format!(
                "first released {}, {}",
                day_of(&first.released_at),
                age_words(seconds_of(&first.released_at), now)
            ),
        ));
    }
    if !fetched.package.maintained {
        rows.push((
            "status".to_string(),
            match &fetched.package.successor {
                Some(next) => format!("unmaintained, carried on by {next}"),
                None => "unmaintained".to_string(),
            },
        ));
    }
    rows.push((
        "log".to_string(),
        format!(
            "entry {} of {}",
            fetched.logged.index + 1,
            fetched.logged.size
        ),
    ));
    rows
}

// --- update ----------------------------------------------------------------

/// A newer release `update` didn't take.
#[derive(Clone, Debug)]
pub(crate) struct Waiting {
    pub(crate) version: Version,
    pub(crate) why: String,
    /// Whether `--to` can take it: a new major can't.
    pub(crate) same_major: bool,
}

/// Which release `update` takes.
#[derive(Clone, Debug)]
pub(crate) struct Choice {
    pub(crate) chosen: Option<Version>,
    pub(crate) urgent: bool,
    pub(crate) waiting: Vec<Waiting>,
}

/// The newest release of the installed major version at least three days old
/// (area 11), or marked urgent where `urgent_counts`, and the newer ones it
/// leaves. A yanked release or a pre-release is never chosen; a time that
/// can't be read, or one still to come, is never old enough.
pub(crate) fn choose(
    releases: &[ReleaseSummary],
    installed: &Version,
    now: i64,
    urgent_counts: bool,
) -> Choice {
    let wanted = |release: &&ReleaseSummary| {
        !release.yanked && !is_pre_release(&release.version) && release.version > *installed
    };
    let old_enough = |release: &ReleaseSummary| {
        seconds_of(&release.released_at).is_some_and(|at| at <= now && now - at >= WAIT)
    };
    let chosen = releases
        .iter()
        .filter(wanted)
        .filter(|release| major(&release.version) == major(installed))
        .filter(|release| old_enough(release) || (urgent_counts && release.urgent))
        .max_by(|a, b| a.version.cmp(&b.version));
    let waiting = releases
        .iter()
        .filter(wanted)
        .filter(|release| chosen.is_none_or(|chosen| release.version > chosen.version))
        .map(|release| {
            let same_major = major(&release.version) == major(installed);
            Waiting {
                version: release.version.clone(),
                same_major,
                why: if same_major {
                    let at = seconds_of(&release.released_at);
                    match at.and_then(|at| day_from(at + WAIT)) {
                        Some(day) => {
                            format!("{} old, taken from {day}", age_words_bare(at, now))
                        }
                        None => "its time can't be read".to_string(),
                    }
                } else {
                    "a new major, added beside this one with `jadpo package add`".to_string()
                },
            }
        })
        .collect();
    Choice {
        chosen: chosen.map(|release| release.version.clone()),
        urgent: chosen
            .is_some_and(|release| urgent_counts && release.urgent && !old_enough(release)),
        waiting,
    }
}

/// What the review of a registry update adds (area 11, "Installing and
/// updating from the registry").
pub(crate) struct Notes {
    pub(crate) released: String,
    pub(crate) urgent: Option<String>,
    pub(crate) waiting: Vec<Waiting>,
    pub(crate) moved: Option<(String, String)>,
    pub(crate) record: Vec<ChangeEntry>,
    pub(crate) from: Version,
    pub(crate) to: Version,
}

/// What `update` does with a package from the registry.
pub(crate) enum Update {
    UpToDate {
        name: ScopedName,
        waiting: Vec<Waiting>,
        moved: Option<(String, String)>,
    },
    Take(Box<(Fetched, Notes)>),
}

/// Works out and fetches what `update` takes. `changed` says the installed
/// copy no longer has its digest, so the release it names comes back even
/// with nothing newer.
pub(crate) fn for_update(
    call: &Call,
    root: &Path,
    locked_name: &str,
    installed: &str,
    changed: bool,
) -> Result<Update, Reply> {
    let client = Client::of(&call.environment);
    let name = ScopedName::parse(locked_name).map_err(|why| {
        Reply::failed(crate::package::arguments(
            format!(
                "`{}` has a registry name that can't be read",
                jadpo_db::LOCK
            ),
            format!("{why}."),
            "Run `jadpo records` to see what is wrong with it, then put it right".to_string(),
        ))
    })?;
    let installed_version = Version::parse(installed).map_err(|why| {
        Reply::failed(crate::package::arguments(
            format!("`{}` has a version that can't be read", jadpo_db::LOCK),
            format!("{why}."),
            "Run `jadpo records` to see what is wrong with it, then put it right".to_string(),
        ))
    })?;
    let first = package_of(&client, &name, locked_name)?;
    // A package handed to another publisher goes on under its new name, and
    // the review says so. Its releases there are new to this app, so they
    // wait, urgent or not: a mark its new publisher made can't be what lets
    // their first release past the wait.
    let names_an_old_release = call.value("--to").is_some_and(|to| {
        first
            .releases
            .iter()
            .any(|release| release.version.as_str() == to.trim_start_matches('v'))
    });
    let carried_on = first
        .successor
        .clone()
        .filter(|next| *next != name && !names_an_old_release);
    let (name_now, package, mut moved) = match carried_on {
        Some(next) => {
            let package = package_of(&client, &next, next.to_string().as_str())?;
            let moved = Some((
                first.publisher.handle.to_string(),
                package.publisher.handle.to_string(),
            ));
            (next, package, moved)
        }
        None => (name.clone(), first, None),
    };
    if moved.is_none() && package.publisher.handle != name.handle() {
        moved = Some((
            name.handle().to_string(),
            package.publisher.handle.to_string(),
        ));
    }
    let now = now_seconds();
    let choice = match call.value("--to") {
        Some(to) => {
            let version = wanted_version(to)?;
            match package.releases.iter().find(|r| r.version == version) {
                None => return Err(no_release(&name_now, &package, &version)),
                Some(found) if found.yanked => return Err(yanked(&name_now, &version)),
                Some(_) => Choice {
                    chosen: Some(version),
                    urgent: false,
                    waiting: Vec::new(),
                },
            }
        }
        None => choose(&package.releases, &installed_version, now, moved.is_none()),
    };
    let target = match (&choice.chosen, changed) {
        (Some(chosen), _) => chosen.clone(),
        (None, true) => installed_version.clone(),
        (None, false) => {
            return Ok(Update::UpToDate {
                name: name_now,
                waiting: choice.waiting,
                moved,
            });
        }
    };
    let urgent = if choice.urgent {
        Some(urgent_reason(&client, &name, &installed_version))
    } else {
        None
    };
    let released = package
        .releases
        .iter()
        .find(|release| release.version == target)
        .map_or_else(String::new, |release| {
            format!(
                "{}, {}",
                day_of(&release.released_at),
                age_words(seconds_of(&release.released_at), now)
            )
        });
    let between: Vec<Version> = package
        .releases
        .iter()
        .filter(|release| release.version > installed_version && release.version <= target)
        .map(|release| release.version.clone())
        .collect();
    let mut record = Vec::new();
    for version in between.iter().filter(|version| **version != target) {
        let release: Release = client
            .ask(&Request::Release {
                package: name_now.clone(),
                version: version.clone(),
            })
            .map_err(Asked::reply)?;
        record.extend(release.change_record);
    }
    let fetched = fetch(root, &client, &name_now, package, &target, "package update")?;
    if between.contains(&target) {
        record.extend(fetched.release.change_record.clone());
    }
    Ok(Update::Take(Box::new((
        fetched,
        Notes {
            released,
            urgent,
            waiting: choice.waiting,
            moved,
            record,
            from: installed_version,
            to: target,
        },
    ))))
}

/// Why a release was marked urgent, from the advisory against what is
/// installed.
fn urgent_reason(client: &Client, name: &ScopedName, installed: &Version) -> String {
    let answer: Result<AdvisoryAnswer, Asked> =
        client.ask(&Request::QueryAdvisories(AdvisoryQuery {
            packages: vec![Installed {
                package: name.clone(),
                version: installed.clone(),
            }],
        }));
    let said: Vec<String> = answer
        .map(|answer| answer.advisories)
        .unwrap_or_default()
        .iter()
        .filter(|advisory| advisory.package == *name)
        .map(|advisory| format!("{}: {}", advisory.id, advisory.summary))
        .collect();
    if said.is_empty() {
        "its releaser marked it urgent".to_string()
    } else {
        said.join("; ")
    }
}

/// A change record's entries as a block, in the order of their releases.
pub(crate) fn record_block(heading: String, record: &[ChangeEntry]) -> Block {
    let rows = record
        .iter()
        .map(|entry| {
            let (mark, word) = match entry.change {
                Change::Added => (ChangeMark::Added, "added"),
                Change::NewHost => (ChangeMark::Added, "new host"),
                Change::PersonalDataSent => (ChangeMark::Added, "personal data"),
                Change::Removed => (ChangeMark::Removed, "removed"),
                Change::Retyped => (ChangeMark::Changed, "retyped"),
                Change::NewFailure => (ChangeMark::Changed, "new failure"),
                Change::BehaviourChanged => (ChangeMark::Changed, "behaviour changed"),
            };
            let what = match entry.change {
                Change::Added | Change::Removed | Change::Retyped if entry.kind != "provider" => {
                    format!("{} {}", entry.kind, entry.declaration)
                }
                _ => entry.declaration.clone(),
            };
            let mut detail = entry.detail.clone().unwrap_or_default();
            if let Some(why) = &entry.why {
                if !detail.is_empty() {
                    detail.push_str(", ");
                }
                detail.push_str(&format!("a fix: {why}"));
            }
            (mark, vec![word.to_string(), what, detail])
        })
        .collect();
    Block::ChangeList { heading, rows }
}

/// The newer releases not taken, as a list.
pub(crate) fn waiting_block(waiting: &[Waiting]) -> Block {
    Block::list(
        "Newer, not taken",
        waiting
            .iter()
            .map(|entry| Row::new(entry.version.to_string(), entry.why.clone()))
            .collect(),
    )
}

// --- times -----------------------------------------------------------------

pub(crate) fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}

/// `2026-10-31T09:00:00Z`, with any fraction of a second, as seconds since
/// the epoch. The protocol's times are RFC 3339 in UTC, so anything else
/// isn't read.
pub(crate) fn seconds_of(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 20 || !text.is_ascii() || !text.ends_with('Z') {
        return None;
    }
    let number = |from: usize, to: usize| -> Option<i64> {
        let part = &text[from..to];
        part.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    if (bytes[4], bytes[7], bytes[10], bytes[13], bytes[16]) != (b'-', b'-', b'T', b':', b':') {
        return None;
    }
    let fraction = &text[19..text.len() - 1];
    if !fraction.is_empty()
        && (!fraction.starts_with('.')
            || fraction.len() == 1
            || !fraction[1..].bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
    let (hour, minute, second) = (number(11, 13)?, number(14, 16)?, number(17, 19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    Some(days_from_civil(year, month, day) * DAY + hour * 3600 + minute * 60 + second)
}

/// Howard Hinnant's `days_from_civil`.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The ISO date of a time in seconds.
fn day_from(seconds: i64) -> Option<String> {
    let z = seconds.div_euclid(DAY) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// The date in a protocol time as written.
fn day_of(text: &str) -> &str {
    text.get(..10).unwrap_or(text)
}

fn age_words(at: Option<i64>, now: i64) -> String {
    match at {
        None => "at a time that can't be read".to_string(),
        Some(at) if at > now => "dated in the future".to_string(),
        Some(at) if now - at < DAY => "today".to_string(),
        _ => format!("{} ago", age_words_bare(at, now)),
    }
}

/// `1 day`, `40 days`, `less than a day`.
fn age_words_bare(at: Option<i64>, now: i64) -> String {
    match at {
        None => "an unknown time".to_string(),
        Some(at) if at > now => "not yet a day".to_string(),
        Some(at) => match (now - at) / DAY {
            0 => "less than a day".to_string(),
            1 => "1 day".to_string(),
            days => format!("{days} days"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(version: &str, days: i64, now: i64) -> ReleaseSummary {
        ReleaseSummary {
            version: Version::parse(version).unwrap(),
            digest: Digest::of_files(&[]),
            released_at: format!("{}T00:00:00Z", day_from(now - days * DAY).unwrap()),
            yanked: false,
            urgent: false,
        }
    }

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    const NOW: i64 = 1_793_000_000 / DAY * DAY;

    #[test]
    fn a_time_is_read_as_the_protocol_writes_it() {
        assert_eq!(seconds_of("1970-01-02T00:00:01Z"), Some(DAY + 1));
        assert_eq!(seconds_of("2000-03-01T00:00:00.250Z"), Some(951_868_800));
        assert_eq!(day_from(951_868_800).as_deref(), Some("2000-03-01"));
        for bad in [
            "",
            "yesterday",
            "2026-10-31 09:00:00Z",
            "2026-10-31T09:00:00+01:00",
            "2026-13-31T09:00:00Z",
            "2026-10-31T09:00:00.Z",
        ] {
            assert_eq!(seconds_of(bad), None, "{bad}");
        }
    }

    #[test]
    fn update_takes_the_newest_release_three_days_old() {
        let releases = [
            release("2.0.0", 30, NOW),
            release("2.1.0", 4, NOW),
            release("2.2.0", 3, NOW),
            release("2.3.0", 2, NOW),
        ];
        let choice = choose(&releases, &version("2.0.0"), NOW, true);
        assert_eq!(choice.chosen, Some(version("2.2.0")));
        assert!(!choice.urgent);
        assert_eq!(choice.waiting.len(), 1);
        assert_eq!(choice.waiting[0].version, version("2.3.0"));
        assert!(
            choice.waiting[0].why.starts_with("2 days old, taken from "),
            "{}",
            choice.waiting[0].why
        );
    }

    #[test]
    fn a_yanked_or_pre_release_or_unreadable_or_future_one_is_never_taken() {
        let mut yanked = release("2.1.0", 40, NOW);
        yanked.yanked = true;
        let mut unreadable = release("2.2.0", 40, NOW);
        unreadable.released_at = "long ago".into();
        let future = release("2.3.0", -40, NOW);
        let pre = release("2.4.0-beta.1", 40, NOW);
        let choice = choose(
            &[yanked, unreadable, future, pre],
            &version("2.0.0"),
            NOW,
            true,
        );
        assert_eq!(choice.chosen, None);
        let named: Vec<String> = choice
            .waiting
            .iter()
            .map(|w| w.version.to_string())
            .collect();
        assert_eq!(named, ["2.2.0", "2.3.0"]);
    }

    #[test]
    fn an_urgent_release_skips_the_wait_unless_it_doesnt_count() {
        let mut urgent = release("2.0.1", 0, NOW);
        urgent.urgent = true;
        let newer = release("2.1.0", 0, NOW);
        let releases = [urgent, newer];
        let choice = choose(&releases, &version("2.0.0"), NOW, true);
        assert_eq!(choice.chosen, Some(version("2.0.1")));
        assert!(choice.urgent);
        assert_eq!(choice.waiting[0].version, version("2.1.0"));
        assert_eq!(
            choose(&releases, &version("2.0.0"), NOW, false).chosen,
            None
        );
    }

    #[test]
    fn it_never_goes_down_or_across_a_major() {
        let releases = [release("2.0.0", 30, NOW), release("3.0.0", 30, NOW)];
        let choice = choose(&releases, &version("2.1.0"), NOW, true);
        assert_eq!(choice.chosen, None);
        assert_eq!(choice.waiting.len(), 1);
        assert!(!choice.waiting[0].same_major);
    }
}

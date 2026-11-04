//! `jadpo package publish [--dry-run]` (area 11, "Releasing" and "Developing
//! for the registry"; chunk R5). It holds the package to the publishing bar
//! here, then asks the registry to publish a repository at a tag, sending
//! only those two, with no sign-in: nothing goes live without a releaser's
//! passkey on jadpo.dev.
//!
//! So that the bar is run on what will be published, the checkout has to be
//! the tag's commit, unchanged, and that tag on `origin` the same commit.
//!
//! ```text
//! ✓ Asked the registry to release Postcodes.v2 2.1.0
//!     repository  https://github.com/acme/postcodes
//!     tag         v2.1.0
//!     state       pending
//!
//! Registry's bar
//! ✓ tests  3 tests
//!
//! ● release pending  Postcodes.v2 2.1.0 · a releaser confirms it with a passkey on jadpo.dev
//! ```

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jadpo_diag::{Decider, Diagnostic, rule};
use jadpo_registry::protocol::{
    BarRow, ReleaseRequest, ReleaseState, Request, StartRelease, StartedRelease,
};
use jadpo_term::{Block, Fact, Json, Outcome, Row, RowState, Screen};

use crate::install;
use crate::package::{self, Git, Mode, arguments};
use crate::package_release::Previous;
use crate::registry::{Asked, Client};
use crate::run::{Call, Reply};

/// How long a release is waited for: the verifier compiles it and runs its
/// tests against the Jadpo release it declares and the newest.
const LONGEST_WAIT: Duration = Duration::from_secs(20 * 60);

/// The longest gap between asks.
const LONGEST_PAUSE: Duration = Duration::from_secs(5);

pub(crate) fn publish(call: &Call) -> Reply {
    let folder = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    if let Some(extra) = call.arguments.get(1) {
        return Reply::failed(arguments(
            format!("`jadpo package publish` takes no folder, not `{extra}`"),
            "It publishes the package in this folder, at the root of its git repository."
                .to_string(),
            "Run `jadpo package publish` in the package's repository".to_string(),
        ));
    }
    let dry_run = call.flag("--dry-run");
    let Some(package) = std::fs::read_to_string(folder.join(jadpo_db::PACKAGE_INDEX))
        .ok()
        .and_then(|index| package::index_name(&index))
    else {
        return package::no_package("package publish");
    };
    let committed = std::fs::read_to_string(folder.join(jadpo_db::record::SURFACE_FILE))
        .ok()
        .and_then(|text| install::surface_version(&jadpo_db::record::normalise(&text)));
    let Some(version) = committed else {
        return not_at_tag(
            format!("`{package}` has no {} yet", jadpo_db::record::SURFACE_FILE),
            "It says which version this is, and `jadpo package check` writes it.".to_string(),
            "Run `jadpo package check`, commit what it writes and tag the release".to_string(),
        );
    };
    let tag = format!("v{version}");
    let repository = match ready_repository(&folder, &tag) {
        Ok(repository) => repository,
        Err(reply) => return reply,
    };
    // The bar, here, on the commit as it stands: no surprises at the registry.
    match package::bar(
        call,
        &folder,
        Mode::Verify {
            previous: previous_release(&folder, &package, &tag).map(Box::new),
            run: true,
        },
    ) {
        Reply::Report { diagnostics, .. } if diagnostics.is_empty() => {}
        other => return other,
    }
    let client = Client::of(&call.environment);
    let started: StartedRelease = match client.ask(&Request::StartRelease(StartRelease {
        repository: repository.clone(),
        tag: tag.clone(),
        dry_run,
    })) {
        Ok(started) => started,
        Err(Asked::Failure(code)) if code == "repository_refused" => {
            return repository_refused(&repository, "The registry refused it.".to_string());
        }
        Err(asked) => return asked.reply(),
    };
    let settled = match settle(&client, &started.id, LONGEST_WAIT) {
        Ok(Some(settled)) => settled,
        Ok(None) => {
            return Reply::Failed(Box::new(
                Diagnostic::new(
                    &rule::PUBLISH_FAILED,
                    format!("The registry is still checking `{package}` {version}"),
                )
                .reason(format!(
                    "Its release `{}` hasn't finished after {} minutes, so whether it passes isn't known.",
                    started.id,
                    LONGEST_WAIT.as_secs() / 60
                ))
                .next_for("Look at the release on jadpo.dev, or run the command again", Decider::Human),
            ));
        }
        Err(asked) => return asked.reply(),
    };
    shown(
        &package,
        &version,
        &repository,
        &tag,
        dry_run,
        &started.id,
        &settled,
    )
}

/// Asks after a release until it stops verifying; `None` when `longest` runs
/// out first.
fn settle(client: &Client, id: &str, longest: Duration) -> Result<Option<ReleaseRequest>, Asked> {
    let started = Instant::now();
    let mut pause = Duration::from_millis(100);
    loop {
        let asked: ReleaseRequest = client.ask(&Request::ReleaseRequest { id: id.to_string() })?;
        if asked.state != ReleaseState::Verifying {
            return Ok(Some(asked));
        }
        if started.elapsed() >= longest {
            return Ok(None);
        }
        std::thread::sleep(pause);
        pause = (pause * 2).min(LONGEST_PAUSE);
    }
}

fn state_word(state: ReleaseState) -> &'static str {
    match state {
        ReleaseState::Verifying => "verifying",
        ReleaseState::Failed => "failed",
        ReleaseState::Pending => "pending",
        ReleaseState::Cancelled => "cancelled",
        ReleaseState::Live => "live",
    }
}

fn bar_rows(rows: &[BarRow]) -> Vec<Row> {
    rows.iter()
        .map(|row| {
            Row::new(row.check.clone(), row.detail.clone()).state(if row.passed {
                RowState::Done
            } else {
                RowState::Failed
            })
        })
        .collect()
}

/// What the registry said, on the screen.
fn shown(
    package: &str,
    version: &str,
    repository: &str,
    tag: &str,
    dry_run: bool,
    id: &str,
    settled: &ReleaseRequest,
) -> Reply {
    let data = Json::object([
        ("package", Json::from(package)),
        ("version", Json::from(version)),
        ("repository", Json::from(repository)),
        ("tag", Json::from(tag)),
        ("dry_run", dry_run.into()),
        ("id", Json::from(id)),
        ("state", Json::from(state_word(settled.state))),
        (
            "bar",
            Json::array(settled.bar.iter().map(|row| {
                Json::object([
                    ("check", Json::from(row.check.as_str())),
                    ("passed", row.passed.into()),
                    ("detail", Json::from(row.detail.as_str())),
                ])
            })),
        ),
    ]);
    let mut screen = Screen::new();
    let asked = if dry_run {
        format!("Asked the registry to check {package} {version}")
    } else {
        format!("Asked the registry to release {package} {version}")
    };
    screen.push(Block::Status {
        outcome: if settled.state == ReleaseState::Failed {
            Outcome::Failure
        } else {
            Outcome::Success
        },
        text: asked,
        rows: vec![
            ("repository".to_string(), repository.to_string()),
            ("tag".to_string(), tag.to_string()),
            ("state".to_string(), state_word(settled.state).to_string()),
        ],
    });
    if !settled.bar.is_empty() {
        screen.push(Block::list("Registry's bar", bar_rows(&settled.bar)));
    }
    let release = format!("{package} {version}");
    match settled.state {
        ReleaseState::Failed | ReleaseState::Cancelled => {
            let failed: Vec<String> = settled
                .bar
                .iter()
                .filter(|row| !row.passed)
                .map(|row| format!("{}: {}", row.check, row.detail))
                .collect();
            let (message, reason) = if settled.state == ReleaseState::Cancelled {
                (
                    format!("`{release}` was cancelled"),
                    "A member of its publisher cancelled it.".to_string(),
                )
            } else {
                (
                    format!("The registry's verifier refused `{release}`"),
                    if failed.is_empty() {
                        "It gave no rows saying why.".to_string()
                    } else {
                        format!("Failed: {}.", failed.join("; "))
                    },
                )
            };
            let diagnostic = Diagnostic::new(&rule::PUBLISH_FAILED, message)
                .reason(reason)
                .next(
                    "Fix it, run `jadpo package check`, then tag a new release: a tag that moves is refused",
                );
            let sources = std::collections::BTreeMap::new();
            screen.push(Block::diagnostic_from(&diagnostic, &sources, None));
            screen.push(Block::summary(
                Outcome::Failure,
                if dry_run {
                    "dry run failed"
                } else {
                    "publish failed"
                },
                [Fact::plain(release)],
            ));
            Reply::Report {
                screen,
                data,
                diagnostics: vec![diagnostic],
                sources,
            }
        }
        // A dry run ends with nothing published, whatever it is called.
        _ if dry_run => {
            screen.push(Block::summary(
                Outcome::Success,
                "dry run passed",
                [Fact::plain(release), Fact::plain("nothing published")],
            ));
            screen.push(Block::hint(
                "jadpo package publish",
                "asks the registry to release it",
            ));
            Reply::screen(screen, data)
        }
        ReleaseState::Live => {
            screen.push(Block::summary(
                Outcome::Success,
                "release live",
                [Fact::plain(release)],
            ));
            Reply::screen(screen, data)
        }
        _ => {
            screen.push(Block::summary(
                Outcome::Running,
                "release pending",
                [
                    Fact::plain(release),
                    Fact::plain("a releaser confirms it with a passkey on jadpo.dev"),
                ],
            ));
            Reply::screen(screen, data)
        }
    }
}

/// `publish.not_at_tag`.
fn not_at_tag(message: String, reason: String, next: String) -> Reply {
    Reply::Failed(Box::new(
        Diagnostic::new(&rule::PUBLISH_NOT_AT_TAG, message)
            .reason(reason)
            .next(next),
    ))
}

/// `publish.repository_refused`.
fn repository_refused(repository: &str, reason: String) -> Reply {
    Reply::Failed(Box::new(
        Diagnostic::new(
            &rule::PUBLISH_REPOSITORY_REFUSED,
            format!("The registry can't publish from `{repository}`"),
        )
        .reason(format!(
            "{reason} A release is made from a public git repository reached over HTTPS, since the registry never accepts an upload."
        ))
        .next_for(
            "Point `origin` at the repository's `https://` address: `git remote set-url origin https://<host>/<path>`",
            Decider::Human,
        ),
    ))
}

/// A remote's address as the registry fetches it: `https://host/path`, from
/// that, or from an ssh address of the same repository. `None` for anything
/// else, such as `http://`.
pub(crate) fn https_address(remote: &str) -> Option<String> {
    let (host, path) = if let Some(rest) = remote.strip_prefix("https://") {
        rest.split_once('/')?
    } else if let Some(rest) = remote.strip_prefix("ssh://git@") {
        rest.split_once('/')?
    } else {
        remote.strip_prefix("git@")?.split_once(':')?
    };
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    if host.is_empty() || host.contains('@') || path.is_empty() {
        return None;
    }
    Some(format!("https://{host}/{path}"))
}

/// The repository to ask for, once this checkout is `tag`'s commit, unchanged,
/// at the root of a repository, with the same tag on `origin`.
fn ready_repository(folder: &Path, tag: &str) -> Result<String, Reply> {
    let git = |args: &[&str]| install::run_git(Some(folder), args);
    let push = format!("git push origin {tag}");
    let prefix = git(&["rev-parse", "--show-prefix"]).map_err(|said| {
        not_at_tag(
            "This package isn't in a git repository".to_string(),
            format!("A release is a tag in a public git repository. Git said: {said}."),
            "Put it in a repository, push it and tag the release".to_string(),
        )
    })?;
    if !prefix.trim().is_empty() {
        return Err(not_at_tag(
            format!("The package is in `{}` of its repository", prefix.trim()),
            "The registry reads a release from the root of its commit, which holds only the package.".to_string(),
            "Give the package a repository of its own".to_string(),
        ));
    }
    let Some(tagged) = install::tag_commit(folder, tag) else {
        return Err(not_at_tag(
            format!("This repository has no tag `{tag}`"),
            format!(
                "The committed surface says this is {}, and a release is its tag.",
                tag.trim_start_matches('v')
            ),
            format!("Run `git tag {tag}`, then `{push}`"),
        ));
    };
    let head = git(&["rev-parse", "HEAD"]).unwrap_or_default();
    if head.trim() != tagged {
        return Err(not_at_tag(
            format!("This checkout isn't at `{tag}`"),
            "The registry checks the tag's commit, and the bar has to run on the same code."
                .to_string(),
            format!("Run `git checkout {tag}`, or release this commit as a new version"),
        ));
    }
    let changes = git(&["status", "--porcelain"]).unwrap_or_default();
    if let Some(first) = changes.lines().next() {
        return Err(not_at_tag(
            format!("This checkout has changes since `{tag}`"),
            format!(
                "The registry checks the tag's commit, and the bar has to run on the same code. `{}` differs.",
                first.get(3..).unwrap_or(first)
            ),
            "Commit them in a new release, or put them away first".to_string(),
        ));
    }
    let Ok(remote) = git(&["config", "--get", "remote.origin.url"]) else {
        return Err(repository_refused(
            "origin",
            "This repository has no `origin` remote.".to_string(),
        ));
    };
    let remote = remote.trim();
    let Some(repository) = https_address(remote) else {
        return Err(repository_refused(
            remote,
            "Its `origin` isn't an HTTPS address.".to_string(),
        ));
    };
    let pushed = git(&[
        "ls-remote",
        "--tags",
        "origin",
        &format!("refs/tags/{tag}"),
        &format!("refs/tags/{tag}^{{}}"),
    ])
    .map_err(|said| {
        not_at_tag(
            format!("`origin` couldn't be read for the tag `{tag}`"),
            format!("Git said: {said}."),
            format!("Check your git access to it, then run `{push}`"),
        )
    })?;
    // An annotated tag's commit is on the `^{}` line, which comes last.
    let Some(on_remote) = pushed
        .lines()
        .last()
        .and_then(|line| line.split_whitespace().next())
    else {
        return Err(not_at_tag(
            format!("`{tag}` isn't on `origin`"),
            "The registry fetches the tag from the repository, so it has to be pushed.".to_string(),
            format!("Run `{push}`"),
        ));
    };
    if on_remote != tagged {
        return Err(not_at_tag(
            format!("`{tag}` is another commit on `origin`"),
            format!(
                "It is `{on_remote}` there and `{tagged}` here, and a tag that moves is refused."
            ),
            "Release this commit as a new version".to_string(),
        ));
    }
    Ok(repository)
}

/// The release before `tag` of the package named as `package` is in code, by
/// the repository's tags, for the bar to compare with.
pub(crate) fn previous_release(folder: &Path, package: &str, tag: &str) -> Option<Previous> {
    let git = Git::of(folder)?;
    let (code, _) = package.rsplit_once('.')?;
    let current = package::semver(tag);
    let release = git
        .releases_of_code(code)
        .into_iter()
        .find(|release| package::semver(&release.tag) < current)?;
    let named = release.named();
    Some(Previous::from_tag(&git, release, &named))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_repository_reached_over_https_is_asked_for() {
        for good in [
            "https://github.com/acme/postcodes",
            "https://github.com/acme/postcodes.git",
            "https://github.com/acme/postcodes/",
            "git@github.com:acme/postcodes.git",
            "ssh://git@github.com/acme/postcodes.git",
        ] {
            assert_eq!(
                https_address(good).as_deref(),
                Some("https://github.com/acme/postcodes"),
                "{good}"
            );
        }
        for bad in [
            "http://github.com/acme/postcodes",
            "git://github.com/acme/postcodes",
            "file:///srv/postcodes",
            "/srv/postcodes",
            "https://token@github.com/acme/postcodes",
            "https://github.com",
        ] {
            assert_eq!(https_address(bad), None, "{bad}");
        }
    }
}

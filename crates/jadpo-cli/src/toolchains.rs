//! `jadpo toolchain` (area 14, "Which jadpo runs"; roadmap chunk D14): the
//! Jadpo releases the launcher keeps, and the release a project pins.
//!
//! - `list` (or nothing) lists the releases kept in the cache, newest
//!   first, and `remove <version>` deletes one. Both always run with the
//!   installed `jadpo` (crate::launcher).
//! - `pin` writes the release running into `jadpo.lock` as its `jadpo
//!   version` line, so every run of the project uses it; run in a project,
//!   it runs as that project's release, so that is what it pins. `unpin`
//!   takes the line out again.

use std::path::PathBuf;

use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Block, Cell, Fact, Json, Outcome, Screen, Table};

use crate::help::VERSION;
use crate::releases::{Cache, Release};
use crate::run::{Call, Reply};
use crate::table::Value;

/// What `jadpo toolchain` does.
pub const ACTIONS: &[Value] = &[
    Value {
        name: "list",
        description: "List the Jadpo releases kept for projects",
    },
    Value {
        name: "remove",
        description: "Delete a kept release",
    },
    Value {
        name: "pin",
        description: "Pin the release this project runs with in jadpo.lock",
    },
    Value {
        name: "unpin",
        description: "Take the pin out of jadpo.lock",
    },
];

pub fn toolchain(call: &Call) -> Reply {
    let action = call.arguments.first().map(String::as_str);
    let rest = call.arguments.get(1..).unwrap_or_default();
    match (action, rest) {
        (None | Some("list"), []) => list(call),
        (Some("remove"), [version]) => remove(call, version),
        (Some("remove"), _) => Reply::failed(arguments(
            "`jadpo toolchain remove` needs one version",
            "It deletes one kept release, named by its version.",
            "Run `jadpo toolchain list` to see the releases kept, then `jadpo toolchain remove <version>`",
        )),
        (Some("pin" | "unpin"), []) => pin(call, action == Some("pin")),
        (Some(action @ ("list" | "pin" | "unpin")), _) => Reply::failed(arguments(
            format!("`jadpo toolchain {action}` takes no other argument"),
            "It works on the project the command runs in, or on every kept release.",
            format!("Run `jadpo toolchain {action}`"),
        )),
        (None, _) => list(call),
        (Some(other), _) => Reply::failed(arguments(
            format!("`{other}` isn't a toolchain action"),
            "`jadpo toolchain` lists the kept releases, `remove` deletes one, and `pin` and `unpin` write the release a project runs with into `jadpo.lock`.",
            "Use `list`, `remove`, `pin` or `unpin`",
        )),
    }
}

fn arguments(
    message: impl Into<String>,
    reason: impl Into<String>,
    next: impl Into<String>,
) -> Diagnostic {
    Diagnostic::new(&rule::CLI_TOOLCHAIN_ARGUMENTS, message)
        .reason(reason)
        .next(next)
}

/// The cache this `jadpo` keeps releases in, absolute, when the
/// environment names a cache folder.
fn cache(call: &Call) -> Option<Cache> {
    let home = crate::package_cache::home(&call.environment)?;
    let home = std::path::absolute(&home).unwrap_or(home);
    Some(Cache::new(&home, crate::toolchain::HOST))
}

fn list(call: &Call) -> Reply {
    let cache = cache(call);
    let held = cache.as_ref().map(Cache::held).unwrap_or_default();
    let folder = cache
        .as_ref()
        .map(|cache| cache.folder.display().to_string());
    let mut screen = Screen::new();
    if let Some(folder) = &folder {
        screen.push(Block::fields(vec![("cache", folder.clone())]));
    }
    let sizes: Vec<u64> = held
        .iter()
        .map(|release| {
            cache
                .as_ref()
                .and_then(|cache| std::fs::metadata(cache.binary(*release)).ok())
                .map_or(0, |meta| meta.len())
        })
        .collect();
    if !held.is_empty() {
        let mut table = Table::new(["version", "size"]).right(1);
        for (release, size) in held.iter().zip(&sizes) {
            table = table.row(vec![
                Cell::text(release.to_string()),
                Cell::muted(crate::build::size(*size as usize)),
            ]);
        }
        screen.push(Block::Table(table));
    }
    let headline = match held.len() {
        0 => "no releases kept".to_string(),
        1 => "1 release kept".to_string(),
        n => format!("{n} releases kept"),
    };
    screen.push(
        Block::summary(
            Outcome::Success,
            headline,
            held.is_empty()
                .then(|| Fact::plain("one is kept when a project needs another release"))
                .into_iter()
                .chain([Fact::plain(format!("this jadpo is {VERSION}"))]),
        )
        .with_plain_headline("toolchain list"),
    );
    let data = Json::object([
        ("running", Json::from(VERSION)),
        ("cache", folder.map_or(Json::Null, Json::from)),
        (
            "releases",
            Json::array(held.iter().zip(&sizes).map(|(release, size)| {
                Json::object([
                    ("version", Json::from(release.to_string())),
                    (
                        "bytes",
                        Json::from(i64::try_from(*size).unwrap_or(i64::MAX)),
                    ),
                ])
            })),
        ),
    ]);
    Reply::screen(screen, data)
}

fn remove(call: &Call, version: &str) -> Reply {
    let Some(release) = Release::parse(version) else {
        return Reply::failed(arguments(
            format!("`{version}` isn't a Jadpo release"),
            "A release is three numbers, such as `0.1.2`.",
            "Run `jadpo toolchain list` to see the releases kept",
        ));
    };
    let removed = cache(call).map_or(Ok(false), |cache| cache.remove(release));
    match removed {
        Ok(true) => Reply::screen(
            Screen::new().with(Block::Status {
                outcome: Outcome::Success,
                text: format!("Removed Jadpo {release}"),
                rows: Vec::new(),
            }),
            Json::object([("removed", Json::from(release.to_string()))]),
        ),
        Ok(false) => Reply::failed(arguments(
            format!("Jadpo {release} isn't kept"),
            "The cache holds only the releases projects needed, and this isn't one of them.",
            "Run `jadpo toolchain list` to see the releases kept",
        )),
        Err(why) => Reply::failed(
            Diagnostic::new(
                &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                format!("Jadpo {release} couldn't be removed"),
            )
            .reason(format!("Removing it failed: {why}"))
            .next("Check that the cache folder can be written, then run the command again"),
        ),
    }
}

fn pin(call: &Call, pinning: bool) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return Reply::failed(
            Diagnostic::new(
                &rule::TOOLCHAIN_PROJECT_NOT_FOUND,
                "There is no project here to pin a release for",
            )
            .reason("`jadpo toolchain pin` writes into the `jadpo.lock` of the project it runs in, found by walking up to `app.jadpo`, and there is none.")
            .next("Run it in the project's folder"),
        );
    };
    let path = root.join(jadpo_db::LOCK);
    let text = std::fs::read_to_string(&path).ok();
    let packages = match text.as_deref().map(jadpo_db::parse_lock) {
        None => Vec::new(),
        Some(Ok(packages)) => packages,
        Some(Err((line, why))) => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::RECORD_EDITED,
                    format!("`{}` can't be read", jadpo_db::LOCK),
                )
                .reason(format!("Line {}: {why}.", line + 1))
                .next("Run `jadpo records` to see what is wrong with it, then put it right"),
            );
        }
    };
    let pinned = text
        .as_deref()
        .and_then(jadpo_db::locked_jadpo)
        .map(|(version, _)| version);
    if pinning {
        let app = std::fs::read_to_string(root.join(jadpo_db::PROJECT_FILE)).unwrap_or_default();
        let line = jadpo_check::jadpo_version::declared(&app).and_then(|declared| declared.line());
        let running = Release::running();
        if let Some(line) = line.filter(|line| *line != running.line()) {
            return Reply::failed(jadpo_check::jadpo_version::refusal(
                &format!("{}.{}", line.0, line.1),
                line > running.line(),
                false,
                None,
                None,
            ));
        }
    }
    let written = if pinning {
        jadpo_db::write_pinned_lock(Some(VERSION), &packages)
    } else {
        jadpo_db::write_lock(&packages)
    };
    let unchanged = if pinning {
        pinned.as_deref() == Some(VERSION)
    } else {
        pinned.is_none()
    };
    let write = if unchanged {
        Ok(())
    } else if !pinning && packages.is_empty() {
        // A lock that held only the pin goes: no lock is the usual state
        // of a project with no packages.
        std::fs::remove_file(&path)
    } else {
        std::fs::write(&path, written)
    };
    if let Err(error) = write {
        return Reply::failed(
            Diagnostic::new(
                &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                format!("`{}` couldn't be written", jadpo_db::LOCK),
            )
            .reason(format!("Writing it failed: {error}."))
            .next("Check that the project folder can be written, then run the command again"),
        );
    }
    let text = match (pinning, unchanged) {
        (true, true) => format!("`{}` already pins Jadpo {VERSION}", jadpo_db::LOCK),
        (true, false) => format!("Pinned Jadpo {VERSION} in `{}`", jadpo_db::LOCK),
        (false, true) => format!("`{}` pins no release", jadpo_db::LOCK),
        (false, false) => format!("Took the pin out of `{}`", jadpo_db::LOCK),
    };
    let mut screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text,
        rows: Vec::new(),
    });
    if !unchanged {
        screen.push(Block::hint(
            "git diff jadpo.lock",
            "shows the change, to commit with the code",
        ));
    }
    Reply::screen(
        screen,
        Json::object([(
            "pinned",
            if pinning {
                Json::from(VERSION)
            } else {
                Json::Null
            },
        )]),
    )
}

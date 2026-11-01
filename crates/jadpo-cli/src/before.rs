//! The before side of a change, read from git (area 18, "Comparing two
//! builds"; chunk A8): the repository's default branch, read from its
//! remote, at its merge base with the current branch, as a pull request's
//! diff is; on the default branch itself, the previous commit; or the
//! branch or commit `--before` names. `jadpo check` and `jadpo acceptance
//! new` read the project's records and acceptance files there, and the
//! names of its domains, so acceptances can be judged.
//!
//! With no before side, nothing is compared or judged, and `jadpo check`
//! says why in one line (Richard, 2026-10-30): outside a git repository
//! (said only when the project holds acceptances), with no default branch,
//! or for a project new since its base. A base commit a shallow clone
//! doesn't hold is an error naming `git fetch`. A project git ignores,
//! such as a test's under `target/`, isn't looked for at all.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use jadpo_db::{Before, BeforeSide};

/// Runs git in `dir`; its output when it succeeds.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn line(dir: &Path, args: &[&str]) -> Option<String> {
    git(dir, args)
        .map(|out| out.trim().to_string())
        .filter(|out| !out.is_empty())
}

/// The default branch: the remote's `HEAD`, else its `main` or `master`,
/// else a local `main` or `master`. Its ref and its short name.
fn default_branch(top: &Path) -> Option<(String, String)> {
    let remotes = git(top, &["remote"]).unwrap_or_default();
    let mut remotes: Vec<&str> = remotes.lines().filter(|r| !r.is_empty()).collect();
    remotes.sort_by_key(|remote| *remote != "origin");
    for remote in remotes {
        if let Some(head) = line(
            top,
            &["symbolic-ref", "-q", &format!("refs/remotes/{remote}/HEAD")],
        ) {
            let short = head.rsplit('/').next().unwrap_or("main").to_string();
            return Some((head, short));
        }
        for name in ["main", "master"] {
            let candidate = format!("refs/remotes/{remote}/{name}");
            if line(top, &["rev-parse", "-q", "--verify", &candidate]).is_some() {
                return Some((candidate, name.to_string()));
            }
        }
    }
    for name in ["main", "master"] {
        let candidate = format!("refs/heads/{name}");
        if line(top, &["rev-parse", "-q", "--verify", &candidate]).is_some() {
            return Some((candidate, name.to_string()));
        }
    }
    None
}

/// `refs/remotes/origin/main` as people write it: `origin/main`.
fn shown(reference: &str) -> String {
    reference
        .strip_prefix("refs/remotes/")
        .or_else(|| reference.strip_prefix("refs/heads/"))
        .unwrap_or(reference)
        .to_string()
}

/// Why `--before` can't be read: the branch or commit isn't one.
#[derive(Debug)]
pub struct UnknownRef(pub String);

/// The before side of the project at `root`, `with` the branch or commit
/// `--before` names, when it names one; whether the project holds
/// acceptances decides whether being outside git is said.
pub fn read(root: &Path, with: Option<&str>, has_acceptances: bool) -> Result<Before, UnknownRef> {
    let outside = || {
        if has_acceptances {
            Before::Nothing("not a git repository".to_string())
        } else {
            Before::NotRead
        }
    };
    let Some(top) = line(root, &["rev-parse", "--show-toplevel"]) else {
        return Ok(outside());
    };
    let top = Path::new(&top).to_path_buf();
    // A project git ignores isn't tracked: a test's, under `target/`.
    if git(root, &["check-ignore", "-q", "."]).is_some() {
        return Ok(Before::NotRead);
    }
    let prefix = line(root, &["rev-parse", "--show-prefix"]).unwrap_or_default();
    let shallow = line(&top, &["rev-parse", "--is-shallow-repository"]).as_deref() == Some("true");
    let (base, described) = match with {
        Some(reference) => {
            let Some(commit) = line(
                &top,
                &[
                    "rev-parse",
                    "-q",
                    "--verify",
                    &format!("{reference}^{{commit}}"),
                ],
            ) else {
                if shallow {
                    return Ok(Before::Missing {
                        base: reference.to_string(),
                    });
                }
                return Err(UnknownRef(reference.to_string()));
            };
            let short = commit.get(..7).unwrap_or(&commit).to_string();
            (commit, format!("{reference} at {short}"))
        }
        None => {
            if line(&top, &["rev-parse", "-q", "--verify", "HEAD"]).is_none() {
                return Ok(Before::Nothing("no commit yet".to_string()));
            }
            let Some((default, name)) = default_branch(&top) else {
                if shallow {
                    return Ok(Before::Missing {
                        base: "the default branch".to_string(),
                    });
                }
                return Ok(Before::Nothing(
                    "no default branch to compare with; name one with `jadpo check --before <branch or commit>`".to_string(),
                ));
            };
            let current = line(&top, &["symbolic-ref", "-q", "--short", "HEAD"]);
            if current.as_deref() == Some(name.as_str()) {
                let Some(previous) =
                    line(&top, &["rev-parse", "-q", "--verify", "HEAD~1^{commit}"])
                else {
                    if shallow {
                        return Ok(Before::Missing {
                            base: "the previous commit".to_string(),
                        });
                    }
                    return Ok(Before::Nothing(
                        "the first commit has nothing before it".to_string(),
                    ));
                };
                let short = previous.get(..7).unwrap_or(&previous).to_string();
                (previous, format!("the previous commit, {short}"))
            } else {
                match line(&top, &["merge-base", "HEAD", &default]) {
                    Some(commit) => {
                        let short = commit.get(..7).unwrap_or(&commit).to_string();
                        (commit, format!("{} at {short}", shown(&default)))
                    }
                    None if shallow => {
                        return Ok(Before::Missing {
                            base: format!("the merge base with {}", shown(&default)),
                        });
                    }
                    None => {
                        return Ok(Before::Nothing(format!(
                            "this branch shares no history with {}",
                            shown(&default)
                        )));
                    }
                }
            }
        }
    };
    let pathspec = if prefix.is_empty() {
        "."
    } else {
        prefix.as_str()
    };
    let listed = git(
        &top,
        &[
            "ls-tree",
            "-r",
            "-z",
            "--name-only",
            "--full-tree",
            &base,
            "--",
            pathspec,
        ],
    )
    .unwrap_or_default();
    let paths: Vec<String> = listed
        .split('\0')
        .filter_map(|path| path.strip_prefix(prefix.as_str()))
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect();
    if !paths.iter().any(|path| path == jadpo_db::PROJECT_FILE) {
        return Ok(Before::Nothing(format!(
            "the project is new since {described}"
        )));
    }
    let domains: BTreeSet<String> = paths
        .iter()
        .filter_map(|path| {
            let rest = path.strip_prefix("domains/")?;
            Some(rest.split_once('/')?.0.to_string())
        })
        .collect();
    let migrations: BTreeSet<String> = paths
        .iter()
        .filter_map(|path| {
            let rest = path.strip_prefix("database/migrations/")?;
            Some(rest.split_once('/')?.0.to_string())
        })
        .collect();
    let wanted: Vec<&String> = paths
        .iter()
        .filter(|path| jadpo_db::compared_on_the_before_side(path))
        .collect();
    let files = contents(&top, &base, &prefix, &wanted);
    Ok(Before::At(BeforeSide {
        commit: base.get(..7).unwrap_or(&base).to_string(),
        described,
        files,
        domains,
        migrations,
    }))
}

/// The text of each of `paths` at `base`, through one `git cat-file
/// --batch`.
fn contents(top: &Path, base: &str, prefix: &str, paths: &[&String]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if paths.is_empty() {
        return out;
    }
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(top)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return out;
    };
    let request: String = paths
        .iter()
        .map(|path| format!("{base}:{prefix}{path}\n"))
        .collect();
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(request.as_bytes());
    }
    let mut bytes = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_end(&mut bytes);
    }
    let _ = child.wait();
    let mut at = 0usize;
    for path in paths {
        let Some(end) = bytes[at..].iter().position(|b| *b == b'\n') else {
            break;
        };
        let header = String::from_utf8_lossy(&bytes[at..at + end]).into_owned();
        at += end + 1;
        let size: Option<usize> = header
            .rsplit(' ')
            .next()
            .and_then(|size| size.parse().ok())
            .filter(|_| !header.ends_with(" missing"));
        let Some(size) = size else {
            continue;
        };
        let body = bytes.get(at..at + size).unwrap_or_default();
        out.insert(path.to_string(), String::from_utf8_lossy(body).into_owned());
        at += size + 1;
    }
    out
}

/// The whole commit `commit` names, for the project at `root`: what
/// `jadpo approval` builds and keeps the manifest of under it.
pub fn full_commit(root: &Path, commit: &str) -> Option<String> {
    line(
        root,
        &[
            "rev-parse",
            "-q",
            "--verify",
            &format!("{commit}^{{commit}}"),
        ],
    )
}

/// Every file of the project at `root` as commit `commit` holds it, by
/// project-relative path, as [`jadpo_db::read`] finds a project on disk:
/// the files a project can hold, hidden ones and `build/` left out
/// (chunk V2). A package linked to a live checkout is read as installed,
/// since the checkout isn't part of the commit.
pub fn tree(root: &Path, commit: &str) -> jadpo_db::FileSet {
    let mut files = jadpo_db::FileSet::default();
    let Some(top) = line(root, &["rev-parse", "--show-toplevel"]) else {
        return files;
    };
    let top = Path::new(&top).to_path_buf();
    let prefix = line(root, &["rev-parse", "--show-prefix"]).unwrap_or_default();
    let pathspec = if prefix.is_empty() {
        "."
    } else {
        prefix.as_str()
    };
    let listed = git(
        &top,
        &[
            "ls-tree",
            "-r",
            "-z",
            "--name-only",
            "--full-tree",
            commit,
            "--",
            pathspec,
        ],
    )
    .unwrap_or_default();
    let paths: Vec<String> = listed
        .split('\0')
        .filter_map(|path| path.strip_prefix(prefix.as_str()))
        .filter(|path| {
            !path.is_empty()
                && !path.starts_with("build/")
                && !path.split('/').any(|part| part.starts_with('.'))
                && jadpo_db::role(path, jadpo_db::Layout::Project).is_some()
        })
        .map(str::to_string)
        .collect();
    let wanted: Vec<&String> = paths.iter().collect();
    for (path, text) in contents(&top, commit, &prefix, &wanted) {
        files.insert(path, Ok(text));
    }
    files
}

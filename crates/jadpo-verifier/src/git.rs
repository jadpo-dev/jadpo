//! Fetching one commit, and reading it without checking anything out.
//!
//! `git` is given network only to fetch, and only to the address the host
//! was checked to have: the name is looked up here, every address it has
//! must be public, and `git` is handed that address, so a name that changes
//! between the check and the connection (DNS rebinding) reaches nothing
//! else. Redirects aren't followed, nothing else is configured for it, no
//! hook, filter or submodule can run, and the fetched objects are read with
//! `ls-tree` and `cat-file` in a sandbox with no network at all. A commit is
//! never checked out, so `.gitattributes` and paths git would write are
//! never acted on.

use std::ffi::OsString;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::files::Entry;
use crate::request::{self, Repository, is_commit};
use crate::sandbox::{Ended, Finished, Job, Limits, Sandbox, tool};

/// How a host's name is looked up.
pub type Resolver = Arc<dyn Fn(&str) -> std::io::Result<Vec<IpAddr>> + Send + Sync>;

/// Which repositories can be fetched, and how they are reached.
#[derive(Clone)]
pub struct GitPolicy {
    resolver: Resolver,
    /// A folder whose `file://` repositories are taken instead of HTTPS.
    /// Only tests can set it: no flag or variable reaches it.
    local_root: Option<PathBuf>,
}

impl std::fmt::Debug for GitPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitPolicy")
            .field("local_root", &self.local_root)
            .finish_non_exhaustive()
    }
}

impl Default for GitPolicy {
    fn default() -> Self {
        GitPolicy::https()
    }
}

/// Why `git` couldn't do something.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitError {
    /// The repository isn't one to reach.
    Refused(String),
    /// It couldn't be reached, or doesn't hold what was asked for.
    Failed(String),
    /// What a commit holds is past what's read.
    TooMuch,
}

impl GitPolicy {
    /// Public hosts over HTTPS, looked up by the system.
    pub fn https() -> GitPolicy {
        GitPolicy {
            resolver: Arc::new(|host| {
                Ok((host, 443)
                    .to_socket_addrs()?
                    .map(|address| address.ip())
                    .collect())
            }),
            local_root: None,
        }
    }

    /// Public hosts over HTTPS, looked up by `resolver`.
    pub fn https_resolved_by(resolver: Resolver) -> GitPolicy {
        GitPolicy {
            resolver,
            local_root: None,
        }
    }

    /// `file://` repositories under `root`, for tests on a machine with no
    /// git host. The server never makes this one.
    pub fn local_for_tests(root: PathBuf) -> GitPolicy {
        GitPolicy {
            resolver: Arc::new(|_| Ok(Vec::new())),
            local_root: Some(root),
        }
    }

    /// Reads a repository's address.
    pub fn repository(&self, text: &str) -> Result<Repository, String> {
        if let Some(root) = &self.local_root {
            let url = format!("file://{}/", root.display());
            if text.starts_with(&url) && !text.contains("/../") {
                return Ok(Repository::given_by_tests(text.to_string()));
            }
        }
        Repository::parse(text)
    }

    /// The arguments and paths `git` needs to reach `repository`.
    fn reach(&self, repository: &Repository) -> Result<Reach, GitError> {
        if self.local_root.is_some() && repository.url().starts_with("file://") {
            return Ok(Reach {
                config: vec!["protocol.file.allow=always".to_string()],
                allow: "file",
                read_only: self.local_root.clone().into_iter().collect(),
            });
        }
        let host = repository.host();
        let addresses = (self.resolver)(host).map_err(|error| {
            GitError::Failed(format!("`{host}` couldn't be looked up: {error}"))
        })?;
        let address = request::public_address(&addresses)
            .ok_or_else(|| GitError::Refused(format!("`{host}` isn't only at public addresses")))?;
        let shown = match address {
            IpAddr::V4(v4) => v4.to_string(),
            IpAddr::V6(v6) => format!("[{v6}]"),
        };
        Ok(Reach {
            config: vec![
                "protocol.https.allow=always".to_string(),
                format!("http.curloptResolve={host}:443:{shown}"),
            ],
            allow: "https",
            read_only: Vec::new(),
        })
    }
}

struct Reach {
    config: Vec<String>,
    allow: &'static str,
    read_only: Vec<PathBuf>,
}

/// Limits on fetching: bounded by what the host sends, not a package's code.
const FETCH: Limits = Limits {
    cpu_seconds: 120,
    wall: Duration::from_secs(180),
    memory_bytes: 2 << 30,
    file_bytes: 256 << 20,
    tmp_bytes: 16 << 20,
    processes: 4096,
    open_files: 1024,
    output_bytes: 1 << 20,
};

/// Limits on reading what was fetched.
const READ: Limits = Limits {
    cpu_seconds: 60,
    wall: Duration::from_secs(120),
    memory_bytes: 2 << 30,
    file_bytes: 1 << 20,
    tmp_bytes: 16 << 20,
    processes: 4096,
    open_files: 1024,
    output_bytes: 64 << 20,
};

/// Configuration that leaves `git` nothing of its own to run or reach.
const CONFIG: &[&str] = &[
    "protocol.allow=never",
    "core.hooksPath=/dev/null",
    "core.fsmonitor=false",
    "core.askPass=",
    "credential.helper=",
    "http.followRedirects=false",
    "http.proxy=",
    "http.sslVerify=true",
    "http.lowSpeedLimit=1024",
    "http.lowSpeedTime=30",
    "protocol.version=2",
    "transfer.fsckObjects=true",
    "fetch.fsckObjects=true",
    "fetch.unpackLimit=1",
    "fetch.recurseSubmodules=false",
    "submodule.recurse=false",
    "gc.auto=0",
    "maintenance.auto=false",
];

/// `git` with `args` and no configuration but [`CONFIG`] and `reach`'s.
fn job(tmp: &Path, limits: Limits, args: &[&str], reach: Option<&Reach>) -> Result<Job, GitError> {
    let program = tool("git").ok_or_else(|| GitError::Failed("`git` isn't installed".into()))?;
    let mut all: Vec<OsString> = Vec::new();
    for setting in CONFIG
        .iter()
        .map(|s| (*s).to_string())
        .chain(reach.iter().flat_map(|reach| reach.config.iter().cloned()))
    {
        all.push("-c".into());
        all.push(setting.into());
    }
    all.extend(args.iter().map(OsString::from));
    let mut env: Vec<(String, String)> = [
        ("PATH", "/usr/bin:/bin"),
        ("LC_ALL", "C"),
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("GIT_TERMINAL_PROMPT", "0"),
        ("GIT_ATTR_NOSYSTEM", "1"),
    ]
    .iter()
    .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
    .collect();
    env.push((
        "GIT_ALLOW_PROTOCOL".to_string(),
        reach.map_or("none", |reach| reach.allow).to_string(),
    ));
    Ok(Job {
        program,
        args: all,
        env,
        read_only: reach
            .map(|reach| reach.read_only.clone())
            .unwrap_or_default(),
        writable: Vec::new(),
        tmp: tmp.to_path_buf(),
        network: reach.is_some(),
        stdin: None,
        limits,
    })
}

fn run(sandbox: &Sandbox, job: &Job) -> Result<Finished, GitError> {
    sandbox
        .run(job)
        .map_err(|error| GitError::Failed(format!("`git` couldn't start: {error}")))
}

/// The commit `tag` points at; `None` when the repository has no such tag.
pub fn resolve_tag(
    sandbox: &Sandbox,
    policy: &GitPolicy,
    repository: &Repository,
    tag: &str,
    tmp: &Path,
) -> Result<Option<String>, GitError> {
    let reach = policy.reach(repository)?;
    let exact = format!("refs/tags/{tag}");
    let peeled = format!("{exact}^{{}}");
    let job = job(
        tmp,
        FETCH,
        &["ls-remote", "--", repository.url(), &exact, &peeled],
        Some(&reach),
    )?;
    let done = run(sandbox, &job)?;
    if done.ended != Ended::Exited(0) {
        return Err(GitError::Failed("the repository couldn't be read".into()));
    }
    let text = String::from_utf8_lossy(&done.stdout);
    let mut found = None;
    for line in text.lines() {
        let Some((commit, name)) = line.split_once('\t') else {
            continue;
        };
        if !is_commit(commit) {
            continue;
        }
        // An annotated tag's own line comes first: the line it peels to is
        // the commit.
        if name == peeled {
            return Ok(Some(commit.to_string()));
        }
        if name == exact {
            found = Some(commit.to_string());
        }
    }
    Ok(found)
}

/// Fetches `commit` into a new bare repository at `into`.
pub fn fetch(
    sandbox: &Sandbox,
    policy: &GitPolicy,
    repository: &Repository,
    commit: &str,
    into: &Path,
    tmp: &Path,
) -> Result<(), GitError> {
    let reach = policy.reach(repository)?;
    let Some(folder) = into.to_str() else {
        return Err(GitError::Failed("the scratch folder isn't UTF-8".into()));
    };
    let mut init = job(tmp, READ, &["init", "--quiet", "--bare", folder], None)?;
    init.writable = vec![into.to_path_buf()];
    if run(sandbox, &init)?.ended != Ended::Exited(0) {
        return Err(GitError::Failed("a repository couldn't be made".into()));
    }
    let mut fetching = job(
        tmp,
        FETCH,
        &[
            "--git-dir",
            folder,
            "fetch",
            "--quiet",
            "--depth=1",
            "--no-tags",
            "--no-recurse-submodules",
            "--no-write-fetch-head",
            "--",
            repository.url(),
            &format!("{commit}:refs/jadpo/verifying"),
        ],
        Some(&reach),
    )?;
    fetching.writable = vec![into.to_path_buf()];
    let done = run(sandbox, &fetching)?;
    if done.ended != Ended::Exited(0) {
        return Err(GitError::Failed(
            "the repository couldn't be fetched at that commit".into(),
        ));
    }
    // The name asked for has to be a commit, not any other kind of object.
    let done = run(
        sandbox,
        &reading(tmp, into, &["--git-dir", folder, "cat-file", "-t", commit])?,
    )?;
    if String::from_utf8_lossy(&done.stdout).trim() != "commit" {
        return Err(GitError::Failed("that name isn't a commit".into()));
    }
    Ok(())
}

/// A job reading the fetched repository, which it can't change.
fn reading(tmp: &Path, repository: &Path, args: &[&str]) -> Result<Job, GitError> {
    let mut job = job(tmp, READ, args, None)?;
    job.read_only = vec![repository.to_path_buf()];
    Ok(job)
}

/// Every file of `commit`'s tree with its mode, object and size.
pub fn list(
    sandbox: &Sandbox,
    repository: &Path,
    commit: &str,
    tmp: &Path,
) -> Result<Vec<Entry>, GitError> {
    let Some(folder) = repository.to_str() else {
        return Err(GitError::Failed("the scratch folder isn't UTF-8".into()));
    };
    let done = run(
        sandbox,
        &reading(
            tmp,
            repository,
            &[
                "--git-dir",
                folder,
                "ls-tree",
                "-r",
                "-l",
                "-z",
                "--full-tree",
                commit,
            ],
        )?,
    )?;
    match done.ended {
        Ended::Exited(0) => {}
        Ended::OutputTooLarge => return Err(GitError::TooMuch),
        _ => return Err(GitError::Failed("its tree couldn't be read".into())),
    }
    let mut entries = Vec::new();
    for record in done.stdout.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        let Some(tab) = record.iter().position(|byte| *byte == b'\t') else {
            return Err(GitError::Failed("its tree couldn't be read".into()));
        };
        let head = String::from_utf8_lossy(&record[..tab]);
        let mut fields = head.split_whitespace();
        let (Some(mode), Some(_kind), Some(object), Some(size)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(GitError::Failed("its tree couldn't be read".into()));
        };
        entries.push(Entry {
            // A path that isn't UTF-8 comes out with a character no package
            // path may hold, and is refused by its name.
            path: String::from_utf8(record[tab + 1..].to_vec())
                .unwrap_or_else(|_| "\u{0}not UTF-8".to_string()),
            mode: mode.to_string(),
            object: object.to_string(),
            size: size.parse().ok(),
        });
    }
    Ok(entries)
}

/// The bytes of each of `entries`, in their order.
pub fn read(
    sandbox: &Sandbox,
    repository: &Path,
    entries: &[Entry],
    tmp: &Path,
) -> Result<Vec<(String, Vec<u8>)>, GitError> {
    let Some(folder) = repository.to_str() else {
        return Err(GitError::Failed("the scratch folder isn't UTF-8".into()));
    };
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let mut asking = reading(
        tmp,
        repository,
        &["--git-dir", folder, "cat-file", "--batch"],
    )?;
    let mut names = String::new();
    for entry in entries {
        names.push_str(&entry.object);
        names.push('\n');
    }
    asking.stdin = Some(names.into_bytes());
    let done = run(sandbox, &asking)?;
    match done.ended {
        Ended::Exited(0) => {}
        Ended::OutputTooLarge => return Err(GitError::TooMuch),
        _ => return Err(GitError::Failed("its files couldn't be read".into())),
    }
    let unreadable = || GitError::Failed("its files couldn't be read".into());
    let mut files = Vec::with_capacity(entries.len());
    let mut at = 0;
    let out = done.stdout;
    for entry in entries {
        let line_end = out[at..]
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or_else(unreadable)?
            + at;
        let head = String::from_utf8_lossy(&out[at..line_end]);
        let mut fields = head.split(' ');
        let (Some(object), Some("blob"), Some(size), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(unreadable());
        };
        let size: usize = size.parse().map_err(|_| unreadable())?;
        if object != entry.object {
            return Err(unreadable());
        }
        let start = line_end + 1;
        let end = start.checked_add(size).ok_or_else(unreadable)?;
        let bytes = out.get(start..end).ok_or_else(unreadable)?;
        files.push((entry.path.clone(), bytes.to_vec()));
        at = end + 1;
    }
    Ok(files)
}

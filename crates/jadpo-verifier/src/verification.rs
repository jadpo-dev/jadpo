//! One verification: a repository at a commit, against the previous
//! release, worked out with no network.
//!
//! `jadpo package verify` runs twice, each in a sandbox of its own that
//! shares nothing with the other or with any other verification:
//!
//! 1. With `--no-run`, which runs none of the package's code: the surface,
//!    facts, fingerprints, version and the change record's entries from the
//!    source. Only compiling reads the package here.
//! 2. As a whole, which runs its tests and examples and the previous
//!    release's: what the bar's other rows and the `behaviour_changed`
//!    entries say.
//!
//! So everything a release shows of what its code can do comes from a
//! process none of that code ran in. If the second run says anything else
//! about what the first decided, its code has found a way to change its own
//! report, and the release fails with the `agreement` row. The digest and
//! archive are made here, from the files of the commit.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use jadpo_registry::archive;
use jadpo_registry::names::Digest;
use serde_json::{Value, json};

use crate::files::{self, Judgement};
use crate::git::{self, GitError, GitPolicy};
use crate::request::Repository;
use crate::sandbox::{Ended, Finished, Job, Limits, Sandbox};

/// What running a package's own checks may use: enough for any real
/// package's tests on the machine `fly.toml` names, whose memory holds this
/// and the temporary folder together.
pub const CHECKS: Limits = Limits {
    cpu_seconds: 300,
    wall: Duration::from_secs(600),
    memory_bytes: 3 << 30,
    file_bytes: 256 << 20,
    tmp_bytes: 512 << 20,
    processes: 4096,
    open_files: 1024,
    output_bytes: 32 << 20,
};

/// Fetches a release's archive by its digest.
pub type Archives = Arc<dyn Fn(&str) -> Result<Vec<u8>, String> + Send + Sync>;

/// The `jadpo` of a release line such as `0.3`, when it can be had.
pub type Releases = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

/// What a verifier runs with.
#[derive(Clone)]
pub struct Verifier {
    /// The `jadpo` it checks with, the newest release it has.
    pub jadpo: PathBuf,
    pub sandbox: Sandbox,
    pub git: GitPolicy,
    /// Where each verification's folder is made.
    pub scratch: PathBuf,
    pub limits: Limits,
    pub archives: Archives,
    pub releases: Releases,
}

impl std::fmt::Debug for Verifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Verifier")
            .field("jadpo", &self.jadpo)
            .field("sandbox", &self.sandbox)
            .field("scratch", &self.scratch)
            .finish_non_exhaustive()
    }
}

/// What to verify.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub repository: Repository,
    pub commit: String,
    /// The previous release's digest; `None` for a first release.
    pub previous: Option<String>,
}

/// A verification's end.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// It was checked: the answer's fields, `passed` among them.
    Checked(Value),
    /// It couldn't be: nothing is known of the release, and asking again
    /// may work.
    CouldNotCheck { code: &'static str, message: String },
}

/// A verification's own folder, removed when it ends.
struct Workspace(PathBuf);

impl Workspace {
    fn new(root: &Path) -> std::io::Result<Workspace> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!("jadpo-verifier-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&path)?;
        let path = path.canonicalize()?;
        Ok(Workspace(path))
    }

    /// A new empty folder in it.
    fn folder(&self, name: &str) -> std::io::Result<PathBuf> {
        let path = self.0.join(name);
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A verification that couldn't be done, from a scratch folder's error.
fn unwritable(error: std::io::Error) -> Outcome {
    Outcome::CouldNotCheck {
        code: "verifier_failed",
        message: format!("The verifier's scratch folder couldn't be used: {error}."),
    }
}

/// A row of the bar, as `jadpo package verify --json` writes them.
fn row(item: &str, state: &str, detail: &str) -> Value {
    json!({ "item": item, "state": state, "detail": detail })
}

/// A diagnostic that names only its rule and message.
fn problem(rule: &str, message: String) -> Value {
    json!({ "rule": rule, "message": message })
}

/// An answer with every field, nothing yet known but what's given.
fn answer(passed: bool, bar: Vec<Value>, problems: Vec<Value>, left_out: &[String]) -> Value {
    json!({
        "passed": passed,
        "package": null,
        "version": null,
        "digest": null,
        "archive": null,
        "bar": bar,
        "problems": problems,
        "surface": null,
        "facts": null,
        "change_record": [],
        "fingerprints": [],
        "previous": null,
        "jadpo": null,
        "checked_with": [],
        "left_out": left_out,
    })
}

/// The commit's files refused, before any of them was written.
fn refused(judged: &Judgement) -> Outcome {
    let detail = judged.problem().unwrap_or_default();
    let mut problems: Vec<Value> = judged
        .refused
        .iter()
        .map(|(path, why)| {
            problem(
                "publish.file_not_allowed",
                format!("`{path}` can't be in a release: {why}"),
            )
        })
        .collect();
    if judged.refused.is_empty() {
        problems.push(problem(
            "publish.too_large",
            format!("The release is {detail}"),
        ));
    }
    Outcome::Checked(answer(
        false,
        vec![row("files", "failed", &detail)],
        problems,
        &judged.left_out,
    ))
}

impl Verifier {
    /// The commit `tag` points at in `repository`.
    pub fn resolve(&self, repository: &Repository, tag: &str) -> Result<Option<String>, GitError> {
        let work =
            Workspace::new(&self.scratch).map_err(|error| GitError::Failed(error.to_string()))?;
        let tmp = work
            .folder("tmp")
            .map_err(|error| GitError::Failed(error.to_string()))?;
        git::resolve_tag(&self.sandbox, &self.git, repository, tag, &tmp)
    }

    /// Verifies `request`.
    pub fn verify(&self, request: &Request) -> Outcome {
        match self.checked(request) {
            Ok(outcome) | Err(outcome) => outcome,
        }
    }

    fn checked(&self, request: &Request) -> Result<Outcome, Outcome> {
        let work = Workspace::new(&self.scratch).map_err(unwritable)?;
        let tmp = work.folder("tmp-git").map_err(unwritable)?;
        let repository = work.folder("repository").map_err(unwritable)?;
        let could_not =
            |code: &'static str, message: String| Outcome::CouldNotCheck { code, message };
        git::fetch(
            &self.sandbox,
            &self.git,
            &request.repository,
            &request.commit,
            &repository,
            &tmp,
        )
        .map_err(|error| match error {
            GitError::Refused(why) => could_not("repository_refused", why),
            GitError::Failed(why) => could_not("commit_not_fetched", why),
            GitError::TooMuch => could_not("commit_not_fetched", "It holds too much.".into()),
        })?;

        // What it holds, from the listing alone.
        let entries = match git::list(&self.sandbox, &repository, &request.commit, &tmp) {
            Ok(entries) => entries,
            Err(GitError::TooMuch) => {
                return Ok(Outcome::Checked(answer(
                    false,
                    vec![row("files", "failed", "too many files to list")],
                    vec![problem(
                        "publish.too_large",
                        "The commit holds too many files to list".to_string(),
                    )],
                    &[],
                )));
            }
            Err(GitError::Failed(why) | GitError::Refused(why)) => {
                return Err(could_not("commit_not_fetched", why));
            }
        };
        let mut judged = files::judge_tree(entries);
        if judged.problem().is_some() {
            return Ok(refused(&judged));
        }
        let read =
            git::read(&self.sandbox, &repository, &judged.release, &tmp).map_err(|error| {
                match error {
                    GitError::Refused(why) | GitError::Failed(why) => {
                        could_not("commit_not_fetched", why)
                    }
                    GitError::TooMuch => {
                        could_not("commit_not_fetched", "It holds too much.".to_string())
                    }
                }
            })?;
        for (path, bytes) in &read {
            if !files::is_text(bytes) {
                judged
                    .refused
                    .push((path.clone(), files::NOT_TEXT.to_string()));
            }
        }
        if !judged.refused.is_empty() {
            return Ok(refused(&judged));
        }
        let packed = match archive::pack(&read) {
            Ok(packed) => packed,
            Err(error) => {
                judged
                    .refused
                    .push(("the release".to_string(), error.to_string()));
                return Ok(refused(&judged));
            }
        };
        let new = work.folder("new").map_err(unwritable)?;
        write_files(&new, &read).map_err(unwritable)?;

        // The previous release, from the registry, as its digest says.
        let previous = match &request.previous {
            None => None,
            Some(digest) => {
                let folder = work.folder("previous").map_err(unwritable)?;
                self.previous(digest, &folder)?;
                Some(folder)
            }
        };

        // Worked out first, by a process none of its code runs in.
        let source = self.report(
            &work,
            &self.jadpo,
            "source",
            &new,
            previous.as_deref(),
            true,
        )?;
        let ran = self.report(&work, &self.jadpo, "run", &new, previous.as_deref(), false)?;
        let mut checked = combine(&source, &ran, &judged.left_out);
        // The bar's first row, unless the `jadpo` that ran wrote its own.
        if let Some(bar) = checked["bar"].as_array_mut()
            && !bar.iter().any(|row| row["item"] == "files")
        {
            bar.insert(0, row("files", "passed", &judged.summary()));
        }
        self.with_declared(
            &work,
            &source,
            &ran,
            &new,
            previous.as_deref(),
            &mut checked,
        )?;
        // Only what passed has a digest and an archive to keep.
        if checked["passed"] == true {
            let view: Vec<(&str, &[u8])> = read
                .iter()
                .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
                .collect();
            checked["digest"] = json!(Digest::of_files(&view).as_str());
            checked["archive"] = json!(base64(&packed));
        }
        Ok(Outcome::Checked(checked))
    }

    /// The previous release's files in `folder`, once they are what its
    /// digest says.
    fn previous(&self, digest: &str, folder: &Path) -> Result<(), Outcome> {
        let bytes = (self.archives)(digest).map_err(|why| Outcome::CouldNotCheck {
            code: "previous_not_found",
            message: format!("The previous release's archive couldn't be fetched: {why}"),
        })?;
        let files = archive::unpack(&bytes).map_err(|error| Outcome::CouldNotCheck {
            code: "previous_not_found",
            message: format!("The previous release's archive can't be read: {error}"),
        })?;
        let view: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
            .collect();
        let found = Digest::of_files(&view);
        if found.as_str() != digest {
            return Err(Outcome::CouldNotCheck {
                code: "previous_digest_mismatch",
                message: format!("The archive named {digest} holds files of digest {found}."),
            });
        }
        write_files(folder, &files).map_err(unwritable)
    }

    /// `jadpo package verify` in a sandbox of its own.
    fn report(
        &self,
        work: &Workspace,
        jadpo: &Path,
        name: &str,
        new: &Path,
        previous: Option<&Path>,
        source_only: bool,
    ) -> Result<Report, Outcome> {
        let tmp = work.folder(&format!("tmp-{name}")).map_err(unwritable)?;
        let mut args: Vec<OsString> = vec!["package".into(), "verify".into(), new.into()];
        let mut read_only = vec![new.to_path_buf()];
        if let Some(previous) = previous {
            args.push("--previous".into());
            args.push(previous.into());
            read_only.push(previous.to_path_buf());
        }
        args.push("--json".into());
        if source_only {
            args.push("--no-run".into());
        }
        let job = Job {
            program: jadpo.to_path_buf(),
            args,
            env: [
                ("PATH", "/usr/bin:/bin"),
                ("LANG", "C.UTF-8"),
                ("NO_COLOR", "1"),
                // A project's own choice of `jadpo` never reaches here.
                ("JADPO_TOOLCHAIN", "local"),
            ]
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect(),
            read_only,
            writable: Vec::new(),
            tmp,
            network: false,
            stdin: None,
            limits: self.limits,
        };
        let done = self
            .sandbox
            .run(&job)
            .map_err(|error| Outcome::CouldNotCheck {
                code: "verifier_failed",
                message: format!("`jadpo` couldn't start in the sandbox: {error}."),
            })?;
        Ok(Report::of(&done))
    }

    /// Checks with the Jadpo release the package declares, when that
    /// isn't the release already checked with: a row of the bar, the whole
    /// bar for that release passing or failing it. Its report says nothing
    /// else: what a release shows comes from the newest.
    fn with_declared(
        &self,
        work: &Workspace,
        source: &Report,
        ran: &Report,
        new: &Path,
        previous: Option<&Path>,
        checked: &mut Value,
    ) -> Result<(), Outcome> {
        let Report::Answered {
            data: Some(data), ..
        } = source
        else {
            return Ok(());
        };
        if !matches!(ran, Report::Answered { data: Some(_), .. }) {
            return Ok(());
        }
        let (Some(line), Some(own)) = (data["jadpo_declared"].as_str(), data["jadpo"].as_str())
        else {
            return Ok(());
        };
        if own == line || own.starts_with(&format!("{line}.")) {
            return Ok(());
        }
        let mut declared = row(
            "declared jadpo",
            "not_checked",
            &format!("no release of {line} to check with"),
        );
        if let Some(binary) = (self.releases)(line) {
            let report = self.report(work, &binary, "declared", new, previous, false)?;
            declared = match &report {
                Report::Answered {
                    data: Some(data), ..
                } if data["passed"].is_boolean() => {
                    let release = data["jadpo"].as_str().unwrap_or(line).to_string();
                    if let Some(list) = checked["checked_with"].as_array_mut() {
                        list.push(json!(release));
                    }
                    if data["passed"] == true {
                        row(
                            "declared jadpo",
                            "passed",
                            &format!("passes with {release}"),
                        )
                    } else {
                        checked["passed"] = json!(false);
                        row("declared jadpo", "failed", &format!("fails with {release}"))
                    }
                }
                // Too old to have `verify`, or stopped by its limits on a
                // release this one doesn't run with.
                _ => row(
                    "declared jadpo",
                    "not_checked",
                    &format!("{line}'s release can't check it"),
                ),
            };
        }
        if let Some(bar) = checked["bar"].as_array_mut() {
            bar.push(declared);
        }
        Ok(())
    }
}

/// Writes files at their paths under `folder`; their paths have been held
/// to [`archive::is_package_path`], so none leaves it.
fn write_files(folder: &Path, files: &[(String, Vec<u8>)]) -> std::io::Result<()> {
    for (path, bytes) in files {
        let at = folder.join(path);
        if let Some(parent) = at.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(at, bytes)?;
    }
    Ok(())
}

/// What a `jadpo package verify --json` run came to.
#[derive(Clone, Debug)]
enum Report {
    /// It wrote its document: `data` is `null` when it refused the package
    /// before it could run a bar.
    Answered {
        data: Option<Value>,
        diagnostics: Vec<Value>,
    },
    /// It was stopped, or wrote something else: why, for the bar's row.
    Stopped(&'static str),
}

impl Report {
    fn of(done: &Finished) -> Report {
        match done.ended {
            Ended::TimedOut => return Report::Stopped("ran past its time limit"),
            Ended::OutputTooLarge => return Report::Stopped("wrote too much"),
            Ended::Signalled => return Report::Stopped("was stopped at a limit, or crashed"),
            Ended::Exited(_) => {}
        }
        let Ok(document) = serde_json::from_slice::<Value>(&done.stdout) else {
            return Report::Stopped("didn't finish its report");
        };
        let well_formed = document.get("schemaVersion").is_some() && document["ok"].is_boolean();
        let Some(diagnostics) = document["diagnostics"].as_array().filter(|_| well_formed) else {
            return Report::Stopped("didn't finish its report");
        };
        let data = &document["data"];
        if data.is_object() {
            let shaped = data["passed"].is_boolean()
                && data["package"].is_string()
                && data["bar"].as_array().is_some_and(|bar| {
                    bar.iter().all(|row| {
                        row["item"].is_string()
                            && row["detail"].is_string()
                            && matches!(
                                row["state"].as_str(),
                                Some("passed" | "failed" | "not_checked")
                            )
                    })
                })
                && data["change_record"].is_array();
            if !shaped {
                return Report::Stopped("didn't finish its report");
            }
        } else if !data.is_null() {
            return Report::Stopped("didn't finish its report");
        }
        Report::Answered {
            data: data.is_object().then(|| data.clone()),
            diagnostics: diagnostics.clone(),
        }
    }
}

/// What the two runs come to together: what the source decided, and for the
/// rows it left open, what running said.
fn combine(source: &Report, ran: &Report, left_out: &[String]) -> Value {
    let (data, diagnostics) = match source {
        Report::Stopped(why) => {
            return answer(
                false,
                vec![row(
                    "limits",
                    "failed",
                    &format!("Reading its source {why}"),
                )],
                Vec::new(),
                left_out,
            );
        }
        Report::Answered {
            data: None,
            diagnostics,
        } => {
            return answer(
                false,
                vec![row(
                    "compiles",
                    "failed",
                    "it holds no package this can read",
                )],
                diagnostics.clone(),
                left_out,
            );
        }
        Report::Answered {
            data: Some(data),
            diagnostics,
        } => (data, diagnostics),
    };
    let empty = Vec::new();
    let source_bar = data["bar"].as_array().unwrap_or(&empty);
    let mut passed = data["passed"] == true;
    let mut bar: Vec<Value> = source_bar.clone();
    let mut problems = diagnostics.clone();
    let mut change_record = data["change_record"].clone();
    let mut previous = Value::Null;
    match ran {
        Report::Stopped(why) => {
            passed = false;
            bar.push(row("limits", "failed", &format!("Its checks {why}")));
        }
        Report::Answered {
            data: None,
            diagnostics,
        } => {
            passed = false;
            bar.push(row(
                "agreement",
                "failed",
                "its checks refused what its source passed",
            ));
            problems.extend(diagnostics.iter().cloned());
        }
        Report::Answered {
            data: Some(run),
            diagnostics,
        } => {
            let differ = disagreements(data, run);
            bar = merged_rows(source_bar, run["bar"].as_array().unwrap_or(&empty));
            passed = passed
                && differ.is_empty()
                && run["passed"] == true
                && !bar.iter().any(|row| row["state"] == "failed");
            if !differ.is_empty() {
                bar.push(row(
                    "agreement",
                    "failed",
                    &format!("its checks reported other {}", differ.join(", ")),
                ));
            }
            previous = run["previous"].clone();
            // A problem the source found is said once: the run's whole list
            // holds it again.
            problems = diagnostics.clone();
            if let (Some(record), Some(ran)) = (
                change_record.as_array_mut(),
                run["change_record"].as_array(),
            ) {
                record.extend(
                    ran.iter()
                        .filter(|entry| entry["change"] == "behaviour_changed")
                        .cloned(),
                );
            }
        }
    }
    json!({
        "passed": passed,
        "package": data["package"],
        "version": data["version"],
        "digest": null,
        "archive": null,
        "bar": bar,
        "problems": problems,
        "surface": data["surface"],
        "facts": data["facts"],
        "change_record": change_record,
        "fingerprints": data["fingerprints"],
        "previous": previous,
        "jadpo": data["jadpo"],
        "checked_with": [data["jadpo"]],
        "left_out": left_out,
    })
}

/// The run's rows, each of which the source decided taken from the source.
fn merged_rows(source: &[Value], run: &[Value]) -> Vec<Value> {
    let decided = |item: &Value| {
        source
            .iter()
            .find(|row| row["item"] == *item && row["state"] != "not_checked")
    };
    let mut rows: Vec<Value> = run
        .iter()
        .map(|row| decided(&row["item"]).unwrap_or(row).clone())
        .collect();
    for row in source {
        if !rows.iter().any(|kept| kept["item"] == row["item"]) {
            rows.push(row.clone());
        }
    }
    rows
}

/// What the run says of what only the source decides.
fn disagreements(source: &Value, run: &Value) -> Vec<&'static str> {
    let mut differ = Vec::new();
    for (field, name) in [
        ("package", "package"),
        ("version", "version"),
        ("surface", "surface"),
        ("facts", "facts"),
        ("fingerprints", "fingerprints"),
        ("jadpo", "Jadpo release"),
    ] {
        if source[field] != run[field] {
            differ.push(name);
        }
    }
    let of_source = |record: &Value| -> Vec<Value> {
        record
            .as_array()
            .map(|list| {
                list.iter()
                    .filter(|entry| entry["change"] != "behaviour_changed")
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    };
    if of_source(&source["change_record"]) != of_source(&run["change_record"]) {
        differ.push("changes");
    }
    differ
}

/// Standard base64, for the archive in an answer.
fn base64(bytes: &[u8]) -> String {
    const LETTERS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(LETTERS[((group >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

//! Installing packages (area 11, "Where packages come from", "Installing
//! safely" and "Developing and publishing a package"; area 18, "The lock"
//! and "Package upgrades"; chunk 7.1b2): `jadpo package add`, `update`,
//! `remove`, `link` and `unlink`.
//!
//! A package is added from a folder holding `package.jadpo` (a `path`
//! source) or from a git repository at a release tag
//! (`<repository>@v<version>`, a `git` source), and verified first: it is
//! compiled as the app's own integration would be, and the surface its
//! code gives has to be its committed `surface.jadpo_record`, byte for
//! byte, at the version that file names (the tag's, for git). Only then is
//! it copied into `packages/<name>/<version>/`, without hidden files, and
//! locked with its version, digest and source.
//!
//! ```text
//! ✓ Added Postcodes.v2 2.1.0
//!   folder    packages/postcodes/v2
//!   source    git github.com/acme/postcodes tag v2.1.0
//!   surface   matches its code
//!
//! ✓ package added  Postcodes.v2 2.1.0 · 1 function · 2 types · 1 failure
//!
//! → jadpo package show postcodes  shows what it offers
//! ```
//!
//! `link` points the lock at a live checkout, which every check then
//! compiles in the installed folder's place (`jadpo_db::load`); the line it
//! replaces is kept in `.jadpo/links` for `unlink` to restore. CI refuses
//! a link (`install.link_refused`), so one never ships.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use jadpo_check::publishing;
use jadpo_check::surface::Surface;
use jadpo_db::{Locked, Source};
use jadpo_diag::{Diagnostic, nearest, rule};
use jadpo_term::{Block, ChangeMark, Fact, Json, Outcome, Screen};

use crate::fmt::project_needed;
use crate::package::{
    Compiled, Git, Scratch, and_list, arguments, compile, in_package, index_name, plural, semver,
    write_failed,
};
use crate::run::{Call, Reply};

/// Where `link` keeps each lock line it replaced, for `unlink`.
const LINKS: &str = ".jadpo/links";

// --- the project and its lock ----------------------------------------------

fn root_of(call: &Call, command: &str) -> Result<PathBuf, Reply> {
    let cwd = cwd(call);
    jadpo_db::find_root(&cwd).ok_or_else(|| project_needed(command))
}

fn cwd(call: &Call) -> PathBuf {
    call.cwd.clone().unwrap_or_else(|| PathBuf::from("."))
}

/// The lock's lines; none when the project has no lock yet.
fn read_lock(root: &Path) -> Result<Vec<Locked>, Reply> {
    let Ok(text) = std::fs::read_to_string(root.join(jadpo_db::LOCK)) else {
        return Ok(Vec::new());
    };
    jadpo_db::parse_lock(&text).map_err(|(line, why)| {
        Reply::failed(
            Diagnostic::new(
                &rule::RECORD_EDITED,
                format!("`{}` can't be read", jadpo_db::LOCK),
            )
            .reason(format!("Line {}: {why}.", line + 1))
            .next("Run `jadpo records` to see what is wrong with it, then put it right"),
        )
    })
}

fn write_lock(root: &Path, lines: &[Locked], command: &str) -> Result<(), Reply> {
    std::fs::write(root.join(jadpo_db::LOCK), jadpo_db::write_lock(lines))
        .map_err(|error| write_failed(command, jadpo_db::LOCK, &error.to_string()))
}

/// The lock line `wanted` names: its folder's name (`postcodes`), its name
/// in code (`Postcodes`) or with its version (`Postcodes.v2`).
fn find<'a>(lines: &'a [Locked], wanted: &str, command: &str) -> Result<&'a Locked, Reply> {
    let matches = |line: &Locked| {
        let (code, _) = line.package.rsplit_once('.').unwrap_or((&line.package, ""));
        wanted == line.package || wanted == code || wanted == jadpo_db::to_snake(code)
    };
    if let Some(line) = lines.iter().find(|line| matches(line)) {
        return Ok(line);
    }
    let folders: Vec<String> = lines
        .iter()
        .map(|line| {
            let (code, _) = line.package.rsplit_once('.').unwrap_or((&line.package, ""));
            jadpo_db::to_snake(code)
        })
        .collect();
    let message = format!("`{wanted}` isn't an installed package");
    Err(Reply::failed(if folders.is_empty() {
        arguments(
            message,
            format!("`{}` names no package.", jadpo_db::LOCK),
            "Install one with `jadpo package add <folder or repository@tag>`".to_string(),
        )
    } else {
        let holds = format!(
            "`{}` names {}.",
            jadpo_db::LOCK,
            and_list(&crate::run::quoted(&folders))
        );
        match nearest(wanted, folders.iter().map(String::as_str)) {
            Some(near) => arguments(
                message,
                format!("{holds} Did you mean `{near}`?"),
                format!("Use `jadpo package {command} {near}`"),
            ),
            None => arguments(
                message,
                holds,
                format!("Run `jadpo package {command} <name>`"),
            ),
        }
    }))
}

/// The folder's name of `Name.vN`: `postcodes`.
fn folder_name(package: &str) -> String {
    let (code, _) = package.rsplit_once('.').unwrap_or((package, ""));
    jadpo_db::to_snake(code)
}

/// `to` as the lock writes it from `root`: relative, with `/`, or
/// absolute when nothing is shared.
fn relative(root: &Path, to: &Path) -> String {
    let (root, to) = (real(root), real(to));
    let from: Vec<_> = root.components().collect();
    let target: Vec<_> = to.components().collect();
    let shared = from.iter().zip(&target).take_while(|(a, b)| a == b).count();
    if shared == 0 {
        return to.to_string_lossy().replace('\\', "/");
    }
    let mut parts: Vec<String> = vec!["..".to_string(); from.len() - shared];
    parts.extend(
        target[shared..]
            .iter()
            .map(|part| part.as_os_str().to_string_lossy().into_owned()),
    );
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

/// `path` with `.` and `..` worked out and its longest existing start
/// resolved as the file system does (`/var` is `/private/var` on macOS),
/// so a folder that isn't there yet is written like one that is.
fn real(path: &Path) -> PathBuf {
    let mut plain = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                plain.pop();
            }
            std::path::Component::CurDir => {}
            other => plain.push(other.as_os_str()),
        }
    }
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let mut base = plain.clone();
    loop {
        if let Ok(found) = std::fs::canonicalize(&base) {
            let mut out = found;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (
            base.file_name().map(|name| name.to_os_string()),
            base.parent(),
        ) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                base = parent.to_path_buf();
            }
            _ => return plain,
        }
    }
}

// --- verifying a package ---------------------------------------------------

/// A package verified for installing: compiled, its surface its code's.
struct Verified {
    package: String,
    version: String,
    /// Its files, relative to its folder, with their bytes.
    files: Vec<(String, Vec<u8>)>,
    digest: String,
    surface: Surface,
    /// Its committed `surface.jadpo_record`.
    surface_text: String,
}

/// The version a committed surface names in its package entry.
fn surface_version(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("    version "))
        .map(str::to_string)
}

/// Verifies the package in `folder` (area 11, "Installing safely"): it
/// compiles, and the surface its code gives at its committed surface's
/// version is that surface, byte for byte. `tag` is a git release's,
/// which the surface's version has to be. `shown` is how the reader
/// knows the folder, for its problems' paths.
fn verify(folder: &Path, tag: Option<&str>, shown: &str, command: &str) -> Result<Verified, Reply> {
    let compiled = match compile(folder, command) {
        Ok(compiled) => compiled,
        Err(Some(reply)) => return Err(reply),
        Err(None) => {
            return Err(Reply::failed(arguments(
                format!("`{shown}` isn't a package"),
                format!(
                    "A package's folder holds `{}` at its root, naming the package and its version.",
                    jadpo_db::PACKAGE_INDEX
                ),
                "Give the folder of a package, or a git repository whose tag holds one".to_string(),
            )));
        }
    };
    if !compiled.problems.is_empty() {
        return Err(does_not_compile(&compiled, shown, command));
    }
    let package = compiled.package.clone();
    let mismatch = |reason: String| {
        Reply::failed(
            Diagnostic::new(
                &rule::INSTALL_SURFACE_MISMATCH,
                format!(
                    "`{package}`'s {} doesn't match its code",
                    jadpo_db::record::SURFACE_FILE
                ),
            )
            .reason(format!(
                "{reason} A package is installed only when its surface, what reviewers read, is what its code offers."
            ))
            .next(format!(
                "Run `jadpo package check` in the package's repository, commit its {}, and release it again",
                jadpo_db::record::SURFACE_FILE
            )),
        )
    };
    let Ok(committed) = std::fs::read_to_string(folder.join(jadpo_db::record::SURFACE_FILE)) else {
        return Err(mismatch(format!(
            "It has no {}: a package's repository commits the surface `jadpo package check` writes.",
            jadpo_db::record::SURFACE_FILE
        )));
    };
    let committed = jadpo_db::record::normalise(&committed);
    let Some(version) = surface_version(&committed) else {
        return Err(mismatch(
            "Its committed surface names no version.".to_string(),
        ));
    };
    if let Some(tag) = tag
        && tag.strip_prefix('v') != Some(version.as_str())
    {
        return Err(mismatch(format!(
            "Its tag is `{tag}`, and its committed surface says {version}."
        )));
    }
    let Some(surface) = compiled.surface() else {
        return Err(mismatch("Its code has no package.".to_string()));
    };
    let now = surface.text(Some(&version));
    if now != committed {
        let changes = publishing::changes(&committed, &now);
        let said: Vec<String> = changes
            .iter()
            .map(|(word, heading)| match word.as_str() {
                "added" => format!("its code has `{heading}`, which its committed surface doesn't"),
                "removed" => {
                    format!("its committed surface has `{heading}`, which its code doesn't")
                }
                _ => format!("`{heading}` differs"),
            })
            .collect();
        let reason = if said.is_empty() {
            "Its code's surface differs from its committed one.".to_string()
        } else {
            let mut sentence = and_list(&said);
            sentence[..1].make_ascii_uppercase();
            format!("{sentence}.")
        };
        return Err(mismatch(reason));
    }
    let mut files = Vec::new();
    for path in jadpo_db::package_files(folder) {
        let relative = path
            .strip_prefix(folder)
            .unwrap_or(&path)
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let bytes = std::fs::read(&path)
            .map_err(|error| write_failed(command, &format!("`{relative}`"), &error.to_string()))?;
        files.push((relative, bytes));
    }
    let borrowed: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    let digest = jadpo_db::package_digest(&borrowed);
    Ok(Verified {
        package,
        version,
        files,
        digest,
        surface,
        surface_text: committed.into_owned(),
    })
}

/// A package that doesn't compile: its problems in its folder, as the
/// reader knows it.
fn does_not_compile(compiled: &Compiled, shown: &str, command: &str) -> Reply {
    let diagnostics: Vec<Diagnostic> = in_package(compiled, compiled.problems.clone())
        .into_iter()
        .map(|mut d| {
            for label in d.primary.iter_mut().chain(d.secondary.iter_mut()) {
                label.span.path = format!("{shown}/{}", label.span.path);
            }
            d
        })
        .collect();
    let sources: BTreeMap<String, String> = compiled
        .sources()
        .into_iter()
        .map(|(path, text)| (format!("{shown}/{path}"), text))
        .collect();
    let mut screen = Screen::new();
    for diagnostic in &diagnostics {
        screen.push(Block::diagnostic_from(diagnostic, &sources, None));
    }
    let verb = if command == "package update" {
        "package not updated"
    } else {
        "package not added"
    };
    screen.push(Block::summary(
        Outcome::Failure,
        verb,
        [
            Fact::problem(plural(diagnostics.len(), "error", "errors")),
            Fact::plain(format!("{} doesn't compile", compiled.package)),
        ],
    ));
    Reply::Report {
        screen,
        data: Json::object([("package", Json::from(compiled.package.as_str()))]),
        diagnostics,
        sources,
    }
}

/// Writes the verified package into `packages/<name>/<version>/`,
/// replacing what is there, through a hidden folder beside it so a failed
/// write leaves the old one.
fn install(root: &Path, verified: &Verified, command: &str) -> Result<(), Reply> {
    let folder = root.join(jadpo_db::package_folder(&verified.package));
    let parent = folder.parent().unwrap_or(root).to_path_buf();
    let label = folder
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let staging = parent.join(format!(".{label}.installing"));
    let failed = |error: std::io::Error| {
        write_failed(
            command,
            &format!("`{}`", jadpo_db::package_folder(&verified.package)),
            &error.to_string(),
        )
    };
    let _ = std::fs::remove_dir_all(&staging);
    for (path, bytes) in &verified.files {
        let to = staging.join(path);
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(failed)?;
        }
        std::fs::write(&to, bytes).map_err(failed)?;
    }
    if folder.exists() {
        std::fs::remove_dir_all(&folder).map_err(failed)?;
    }
    std::fs::rename(&staging, &folder).map_err(failed)?;
    Ok(())
}

/// `lines` with `line` in place of the package's own, or added.
fn with_line(lines: &[Locked], line: Locked) -> Vec<Locked> {
    let mut out: Vec<Locked> = lines
        .iter()
        .filter(|other| other.package != line.package)
        .cloned()
        .collect();
    out.push(line);
    out
}

/// The surface's counts for a footer: `1 function`, `2 types`.
fn counts(surface: &Surface) -> Vec<Fact> {
    let mut out = Vec::new();
    for (word, noun) in [
        ("operation", "operation"),
        ("function", "function"),
        ("type", "type"),
        ("failure", "failure"),
        ("recipe", "recipe"),
    ] {
        let n = surface
            .declarations
            .iter()
            .filter(|d| d.what.word() == word)
            .count();
        if n > 0 {
            out.push(Fact::plain(plural(n, noun, &format!("{noun}s"))));
        }
    }
    out
}

/// A lock line's source in words: `path ../postcodes`, `git <repository>
/// tag v2.1.0`.
fn source_words(source: &Source) -> String {
    match source {
        Source::Path { path, .. } => format!("path {path}"),
        Source::Git {
            repository, tag, ..
        } => format!("git {repository} tag {tag}"),
        Source::Registry { .. } => "the registry".to_string(),
        Source::Link { path } => format!("a link to {path}"),
    }
}

fn version_of(source: &Source) -> Option<&str> {
    match source {
        Source::Path { version, .. }
        | Source::Git { version, .. }
        | Source::Registry { version, .. } => Some(version),
        Source::Link { .. } => None,
    }
}

// --- git -------------------------------------------------------------------

/// A git repository as `add` was given it: where to clone it from, and
/// how the lock writes it.
struct Repository {
    url: String,
    locked: String,
}

/// `github.com/acme/postcodes` is fetched over HTTPS; a URL git reads
/// (`https://`, `ssh://`, `file://`, `git@host:`) as written; a folder
/// on this machine from its path, which the lock writes from the root.
fn repository(written: &str, cwd: &Path, root: &Path) -> Repository {
    let local = cwd.join(written);
    let path_like = written.starts_with('.') || Path::new(written).is_absolute();
    if !written.contains("://") && !written.starts_with("git@") && (path_like || local.is_dir()) {
        let real = real(&local);
        return Repository {
            url: real.to_string_lossy().into_owned(),
            locked: if Path::new(written).is_absolute() {
                written.to_string()
            } else {
                relative(root, &real)
            },
        };
    }
    let url = if written.contains("://") || written.starts_with("git@") {
        written.to_string()
    } else {
        format!("https://{written}")
    };
    Repository {
        url,
        locked: written.to_string(),
    }
}

/// The lock's repository as git reads it, from the root.
fn repository_url(locked: &str, root: &Path) -> String {
    repository(locked, root, root).url
}

fn fetch_failed(repository: &str, reason: String) -> Reply {
    Reply::failed(
        Diagnostic::new(
            &rule::INSTALL_FETCH_FAILED,
            format!("`{repository}` couldn't be read"),
        )
        .reason(reason)
        .next(
            "Check the repository's address and your git access to it, then run the command again",
        ),
    )
}

/// A repository that was read but holds no release to install.
fn no_release(message: String, reason: String) -> Reply {
    Reply::failed(
        Diagnostic::new(&rule::INSTALL_FETCH_FAILED, message)
            .reason(reason)
            .next("Give one of its release tags, or tag the release in the package's repository after `jadpo package check` passes"),
    )
}

/// What `git <args>` prints, or what it said when it failed.
fn run_git(folder: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("git");
    command
        .args(["-c", "advice.detachedHead=false"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0");
    if let Some(folder) = folder {
        command.current_dir(folder);
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let said = String::from_utf8_lossy(&output.stderr);
        Err(said
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("git failed")
            .trim()
            .trim_start_matches("fatal: ")
            .to_string())
    }
}

/// A clone of the repository with nothing checked out, in a scratch
/// folder removed when dropped.
fn clone(repository: &Repository) -> Result<Scratch, Reply> {
    let scratch = Scratch::new()
        .map_err(|error| write_failed("package add", "a scratch folder", &error.to_string()))?;
    let into = scratch.0.join("repository");
    let into_text = into.to_string_lossy().into_owned();
    run_git(
        None,
        &[
            "clone",
            "--quiet",
            "--no-checkout",
            &repository.url,
            &into_text,
        ],
    )
    .map_err(|said| fetch_failed(&repository.locked, format!("Git said: {said}.")))?;
    Ok(scratch)
}

/// The commit a tag names in a clone.
fn tag_commit(clone: &Path, tag: &str) -> Option<String> {
    run_git(
        Some(clone),
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/tags/{tag}^{{commit}}"),
        ],
    )
    .ok()
    .map(|commit| commit.trim().to_string())
}

/// Checks out `commit` in the clone.
fn checkout(clone: &Path, commit: &str, repository: &str) -> Result<(), Reply> {
    run_git(Some(clone), &["checkout", "--quiet", commit])
        .map(|_| ())
        .map_err(|said| fetch_failed(repository, format!("Git said: {said}.")))
}

// --- add -------------------------------------------------------------------

/// Where `add` was told to install from.
enum Wanted {
    Path(PathBuf),
    Git { repository: String, tag: String },
}

pub fn add(call: &Call) -> Reply {
    let root = match root_of(call, "package add") {
        Ok(root) => root,
        Err(reply) => return reply,
    };
    let Some(written) = call.arguments.get(1) else {
        return Reply::failed(arguments(
            "`jadpo package add` needs a package".to_string(),
            "A package is added from its folder, or from a git repository at a release tag."
                .to_string(),
            "Run `jadpo package add <folder>` or `jadpo package add <repository>@v<version>`"
                .to_string(),
        ));
    };
    if call.arguments.len() > 2 {
        return Reply::failed(arguments(
            format!(
                "`jadpo package add` takes one package, not `{}`",
                call.arguments[2]
            ),
            "It adds one package at a time.".to_string(),
            format!("Run `jadpo package add {written}`"),
        ));
    }
    let cwd = cwd(call);
    let wanted = match written.rsplit_once('@') {
        Some((repository, tag)) if !repository.is_empty() && semver(tag).is_some() => Wanted::Git {
            repository: repository.to_string(),
            tag: tag.to_string(),
        },
        _ => {
            let folder = cwd.join(written);
            if folder.join(jadpo_db::PACKAGE_INDEX).is_file() {
                Wanted::Path(folder)
            } else if folder.join(jadpo_db::PROJECT_FILE).is_file() {
                return Reply::failed(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_NOT_BUILT_YET,
                        "Adding another Jadpo app's API isn't built yet",
                    )
                    .reason(format!(
                        "`{written}` is a Jadpo app, whose package comes from its OpenAPI document with the importer (area 11, \"Packages from other Jadpo apps\")."
                    ))
                    .next("Add a package's own folder, or a git repository at a release tag, meanwhile")
                    .help_at("11-integrations.html#packages-from-other-jadpo-apps"),
                );
            } else if !written.contains(['/', '\\', '.']) {
                return Reply::failed(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_NOT_BUILT_YET,
                        "Installing from the registry isn't built yet",
                    )
                    .reason(format!(
                        "`{written}` names a package in the registry, which comes after the first build (area 11, \"Where packages come from\")."
                    ))
                    .next("Add the package from its folder, or from its git repository at a release tag: `<repository>@v<version>`")
                    .help_at("11-integrations.html#where-packages-come-from"),
                );
            } else {
                return Reply::failed(arguments(
                    format!("`{written}` isn't a package's folder or a git repository"),
                    format!(
                        "A package is added from a folder holding `{}`, or from a git repository at a release tag, such as `github.com/acme/holidays@v2.1.0`.",
                        jadpo_db::PACKAGE_INDEX
                    ),
                    "Give the package's folder, or its repository and tag: `<repository>@v<version>`".to_string(),
                ));
            }
        }
    };
    let lines = match read_lock(&root) {
        Ok(lines) => lines,
        Err(reply) => return reply,
    };
    // Fetch and verify.
    let (verified, source, _clone) = match &wanted {
        Wanted::Path(folder) => {
            let locked = relative(&root, folder);
            if locked.contains(char::is_whitespace) {
                return Reply::failed(arguments(
                    format!("`{written}` can't be locked: its path has a space"),
                    format!("`{}` writes each source as one word.", jadpo_db::LOCK),
                    "Move the package to a folder whose path has no spaces".to_string(),
                ));
            }
            let verified = match verify(folder, None, written, "package add") {
                Ok(verified) => verified,
                Err(reply) => return reply,
            };
            let source = Source::Path {
                version: verified.version.clone(),
                path: locked,
                digest: verified.digest.clone(),
            };
            (verified, source, None)
        }
        Wanted::Git {
            repository: given,
            tag,
        } => {
            let repo = repository(given, &cwd, &root);
            if repo.locked.contains(char::is_whitespace) {
                return Reply::failed(arguments(
                    format!("`{given}` can't be locked: it has a space"),
                    format!("`{}` writes each source as one word.", jadpo_db::LOCK),
                    "Give the repository's address without spaces".to_string(),
                ));
            }
            let scratch = match clone(&repo) {
                Ok(scratch) => scratch,
                Err(reply) => return reply,
            };
            let at = scratch.0.join("repository");
            let Some(commit) = tag_commit(&at, tag) else {
                let tags: Vec<String> = run_git(Some(&at), &["tag", "--list", "v*"])
                    .unwrap_or_default()
                    .lines()
                    .map(|tag| format!("`{}`", tag.trim()))
                    .collect();
                let has = if tags.is_empty() {
                    "It has no release tags yet.".to_string()
                } else {
                    format!("Its release tags are {}.", and_list(&tags))
                };
                return no_release(format!("`{}` has no tag `{tag}`", repo.locked), has);
            };
            if let Err(reply) = checkout(&at, &commit, &repo.locked) {
                return reply;
            }
            let verified = match verify(&at, Some(tag), given, "package add") {
                Ok(verified) => verified,
                Err(reply) => return reply,
            };
            let source = Source::Git {
                version: verified.version.clone(),
                repository: repo.locked.clone(),
                tag: tag.clone(),
                commit,
                digest: verified.digest.clone(),
            };
            (verified, source, Some(scratch))
        }
    };
    if lines.iter().any(|line| line.package == verified.package) {
        let name = folder_name(&verified.package);
        return Reply::failed(arguments(
            format!("`{}` is already installed", verified.package),
            format!(
                "`{}` has it, from {}; one version of a package is installed at a time.",
                jadpo_db::LOCK,
                lines
                    .iter()
                    .find(|line| line.package == verified.package)
                    .map_or_else(String::new, |line| source_words(&line.source))
            ),
            format!("Run `jadpo package update {name}` to move it to its newest release"),
        ));
    }
    let lock_before = std::fs::read_to_string(root.join(jadpo_db::LOCK)).ok();
    let folder = jadpo_db::package_folder(&verified.package);
    if let Err(reply) = install(&root, &verified, "package add") {
        return reply;
    }
    let locked = Locked {
        package: verified.package.clone(),
        source: source.clone(),
    };
    if let Err(reply) = write_lock(&root, &with_line(&lines, locked), "package add") {
        let _ = std::fs::remove_dir_all(root.join(&folder));
        return reply;
    }
    // A package's name can't be one the app already uses: undo the
    // install and say so (area 11, "Names").
    let loaded = jadpo_db::load(&root);
    let taken: Vec<Diagnostic> = jadpo_check::check(&loaded.db, loaded.project)
        .into_iter()
        .filter(|d| {
            d.rule.id == rule::PACKAGE_NAME_TAKEN.id
                && d.primary
                    .as_ref()
                    .is_some_and(|label| label.span.path.starts_with(&format!("{folder}/")))
        })
        .collect();
    if !taken.is_empty() {
        let _ = std::fs::remove_dir_all(root.join(&folder));
        let parent = root.join(&folder).parent().map(Path::to_path_buf);
        if let Some(parent) = parent {
            let _ = std::fs::remove_dir(&parent);
        }
        let _ = std::fs::remove_dir(root.join(jadpo_db::PACKAGES));
        let _ = match &lock_before {
            Some(text) => std::fs::write(root.join(jadpo_db::LOCK), text),
            None => std::fs::remove_file(root.join(jadpo_db::LOCK)),
        };
        let sources = jadpo_db::source_texts(&loaded.db, loaded.project);
        let mut screen = Screen::new();
        for d in &taken {
            screen.push(Block::diagnostic_from(d, &sources, None));
        }
        screen.push(Block::summary(
            Outcome::Failure,
            "package not added",
            [Fact::plain(format!(
                "{} takes a name the app uses",
                verified.package
            ))],
        ));
        return Reply::Report {
            screen,
            data: Json::object([("package", Json::from(verified.package.as_str()))]),
            diagnostics: taken,
            sources,
        };
    }
    let name = folder_name(&verified.package);
    let mut screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: format!("Added {} {}", verified.package, verified.version),
        rows: vec![
            ("folder".to_string(), folder.clone()),
            ("source".to_string(), source_words(&source)),
            ("surface".to_string(), "matches its code".to_string()),
        ],
    });
    let mut facts = vec![Fact::plain(format!(
        "{} {}",
        verified.package, verified.version
    ))];
    facts.extend(counts(&verified.surface));
    screen.push(Block::summary(Outcome::Success, "package added", facts));
    screen.push(Block::hint(
        format!("jadpo package show {name}"),
        "shows what it offers",
    ));
    Reply::screen(
        screen,
        Json::object([
            ("package", Json::from(verified.package.as_str())),
            ("version", Json::from(verified.version.as_str())),
            ("folder", Json::from(folder.as_str())),
            ("source", Json::from(source_words(&source).as_str())),
            ("digest", Json::from(verified.digest.as_str())),
        ]),
    )
}

// --- update ----------------------------------------------------------------

pub fn update(call: &Call) -> Reply {
    let root = match root_of(call, "package update") {
        Ok(root) => root,
        Err(reply) => return reply,
    };
    let Some(wanted) = call.arguments.get(1) else {
        return Reply::failed(arguments(
            "`jadpo package update` needs a package".to_string(),
            "It moves one installed package to its newest release.".to_string(),
            "Run `jadpo package update <name>`".to_string(),
        ));
    };
    let lines = match read_lock(&root) {
        Ok(lines) => lines,
        Err(reply) => return reply,
    };
    let line = match find(&lines, wanted, "update") {
        Ok(line) => line.clone(),
        Err(reply) => return reply,
    };
    let name = folder_name(&line.package);
    let folder = jadpo_db::package_folder(&line.package);
    let before = std::fs::read_to_string(root.join(&folder).join(jadpo_db::record::SURFACE_FILE))
        .unwrap_or_default();
    let (verified, source, _clone) = match &line.source {
        Source::Link { path } => {
            return Reply::failed(arguments(
                format!("`{}` is linked to `{path}`", line.package),
                "A linked package is compiled from its checkout, so there is nothing to update."
                    .to_string(),
                format!("Run `jadpo package unlink {name}` first"),
            ));
        }
        Source::Registry { .. } => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    "Updating from the registry isn't built yet",
                )
                .reason(format!(
                    "`{}` came from the registry, which comes after the first build.",
                    line.package
                ))
                .next("Leave it as it is meanwhile")
                .help_at("11-integrations.html#where-packages-come-from"),
            );
        }
        Source::Path { path, digest, .. } => {
            let from = root.join(path);
            let verified = match verify(&from, None, path, "package update") {
                Ok(verified) => verified,
                Err(reply) => return reply,
            };
            if &verified.digest == digest {
                return up_to_date(&line);
            }
            let source = Source::Path {
                version: verified.version.clone(),
                path: path.clone(),
                digest: verified.digest.clone(),
            };
            (verified, source, None)
        }
        Source::Git {
            repository,
            tag,
            commit,
            ..
        } => {
            let repo = Repository {
                url: repository_url(repository, &root),
                locked: repository.clone(),
            };
            let scratch = match clone(&repo) {
                Ok(scratch) => scratch,
                Err(reply) => return reply,
            };
            let at = scratch.0.join("repository");
            let newest = Git::of(&at)
                .map(|git| git.releases(&line.package))
                .unwrap_or_default()
                .into_iter()
                .next();
            let Some(newest) = newest else {
                return no_release(
                    format!("`{repository}` has no release of `{}`", line.package),
                    format!(
                        "No `v<version>` tag there has a {} naming `{}`.",
                        jadpo_db::record::SURFACE_FILE,
                        line.package
                    ),
                );
            };
            let older = semver(&newest.tag) <= semver(tag);
            let Some(new_commit) = tag_commit(&at, &newest.tag) else {
                return fetch_failed(
                    repository,
                    format!("Its tag `{}` can't be read.", newest.tag),
                );
            };
            if older && (&newest.tag != tag || &new_commit == commit) {
                return up_to_date(&line);
            }
            if let Err(reply) = checkout(&at, &new_commit, repository) {
                return reply;
            }
            let verified = match verify(&at, Some(&newest.tag), repository, "package update") {
                Ok(verified) => verified,
                Err(reply) => return reply,
            };
            let source = Source::Git {
                version: verified.version.clone(),
                repository: repository.clone(),
                tag: newest.tag.clone(),
                commit: new_commit,
                digest: verified.digest.clone(),
            };
            (verified, source, Some(scratch))
        }
    };
    if verified.package != line.package {
        return Reply::failed(arguments(
            format!(
                "`{}` now holds `{}`",
                source_words(&source),
                verified.package
            ),
            format!(
                "A new major version is a new package beside `{}`, not an update of it.",
                line.package
            ),
            format!(
                "Add it with `jadpo package add`, and move the app's code across before removing `{name}`"
            ),
        ));
    }
    if let Err(reply) = install(&root, &verified, "package update") {
        return reply;
    }
    let locked = Locked {
        package: line.package.clone(),
        source: source.clone(),
    };
    if let Err(reply) = write_lock(&root, &with_line(&lines, locked), "package update") {
        return reply;
    }
    // Area 18's review of the change: what the app uses and a person
    // decides.
    let used = used_names(&root, &line.package);
    let changes = publishing::changes(&before, &verified.surface_text);
    let mut rows: Vec<(ChangeMark, Vec<String>)> = Vec::new();
    let mut decide = 0;
    for (word, heading) in &changes {
        let mark = match word.as_str() {
            "added" => ChangeMark::Added,
            "removed" => ChangeMark::Removed,
            _ => ChangeMark::Changed,
        };
        let declaration = heading.split_once(' ').map_or(heading.as_str(), |(_, n)| n);
        let in_use = used.iter().any(|name| name == declaration);
        let mut columns = vec![heading.clone()];
        if in_use {
            columns.push("used by the app".to_string());
            if mark != ChangeMark::Added {
                decide += 1;
                columns.push("needs a person".to_string());
            }
        }
        rows.push((mark, columns));
    }
    let old_version = version_of(&line.source).unwrap_or("").to_string();
    let mut screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: format!(
            "Updated {} from {old_version} to {}",
            line.package, verified.version
        ),
        rows: vec![
            ("folder".to_string(), folder.clone()),
            ("source".to_string(), source_words(&source)),
            ("surface".to_string(), "matches its code".to_string()),
        ],
    });
    if !rows.is_empty() {
        screen.push(Block::Changes {
            note: None,
            groups: vec![(format!("Changes since {old_version}"), rows)],
        });
    }
    let mut facts = vec![Fact::plain(format!(
        "{} {}",
        line.package, verified.version
    ))];
    for (word, label) in [
        ("added", "added"),
        ("changed", "changed"),
        ("removed", "removed"),
    ] {
        let n = changes.iter().filter(|(w, _)| w == word).count();
        if n > 0 {
            facts.push(Fact::plain(format!("{n} {label}")));
        }
    }
    if decide > 0 {
        facts.push(Fact::problem(format!(
            "{decide} {}",
            if decide == 1 {
                "needs a person"
            } else {
                "need a person"
            }
        )));
    }
    screen.push(Block::summary(Outcome::Success, "package updated", facts));
    screen.push(Block::hint(
        "jadpo check",
        format!("checks the app against {}", verified.version),
    ));
    Reply::screen(
        screen,
        Json::object([
            ("package", Json::from(line.package.as_str())),
            ("from", Json::from(old_version.as_str())),
            ("version", Json::from(verified.version.as_str())),
            ("source", Json::from(source_words(&source).as_str())),
            (
                "changes",
                Json::array(changes.iter().map(|(word, heading)| {
                    let declaration = heading.split_once(' ').map_or(heading.as_str(), |(_, n)| n);
                    Json::object([
                        ("change", Json::from(word.as_str())),
                        ("entry", Json::from(heading.as_str())),
                        ("used", used.iter().any(|name| name == declaration).into()),
                    ])
                })),
            ),
        ]),
    )
}

fn up_to_date(line: &Locked) -> Reply {
    let version = version_of(&line.source).unwrap_or("").to_string();
    let screen = Screen::new().with(Block::summary(
        Outcome::Success,
        format!("{} is up to date", line.package),
        [
            Fact::plain(version.clone()),
            Fact::plain(source_words(&line.source)),
        ],
    ));
    Reply::screen(
        screen,
        Json::object([
            ("package", Json::from(line.package.as_str())),
            ("version", Json::from(version.as_str())),
            ("updated", false.into()),
        ]),
    )
}

/// The app's own source texts: everything outside `packages/`.
fn app_sources(root: &Path) -> Vec<(String, String)> {
    jadpo_db::files_under(root, true)
        .into_iter()
        .filter(|path| {
            path.ends_with(".jadpo") && !path.starts_with(&format!("{}/", jadpo_db::PACKAGES))
        })
        .filter_map(|path| {
            let text = std::fs::read_to_string(root.join(&path)).ok()?;
            Some((path, text))
        })
        .collect()
}

/// The package's names the app writes, after its name: `uk.parse`,
/// `UkPostcode`.
fn used_names(root: &Path, package: &str) -> Vec<String> {
    let (code, _) = package.rsplit_once('.').unwrap_or((package, ""));
    let lead = format!("{code}.");
    let mut out: Vec<String> = Vec::new();
    for (_, text) in app_sources(root) {
        for (at, _) in text.match_indices(&lead) {
            if text[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
            {
                continue;
            }
            let rest = &text[at + lead.len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
                .collect();
            let name = name.trim_end_matches('.').to_string();
            if !name.is_empty() && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

// --- remove ----------------------------------------------------------------

pub fn remove(call: &Call) -> Reply {
    let root = match root_of(call, "package remove") {
        Ok(root) => root,
        Err(reply) => return reply,
    };
    let Some(wanted) = call.arguments.get(1) else {
        return Reply::failed(arguments(
            "`jadpo package remove` needs a package".to_string(),
            "It removes one installed package.".to_string(),
            "Run `jadpo package remove <name>`".to_string(),
        ));
    };
    let lines = match read_lock(&root) {
        Ok(lines) => lines,
        Err(reply) => return reply,
    };
    let line = match find(&lines, wanted, "remove") {
        Ok(line) => line.clone(),
        Err(reply) => return reply,
    };
    let name = folder_name(&line.package);
    if let Source::Link { path } = &line.source {
        return Reply::failed(arguments(
            format!("`{}` is linked to `{path}`", line.package),
            "A linked package goes back to its installed version first.".to_string(),
            format!("Run `jadpo package unlink {name}`, then remove it"),
        ));
    }
    let use_line = format!("use {}/{name}", jadpo_db::PACKAGES);
    let users: Vec<String> = app_sources(&root)
        .into_iter()
        .filter(|(_, text)| text.lines().any(|l| l.trim() == use_line))
        .map(|(path, _)| path)
        .collect();
    if !users.is_empty() {
        let quoted: Vec<String> = users.iter().map(|path| format!("`{path}`")).collect();
        return Reply::failed(
            Diagnostic::new(
                &rule::INSTALL_IN_USE,
                format!("The app still uses `{}`", line.package),
            )
            .reason(format!(
                "{} {} it in with `{use_line}`, so removing it would leave {} unable to compile.",
                and_list(&quoted),
                if users.len() == 1 { "brings" } else { "bring" },
                if users.len() == 1 { "that file" } else { "them" }
            ))
            .next(format!(
                "Take `{use_line}` and what it brings in out of the app's code, then run `jadpo package remove {name}` again"
            )),
        );
    }
    let folder = jadpo_db::package_folder(&line.package);
    if root.join(&folder).exists()
        && let Err(error) = std::fs::remove_dir_all(root.join(&folder))
    {
        return write_failed("package remove", &format!("`{folder}`"), &error.to_string());
    }
    if let Some(parent) = root.join(&folder).parent() {
        let _ = std::fs::remove_dir(parent);
    }
    let _ = std::fs::remove_dir(root.join(jadpo_db::PACKAGES));
    let kept: Vec<Locked> = lines
        .iter()
        .filter(|other| other.package != line.package)
        .cloned()
        .collect();
    if let Err(reply) = write_lock(&root, &kept, "package remove") {
        return reply;
    }
    let version = version_of(&line.source).unwrap_or("").to_string();
    let screen = Screen::new()
        .with(Block::Status {
            outcome: Outcome::Success,
            text: format!("Removed {folder} and its line in {}", jadpo_db::LOCK),
            rows: Vec::new(),
        })
        .with(Block::summary(
            Outcome::Success,
            "package removed",
            [Fact::plain(format!("{} {version}", line.package))],
        ));
    Reply::screen(
        screen,
        Json::object([
            ("package", Json::from(line.package.as_str())),
            ("version", Json::from(version.as_str())),
            ("folder", Json::from(folder.as_str())),
        ]),
    )
}

// --- link and unlink -------------------------------------------------------

/// The lock lines `link` replaced, one per package, kept in the lock's
/// own format.
fn saved_links(root: &Path) -> Vec<Locked> {
    std::fs::read_to_string(root.join(LINKS))
        .ok()
        .and_then(|text| jadpo_db::parse_lock(&text).ok())
        .unwrap_or_default()
}

fn write_saved_links(root: &Path, lines: &[Locked], command: &str) -> Result<(), Reply> {
    let written = if lines.is_empty() {
        match std::fs::remove_file(root.join(LINKS)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    } else {
        crate::suggestions::local_folder(root)
            .and_then(|()| std::fs::write(root.join(LINKS), jadpo_db::write_lock(lines)))
    };
    written.map_err(|error| write_failed(command, LINKS, &error.to_string()))
}

pub fn link(call: &Call) -> Reply {
    let root = match root_of(call, "package link") {
        Ok(root) => root,
        Err(reply) => return reply,
    };
    let Some(written) = call.arguments.get(1) else {
        return Reply::failed(arguments(
            "`jadpo package link` needs a checkout".to_string(),
            "It compiles the app against a package's live checkout, a folder holding its `package.jadpo`.".to_string(),
            "Run `jadpo package link <folder>`".to_string(),
        ));
    };
    let checkout = cwd(call).join(written);
    let index = std::fs::read_to_string(checkout.join(jadpo_db::PACKAGE_INDEX)).ok();
    let Some(package) = index.as_deref().and_then(index_name) else {
        return Reply::failed(arguments(
            format!("`{written}` isn't a package's checkout"),
            format!(
                "A link is to a folder holding `{}`, the package's own repository.",
                jadpo_db::PACKAGE_INDEX
            ),
            "Give the folder of the package's checkout".to_string(),
        ));
    };
    let path = relative(&root, &checkout);
    if path.contains(char::is_whitespace) {
        return Reply::failed(arguments(
            format!("`{written}` can't be linked: its path has a space"),
            format!("`{}` writes each source as one word.", jadpo_db::LOCK),
            "Move the checkout to a folder whose path has no spaces".to_string(),
        ));
    }
    let lines = match read_lock(&root) {
        Ok(lines) => lines,
        Err(reply) => return reply,
    };
    let mut saved = saved_links(&root);
    let replaced = lines.iter().find(|line| line.package == package).cloned();
    if let Some(line) = &replaced
        && !matches!(line.source, Source::Link { .. })
    {
        saved.retain(|other| other.package != package);
        saved.push(line.clone());
        if let Err(reply) = write_saved_links(&root, &saved, "package link") {
            return reply;
        }
    }
    let installed = saved.iter().find(|line| line.package == package).cloned();
    let locked = Locked {
        package: package.clone(),
        source: Source::Link { path: path.clone() },
    };
    if let Err(reply) = write_lock(&root, &with_line(&lines, locked), "package link") {
        return reply;
    }
    // The app compiled against the checkout, as every check now is.
    let name = folder_name(&package);
    let loaded = jadpo_db::load(&root);
    let mut diagnostics: Vec<Diagnostic> = loaded
        .unreadable
        .iter()
        .map(|file| {
            Diagnostic::new(
                &rule::TOOLCHAIN_SOURCE_READ_FAILED,
                format!("`{}` couldn't be read", file.path),
            )
            .reason(format!("Reading it failed: {}.", file.error))
            .next("Check it can be read, then run `jadpo check`")
        })
        .collect();
    diagnostics.extend(jadpo_check::check(&loaded.db, loaded.project));
    let mut sources = jadpo_db::source_texts(&loaded.db, loaded.project);
    jadpo_db::show_in_checkouts(&loaded.links, &mut diagnostics, &mut sources);
    let mut screen = Screen::new();
    for d in &diagnostics {
        screen.push(Block::diagnostic_from(d, &sources, None));
    }
    screen.push(Block::Status {
        outcome: Outcome::Success,
        text: format!("Linked {package} to {path}"),
        rows: vec![
            ("checkout".to_string(), path.clone()),
            (
                "installed".to_string(),
                match installed.as_ref().and_then(|line| version_of(&line.source)) {
                    Some(version) => {
                        format!("{version}, restored by jadpo package unlink {name}")
                    }
                    None => "none".to_string(),
                },
            ),
        ],
    });
    let (outcome, fact) = if diagnostics.is_empty() {
        (Outcome::Success, Fact::plain("the app checks against it"))
    } else {
        (
            Outcome::Failure,
            Fact::problem(plural(diagnostics.len(), "error", "errors")),
        )
    };
    screen.push(Block::summary(
        outcome,
        "package linked",
        [Fact::plain(package.clone()), fact],
    ));
    screen.push(Block::hint(
        format!("jadpo package unlink {name}"),
        "goes back to the installed version before you push",
    ));
    Reply::Report {
        screen,
        data: Json::object([
            ("package", Json::from(package.as_str())),
            ("checkout", Json::from(path.as_str())),
            (
                "installed",
                installed
                    .as_ref()
                    .and_then(|line| version_of(&line.source))
                    .map_or(Json::Null, Json::from),
            ),
        ]),
        diagnostics,
        sources,
    }
}

pub fn unlink(call: &Call) -> Reply {
    let root = match root_of(call, "package unlink") {
        Ok(root) => root,
        Err(reply) => return reply,
    };
    let Some(wanted) = call.arguments.get(1) else {
        return Reply::failed(arguments(
            "`jadpo package unlink` needs a package".to_string(),
            "It takes a linked package back to its installed version.".to_string(),
            "Run `jadpo package unlink <name>`".to_string(),
        ));
    };
    let lines = match read_lock(&root) {
        Ok(lines) => lines,
        Err(reply) => return reply,
    };
    let line = match find(&lines, wanted, "unlink") {
        Ok(line) => line.clone(),
        Err(reply) => return reply,
    };
    let Source::Link { path } = &line.source else {
        return Reply::failed(arguments(
            format!("`{}` isn't linked", line.package),
            format!(
                "`{}` installs it from {}.",
                jadpo_db::LOCK,
                source_words(&line.source)
            ),
            "Run `jadpo package link <folder>` to link it to a checkout".to_string(),
        ));
    };
    let mut saved = saved_links(&root);
    let restored = saved
        .iter()
        .find(|other| other.package == line.package)
        .cloned();
    let mut kept: Vec<Locked> = lines
        .iter()
        .filter(|other| other.package != line.package)
        .cloned()
        .collect();
    if let Some(restored) = &restored {
        kept.push(restored.clone());
    }
    if let Err(reply) = write_lock(&root, &kept, "package unlink") {
        return reply;
    }
    saved.retain(|other| other.package != line.package);
    if let Err(reply) = write_saved_links(&root, &saved, "package unlink") {
        return reply;
    }
    let installed = match &restored {
        Some(restored) => format!(
            "{}, {}",
            version_of(&restored.source).unwrap_or(""),
            source_words(&restored.source)
        ),
        None => format!("none: `{}` no longer names it", jadpo_db::LOCK),
    };
    let version = restored
        .as_ref()
        .and_then(|restored| version_of(&restored.source))
        .map(str::to_string);
    let mut screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: format!("Unlinked {} from {path}", line.package),
        rows: vec![("installed".to_string(), installed)],
    });
    screen.push(Block::summary(
        Outcome::Success,
        "package unlinked",
        [Fact::plain(match &version {
            Some(version) => format!("{} {version}", line.package),
            None => line.package.clone(),
        })],
    ));
    if restored.is_none() {
        screen.push(Block::hint(
            "jadpo package add <folder or repository@tag>",
            "installs it again",
        ));
    }
    Reply::screen(
        screen,
        Json::object([
            ("package", Json::from(line.package.as_str())),
            ("checkout", Json::from(path.as_str())),
            ("version", version.as_deref().map_or(Json::Null, Json::from)),
        ]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_written_from_the_root() {
        let base =
            std::env::temp_dir().join(format!("jadpo-install-relative-{}", std::process::id()));
        let app = base.join("shop");
        let package = base.join("postcodes");
        std::fs::create_dir_all(&app).expect("app");
        std::fs::create_dir_all(package.join("v2")).expect("package");
        assert_eq!(relative(&app, &package), "../postcodes");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_surface_names_its_version() {
        assert_eq!(
            surface_version(
                "record surface 1\n\npackage A.v2\n    kind library\n    version 2.1.0\n"
            ),
            Some("2.1.0".to_string())
        );
    }
}

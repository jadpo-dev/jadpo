//! Which `jadpo` runs a command (area 14, "Which jadpo runs"; roadmap
//! chunk D14).
//!
//! The installed `jadpo` is a whole `jadpo`, as the `go` command is.
//! Before it does anything else it reads the project the command runs in
//! (the folder it runs from, walked up to `app.jadpo`, or else the first
//! argument naming a file or folder): `application { jadpo: "0.2" }`, the
//! release line the project is written for, and the exact release
//! `jadpo.lock` pins, if any. When the project needs this release, or
//! names no line, it runs the command itself. Otherwise it runs the
//! release the project needs, with the same arguments and working folder:
//! the pinned one, else the newest of the line it keeps, else the newest
//! of the line GitHub has published, fetched into its cache and checked
//! first ([`crate::releases`]). When it can't, it refuses the project
//! with `toolchain.version_newer` or `toolchain.version_older`, naming the
//! release to install and why it couldn't fetch it, rather than compile
//! the project itself.
//!
//! `jadpo toolchain list` and `remove` always run here, since they look
//! after the cache, and so does every command outside a project. Setting
//! `JADPO_TOOLCHAIN=local` makes this `jadpo` run everything itself, for
//! people working on the compiler; a project on another line is then the
//! checker's to refuse. A release this one starts is given
//! `JADPO_TOOLCHAIN_LAUNCHED`, its own version, so it never hands the
//! command on again.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use jadpo_check::jadpo_version;
use jadpo_diag::Span;
use jadpo_term::{Block, Environment, Screen, Stream, Style, envelope};

use crate::attestation::{self, Attest};
use crate::releases::{self, Cache, Network, Release, Unfetched};
use crate::run::{Output, presentation};

/// The variable that makes this `jadpo` run every command itself, when it
/// is `local`.
pub const FORCE: &str = "JADPO_TOOLCHAIN";

/// The variable a release this `jadpo` starts is given: its version.
pub const LAUNCHED: &str = "JADPO_TOOLCHAIN_LAUNCHED";

/// Who runs a command.
#[derive(Debug, PartialEq, Eq)]
pub enum Launch {
    /// This `jadpo`.
    Here,
    /// Release `version`, kept at `binary`.
    Run { binary: PathBuf, version: String },
    /// Nobody: the project needs a release this `jadpo` couldn't run.
    Refused(Output),
}

/// Who runs `jadpo` with `args`, in `env`.
pub fn launch(args: &[String], env: &Environment) -> Launch {
    launch_with(
        args,
        env,
        &releases::Https,
        &attestation::Sigstore::JADPO,
        crate::toolchain::HOST,
    )
}

/// Runs release `binary`, `version`, with this process's arguments, in
/// its folder, and exits as it does. On Unix it replaces this process.
pub fn hand_over(binary: &Path, version: &str) -> ExitCode {
    let mut command = std::process::Command::new(binary);
    command
        .args(std::env::args_os().skip(1))
        .env(LAUNCHED, version);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        eprintln!(
            "error: Jadpo {version} couldn't start\n  reason: `{}`: {error}.",
            binary.display()
        );
        ExitCode::from(1)
    }
    #[cfg(not(unix))]
    {
        match command.status() {
            Ok(status) => ExitCode::from(u8::try_from(status.code().unwrap_or(1)).unwrap_or(1)),
            Err(error) => {
                eprintln!(
                    "error: Jadpo {version} couldn't start\n  reason: `{}`: {error}.",
                    binary.display()
                );
                ExitCode::from(1)
            }
        }
    }
}

/// What a project asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wanted {
    /// The newest release of a line.
    Line((u64, u64)),
    /// The release `jadpo.lock` pins.
    Pinned(Release),
}

/// [`launch`], fetching from `network`, checking with `attest`, for the
/// target `target`.
pub(crate) fn launch_with(
    args: &[String],
    env: &Environment,
    network: &dyn Network,
    attest: &dyn Attest,
    target: &str,
) -> Launch {
    if env.var(FORCE) == Some("local") || env.var(LAUNCHED) == Some(crate::help::VERSION) {
        return Launch::Here;
    }
    let words = words(args);
    match words.as_slice() {
        [first, ..] if first == crate::completion::CALLBACK => return Launch::Here,
        [toolchain] if toolchain == "toolchain" => return Launch::Here,
        [toolchain, action, ..]
            if toolchain == "toolchain" && matches!(action.as_str(), "list" | "remove") =>
        {
            return Launch::Here;
        }
        _ => {}
    }
    let Some(root) = project(env, &words) else {
        return Launch::Here;
    };
    let Ok(app) = std::fs::read_to_string(root.join(jadpo_db::PROJECT_FILE)) else {
        return Launch::Here;
    };
    let Some(declared) = jadpo_version::declared(&app) else {
        return Launch::Here;
    };
    // A `jadpo:` that isn't a line is the checker's to report.
    let Some(line) = declared.line() else {
        return Launch::Here;
    };
    let lock = std::fs::read_to_string(root.join(jadpo_db::LOCK)).ok();
    let pin = lock
        .as_deref()
        .and_then(jadpo_db::locked_jadpo)
        .and_then(|(version, at)| Some((Release::parse(&version)?, at)))
        // A pin on another line is out of date: the checker says so.
        .filter(|(release, _)| release.line() == line);
    let running = Release::running();
    let wanted = match pin {
        Some((release, _)) if release == running => return Launch::Here,
        Some((release, _)) => Wanted::Pinned(release),
        None if line == running.line() => return Launch::Here,
        None => Wanted::Line(line),
    };
    let refuse = |(kind, why): (Unfetched, String)| {
        let (text, later, at, sources) = match (wanted, pin, &lock) {
            (Wanted::Pinned(release), Some((_, at)), Some(lock)) => {
                let offset: usize = lock.split_inclusive('\n').take(at).map(str::len).sum();
                let length = lock.lines().nth(at).map_or(0, str::len);
                (
                    release.to_string(),
                    release > running,
                    Span::new(jadpo_db::LOCK, offset as u32, (offset + length) as u32),
                    (jadpo_db::LOCK, lock.clone()),
                )
            }
            _ => (
                format!("{}.{}", line.0, line.1),
                line > running.line(),
                Span::new(jadpo_db::PROJECT_FILE, declared.start, declared.end),
                (jadpo_db::PROJECT_FILE, app.clone()),
            ),
        };
        let diagnostic = jadpo_version::refusal(
            &text,
            later,
            matches!(wanted, Wanted::Pinned(_)),
            Some(at),
            Some((kind, &why)),
        );
        let sources = BTreeMap::from([(sources.0.to_string(), sources.1)]);
        Launch::Refused(refused(args, env, &words, &diagnostic, &sources))
    };
    let Some(home) = crate::package_cache::home(env) else {
        return refuse((
            Unfetched::Uncached,
            "there is no cache folder to keep it in, since none of `XDG_CACHE_HOME`, `HOME` and `LOCALAPPDATA` is set.".to_string(),
        ));
    };
    let cache = Cache::new(&std::path::absolute(&home).unwrap_or(home), target);
    let release = match wanted {
        Wanted::Pinned(release) => release,
        Wanted::Line(line) => match cache.newest_of(line) {
            Some(release) => release,
            None => match releases::newest(network, line) {
                Ok(Some(release)) => release,
                Ok(None) => {
                    return refuse((
                        Unfetched::Unreleased,
                        format!("no release of line {}.{} is published.", line.0, line.1),
                    ));
                }
                Err(failure) => return refuse(failure),
            },
        },
    };
    let (json, choice) = presentation(args);
    let report = Fetching {
        style: Style::detect(env, json, choice, Stream::Stderr),
        json,
        target: target.to_string(),
        spinner: std::cell::RefCell::new(None),
        bytes: std::cell::Cell::new(0),
        started: std::time::Instant::now(),
    };
    let installed = cache.install(release, network, attest, &report);
    report.finish(release, installed.is_ok());
    match installed {
        Ok(binary) => Launch::Run {
            binary,
            version: release.to_string(),
        },
        Err(failure) => refuse(failure),
    }
}

/// How a fetch shows on stderr (the CLI design language): a live line with
/// a bar once the archive's size is known, where stderr animates, else one
/// line when it starts; then the status line of what was fetched and
/// checked, with its size and time. Nothing under `--json`.
struct Fetching {
    style: Style,
    json: bool,
    target: String,
    spinner: std::cell::RefCell<Option<jadpo_term::Spinner>>,
    bytes: std::cell::Cell<u64>,
    started: std::time::Instant,
}

impl Fetching {
    fn write(&self, text: &str) {
        use std::io::Write;
        let _ = std::io::stderr().write_all(text.as_bytes());
    }

    /// Ends the live line and, when the fetch got the release, says so.
    fn finish(&self, release: Release, got: bool) {
        let Some(spinner) = self.spinner.borrow_mut().take() else {
            return;
        };
        spinner.finish();
        if !got || self.json {
            return;
        }
        let size = crate::build::size(self.bytes.get() as usize);
        let seconds = self.started.elapsed().as_secs_f64();
        let time = if seconds >= 0.1 {
            format!(" in {seconds:.1} s")
        } else {
            String::new()
        };
        let line = Screen::new()
            .with(Block::Status {
                outcome: jadpo_term::Outcome::Success,
                text: format!(
                    "Fetched Jadpo {release} ({size}) and checked its SHA-256 and attestation{time}"
                ),
                rows: Vec::new(),
            })
            .render(&self.style);
        self.write(&line);
    }
}

impl releases::Report for Fetching {
    fn fetching(&self, release: Release) {
        let words = format!("Fetching Jadpo {release} for {}", self.target);
        if !self.json && !self.style.animate {
            self.write(&format!("{words}\n"));
        }
        *self.spinner.borrow_mut() = Some(jadpo_term::Spinner::start(&self.style, &words));
    }

    fn progress(&self, done: u64, total: Option<u64>) {
        self.bytes.set(done);
        if let (Some(spinner), Some(total)) = (self.spinner.borrow().as_ref(), total)
            && let Ok(mut far) = spinner.progress().lock()
        {
            *far = Some((done, total));
        }
    }
}

/// The command line's words, without its flags and `--color`'s value.
fn words(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        if arg == "--" {
            out.extend(iter.by_ref().cloned());
            break;
        }
        if arg == "--color" {
            iter.next_if(|value| jadpo_term::ColorChoice::parse(value).is_some());
            continue;
        }
        if !arg.starts_with('-') || arg == "-" {
            out.push(arg.clone());
        }
    }
    out
}

/// The root of the project a command runs on: the folder it runs in,
/// walked up to `app.jadpo`, or else that of the first argument after
/// the command that names a file or folder.
fn project(env: &Environment, words: &[String]) -> Option<PathBuf> {
    let cwd = env.cwd.clone()?;
    if let Some(root) = jadpo_db::find_root(&cwd) {
        return Some(root);
    }
    words.iter().skip(1).find_map(|word| {
        let path = cwd.join(word);
        let start = if path.is_file() {
            path.parent()?.to_path_buf()
        } else if path.is_dir() {
            path
        } else {
            return None;
        };
        jadpo_db::find_root(&start)
    })
}

/// The output of a refusal, as the command was asked to write it: one
/// JSON document on stdout, or the error on stderr.
fn refused(
    args: &[String],
    env: &Environment,
    words: &[String],
    diagnostic: &jadpo_diag::Diagnostic,
    sources: &BTreeMap<String, String>,
) -> Output {
    let (json, choice) = presentation(args);
    let name = words
        .first()
        .and_then(|word| crate::table::find(word))
        .map(|command| command.name);
    let (stdout, stderr) = if json {
        (
            envelope(name, None, std::slice::from_ref(diagnostic), sources).to_document(),
            String::new(),
        )
    } else {
        let screen = Screen::new().with(Block::diagnostic(diagnostic, sources));
        (
            String::new(),
            screen.render(&Style::detect(env, false, choice, Stream::Stderr)),
        )
    };
    Output {
        stdout,
        stderr,
        code: 1,
        session: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::releases::tests::{Attested, Served, list, serve, tempdir};

    const TARGET: &str = "aarch64-apple-darwin";

    /// A project in a folder of its own, `app.jadpo` naming `line`, with
    /// `lock` as its `jadpo.lock` when given, and a cache folder beside it.
    struct Fixture {
        dir: tempdir::Dir,
    }

    impl Fixture {
        fn new(line: Option<&str>, lock: Option<&str>) -> Fixture {
            let dir = tempdir::Dir::new();
            let project = dir.path().join("shop");
            std::fs::create_dir_all(project.join("domains")).expect("a project");
            let settings = line.map_or_else(String::new, |line| format!("    jadpo: \"{line}\"\n"));
            std::fs::write(
                project.join("app.jadpo"),
                format!("application {{\n{settings}}}\n"),
            )
            .expect("app.jadpo");
            if let Some(lock) = lock {
                std::fs::write(project.join("jadpo.lock"), lock).expect("jadpo.lock");
            }
            Fixture { dir }
        }

        fn project(&self) -> PathBuf {
            self.dir.path().join("shop")
        }

        fn cache(&self) -> Cache {
            Cache::new(&self.dir.path().join("cache"), TARGET)
        }

        fn env(&self) -> Environment {
            Environment::empty()
                .with_cwd(self.project().join("domains"))
                .with_var(
                    "XDG_CACHE_HOME",
                    &self.dir.path().join("cache").display().to_string(),
                )
        }

        /// Keeps release `version` in the cache as if it had been fetched.
        fn keep(&self, version: &str) -> PathBuf {
            let binary = self.cache().folder.join(version).join("jadpo");
            std::fs::create_dir_all(binary.parent().expect("a folder")).expect("kept");
            std::fs::write(&binary, format!("#!jadpo {version}")).expect("kept");
            binary
        }

        fn launch(&self, args: &[&str], network: &Served, attest: &dyn Attest) -> Launch {
            let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
            launch_with(&args, &self.env(), network, attest, TARGET)
        }
    }

    /// This jadpo's line, `0.1`.
    fn own_line() -> String {
        let running = Release::running();
        format!("{}.{}", running.0, running.1)
    }

    /// A line later than this jadpo's.
    fn later_line() -> String {
        let running = Release::running();
        format!("{}.{}", running.0, running.1 + 1)
    }

    fn stderr(launch: &Launch) -> String {
        match launch {
            Launch::Refused(output) => output.stderr.clone(),
            other => panic!("not refused: {other:?}"),
        }
    }

    #[test]
    fn a_project_on_this_line_runs_here() {
        let fixture = Fixture::new(Some(&own_line()), None);
        let served = Served::default();
        assert_eq!(
            fixture.launch(&["check"], &served, &Attested(None)),
            Launch::Here
        );
        assert_eq!(*served.asked.lock().expect("asked"), Vec::<String>::new());
    }

    #[test]
    fn a_project_naming_no_line_or_a_bad_one_runs_here() {
        for line in [None, Some("0.x")] {
            let fixture = Fixture::new(line, None);
            assert_eq!(
                fixture.launch(&["check"], &Served::default(), &Attested(None)),
                Launch::Here,
                "{line:?}"
            );
        }
    }

    #[test]
    fn a_project_on_another_line_runs_the_release_kept_for_it() {
        let fixture = Fixture::new(Some("0.0"), None);
        fixture.keep("0.0.2");
        let binary = fixture.keep("0.0.4");
        fixture.keep(&format!("{}.0", later_line()));
        let served = Served::default();
        assert_eq!(
            fixture.launch(&["check", "--json"], &served, &Attested(None)),
            Launch::Run {
                binary,
                version: "0.0.4".to_string()
            }
        );
        assert_eq!(*served.asked.lock().expect("asked"), Vec::<String>::new());
    }

    #[test]
    fn a_project_on_another_line_fetches_its_newest_release() {
        let fixture = Fixture::new(Some(&later_line()), None);
        let newest = format!("{}.3", later_line());
        let mut served = Served::default();
        list(
            &mut served,
            &[&format!("v{newest}"), &format!("v{}.2", later_line())],
            &[],
        );
        serve(&mut served, &newest, TARGET, b"#!jadpo later", None);
        let launched = fixture.launch(&["test"], &served, &Attested(None));
        let binary = fixture.cache().folder.join(&newest).join("jadpo");
        assert_eq!(
            launched,
            Launch::Run {
                binary: binary.clone(),
                version: newest.clone()
            }
        );
        assert_eq!(std::fs::read(binary).expect("kept"), b"#!jadpo later");
    }

    #[test]
    fn a_download_that_isnt_the_published_one_is_refused() {
        let fixture = Fixture::new(Some("0.0"), None);
        let mut served = Served::default();
        list(&mut served, &["v0.0.4"], &[]);
        serve(
            &mut served,
            "0.0.4",
            TARGET,
            b"#!evil",
            Some(&"0".repeat(64)),
        );
        let launched = fixture.launch(&["check"], &served, &Attested(None));
        let text = stderr(&launched);
        assert!(
            text.starts_with("error: This project needs Jadpo 0.0\n"),
            "{text}"
        );
        assert!(text.contains("SHA-256"), "{text}");
        assert!(text.contains("rule: toolchain.version_older"), "{text}");
        assert!(fixture.cache().held().is_empty());
    }

    #[test]
    fn a_download_whose_attestation_fails_is_refused() {
        let fixture = Fixture::new(Some("0.0"), None);
        let mut served = Served::default();
        list(&mut served, &["v0.0.4"], &[]);
        serve(&mut served, "0.0.4", TARGET, b"#!jadpo", None);
        let launched = fixture.launch(
            &["check"],
            &served,
            &Attested(Some("its archive's attestation doesn't verify.")),
        );
        assert!(stderr(&launched).contains("attestation doesn't verify"));
        assert!(fixture.cache().held().is_empty());
    }

    #[test]
    fn offline_with_nothing_kept_an_earlier_line_is_refused_naming_it() {
        let fixture = Fixture::new(Some("0.0"), None);
        let launched = fixture.launch(&["check"], &Served::default(), &Attested(None));
        let text = stderr(&launched);
        assert!(
            text.starts_with("error: This project needs Jadpo 0.0\n"),
            "{text}"
        );
        assert!(text.contains(" --> app.jadpo:2:12\n"), "{text}");
        assert!(
            text.contains("reason: Jadpo 0.0 couldn't be fetched to run it: No connection"),
            "{text}"
        );
        assert!(
            text.contains("next: Run the command again once this machine can reach github.com"),
            "{text}"
        );
        assert!(text.contains("rule: toolchain.version_older"), "{text}");
    }

    #[test]
    fn offline_with_nothing_kept_a_later_line_is_refused_as_newer() {
        let fixture = Fixture::new(Some(&later_line()), None);
        let launched = fixture.launch(&["check", "--json"], &Served::default(), &Attested(None));
        let Launch::Refused(output) = launched else {
            panic!("not refused");
        };
        assert_eq!(output.code, 1);
        assert!(output.stderr.is_empty());
        let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
        assert_eq!(document["ok"], false);
        assert_eq!(document["command"], "check");
        assert_eq!(
            document["diagnostics"][0]["rule"], "toolchain.version_newer",
            "{document}"
        );
    }

    #[test]
    fn a_line_with_no_release_is_refused() {
        let fixture = Fixture::new(Some("0.0"), None);
        let mut served = Served::default();
        list(&mut served, &[], &[]);
        let text = stderr(&fixture.launch(&["check"], &served, &Attested(None)));
        assert!(
            text.contains("no release of line 0.0 is published."),
            "{text}"
        );
    }

    #[test]
    fn outside_a_project_everything_runs_here() {
        let fixture = Fixture::new(Some("0.0"), None);
        let env = Environment::empty().with_cwd(fixture.dir.path());
        for args in [
            vec!["new", "shop2"],
            vec!["--version"],
            vec!["help"],
            vec![],
        ] {
            let args: Vec<String> = args.into_iter().map(String::from).collect();
            assert_eq!(
                launch_with(&args, &env, &Served::default(), &Attested(None), TARGET),
                Launch::Here,
                "{args:?}"
            );
        }
        // A path into the project is its project.
        let args = vec!["check".to_string(), "shop".to_string()];
        assert!(matches!(
            launch_with(&args, &env, &Served::default(), &Attested(None), TARGET),
            Launch::Refused(_)
        ));
    }

    #[test]
    fn the_toolchain_commands_that_look_after_the_cache_run_here() {
        let fixture = Fixture::new(Some("0.0"), None);
        for args in [
            vec!["toolchain"],
            vec!["toolchain", "list"],
            vec!["toolchain", "remove", "0.0.4"],
        ] {
            assert_eq!(
                fixture.launch(&args, &Served::default(), &Attested(None)),
                Launch::Here,
                "{args:?}"
            );
        }
        // Pinning pins the release the project runs with, so it hands on.
        assert!(matches!(
            fixture.launch(&["toolchain", "pin"], &Served::default(), &Attested(None)),
            Launch::Refused(_)
        ));
    }

    #[test]
    fn jadpo_toolchain_local_runs_everything_here() {
        let fixture = Fixture::new(Some("0.0"), None);
        let args = vec!["check".to_string()];
        let env = fixture.env().with_var(FORCE, "local");
        assert_eq!(
            launch_with(&args, &env, &Served::default(), &Attested(None), TARGET),
            Launch::Here
        );
        // Any other value is the usual behaviour.
        let env = fixture.env().with_var(FORCE, "auto");
        assert!(matches!(
            launch_with(&args, &env, &Served::default(), &Attested(None), TARGET),
            Launch::Refused(_)
        ));
    }

    #[test]
    fn a_launched_release_never_hands_on_again() {
        let fixture = Fixture::new(Some("0.0"), None);
        let args = vec!["check".to_string()];
        let env = fixture.env().with_var(LAUNCHED, crate::help::VERSION);
        assert_eq!(
            launch_with(&args, &env, &Served::default(), &Attested(None), TARGET),
            Launch::Here
        );
    }

    #[test]
    fn the_lock_pins_the_exact_release() {
        let pinned = format!("{}.7", own_line());
        let lock = jadpo_db::write_pinned_lock(Some(&pinned), &[]);
        let fixture = Fixture::new(Some(&own_line()), Some(&lock));
        // On this jadpo's own line, the pinned patch still runs.
        let binary = fixture.keep(&pinned);
        fixture.keep(&format!("{}.9", own_line()));
        assert_eq!(
            fixture.launch(&["build"], &Served::default(), &Attested(None)),
            Launch::Run {
                binary,
                version: pinned.clone()
            }
        );
        // Offline with it not kept, it is refused at the lock's line.
        let fixture = Fixture::new(Some(&own_line()), Some(&lock));
        let text = stderr(&fixture.launch(&["build"], &Served::default(), &Attested(None)));
        assert!(
            text.starts_with(&format!("error: This project needs Jadpo {pinned}\n")),
            "{text}"
        );
        assert!(text.contains(" --> jadpo.lock:4:1\n"), "{text}");
        assert!(text.contains("`jadpo.lock` pins Jadpo"), "{text}");
        // A pin of this very release runs here.
        let lock = jadpo_db::write_pinned_lock(Some(crate::help::VERSION), &[]);
        let fixture = Fixture::new(Some(&own_line()), Some(&lock));
        assert_eq!(
            fixture.launch(&["build"], &Served::default(), &Attested(None)),
            Launch::Here
        );
        // A pin on another line than `jadpo:` is out of date and left to
        // the checker: the line decides.
        let lock = jadpo_db::write_pinned_lock(Some("0.0.3"), &[]);
        let fixture = Fixture::new(Some(&own_line()), Some(&lock));
        assert_eq!(
            fixture.launch(&["build"], &Served::default(), &Attested(None)),
            Launch::Here
        );
    }

    #[test]
    fn the_command_line_is_read_for_its_words() {
        let args: Vec<String> = ["--color", "never", "--json", "toolchain", "-h", "list"]
            .iter()
            .map(|arg| arg.to_string())
            .collect();
        assert_eq!(words(&args), ["toolchain", "list"]);
    }
}

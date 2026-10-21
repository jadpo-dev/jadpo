//! The differential harness (roadmap chunk 8.1f; docs/compiler-runtime.md
//! §2.5, "Differential test" and "Compiled code"): a project's tests run
//! interpreted and compiled, their records compared.
//!
//! [`differential_all`] compiles each project ([`jadpo_codegen::module`])
//! as a module of one crate, `jadpo-compiled`, in a workspace it keeps,
//! beside one harness binary depending on that crate and on this one,
//! builds that binary offline with cargo, once, so the whole corpus is one
//! rustc invocation (chunk 8.1j), and runs it on each project in turn
//! ([`differential`] does it for one). The harness ([`harness`]) runs every
//! test of the project it is given with records, seed 1, on the
//! interpreter and then on that project's compiled program, in one
//! process, and compares the two runs test by test
//! ([`crate::differential`]): how many it compared and how many passed, or
//! the first line that differs, with both sides.
//!
//! The workspace's files are written only when they change, and its
//! target folder is kept, so a run with nothing changed compiles nothing:
//! the first builds the runtime crates and this one once, and every run
//! links one binary however many projects it compares (chunk 8.1g).

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use jadpo_interp::Prepared;
use jadpo_runtime::Executor;
use jadpo_runtime::compiled::Program;
use jadpo_runtime::config::ConfigValues;

use crate::{Options, Ran, Run, run, run_with};

/// What the harness found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Compared {
    /// The tests whose records were compared, each the same both ways.
    pub tests: usize,
    /// The tests that passed compiled.
    pub passed: usize,
}

impl fmt::Display for Compared {
    /// `compared 3 tests, 3 passed`: the line the harness ends with.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "compared {} tests, {} passed", self.tests, self.passed)
    }
}

impl Compared {
    /// The counts in `line`, as [`Compared`]'s `Display` writes them.
    fn read(line: &str) -> Option<Compared> {
        let rest = line.strip_prefix("compared ")?;
        let (tests, rest) = rest.split_once(" tests, ")?;
        let passed = rest.strip_suffix(" passed")?;
        Some(Compared {
            tests: tests.parse().ok()?,
            passed: passed.parse().ok()?,
        })
    }
}

/// Why no comparison was made, or how it went wrong.
#[derive(Debug)]
pub enum Problem {
    /// The project's tests didn't run: why, as the runner said.
    NotRun(String),
    /// The server lacks an extension the project's migrations create, or
    /// has too old a one (areas 20 and 21), so neither executor could run
    /// its tests: which, as the runner said. A developer's server may; CI's
    /// never does (`jadpo_test::skip_for_version`).
    Unserved(String),
    /// The generator refused the project or failed.
    Generate(jadpo_codegen::Error),
    /// The harness's workspace couldn't be written.
    Write(String),
    /// cargo couldn't build the harness.
    Build(jadpo_codegen::cargo::Failed),
    /// The two runs' records differ: the first difference, both sides.
    Differs(String),
    /// The harness ended without saying what it compared: what it said.
    Harness(String),
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::NotRun(why) => write!(f, "the tests didn't run: {why}"),
            Problem::Unserved(why) => write!(f, "{UNSERVED}{why}"),
            Problem::Generate(error) => write!(f, "the project wasn't compiled:\n{error}"),
            Problem::Write(error) => write!(f, "the harness couldn't be written: {error}"),
            Problem::Build(failed) => write!(f, "the harness wasn't built: {failed}"),
            Problem::Differs(difference) => {
                write!(
                    f,
                    "compiled code's records differ from the interpreter's:\n{difference}"
                )
            }
            Problem::Harness(said) => write!(f, "the harness ended without a comparison:\n{said}"),
        }
    }
}

impl std::error::Error for Problem {}

/// How the harness says the server can't serve the project's migrations.
const UNSERVED: &str = "the server can't serve the project's migrations: ";

/// The finished run in `ran`.
fn done(ran: Ran) -> Result<Box<Run>, Problem> {
    match ran {
        Ran::Done(run) => Ok(run),
        Ran::Unmigrated { store, error } => Err(Problem::Unserved(format!("{store}: {error:?}"))),
        other => Err(Problem::NotRun(format!("{other:?}"))),
    }
}

/// Every test of the project at `folder`, with records and seed 1, each
/// batch run it started run to its end (chunk 8.1h), run on the
/// interpreter and then on the executors `make` gives: how many were
/// compared and passed compiled, or the first difference.
pub fn compare<E: Executor + 'static>(
    folder: &Path,
    make: impl Fn(Option<&ConfigValues>) -> E + Sync,
) -> Result<Compared, Problem> {
    let options = || Options {
        cwd: folder.to_path_buf(),
        target: Some(".".into()),
        seed: Some(1),
        record: true,
        batches: true,
        ..Options::default()
    };
    let interpreted = match run(options()) {
        // A project with no tests compares none, its program still made,
        // so its plan is read back.
        Ran::Stopped(diagnostic) if diagnostic.rule.id == "test.no_tests" => {
            drop(make(None));
            return Ok(Compared {
                tests: 0,
                passed: 0,
            });
        }
        ran => done(ran)?,
    };
    let compiled = done(run_with(options(), make))?;
    let tests = crate::differential(&interpreted, &compiled)
        .map_err(|difference| Problem::Differs(difference.to_string()))?;
    Ok(Compared {
        tests,
        passed: compiled.passed(),
    })
}

/// What a generated crate gives: its compiled program on this thread.
pub type MakeProgram = fn() -> Result<Program, jadpo_std::Unreadable>;

/// The harness binary's `main`, holding each generated crate's program by
/// place: [`compare`] on the project folder its second argument names,
/// with the program its first argument places, printing what it compared,
/// or the problem and ending in failure.
pub fn harness(programs: &[MakeProgram]) -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let (Some(place), Some(folder)) = (args.next(), args.next()) else {
        eprintln!("usage: harness <program> <project folder>");
        return ExitCode::from(2);
    };
    let Some(make) = place
        .to_str()
        .and_then(|place| place.parse::<usize>().ok())
        .and_then(|place| programs.get(place).copied())
    else {
        eprintln!("harness: no program {}", place.to_string_lossy());
        return ExitCode::from(2);
    };
    let compared = compare(Path::new(&folder), |values| {
        let program = make().expect("the compiled plan reads back");
        match values {
            Some(values) => program.with_config(values),
            None => program,
        }
    });
    match compared {
        Ok(compared) => {
            println!("{compared}");
            ExitCode::SUCCESS
        }
        Err(problem) => {
            println!("{problem}");
            ExitCode::FAILURE
        }
    }
}

/// The folder holding the workspace's crates, this one's among them.
fn sources() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from(".."), Path::to_path_buf)
}

/// Writes `bytes` to `path` unless it holds them already.
fn write(path: &Path, bytes: &[u8]) -> Result<(), Problem> {
    if std::fs::read(path).is_ok_and(|there| there == bytes) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| Problem::Write(error.to_string()))?;
    }
    std::fs::write(path, bytes).map_err(|error| Problem::Write(error.to_string()))
}

/// The `[profile.*]` tables of the workspace's own manifest, which the
/// harness's builds the same way (the password crates optimised).
fn profiles(manifest: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in manifest.lines() {
        if line.starts_with('[') {
            inside = line.starts_with("[profile.");
        }
        if inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Compiles the project at `folder`, which must check cleanly, into the
/// harness workspace at `work`, builds the harness and runs it on the
/// project: what it compared, or why it couldn't.
pub fn differential(folder: &Path, work: &Path) -> Result<Compared, Problem> {
    differential_all(&[folder.to_path_buf()], work)?
        .pop()
        .unwrap_or_else(|| Err(Problem::Harness("no project was compared".into())))
}

/// The crate every project is compiled into, in the harness's workspace.
const CRATE: &str = "compiled";

/// Compiles each project at `folders` into the harness workspace at
/// `work`, as a module of one crate (`compiled/src/p<n>/`, by its place in
/// `folders`), builds one harness binary depending on it, and runs it on
/// each project in turn: what each compared, or why it couldn't, in
/// `folders`' order. A project that doesn't check or isn't compiled is
/// that project's problem; a harness that doesn't build is every
/// project's.
pub fn differential_all(
    folders: &[PathBuf],
    work: &Path,
) -> Result<Vec<Result<Compared, Problem>>, Problem> {
    let sources = sources();
    let crate_folder = work.join(CRATE);
    // Each project checked and generated on a thread of its own.
    let generated = shared_out(folders, |place, folder| generated(folder, place));
    let mut results: Vec<Option<Result<Compared, Problem>>> = Vec::new();
    // Each compiled project's place in `folders`, in the order the harness
    // holds their programs.
    let mut compiled: Vec<usize> = Vec::new();
    for (place, generated) in generated.into_iter().enumerate() {
        match generated {
            Ok(generated) => {
                generated
                    .write(&crate_folder)
                    .map_err(|error| Problem::Write(error.to_string()))?;
                compiled.push(place);
                results.push(None);
            }
            Err(problem) => results.push(Some(Err(problem))),
        }
    }
    if !compiled.is_empty() {
        jadpo_codegen::holding(
            &compiled,
            &jadpo_codegen::Options {
                name: "jadpo-compiled".into(),
                runtime: sources.display().to_string(),
            },
        )
        .map_err(Problem::Generate)?
        .write(&crate_folder)
        .map_err(|error| Problem::Write(error.to_string()))?;
        remove_stale_modules(&crate_folder, &compiled)?;
        let binary = harness_binary(work, &sources)?;
        // A few projects at a time: each runs its own tests on a thread
        // per core, and most have one or two.
        let runs: Vec<(usize, usize)> = compiled.iter().copied().enumerate().collect();
        let ran = shared_out(&runs, |_, (program, place)| {
            folders
                .get(*place)
                .map(|folder| run_harness(&binary, *program, folder))
        });
        for ((_, place), ran) in runs.into_iter().zip(ran) {
            results[place] = ran;
        }
    }
    Ok(results
        .into_iter()
        .map(|result| {
            result.unwrap_or_else(|| Err(Problem::Harness("the project wasn't run".into())))
        })
        .collect())
}

/// `work` done for each of `items` with its place, on a few threads at
/// once, the results in `items`' order.
fn shared_out<T: Sync, R: Send>(items: &[T], work: impl Fn(usize, &T) -> R + Sync) -> Vec<R> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .clamp(1, 4);
    let mut done: Vec<(usize, R)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let at = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(item) = items.get(at) else {
                            break;
                        };
                        done.push((at, work(at, item)));
                    }
                    done
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| match worker.join() {
                Ok(done) => done,
                Err(panic) => std::panic::resume_unwind(panic),
            })
            .collect()
    });
    done.sort_by_key(|(at, _)| *at);
    done.into_iter().map(|(_, result)| result).collect()
}

/// Removes each project's module in the crate at `folder` that isn't one
/// of `compiled`'s, left by an earlier run over other projects, so the
/// folder holds what is compiled.
fn remove_stale_modules(folder: &Path, compiled: &[usize]) -> Result<(), Problem> {
    let Ok(entries) = std::fs::read_dir(folder.join("src")) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let stale = name
            .to_str()
            .and_then(|name| name.strip_prefix('p'))
            .and_then(|place| place.parse::<usize>().ok())
            .is_some_and(|place| !compiled.contains(&place));
        if stale {
            std::fs::remove_dir_all(entry.path())
                .map_err(|error| Problem::Write(error.to_string()))?;
        }
    }
    Ok(())
}

/// The module the project at `folder` compiles to, named for its `place`.
fn generated(folder: &Path, place: usize) -> Result<jadpo_codegen::Generated, Problem> {
    let loaded = jadpo_db::load(folder);
    let errors: Vec<String> = jadpo_check::check(&loaded.db, loaded.project)
        .iter()
        .filter(|diagnostic| jadpo_check::blocks_running(diagnostic))
        .map(|diagnostic| format!("{}: {}", diagnostic.rule.id, diagnostic.message))
        .collect();
    if !errors.is_empty() {
        return Err(Problem::NotRun(errors.join("\n")));
    }
    let prepared = Prepared::checked(&loaded.db, loaded.project);
    jadpo_codegen::module(prepared.plan(), place).map_err(Problem::Generate)
}

/// Writes the harness's workspace around the crate holding every project,
/// and builds its binary: its path.
fn harness_binary(work: &Path, sources: &Path) -> Result<PathBuf, Problem> {
    let root = sources.parent().unwrap_or(sources);
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    write(
        &work.join("Cargo.toml"),
        format!(
            "# The differential harness's workspace (docs/compiler-runtime.md §2.5), written by jadpo-test; don't edit it.\n\
             [workspace]\n\
             members = [\"{CRATE}\", \"harness\"]\n\
             resolver = \"3\"\n\
             \n\
             {}",
            profiles(&manifest)
        )
        .as_bytes(),
    )?;
    // The workspace's own lock, so every version is the one it builds
    // with and nothing is fetched; cargo adds the generated packages to it.
    let lock = std::fs::read(root.join("Cargo.lock")).unwrap_or_default();
    write(&work.join("Cargo.lock"), &lock)?;
    write(
        &work.join("harness/Cargo.toml"),
        format!(
            "# The differential harness, written by jadpo-test; don't edit it.\n\
             [package]\n\
             name = \"jadpo-harness\"\n\
             version = \"0.0.0\"\n\
             edition = \"2024\"\n\
             publish = false\n\
             \n\
             [[bin]]\n\
             name = \"harness\"\n\
             path = \"src/main.rs\"\n\
             \n\
             [dependencies]\n\
             jadpo-compiled = {{ path = \"../{CRATE}\", features = [\"testing\"] }}\n\
             jadpo-test = {{ path = \"{}\" }}\n",
            sources.join("jadpo-test").display()
        )
        .as_bytes(),
    )?;
    write(
        &work.join("harness/src/main.rs"),
        "//! Runs a project's tests interpreted and compiled and compares their\n\
         //! records (docs/compiler-runtime.md §2.5); written by jadpo-test.\n\
         \n\
         fn main() -> std::process::ExitCode {\n    \
             jadpo_test::compiled::harness(jadpo_compiled::PROGRAMS)\n\
         }\n"
        .as_bytes(),
    )?;
    jadpo_codegen::cargo::build(&jadpo_codegen::cargo::Build {
        workspace: work,
        package: "jadpo-harness",
        bin: "harness",
        target: &work.join("target"),
    })
    .map_err(Problem::Build)
}

/// The harness at `binary` run on the project at `folder` with its
/// `program`th program: what it compared, or why it couldn't.
fn run_harness(binary: &Path, program: usize, folder: &Path) -> Result<Compared, Problem> {
    let output = Command::new(binary)
        .arg(program.to_string())
        .arg(folder)
        .output()
        .map_err(|error| Problem::Harness(error.to_string()))?;
    let said = String::from_utf8_lossy(&output.stdout).into_owned();
    let compared = said.lines().last().and_then(Compared::read);
    const DIFFERS: &str = "compiled code's records differ from the interpreter's:\n";
    match compared {
        Some(compared) if output.status.success() => Ok(compared),
        _ if said.starts_with(DIFFERS) => Err(Problem::Differs(
            said[DIFFERS.len()..].trim_end().to_string(),
        )),
        _ if said.starts_with(UNSERVED) => Err(Problem::Unserved(
            said[UNSERVED.len()..].trim_end().to_string(),
        )),
        _ => Err(Problem::Harness(format!(
            "{said}{}",
            String::from_utf8_lossy(&output.stderr)
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comparison_reads_back_from_its_line() {
        let compared = Compared {
            tests: 3,
            passed: 2,
        };
        assert_eq!(compared.to_string(), "compared 3 tests, 2 passed");
        assert_eq!(Compared::read(&compared.to_string()), Some(compared));
        assert_eq!(Compared::read("compared three"), None);
    }

    #[test]
    fn only_the_profiles_are_copied() {
        let manifest = "[workspace]\nmembers = []\n\n[profile.dev.package.x]\nopt-level = 3\n\n[workspace.lints]\nx = 1\n";
        assert_eq!(
            profiles(manifest),
            "[profile.dev.package.x]\nopt-level = 3\n\n"
        );
    }
}

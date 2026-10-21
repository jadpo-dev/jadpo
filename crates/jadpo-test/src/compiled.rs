//! The differential harness (roadmap chunk 8.1f; docs/compiler-runtime.md
//! §2.5, "Differential test" and "Compiled code"): a project's tests run
//! interpreted and compiled, their records compared.
//!
//! [`differential`] compiles the project ([`jadpo_codegen::generate`])
//! into a workspace it keeps, beside a harness binary depending on the
//! generated crate and on this one, builds that binary offline with
//! cargo, and runs it on the project. The harness ([`harness`]) runs every
//! test with records, seed 1, on the interpreter and then on the compiled
//! program, in one process, and compares the two runs test by test
//! ([`crate::differential`]): how many it compared and how many passed, or
//! the first line that differs, with both sides.
//!
//! The workspace's files are written only when they change, and its
//! target folder is kept, so a second run compiles only what changed: the
//! first builds the runtime crates and this one once.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use jadpo_interp::Prepared;
use jadpo_runtime::Executor;
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

/// The finished run in `ran`.
fn done(ran: Ran) -> Result<Box<Run>, Problem> {
    match ran {
        Ran::Done(run) => Ok(run),
        other => Err(Problem::NotRun(format!("{other:?}"))),
    }
}

/// Every test of the project at `folder`, with records and seed 1, run on
/// the interpreter and then on the executors `make` gives: how many were
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
        ..Options::default()
    };
    let interpreted = done(run(options()))?;
    let compiled = done(run_with(options(), make))?;
    let tests = crate::differential(&interpreted, &compiled)
        .map_err(|difference| Problem::Differs(difference.to_string()))?;
    Ok(Compared {
        tests,
        passed: compiled.passed(),
    })
}

/// The harness binary's `main`: [`compare`] on the project folder its
/// first argument names, printing what it compared, or the problem and
/// ending in failure.
pub fn harness<E: Executor + 'static>(
    make: impl Fn(Option<&ConfigValues>) -> E + Sync,
) -> ExitCode {
    let Some(folder) = std::env::args_os().nth(1) else {
        eprintln!("usage: harness <project folder>");
        return ExitCode::from(2);
    };
    match compare(Path::new(&folder), make) {
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
    let sources = sources();
    let generated = jadpo_codegen::generate(
        prepared.plan(),
        &jadpo_codegen::Options {
            name: "jadpo-compiled".into(),
            runtime: sources.display().to_string(),
        },
    )
    .map_err(Problem::Generate)?;
    generated
        .write(&work.join("app"))
        .map_err(|error| Problem::Write(error.to_string()))?;
    let root = sources.parent().unwrap_or(&sources);
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    write(
        &work.join("Cargo.toml"),
        format!(
            "# The differential harness's workspace (docs/compiler-runtime.md §2.5), written by jadpo-test; don't edit it.\n\
             [workspace]\n\
             members = [\"app\", \"harness\"]\n\
             resolver = \"3\"\n\
             \n\
             {}",
            profiles(&manifest)
        )
        .as_bytes(),
    )?;
    // The workspace's own lock, so every version is the one it builds
    // with and nothing is fetched; cargo adds the two packages to it.
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
             jadpo-compiled = {{ path = \"../app\", features = [\"testing\"] }}\n\
             jadpo-test = {{ path = \"{}\" }}\n",
            sources.join("jadpo-test").display()
        )
        .as_bytes(),
    )?;
    write(
        &work.join("harness/src/main.rs"),
        b"//! Runs a project's tests interpreted and compiled and compares their\n\
          //! records (docs/compiler-runtime.md \xc2\xa72.5); written by jadpo-test.\n\
          \n\
          fn main() -> std::process::ExitCode {\n    \
              jadpo_test::compiled::harness(|values| {\n        \
                  let program = jadpo_compiled::program().expect(\"the compiled plan reads back\");\n        \
                  match values {\n            \
                      Some(values) => program.with_config(values),\n            \
                      None => program,\n        \
                  }\n    \
              })\n\
          }\n",
    )?;
    let binary = jadpo_codegen::cargo::build(&jadpo_codegen::cargo::Build {
        workspace: work,
        package: "jadpo-harness",
        bin: "harness",
        target: &work.join("target"),
    })
    .map_err(Problem::Build)?;
    let output = Command::new(&binary)
        .arg(folder)
        .output()
        .map_err(|error| Problem::Harness(error.to_string()))?;
    let said = String::from_utf8_lossy(&output.stdout).into_owned();
    let compared = said.lines().last().and_then(Compared::read);
    match compared {
        Some(compared) if output.status.success() => Ok(compared),
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

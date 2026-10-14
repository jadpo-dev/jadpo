//! The test runner (area 13, "Running tests"; docs/compiler-runtime.md
//! §2.1 and §2.7): finds the `test` declarations of a project that checks
//! without errors, picks the ones asked for (a file, the test at a line, a
//! name, a kind, last run's failures), runs them in an order a seed decides
//! and reproduces, each in the interpreter, and reports every failure as
//! area 13 decides: the assertion broken into its parts' values, the
//! failure or fault that happened instead with its `.jadpo` frames, a
//! stable code, and the command that reruns it.
//!
//! [`run`] does the work; [`screen`] renders a run for people, on the CLI
//! design language's grid, and [`json`] as data for `--json`. A project
//! keeps its last run's failures in `build/test/last-run.json`, so
//! `--failed` can rerun them.
//!
//! Each test runs through the runtime's executor trait, the interpreter
//! until compiled code joins it in Phase 8, with its own environment
//! ([`TestEnv`]: its seed and clock, and the places its database and fakes
//! will take), on one thread per core; results come back in the seed's
//! order.
//!
//! [`postgres`] gives tests a PostgreSQL server: the one
//! `JADPO_TEST_DATABASE_URL` names, or a disposable one from the machine's
//! own PostgreSQL tools.
//!
//! What later phases add is left out rather than imitated: the trace of
//! what ran, fakes, the kept database and clock overrides. Route and
//! schedule tests come with their areas.

mod diff;
mod env;
mod json;
mod outcome;
pub mod postgres;
mod record;
mod run;
mod screen;
mod select;
pub mod watch;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jadpo_diag::{Diagnostic, Severity, Span, rule};
use jadpo_graph::{EdgeKind, Graph, Kind};
use jadpo_interp::Prepared;

pub use env::{Fakes, TestDatabase, TestEnv};
pub use json::{json, schema};
pub use outcome::{
    AssertionFailure, Change, ChangeMark, Detail, FailureInfo, FaultInfo, Located, ShownLine,
    ShownPart, StackFrame, TestFailure,
};
pub use record::RECORD;
pub use screen::screen;

/// What a test calls, which decides what it proves (area 13, "Kinds of
/// test").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TestKind {
    /// Calls functions only.
    Pure,
    /// Calls actions, queries or fixtures directly.
    Action,
    /// Sends a request through a route (area 7).
    Route,
    /// Runs a schedule (area 12).
    Schedule,
}

impl TestKind {
    pub const ALL: [TestKind; 4] = [
        TestKind::Pure,
        TestKind::Action,
        TestKind::Route,
        TestKind::Schedule,
    ];

    pub fn word(self) -> &'static str {
        match self {
            TestKind::Pure => "pure",
            TestKind::Action => "action",
            TestKind::Route => "route",
            TestKind::Schedule => "schedule",
        }
    }

    pub fn from_word(word: &str) -> Option<TestKind> {
        TestKind::ALL.into_iter().find(|kind| kind.word() == word)
    }

    /// Whether this toolchain can run tests of this kind yet.
    pub fn built(self) -> bool {
        matches!(self, TestKind::Pure | TestKind::Action)
    }
}

/// One `test` declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Test {
    /// Its identity key: the subject it sits beside and its sentence.
    pub key: String,
    /// Its sentence.
    pub name: String,
    /// Its file, relative to the project root.
    pub file: String,
    /// The lines it covers, both ends included.
    pub line: u32,
    pub end_line: u32,
    pub kind: TestKind,
}

/// What `jadpo test` was asked to run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// The folder the command runs in.
    pub cwd: PathBuf,
    /// A file or folder, as written, with an optional `:line`.
    pub target: Option<String>,
    /// Text the test's sentence contains, ignoring case.
    pub name: Option<String>,
    pub kind: Option<TestKind>,
    /// Only the tests that failed last time.
    pub failed: bool,
    /// The order seed; a new one when not given.
    pub seed: Option<u64>,
}

/// How a request to run tests ended.
#[derive(Debug)]
pub enum Ran {
    /// Run outside any project, with no path to say what to run.
    NoProject,
    /// The project has errors, so nothing ran: its check's diagnostics
    /// (warnings too), the sources they point into, and how many files.
    Refused {
        diagnostics: Vec<Diagnostic>,
        sources: BTreeMap<String, String>,
        files: usize,
        /// Where the project is from the folder the command ran in.
        root: Option<String>,
    },
    /// Nothing ran, and this says why: no tests, nothing matched, a
    /// command-line mistake, or a kind not built yet.
    Stopped(Box<Diagnostic>),
    /// `--failed`, and nothing failed last time (`gone` is 0), or every
    /// test that did is gone from the project now (`gone` counts them).
    NothingToRerun {
        gone: usize,
    },
    Done(Box<Run>),
}

/// The tests that ran and how each ended.
#[derive(Debug)]
pub struct Run {
    pub seed: u64,
    /// The folder the run's paths are relative to, as the command line
    /// spelt it when the command ran outside it (`jadpo_db::shown_root`);
    /// `None` from inside, where the project's own paths work.
    pub root: Option<String>,
    /// What was asked for, for `--json`.
    pub selection: Selection,
    /// Every test in the project, by kind.
    pub total: BTreeMap<TestKind, usize>,
    /// In the order they ran.
    pub results: Vec<TestResult>,
    pub elapsed: Duration,
}

impl Run {
    pub fn total(&self) -> usize {
        self.total.values().sum()
    }

    pub fn failed(&self) -> usize {
        self.results
            .iter()
            .filter(|result| result.failure.is_some())
            .count()
    }

    pub fn passed(&self) -> usize {
        self.results.len() - self.failed()
    }
}

/// The filters a run applied, as asked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// The file or folder, relative to the root.
    pub path: Option<String>,
    pub line: Option<u32>,
    pub name: Option<String>,
    pub kind: Option<TestKind>,
    pub failed: bool,
}

#[derive(Debug)]
pub struct TestResult {
    pub test: Test,
    /// `None` when it passed.
    pub failure: Option<Box<TestFailure>>,
}

/// Runs what `options` asks for.
pub fn run(options: Options) -> Ran {
    if let Some(kind) = options.kind
        && !kind.built()
    {
        return Ran::Stopped(Box::new(select::kind_not_built(kind)));
    }
    let target = match select::resolve(&options) {
        Ok(target) => target,
        Err(ran) => return ran,
    };
    let started = Instant::now();
    match jadpo_interp::on_large_stack(move || execute(target, options, started)) {
        Ok(ran) => ran,
        Err(error) => Ran::Stopped(Box::new(
            Diagnostic::new(
                &rule::TEST_RUNTIME_START_FAILED,
                "The interpreter couldn't start",
            )
            .reason(format!(
                "Tests run on a thread with a large stack, and starting it failed: {error}."
            ))
            .next("Free some memory, then run `jadpo test` again"),
        )),
    }
}

/// Loads, checks, selects and runs, on the interpreter's thread.
fn execute(target: select::Target, options: Options, started: Instant) -> Ran {
    let loaded = jadpo_db::load(&target.folder);
    let checked = match prepare(&loaded, &options.cwd) {
        Ok(checked) => checked,
        Err(ran) => return ran,
    };
    choose_and_run(&loaded, &checked, &target, &options, None, started)
}

/// A project that checked without errors, ready to run: its prepared
/// program, shared by the threads that run it, and every test in it.
pub(crate) struct Checked {
    pub prepared: Arc<Prepared>,
    pub tests: Vec<Test>,
    /// A project, rather than loose files: only a project keeps a record.
    pub project: bool,
}

/// The project's sources by path, for the code frames of its errors.
pub(crate) fn sources(loaded: &jadpo_db::Loaded) -> BTreeMap<String, String> {
    let db = &loaded.db;
    loaded
        .project
        .files(db)
        .iter()
        .map(|file| (file.path(db).clone(), file.text(db).clone()))
        .collect()
}

/// Checks what `loaded` holds and loads its program, or refuses with its
/// errors.
pub(crate) fn prepare(loaded: &jadpo_db::Loaded, cwd: &std::path::Path) -> Result<Checked, Ran> {
    let db = &loaded.db;
    let root = jadpo_db::shown_root(cwd, &loaded.root);
    let file_count = loaded.project.files(db).len() + loaded.unreadable.len();
    if !loaded.unreadable.is_empty() {
        let diagnostics = loaded
            .unreadable
            .iter()
            .map(|file| {
                Diagnostic::new(
                    &rule::TOOLCHAIN_SOURCE_READ_FAILED,
                    format!("`{}` couldn't be read", file.path),
                )
                .at(Span::new(&file.path, 0, 0), "")
                .reason(format!("Reading it failed: {}.", file.error))
                .next("Check it can be read, then run `jadpo test` again")
            })
            .collect();
        return Err(Ran::Refused {
            diagnostics,
            sources: sources(loaded),
            files: file_count,
            root: root.clone(),
        });
    }
    let diagnostics = jadpo_check::check(db, loaded.project);
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity() == Severity::Error)
    {
        return Err(Ran::Refused {
            diagnostics,
            sources: sources(loaded),
            files: file_count,
            root: root.clone(),
        });
    }
    let graph = jadpo_check::graph::graph(db, loaded.project);
    Ok(Checked {
        prepared: Prepared::checked(db, loaded.project),
        tests: discover(graph),
        project: loaded.project.layout(db) == jadpo_db::Layout::Project,
    })
}

/// Picks the tests `options` asks for, narrowed to `only` when given (the
/// tests a change reaches, by key), and runs them in the seed's order.
/// When `only` leaves nothing, the run has no results.
pub(crate) fn choose_and_run(
    loaded: &jadpo_db::Loaded,
    checked: &Checked,
    target: &select::Target,
    options: &Options,
    only: Option<&std::collections::BTreeSet<String>>,
    started: Instant,
) -> Ran {
    let Checked {
        prepared,
        tests,
        project,
    } = checked;
    let project = *project;
    let filter = target.filter(loaded);
    let last = if options.failed {
        match record::read(&loaded.root, project) {
            Ok(last) => Some(last),
            Err(diagnostic) => return Ran::Stopped(diagnostic),
        }
    } else {
        None
    };
    if let Some(last) = &last {
        if last.is_empty() {
            return Ran::NothingToRerun { gone: 0 };
        }
        if !tests.iter().any(|test| last.contains(&test.key)) {
            return Ran::NothingToRerun { gone: last.len() };
        }
    }
    let selection = Selection {
        path: filter.path.clone(),
        line: filter.line,
        name: options.name.clone(),
        kind: options.kind,
        failed: options.failed,
    };
    let mut chosen = match select::choose(tests, &selection, last.as_ref(), project) {
        Ok(chosen) => chosen,
        Err(diagnostic) => return Ran::Stopped(diagnostic),
    };
    if let Some(only) = only {
        chosen.retain(|test| only.contains(&test.key));
    }
    let seed = options.seed.unwrap_or_else(select::new_seed);
    let ordered = select::shuffle(chosen, seed);
    let root = jadpo_db::shown_root(&options.cwd, &loaded.root);
    let rerun = Rerun {
        root: root.clone(),
        seed,
    };
    let failures = run::run_all(prepared, &ordered, seed, &rerun);
    let results: Vec<TestResult> = ordered
        .into_iter()
        .zip(failures)
        .map(|(test, failure)| TestResult { test, failure })
        .collect();
    if project {
        record::write(&loaded.root, tests, &results);
    }
    let mut total = BTreeMap::new();
    for test in tests {
        *total.entry(test.kind).or_default() += 1;
    }
    Ran::Done(Box::new(Run {
        seed,
        root,
        selection,
        total,
        results,
        elapsed: started.elapsed(),
    }))
}

/// How a failed test's rerun command names its file: under the project
/// as the command line spelt it when the command ran outside it, so the
/// command runs from the same folder; otherwise relative to the root,
/// which works from anywhere inside.
#[derive(Clone, Debug)]
pub(crate) struct Rerun {
    pub root: Option<String>,
    pub seed: u64,
}

impl Rerun {
    /// `jadpo test <file>:<line> --seed <seed>`.
    pub fn command(&self, file: &str, line: u32) -> String {
        let path = jadpo_term::shown_path(self.root.as_deref(), file);
        format!("jadpo test {path}:{line} --seed {}", self.seed)
    }
}

/// Every test in the graph, in file and line order, each with the kind
/// what it calls gives it.
pub fn discover(graph: &Graph) -> Vec<Test> {
    let mut tests: Vec<Test> = graph
        .ids()
        .filter_map(|id| {
            let node = graph.node(id);
            if node.kind != Kind::Test {
                return None;
            }
            let place = node.place.as_ref()?;
            let acts = graph.outgoing(id, EdgeKind::Calls).any(|callee| {
                matches!(
                    graph.node(callee).kind,
                    Kind::Action | Kind::Query | Kind::Fixture
                )
            });
            Some(Test {
                key: node.key.as_str().to_string(),
                name: node.name.clone(),
                file: place.file.clone(),
                line: place.lines.0,
                end_line: place.lines.1,
                kind: if acts {
                    TestKind::Action
                } else {
                    TestKind::Pure
                },
            })
        })
        .collect();
    tests.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    tests
}

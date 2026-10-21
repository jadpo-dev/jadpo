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
//! [`compiled`] runs a project's tests on its compiled program beside the
//! interpreter and compares their records, and [`workerd`] its route tests
//! on its compiled program inside workerd (chunk 8.2e).
//!
//! [`postgres`] gives tests a PostgreSQL server: the one
//! `JADPO_TEST_DATABASE_URL` names, or a disposable one from the machine's
//! own PostgreSQL tools; [`live`] says whether a Rust test's live part
//! runs, the one helper every such test calls; and [`host::Harness`] is the
//! one host a Rust test that drives the runtime runs on (chunk 7.0c).
//!
//! What later phases add is left out rather than imitated: the trace of
//! what ran, fakes, the kept database and clock overrides. Route and
//! schedule tests come with their areas.

pub mod compiled;
pub mod database;
mod diff;
pub mod differential;
mod entries;
mod env;
pub mod host;
mod json;
mod live;
mod outcome;
pub mod postgres;
mod record;
mod run;
mod screen;
mod select;
pub mod watch;
pub mod workerd;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::{EdgeKind, Graph, Kind};
use jadpo_interp::Prepared;

pub use differential::{Difference, TestRecord, differential};
pub use env::{Fakes, TestDatabase, TestEnv};
pub use json::{json, schema};
pub use live::{REQUIRE_VARIABLE, SKIP_VARIABLE, live, skip_for_version};
pub use outcome::{
    AssertionFailure, Change, ChangeMark, Detail, FailureInfo, FaultInfo, Located, RecordingInfo,
    ShownLine, ShownPart, StackFrame, TestFailure,
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
    /// Reads a recording of one of the app's own integrations as its
    /// operation's output, through the package's decoders (area 11,
    /// "Provenance and confirmation"; chunk 7.2d2), or a recording of a
    /// prompt's answer through each type the prompt is asked for as (area
    /// 21, "Tests"; chunk 5.5v4): not a `test` declaration, but checked on
    /// every run as one.
    Recording,
}

impl TestKind {
    pub const ALL: [TestKind; 5] = [
        TestKind::Pure,
        TestKind::Action,
        TestKind::Route,
        TestKind::Schedule,
        TestKind::Recording,
    ];

    pub fn word(self) -> &'static str {
        match self {
            TestKind::Pure => "pure",
            TestKind::Action => "action",
            TestKind::Route => "route",
            TestKind::Schedule => "schedule",
            TestKind::Recording => "recording",
        }
    }

    pub fn from_word(word: &str) -> Option<TestKind> {
        TestKind::ALL.into_iter().find(|kind| kind.word() == word)
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
    /// Keep a failed test's database for inspection (area 13).
    pub keep_failed: bool,
    /// Running in CI (the `CI` variable is set), where a package linked to
    /// a live checkout is refused (area 11).
    pub ci: bool,
    /// Keep each test's outcome-and-effect record ([`TestRecord`]), for
    /// the differential run (chunk 8.1c). Off, nothing is recorded.
    pub record: bool,
    /// The differential run's (chunk 8.1h): once a test has ended, each
    /// batch run it started is run to its end at the test's clock, its
    /// pages, rows and calls kept in the test's record, so compiled code's
    /// runs and an export's rows are compared too. `jadpo test` leaves a
    /// run as it started (area 13, "Caches and events") until question
    /// 18's batches in tests are built.
    pub batches: bool,
}

/// How a request to run tests ended.
#[derive(Debug)]
pub enum Ran {
    /// Run outside any project, with no path to say what to run.
    NoProject,
    /// The project has errors, so nothing ran: its check's diagnostics,
    /// the sources they point into, and how many files.
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
    /// The test database's server refused `store`'s migrations for an
    /// extension it doesn't have, or has too old, so no test that needs a
    /// database could run (areas 20 and 21): the command reports it as
    /// `jadpo migrate apply` does.
    Unmigrated {
        store: String,
        error: jadpo_runtime::MigrateError,
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
    /// Each recording of a prompt's answer the run read that was made from
    /// another version of its prompt (chunk 5.5v4), in path order.
    pub alerts: Vec<StaleRecording>,
    pub elapsed: Duration,
}

/// A recording of a prompt's answer made from another version of its
/// prompt than the one there now (area 21, "Tests"; chunk 5.5v4): its
/// answer still decodes, but may no longer be what the model says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaleRecording {
    pub file: String,
    /// `answer_question`.
    pub prompt: String,
    /// `sesame`.
    pub name: String,
    /// The version it was made from, `answer_question@1a2b3c4d`.
    pub recorded: String,
    /// The prompt's version now, `answer_question@864ad76b`.
    pub now: String,
}

impl StaleRecording {
    /// The command that records it again, from the prompt now (chunk
    /// 5.5v4b): `jadpo ai record answer_question --as not_covered`.
    pub fn next(&self) -> String {
        format!("jadpo ai record {} --as {}", self.prompt, self.name)
    }
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
    /// What it did, when the run kept records ([`Options::record`]).
    pub record: Option<TestRecord>,
}

/// Runs what `options` asks for.
pub fn run(options: Options) -> Ran {
    running(options, None)
}

/// Runs what `options` asks for as [`run`] does, each test through the
/// executor `make` gives each thread, given the settings a test run reads,
/// rather than the interpreter: compiled code (chunk 8.1f).
pub fn run_with<E: jadpo_runtime::Executor + 'static>(
    options: Options,
    make: impl Fn(Option<&jadpo_runtime::config::ConfigValues>) -> E + Sync,
) -> Ran {
    let boxed = move |values: Option<&jadpo_runtime::config::ConfigValues>| {
        Box::new(make(values)) as Box<dyn jadpo_runtime::Executor>
    };
    running(options, Some(&boxed))
}

/// Runs what `options` asks for, on the interpreter or through `maker`'s
/// executors.
fn running(options: Options, maker: Option<run::Maker<'_>>) -> Ran {
    let target = match select::resolve(&options) {
        Ok(target) => target,
        Err(ran) => return ran,
    };
    let started = Instant::now();
    match jadpo_interp::on_large_stack(move || execute(target, options, started, maker)) {
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
fn execute(
    target: select::Target,
    options: Options,
    started: Instant,
    maker: Option<run::Maker<'_>>,
) -> Ran {
    let loaded = jadpo_db::load(&target.folder);
    if options.ci {
        let refused = jadpo_check::packages::links_refused(&loaded.db, loaded.project);
        if !refused.is_empty() {
            return Ran::Refused {
                diagnostics: refused,
                sources: sources(&loaded),
                files: loaded.project.files(&loaded.db).len(),
                root: jadpo_db::shown_root(&options.cwd, &loaded.root),
            };
        }
    }
    let checked = match prepare(&loaded, &options.cwd) {
        Ok(checked) => checked,
        Err(ran) => return ran,
    };
    choose_and_run(&loaded, &checked, &target, &options, None, started, maker)
}

/// A project that checked without errors, ready to run: its prepared
/// program, shared by the threads that run it, and every test in it.
pub(crate) struct Checked {
    pub prepared: Arc<Prepared>,
    pub tests: Vec<Test>,
    /// A project, rather than loose files: only a project keeps a record.
    pub project: bool,
    /// Each store's migrations, which build the test databases; empty
    /// when the project has no tables.
    pub steps: BTreeMap<String, Vec<jadpo_runtime::Migration>>,
}

/// The project's sources by path, for the code frames of its errors.
pub(crate) fn sources(loaded: &jadpo_db::Loaded) -> BTreeMap<String, String> {
    jadpo_db::source_texts(&loaded.db, loaded.project)
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
    let mut diagnostics = jadpo_check::check(db, loaded.project);
    if diagnostics.iter().any(jadpo_check::blocks_running) {
        // A linked package's problems show in its checkout, where they are
        // fixed (area 11).
        let mut sources = sources(loaded);
        jadpo_db::show_in_checkouts(&loaded.links, &mut diagnostics, &mut sources);
        return Err(Ran::Refused {
            diagnostics,
            sources,
            files: file_count,
            root: root.clone(),
        });
    }
    let graph = jadpo_check::graph::graph(db, loaded.project);
    let project = loaded.project.layout(db) == jadpo_db::Layout::Project;
    let steps = if project {
        jadpo_check::data::migrations::store_steps(jadpo_check::data::migrations::migrations(
            db,
            loaded.project,
        ))
    } else {
        BTreeMap::new()
    };
    let prepared = Prepared::checked(db, loaded.project);
    let mut tests = discover(graph);
    tests.extend(recordings(&prepared));
    tests.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    Ok(Checked {
        prepared,
        tests,
        project,
        steps,
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
    maker: Option<run::Maker<'_>>,
) -> Ran {
    let Checked {
        prepared,
        tests,
        project,
        steps,
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
    // Only tests that call actions, queries or fixtures use a store.
    let users = ordered
        .iter()
        .filter(|test| !matches!(test.kind, TestKind::Pure | TestKind::Recording))
        .count();
    let databases = (!steps.is_empty() && users > 0)
        .then(|| database::TestDatabases::new(steps.clone(), options.keep_failed).for_tests(users));
    // A server without an extension the migrations create, or with too
    // old a one, stops the run before any test does; anything else that
    // keeps a test database from being made is each test's fault to report.
    if let Some((store, error)) = databases.as_ref().and_then(|databases| databases.refused())
        && matches!(
            error,
            jadpo_runtime::MigrateError::ExtensionMissing { .. }
                | jadpo_runtime::MigrateError::ExtensionNotCreatable { .. }
                | jadpo_runtime::MigrateError::ExtensionOutdated { .. }
        )
    {
        return Ran::Unmigrated { store, error };
    }
    let failures = run::run_all(
        prepared,
        &ordered,
        &rerun,
        databases.as_ref(),
        run::Keep {
            record: options.record,
            batches: options.batches,
        },
        maker,
    );
    drop(databases);
    let results: Vec<TestResult> = ordered
        .into_iter()
        .zip(failures)
        .map(|(test, (failure, record))| TestResult {
            test,
            failure,
            record,
        })
        .collect();
    if project {
        record::write(&loaded.root, tests, &results);
    }
    let mut total = BTreeMap::new();
    for test in tests {
        *total.entry(test.kind).or_default() += 1;
    }
    // Each recording read made from another version of its prompt.
    let alerts = prepared
        .plan()
        .prompt_recordings()
        .iter()
        .filter(|recording| {
            let key = format!("{RECORDING_KEY}{}", recording.file);
            jadpo_runtime::prompt_recordings::stale(recording)
                && results.iter().any(|result| result.test.key == key)
        })
        .filter_map(|recording| {
            Some(StaleRecording {
                file: recording.file.clone(),
                prompt: recording.prompt.clone(),
                name: recording.name.clone(),
                recorded: jadpo_runtime::prompt_recordings::recorded_version(&recording.text)?,
                now: recording.version.clone()?,
            })
        })
        .collect();
    Ran::Done(Box::new(Run {
        seed,
        root,
        selection,
        total,
        results,
        alerts,
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

/// The key of the check of the recording at `file` (chunk 7.2d2).
pub const RECORDING_KEY: &str = "recording ";

/// A check for each recording of the app's own integrations (area 11,
/// "Provenance and confirmation"; chunk 7.2d2), run as a test: its
/// operation's decoders read it; and for each recording of a prompt's
/// answer (area 21, "Tests"; chunk 5.5v4): each type the prompt is asked
/// for as decodes it.
pub fn recordings(prepared: &Prepared) -> Vec<Test> {
    let lines = |text: &str| u32::try_from(text.lines().count().max(1)).unwrap_or(u32::MAX);
    let mut out: Vec<Test> = prepared
        .plan()
        .testing()
        .recorded
        .iter()
        .map(|recorded| Test {
            key: format!("{RECORDING_KEY}{}", recorded.file),
            name: format!("{}'s recording decodes", recorded.operation),
            file: recorded.file.clone(),
            line: 1,
            end_line: lines(&recorded.text),
            kind: TestKind::Recording,
        })
        .collect();
    out.extend(
        prepared
            .plan()
            .prompt_recordings()
            .iter()
            .map(|recording| Test {
                key: format!("{RECORDING_KEY}{}", recording.file),
                name: prompt_recording_name(&recording.prompt, &recording.name),
                file: recording.file.clone(),
                line: 1,
                end_line: lines(&recording.text),
                kind: TestKind::Recording,
            }),
    );
    out
}

/// The name of the check of prompt `prompt`'s recording `name`:
/// `answer_question's sesame recording decodes`.
pub fn prompt_recording_name(prompt: &str, name: &str) -> String {
    format!("{prompt}'s {name} recording decodes")
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
            // An entity's `get` read straight from the test needs a store
            // as much as a query does.
            let acts = graph.outgoing(id, EdgeKind::Calls).any(|callee| {
                matches!(
                    graph.node(callee).kind,
                    Kind::Action | Kind::Query | Kind::Fixture
                )
            }) || graph.outgoing(id, EdgeKind::Reads).next().is_some();
            // A request through a route makes it a route test (area 13).
            let routes = graph
                .outgoing(id, EdgeKind::Calls)
                .any(|callee| graph.node(callee).kind == Kind::Route);
            // Running a schedule makes it a schedule test, whatever set up
            // its data (chunk 6.2c).
            let schedules = graph
                .outgoing(id, EdgeKind::Calls)
                .any(|callee| graph.node(callee).kind == Kind::Schedule);
            Some(Test {
                key: node.key.as_str().to_string(),
                name: node.name.clone(),
                file: place.file.clone(),
                line: place.lines.0,
                end_line: place.lines.1,
                kind: if schedules {
                    TestKind::Schedule
                } else if routes {
                    TestKind::Route
                } else if acts {
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

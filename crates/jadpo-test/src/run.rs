//! Running the chosen tests (docs/compiler-runtime.md §2.6 and §2.7): each
//! through the runtime's [`Executor`], the interpreter now and compiled code
//! from Phase 8, with its own environment ([`TestEnv`]) in a context the
//! runner makes for it. Tests are independent (area 13), so they run on one
//! single-threaded executor per core, each thread with its own view of the
//! one prepared project, and their results come back in the seed's order
//! whichever thread finished first, so a run reads the same however its
//! tests were spread.

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use jadpo_interp::{CallError, Prepared, Program, STACK_BYTES};
use jadpo_runtime::config::Mode;
use jadpo_runtime::crashes::{Boundary, Reached};
use jadpo_runtime::record::{Entry, Record};
use jadpo_runtime::{Ctx, Executor, Host, Outcome, Sources, block_on, test_run};

use crate::database::{TestDatabases, TestHost};
use crate::env::TestEnv;
use crate::outcome::CrashPoint;
use crate::outcome::{self, TestFailure};
use crate::{Rerun, Test, TestKind, TestRecord};

/// A test run reads no environment variable.
const NO_VARIABLES: fn(&str) -> Option<String> = |_| None;

/// How one test went: what it failed with, `None` when it passed, and its
/// record when the run keeps them (chunk 8.1c).
pub(crate) type Done = (Option<Box<TestFailure>>, Option<TestRecord>);

/// What a test's run kept of what it did, for its record: the runtime's
/// entries and its database's rows when it ended.
#[derive(Default)]
struct Kept {
    entries: Vec<Entry>,
    rows: Vec<String>,
}

/// What a run keeps besides how each test went: each test's record
/// (chunk 8.1c), and, for the differential run, what the batch runs a
/// test started do once run to their end (chunk 8.1h).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Keep {
    pub record: bool,
    pub batches: bool,
}

/// What makes a thread's executor when tests run on one other than the
/// interpreter (compiled code, chunk 8.1f), given the settings a test run
/// reads.
pub(crate) type Maker<'m> =
    &'m (dyn Fn(Option<&jadpo_runtime::config::ConfigValues>) -> Box<dyn Executor> + Sync);

/// Runs `tests`, in order, on the interpreter, or on the executors `maker`
/// makes: how each went, by its place in `tests`, with its record when
/// `keep` asks for one.
pub(crate) fn run_all(
    prepared: &Arc<Prepared>,
    tests: &[Test],
    rerun: &Rerun,
    databases: Option<&TestDatabases>,
    keep: Keep,
    maker: Option<Maker<'_>>,
) -> Vec<Done> {
    let sources = prepared.plan().files();
    // Every test reads the same settings: each field's `test:` value, its
    // default, and throwaway secrets made for this run (area 10).
    let settings = prepared
        .config(&NO_VARIABLES, Mode::Test, &|bytes: &mut [u8]| {
            jadpo_host_native::NativeHost::new().random(bytes)
        })
        .ok();
    if let Some(make) = maker {
        return spread(
            || Boxed(make(settings.as_ref())),
            tests,
            rerun,
            sources,
            databases,
            keep,
        );
    }
    spread(
        || {
            let program = Program::new(Arc::clone(prepared));
            match &settings {
                Some(values) => program.with_config(values),
                None => program,
            }
        },
        tests,
        rerun,
        sources,
        databases,
        keep,
    )
}

/// An executor another crate made, run as any other.
struct Boxed(Box<dyn Executor>);

impl Executor for Boxed {
    fn places(&self) -> &jadpo_runtime::Places {
        self.0.places()
    }

    fn plan(&self) -> jadpo_runtime::Plan<'_> {
        self.0.plan()
    }

    fn app(&self) -> jadpo_runtime::http::App<'_> {
        self.0.app()
    }

    fn started(&self) -> Option<Rc<jadpo_runtime::Started>> {
        self.0.started()
    }

    fn run<'a, 'p: 'a>(
        &'a self,
        ctx: &'a mut Ctx<'p>,
        callable: u32,
        args: Vec<jadpo_interp::Value>,
    ) -> jadpo_runtime::LocalFuture<'a, Outcome> {
        self.0.run(ctx, callable, args)
    }

    fn function(
        &self,
        ctx: &mut Ctx<'_>,
        callable: u32,
        args: Vec<jadpo_interp::Value>,
        at: u32,
    ) -> Result<jadpo_interp::Value, String> {
        self.0.function(ctx, callable, args, at)
    }

    fn row<'a, 'p: 'a>(
        &'a self,
        ctx: &'a mut Ctx<'p>,
        row: &'a jadpo_std::batch::BatchRow,
        item: jadpo_interp::Value,
        saved: &'a [jadpo_interp::Value],
    ) -> jadpo_runtime::LocalFuture<'a, Outcome> {
        self.0.row(ctx, row, item, saved)
    }
}

/// How many threads to spread `count` tests over: one per core, never
/// more than the tests.
fn workers(count: usize) -> usize {
    std::thread::available_parallelism()
        .map_or(1, |cores| cores.get())
        .min(count)
}

/// Runs `tests` on executors `make` gives, one per thread, and gives how
/// each went in the tests' own order.
fn spread<E: Executor>(
    make: impl Fn() -> E + Sync,
    tests: &[Test],
    rerun: &Rerun,
    sources: &(dyn Sources + Sync),
    databases: Option<&TestDatabases>,
    keep: Keep,
) -> Vec<Done> {
    let next = AtomicUsize::new(0);
    let work = || {
        let executor = make();
        // A thread whose tests may use a store drives them on the native
        // host's runtime, where the driver's connections run.
        let runtime = databases.and_then(|_| jadpo_host_native::Runtime::new().ok());
        let stores = Stores {
            databases,
            runtime: runtime.as_ref(),
        };
        let mut done = Vec::new();
        loop {
            let index = next.fetch_add(1, Ordering::Relaxed);
            let Some(test) = tests.get(index) else {
                break;
            };
            done.push((index, one(&executor, test, rerun, sources, &stores, keep)));
        }
        done
    };
    let mut results: Vec<(usize, Done)> = if workers(tests.len()) <= 1 {
        work()
    } else {
        std::thread::scope(|scope| {
            let threads: Vec<_> = (0..workers(tests.len()))
                .filter_map(|n| {
                    std::thread::Builder::new()
                        .name(format!("jadpo-test-{n}"))
                        .stack_size(STACK_BYTES)
                        .spawn_scoped(scope, work)
                        .ok()
                })
                .collect();
            // Should no thread start, this one runs them all.
            let mut results = if threads.is_empty() {
                work()
            } else {
                Vec::new()
            };
            for thread in threads {
                match thread.join() {
                    Ok(done) => results.extend(done),
                    Err(panic) => std::panic::resume_unwind(panic),
                }
            }
            results
        })
    };
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, failure)| failure).collect()
}

/// The run's test databases, when the project has tables, and the
/// runtime that drives a thread's tests that use them.
struct Stores<'a> {
    databases: Option<&'a TestDatabases>,
    runtime: Option<&'a jadpo_host_native::Runtime>,
}

/// Runs one test, keeping its record when `keep` asks for one.
fn one<E: Executor>(
    executor: &E,
    test: &Test,
    rerun: &Rerun,
    sources: &dyn Sources,
    stores: &Stores<'_>,
    keep: Keep,
) -> Done {
    let mut kept = keep.record.then(Kept::default);
    let failure = tested(
        executor,
        test,
        rerun,
        sources,
        stores,
        kept.as_mut(),
        keep.batches,
    );
    let record =
        kept.map(|kept| TestRecord::new(&kept.entries, &verdict(failure.as_deref()), &kept.rows));
    (failure, record)
}

/// How a test ended, as its record says: `passed`, or the code it failed
/// with and the crash its failing run was made with.
fn verdict(failure: Option<&TestFailure>) -> String {
    match failure {
        None => "passed".to_string(),
        Some(failure) => match &failure.crash {
            Some(crash) => format!("{} {}", failure.rule.id, crash.text()),
            None => failure.rule.id.to_string(),
        },
    }
}

/// The record `kept` asks for, when it asks for one.
fn recording(kept: Option<&Kept>) -> Option<Rc<Record>> {
    kept.map(|_| Rc::new(Record::new()))
}

/// Runs one test: `None` when it passed. What it did goes into `kept`,
/// when given, and with `batches` each batch run it started is run to its
/// end once it has ended.
fn tested<E: Executor>(
    executor: &E,
    test: &Test,
    rerun: &Rerun,
    sources: &dyn Sources,
    stores: &Stores<'_>,
    kept: Option<&mut Kept>,
    batches: bool,
) -> Option<Box<TestFailure>> {
    // A recording of one of the app's own integrations, read by its
    // operation's decoders (chunk 7.2d2).
    if test.kind == TestKind::Recording {
        let env = TestEnv::new(&test.key);
        let host = env.host();
        let mut ctx = Ctx::of(executor, &host, env.env);
        // A recording of a prompt's answer, read through each type the
        // prompt is asked for as (chunk 5.5v4).
        if let Some(read) =
            jadpo_runtime::prompt_recordings::check_file(&executor.plan(), &mut ctx, &test.file)
        {
            return read
                .err()
                .map(|unread| Box::new(outcome::prompt_recording(sources, test, &unread, rerun)));
        }
        return match jadpo_runtime::recorded::run(executor, &mut ctx, &test.file) {
            Some(Ok(())) => None,
            Some(Err(unread)) => {
                let operation = test
                    .name
                    .strip_suffix("'s recording decodes")
                    .unwrap_or(&test.name);
                let package = jadpo_db::package_at(&test.file)
                    .map_or_else(|| test.file.clone(), |(home, _)| home.versioned());
                Some(Box::new(outcome::recording(
                    sources,
                    test,
                    (&package, operation),
                    &unread,
                    rerun,
                )))
            }
            None => Some(Box::new(outcome::not_runnable(
                sources,
                test,
                &CallError::Unknown(test.key.clone()),
                rerun,
            ))),
        };
    }
    let Some(callable) = executor.plan().callable(&test.key) else {
        return Some(Box::new(outcome::not_runnable(
            sources,
            test,
            &CallError::Unknown(test.key.clone()),
            rerun,
        )));
    };
    // Only a test that calls an action, query or fixture, sends a request
    // through a route or runs a schedule can touch a store.
    if let (
        Some(databases),
        Some(runtime),
        TestKind::Action | TestKind::Route | TestKind::Schedule,
    ) = (stores.databases, stores.runtime, test.kind)
    {
        let stored = Stored {
            executor,
            test,
            rerun,
            sources,
            databases,
            runtime,
            callable,
            batches,
        };
        let first = stored.run(None, kept);
        if first.failure.is_some() {
            return first.failure;
        }
        // A test that reaches an action with several changing calls runs
        // again with a crash at each boundary of each step it reached, and
        // must pass every time, its fake's effects those of the run
        // without one (area 13; chunk 7.5d).
        return match crash_runs(&first.reached, &first.effects, |at, boundary| {
            let ran = stored.run(Some((at, boundary)), None);
            CrashRun {
                failure: ran.failure,
                effects: ran.effects,
                crashed: ran.crashed,
            }
        }) {
            None => None,
            Some(Crashed::Failed(point, mut failure)) => {
                failure.crash = Some(point);
                Some(failure)
            }
            Some(Crashed::Acted(point, effects)) => Some(Box::new(outcome::effects_differ(
                sources,
                test,
                point,
                &first.effects,
                &effects,
                rerun,
            ))),
        };
    }
    let places = executor.places();
    let record = recording(kept.as_deref());
    let host = TestEnv::new(&test.key).host();
    let once = test_run::Once {
        key: &test.key,
        callable,
        order: None,
        crash: None,
        batches: false,
        record: record.clone(),
    };
    let ran = block_on(test_run::once(executor, &host, once))
        .unwrap_or_else(|_| test_run::stalled(places, record.as_deref()));
    if let (Some(record), Some(kept)) = (&record, kept) {
        kept.entries = record.entries();
    }
    let failure = match &ran.outcome {
        Outcome::Value(_) => None,
        other => Some(Box::new(outcome::failure(
            places, sources, test, other, rerun, None,
        ))),
    };
    outcome::fakes_used(places, sources, test, &ran.usage, failure, rerun)
}

/// What a run with a crash gave (chunk 7.5d): how it failed, if it did,
/// its fake's effects, and whether the crash it was asked for happened.
pub(crate) struct CrashRun<F> {
    pub failure: Option<F>,
    pub effects: Vec<(String, usize)>,
    pub crashed: bool,
}

/// The first run with a crash that fails a test: one that failed, or one
/// whose fake's effects aren't those of the run without one.
pub(crate) enum Crashed<F> {
    Failed(CrashPoint, F),
    Acted(CrashPoint, Vec<(String, usize)>),
}

/// Runs a test again, through `run`, with a crash at each boundary of each
/// step in `reached`, the steps its run without one reached, whose fake's
/// effects were `first` (area 13, "Provider calls and capabilities in
/// tests"; chunk 7.5d): the first run that crashed and failed or acted
/// otherwise, if one did. A run whose crash never happened says nothing.
/// The same whichever executor runs the test and wherever (chunk 8.2e).
pub(crate) fn crash_runs<F>(
    reached: &[Reached],
    first: &[(String, usize)],
    mut run: impl FnMut(usize, Boundary) -> CrashRun<F>,
) -> Option<Crashed<F>> {
    for (at, step) in reached.iter().enumerate() {
        for boundary in Boundary::ALL {
            let ran = run(at + 1, boundary);
            if !ran.crashed {
                continue;
            }
            let point = CrashPoint {
                step: step.clone(),
                boundary,
            };
            if let Some(failure) = ran.failure {
                return Some(Crashed::Failed(point, failure));
            }
            if ran.effects != first {
                return Some(Crashed::Acted(point, ran.effects));
            }
        }
    }
    None
}

/// What a test with a database runs with, once for the run without a
/// crash and once for each crash (chunk 7.5d).
struct Stored<'s, E> {
    executor: &'s E,
    test: &'s Test,
    rerun: &'s Rerun,
    sources: &'s dyn Sources,
    databases: &'s TestDatabases,
    runtime: &'s jadpo_host_native::Runtime,
    callable: u32,
    /// Each batch run the test started run to its end once it has ended.
    batches: bool,
}

/// How one run of a test with a database went.
struct Ran {
    failure: Option<Box<TestFailure>>,
    /// The steps it reached under a record, in order.
    reached: Vec<Reached>,
    /// What its keyed provider calls did, by operation.
    effects: Vec<(String, usize)>,
    /// Whether the crash it was asked to make happened.
    crashed: bool,
}

impl<E: Executor> Stored<'_, E> {
    /// Runs the test on its own copy of the database, with its own fakes
    /// ([`test_run::once`]), crashing at `crash` (the step, from 1, and the
    /// boundary) when given, keeping what it did and the rows it left in
    /// `kept` when given.
    fn run(&self, crash: Option<(usize, Boundary)>, kept: Option<&mut Kept>) -> Ran {
        let (executor, test) = (self.executor, self.test);
        let places = executor.places();
        let record = recording(kept.as_deref());
        let host = TestHost::new(self.databases)
            .with_record(record.clone())
            .with_caught(test_run::Caught::of(executor.plan()));
        let ran = self.runtime.block_on(test_run::once(
            executor,
            &host,
            test_run::Once {
                key: &test.key,
                callable: self.callable,
                order: Some(self.rerun.seed),
                crash,
                batches: self.batches,
                record: record.clone(),
            },
        ));
        let failure = match &ran.outcome {
            Outcome::Value(_) => None,
            other => Some(Box::new(outcome::failure(
                places,
                self.sources,
                test,
                other,
                self.rerun,
                ran.undelivered.as_ref(),
            ))),
        };
        // A crash sends its step again, and a fake keeping no key for it
        // (the `ai` fake, a call with no key) answers it with the next
        // answer, as a model or provider asked again answers afresh: how
        // often each scripted operation was called is judged on the run
        // without a crash only (chunk 7.9a).
        let failure = if crash.is_none() {
            outcome::fakes_used(places, self.sources, test, &ran.usage, failure, self.rerun)
        } else {
            failure
        };
        if let (Some(record), Some(kept)) = (&record, kept) {
            kept.entries = record.entries();
            kept.rows = host.rows(self.runtime);
        }
        let databases = host.finish(failure.is_some());
        Ran {
            failure: failure.map(|mut failure| {
                failure.kept = databases;
                failure
            }),
            reached: ran.reached,
            effects: ran.effects,
            crashed: ran.crashed,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use jadpo_interp::Value;
    use jadpo_runtime::{Fault, FaultKind, LocalFuture, Places};

    use super::*;
    use crate::TestKind;

    /// An executor whose callable `n` faults when `n` is a multiple of 3,
    /// and takes longer the earlier it comes, so threads finish out of
    /// order; test `test n<n>` is callable `n` in its plan.
    struct Fake {
        places: Places,
        loaded: jadpo_runtime::Loaded,
    }

    impl Fake {
        fn new() -> Fake {
            let mut plan = jadpo_runtime::RuntimePlan::default();
            plan.testing.tests = (0..40).map(|n| (format!("test n{n}"), n)).collect();
            Fake {
                places: Places::new(),
                loaded: jadpo_runtime::Loaded::new(std::sync::Arc::new(plan)),
            }
        }
    }

    impl Executor for Fake {
        fn places(&self) -> &Places {
            &self.places
        }

        fn plan(&self) -> jadpo_runtime::Plan<'_> {
            self.loaded.plan()
        }

        fn run<'a, 'p: 'a>(
            &'a self,
            _ctx: &'a mut Ctx<'p>,
            callable: u32,
            _args: Vec<Value>,
        ) -> LocalFuture<'a, Outcome> {
            Box::pin(async move {
                std::thread::sleep(Duration::from_micros(u64::from(40 - callable) * 50));
                if callable.is_multiple_of(3) {
                    Outcome::Fault(Box::new(Fault {
                        kind: FaultKind::IntOverflow,
                        frames: Vec::new(),
                    }))
                } else {
                    Outcome::Value(Value::Unit)
                }
            })
        }
    }

    #[test]
    fn results_come_back_in_the_tests_order_whichever_thread_ran_them() {
        let tests: Vec<Test> = (0..40)
            .map(|n| Test {
                key: format!("test n{n}"),
                name: format!("n{n}"),
                file: "main.test.jadpo".into(),
                line: n + 1,
                end_line: n + 1,
                kind: TestKind::Pure,
            })
            .collect();
        let rerun = Rerun {
            root: None,
            seed: 1,
        };
        let sources: BTreeMap<String, String> = BTreeMap::new();
        let failed: Vec<usize> = spread(Fake::new, &tests, &rerun, &sources, None, Keep::default())
            .iter()
            .enumerate()
            .filter(|(_, (failure, _))| failure.is_some())
            .map(|(n, _)| n)
            .collect();
        let expected: Vec<usize> = (0..40).filter(|n: &usize| n.is_multiple_of(3)).collect();
        assert_eq!(failed, expected);
    }
}

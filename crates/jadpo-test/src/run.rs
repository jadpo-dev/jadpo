//! Running the chosen tests (docs/compiler-runtime.md §2.6 and §2.7): each
//! through the runtime's [`Executor`], the interpreter now and compiled code
//! from Phase 8, with its own environment ([`TestEnv`]) in a context the
//! runner makes for it. Tests are independent (area 13), so they run on one
//! single-threaded executor per core, each thread with its own view of the
//! one prepared project, and their results come back in the seed's order
//! whichever thread finished first, so a run reads the same however its
//! tests were spread.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use jadpo_interp::{CallError, Prepared, Program, STACK_BYTES};
use jadpo_runtime::config::Mode;
use jadpo_runtime::{Ctx, Executor, FaultKind, Frame, Host, Outcome, Sources, block_on};

use crate::database::{TestDatabases, TestHost};
use crate::env::{TestDatabase, TestEnv};
use crate::outcome::{self, TestFailure};
use crate::{Rerun, Test, TestKind};

/// A test run reads no environment variable.
const NO_VARIABLES: fn(&str) -> Option<String> = |_| None;

/// Runs `tests`, in order, on the interpreter: what each failed with, by
/// its place in `tests`.
pub(crate) fn run_all(
    prepared: &Arc<Prepared>,
    tests: &[Test],
    rerun: &Rerun,
    databases: Option<&TestDatabases>,
) -> Vec<Option<Box<TestFailure>>> {
    let sources = prepared.plan().files();
    // Every test reads the same settings: each field's `test:` value, its
    // default, and throwaway secrets made for this run (area 10).
    let settings = prepared
        .config(&NO_VARIABLES, Mode::Test, &|bytes: &mut [u8]| {
            jadpo_host_native::NativeHost::new().random(bytes)
        })
        .ok();
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
    )
}

/// How many threads to spread `count` tests over: one per core, never
/// more than the tests.
fn workers(count: usize) -> usize {
    std::thread::available_parallelism()
        .map_or(1, |cores| cores.get())
        .min(count)
}

/// Runs `tests` on executors `make` gives, one per thread, and gives each
/// test's failure in the tests' own order.
fn spread<E: Executor>(
    make: impl Fn() -> E + Sync,
    tests: &[Test],
    rerun: &Rerun,
    sources: &(dyn Sources + Sync),
    databases: Option<&TestDatabases>,
) -> Vec<Option<Box<TestFailure>>> {
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
            done.push((index, one(&executor, test, rerun, sources, &stores)));
        }
        done
    };
    let mut results: Vec<(usize, Option<Box<TestFailure>>)> = if workers(tests.len()) <= 1 {
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

/// Runs one test: `None` when it passed.
fn one<E: Executor>(
    executor: &E,
    test: &Test,
    rerun: &Rerun,
    sources: &dyn Sources,
    stores: &Stores<'_>,
) -> Option<Box<TestFailure>> {
    let Some(callable) = executor.callable(&test.key) else {
        return Some(Box::new(outcome::not_runnable(
            sources,
            test,
            &CallError::Unknown(test.key.clone()),
            rerun,
        )));
    };
    let mut env = TestEnv::new(&test.key);
    if let Some(start) = executor.clock(callable) {
        env.env.clock = jadpo_runtime::Clock::fixed(start);
    }
    let places = executor.places();
    // Only a test that calls an action, query or fixture, or sends a
    // request through a route, can touch a store.
    if let (Some(databases), Some(runtime), TestKind::Action | TestKind::Route) =
        (stores.databases, stores.runtime, test.kind)
    {
        env.database = TestDatabase::Copied {
            stores: databases.stores(),
        };
        let host = TestHost::new(databases);
        let outcome = {
            let mut ctx = Ctx::new(places, &host, env.env);
            ctx.set_settings(executor.settings(callable));
            runtime.block_on(executor.run(&mut ctx, callable, Vec::new()))
        };
        let failure = match outcome {
            Outcome::Value(_) => None,
            other => Some(Box::new(outcome::failure(
                places, sources, test, &other, rerun,
            ))),
        };
        let kept = host.finish(failure.is_some());
        return failure.map(|mut failure| {
            failure.kept = kept;
            failure
        });
    }
    let host = env.host();
    let mut ctx = Ctx::new(places, &host, env.env);
    ctx.set_settings(executor.settings(callable));
    let ran = block_on(executor.run(&mut ctx, callable, Vec::new()));
    let outcome = ran.unwrap_or_else(|_| {
        Outcome::Fault(ctx.fault(
            FaultKind::Defect("a wait nothing would end".into()),
            Frame::ENTRY,
        ))
    });
    match outcome {
        Outcome::Value(_) => None,
        other => Some(Box::new(outcome::failure(
            places, sources, test, &other, rerun,
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use jadpo_interp::Value;
    use jadpo_runtime::{Fault, LocalFuture, Places};

    use super::*;
    use crate::TestKind;

    /// An executor whose callable `n` faults when `n` is a multiple of 3,
    /// and takes longer the earlier it comes, so threads finish out of
    /// order.
    struct Fake {
        places: Places,
    }

    impl Executor for Fake {
        fn places(&self) -> &Places {
            &self.places
        }

        fn callable(&self, key: &str) -> Option<u32> {
            key.strip_prefix("test n").and_then(|n| n.parse().ok())
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
        let failed: Vec<usize> = spread(
            || Fake {
                places: Places::new(),
            },
            &tests,
            &rerun,
            &sources,
            None,
        )
        .iter()
        .enumerate()
        .filter(|(_, failure)| failure.is_some())
        .map(|(n, _)| n)
        .collect();
        let expected: Vec<usize> = (0..40).filter(|n: &usize| n.is_multiple_of(3)).collect();
        assert_eq!(failed, expected);
    }
}

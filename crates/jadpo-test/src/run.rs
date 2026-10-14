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
use jadpo_runtime::{Ctx, Executor, FaultKind, Frame, Outcome, Sources, block_on};

use crate::env::TestEnv;
use crate::outcome::{self, TestFailure};
use crate::{Rerun, Test};

/// Runs `tests`, in order, on the interpreter: what each failed with, by
/// its place in `tests`.
pub(crate) fn run_all(
    prepared: &Arc<Prepared>,
    tests: &[Test],
    seed: u64,
    rerun: &Rerun,
) -> Vec<Option<Box<TestFailure>>> {
    let sources = prepared.plan().files();
    spread(
        || Program::new(Arc::clone(prepared)),
        tests,
        seed,
        rerun,
        sources,
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
    seed: u64,
    rerun: &Rerun,
    sources: &(dyn Sources + Sync),
) -> Vec<Option<Box<TestFailure>>> {
    let next = AtomicUsize::new(0);
    let work = || {
        let executor = make();
        let mut done = Vec::new();
        loop {
            let index = next.fetch_add(1, Ordering::Relaxed);
            let Some(test) = tests.get(index) else {
                break;
            };
            done.push((index, one(&executor, test, seed, rerun, sources)));
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

/// Runs one test: `None` when it passed.
fn one<E: Executor>(
    executor: &E,
    test: &Test,
    seed: u64,
    rerun: &Rerun,
    sources: &dyn Sources,
) -> Option<Box<TestFailure>> {
    let Some(callable) = executor.callable(&test.key) else {
        return Some(Box::new(outcome::not_runnable(
            sources,
            test,
            &CallError::Unknown(test.key.clone()),
            rerun,
        )));
    };
    let env = TestEnv::new(seed, &test.key);
    let host = env.host();
    let places = executor.places();
    let mut ctx = Ctx::new(places, &host, env.env);
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

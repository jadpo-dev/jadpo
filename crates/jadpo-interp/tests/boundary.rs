//! Pausing and the action boundary as the interpreter meets them
//! (docs/compiler-runtime.md §2.5): a call to an action or fixture is a
//! site that pauses, where the walker hands the call to the runtime, which
//! runs it in a boundary of its own, or inside the caller's when an action
//! calls it (areas 1, 6 and 13); everything else runs without pausing.

mod support;

use jadpo_interp::{Executor, Outcome, Program, Value, on_large_stack};
use jadpo_runtime::{Ctx, Env, FaultKind, MAX_DEPTH, MemoryHost, block_on};
use support::{int, program, where_fault};

const SOURCE: &str = "type Count = Int {\n    min: 0\n    max: 1000\n}\n\naction add(n: Count) -> Count {\n    if (n < 1000) {\n        return Count(n + 1)\n    }\n    return n\n}\n\naction twice(n: Count) -> Count {\n    var once: Count = add(n)\n    return add(once)\n}\n\nfixture two_added() -> Count {\n    var one: Count = add(0)\n    return add(one)\n}\n\nfunction plain(n: Count) -> Count {\n    return n\n}\n";

const TESTS: &str = "test \"actions each commit on their own\" {\n    assert twice(1) == 3\n    assert add(0) == 1\n}\n\ntest \"a fixture is one action\" {\n    assert two_added() == 2\n}\n\ntest \"computing opens nothing\" {\n    assert plain(4) == 4\n}\n";

/// The key of the test in `main.test.jadpo` called `sentence`, or `name`
/// itself.
fn key(name: &str) -> String {
    if name.contains(' ') {
        format!("test main \"{name}\"")
    } else {
        name.to_string()
    }
}

/// How `name` ended and how many boundaries it opened.
fn run(program: &Program, name: &str, args: Vec<Value>) -> (Outcome, u32) {
    let host = MemoryHost::new();
    let mut ctx = Ctx::new(program.places(), &host, Env::default());
    let id = program.plan().find(&key(name)).expect("a callable");
    let outcome = block_on(program.run(&mut ctx, id, args)).expect("nothing waits");
    (outcome, ctx.actions())
}

#[test]
fn each_action_a_test_calls_opens_a_boundary_and_actions_it_calls_join() {
    let program = program(&[("main.jadpo", SOURCE), ("main.test.jadpo", TESTS)]);
    let (outcome, actions) = run(&program, "actions each commit on their own", Vec::new());
    assert_eq!(outcome, Outcome::Value(Value::Unit));
    assert_eq!(actions, 2, "`twice` and `add`; `twice`'s calls join it");
    let (_, actions) = run(&program, "a fixture is one action", Vec::new());
    assert_eq!(actions, 1, "a fixture is a test-only action");
    let (_, actions) = run(&program, "computing opens nothing", Vec::new());
    assert_eq!(actions, 0);
    let (outcome, actions) = run(&program, "twice", vec![int(5)]);
    assert_eq!((outcome, actions), (Outcome::Value(int(7)), 1));
}

#[test]
fn only_calls_into_actions_pause() {
    let program = program(&[("main.jadpo", SOURCE), ("main.test.jadpo", TESTS)]);
    let pausing = |name: &str| {
        let callable = program
            .plan()
            .callable(program.plan().find(&key(name)).expect("a callable"))
            .expect("its plan");
        (
            callable.body.pauses,
            callable
                .body
                .sites
                .iter()
                .filter(|site| site.pauses)
                .count(),
        )
    };
    assert_eq!(pausing("twice"), (true, 2));
    assert_eq!(pausing("add"), (false, 0));
    assert_eq!(pausing("computing opens nothing"), (false, 0));
    assert_eq!(pausing("actions each commit on their own"), (true, 2));
}

/// Each level calls itself from an inline function, inside an operation.
const THROUGH_INLINE: &str = "type Count = Int {\n    min: 0\n}\n\nfunction down(n: Count, items: List<Count>) -> Count {\n    if (n == 0) {\n        return n\n    }\n    var next: Count = Count(n - 1)\n    var below: List<Count> = collection.map(items, item -> down(next, items))\n    if (collection.count(below) > 5) {\n        return 0\n    }\n    return next\n}\n";

#[test]
fn recursion_through_inline_functions_reaches_the_depth_limit_without_overflowing() {
    let (shallow, deep) = on_large_stack(|| {
        let program = program(&[("main.jadpo", THROUGH_INLINE)]);
        let items = || Value::list(vec![int(0)]);
        let shallow = match program.call("down", vec![int(20), items()]) {
            Ok(Outcome::Value(value)) => value.to_string(),
            other => panic!("{other:?}"),
        };
        let deep = match program.call("down", vec![int(5000), items()]) {
            Ok(Outcome::Fault(fault)) => {
                let (kind, frames) = where_fault(&program, &fault);
                (kind, frames.len())
            }
            other => panic!("{other:?}"),
        };
        (shallow, deep)
    })
    .expect("a thread");
    assert_eq!(shallow, "19");
    assert_eq!(deep, (FaultKind::RecursionLimit, MAX_DEPTH));
}

#[test]
fn the_prepared_project_is_shared_across_threads_and_each_runs_its_own_program() {
    fn shared<T: Send + Sync>() {}
    shared::<jadpo_interp::Prepared>();
    let program = program(&[("main.jadpo", SOURCE), ("main.test.jadpo", TESTS)]);
    let prepared = std::sync::Arc::clone(program.prepared());
    let ran: Vec<String> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..3)
            .map(|n| {
                let prepared = std::sync::Arc::clone(&prepared);
                scope.spawn(move || {
                    let program = Program::new(prepared);
                    match program.call("twice", vec![int(n)]) {
                        Ok(Outcome::Value(value)) => value.to_string(),
                        other => format!("{other:?}"),
                    }
                })
            })
            .collect();
        threads
            .into_iter()
            .map(|thread| thread.join().expect("a thread"))
            .collect()
    });
    assert_eq!(ran, ["2", "3", "4"]);
}

//! Area 19, "Randomness" (chunk 5.5q, question 19), run: a token is 32
//! bytes as unpadded Base64url, a code Crockford's Base32, a whole number
//! within its bounds, a shuffle and a sample the list's own items; a run
//! under the same seed gives the same values, as a test's rerun does; a
//! host with a generator is read when there is no seed, a recorded run
//! replays what it was given, and with neither a generator nor a seed the
//! call is a fault, never a weaker value.

use std::cell::Cell;

use crate::support;

use jadpo_interp::{Executor, FaultKind, Outcome, Program, Value};
use jadpo_runtime::capability::Recording;
use jadpo_runtime::{Clock, Ctx, Env, Host, MemoryHost};
use support::{fault, int, list, main, text, value};

const SOURCE: &str = "type Seat = Text {\n    max_length: 4\n}\n\ntype Reference = Text {}\n\ntype Roll = Int {}\n\ntype Pick = Int {\n    min: 0\n}\n\ntype Low = Int {}\n\naction cancel_link() -> Reference {\n    return Reference(random.token())\n}\n\naction booking_reference() -> Reference {\n    return Reference(random.code(8))\n}\n\naction roll(low: Low, high: Low) -> Roll {\n    return Roll(random.integer(low, high))\n}\n\naction seating(seats: List<Seat>) -> List<Seat> {\n    return random.shuffle(seats)\n}\n\naction winners(seats: List<Seat>, count: Pick) -> List<Seat> {\n    return random.sample(seats, count)\n}\n\naction draws() -> List<Reference> {\n    return [Reference(random.token()), Reference(random.code(6)), Reference(text.from(random.integer(1, 1000000)))]\n}\n";

fn seats() -> Value {
    list(["1A", "1B", "2A", "2B", "3A"].map(text).to_vec())
}

fn texts(value: &Value) -> Vec<String> {
    match value {
        Value::List(items) => items
            .iter()
            .map(|item| match item {
                Value::Text(text) => text.to_string(),
                other => panic!("not text: {other:?}"),
            })
            .collect(),
        other => panic!("not a list: {other:?}"),
    }
}

fn one_text(value: &Value) -> String {
    match value {
        Value::Text(text) => text.to_string(),
        other => panic!("not text: {other:?}"),
    }
}

#[test]
fn a_token_is_43_characters_of_base64url() {
    let program = main(SOURCE);
    let token = one_text(&value(&program, "cancel_link", vec![]));
    assert_eq!(token.len(), 43);
    assert!(
        token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "{token}"
    );
}

#[test]
fn a_code_is_crockford_base32() {
    let program = main(SOURCE);
    let code = one_text(&value(&program, "booking_reference", vec![]));
    assert_eq!(code.len(), 8);
    assert!(
        code.bytes()
            .all(|b| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&b)),
        "{code}"
    );
}

#[test]
fn a_whole_number_is_within_its_bounds() {
    let program = main(SOURCE);
    for (low, high) in [(1, 6), (-5, -5), (i64::MIN, i64::MAX), (0, 1)] {
        let Value::Int(n) = value(&program, "roll", vec![int(low), int(high)]) else {
            panic!("an Int");
        };
        assert!((low..=high).contains(&n), "{n} in {low}..={high}");
    }
    let stopped = fault(&program, "roll", vec![int(6), int(1)]);
    assert_eq!(
        stopped.kind,
        FaultKind::Defect("`random.integer` given a `min` above its `max`".into())
    );
}

#[test]
fn a_shuffle_and_a_sample_are_the_lists_own_items() {
    let program = main(SOURCE);
    let mut shuffled = texts(&value(&program, "seating", vec![seats()]));
    shuffled.sort();
    assert_eq!(shuffled, ["1A", "1B", "2A", "2B", "3A"]);
    let two = texts(&value(&program, "winners", vec![seats(), int(2)]));
    assert_eq!(two.len(), 2);
    assert_ne!(two[0], two[1], "different positions");
    let mut all = texts(&value(&program, "winners", vec![seats(), int(9)]));
    all.sort();
    assert_eq!(all, ["1A", "1B", "2A", "2B", "3A"], "all of them");
    assert_eq!(
        value(&program, "winners", vec![seats(), int(0)]),
        list(vec![])
    );
}

/// Runs `name` on `host` under `env`, recording the random bytes when
/// asked and replaying `replay` when given.
fn run_on(
    program: &Program,
    host: &dyn Host,
    env: Env,
    name: &str,
    replay: Option<Vec<u8>>,
) -> (Outcome, Option<Vec<u8>>) {
    let id = program.plan().find(name).expect("callable");
    let mut ctx = Ctx::of(program, host, env);
    match replay {
        Some(random) => ctx.replay_run(Recording {
            random,
            calls: Vec::new(),
        }),
        None => ctx.record_run(),
    }
    let outcome = jadpo_runtime::block_on(program.run(&mut ctx, id, Vec::new())).expect("ran");
    let kept = ctx.recorded_run().map(|recording| recording.random.clone());
    (outcome, kept)
}

const TEST_ENV: Env = Env {
    seed: Some(42),
    clock: Clock::TEST,
};

const LIVE: Env = Env {
    seed: None,
    clock: Clock::TEST,
};

/// A host whose generator gives a counter's bytes: a deployable's, as
/// far as these tests can tell.
struct Generator(Cell<u8>);

impl Host for Generator {
    fn database(&self, _store: &str) -> Option<&dyn jadpo_runtime::Database> {
        None
    }

    fn random(&self, bytes: &mut [u8]) -> bool {
        for byte in bytes {
            let next = self.0.get().wrapping_mul(31).wrapping_add(7);
            self.0.set(next);
            *byte = next;
        }
        true
    }
}

#[test]
fn the_same_seed_gives_the_same_values_as_a_tests_rerun_does() {
    let program = main(SOURCE);
    let host = MemoryHost::new();
    let (first, _) = run_on(&program, &host, TEST_ENV, "draws", None);
    let (again, _) = run_on(&program, &host, TEST_ENV, "draws", None);
    assert!(matches!(first, Outcome::Value(_)), "{first:?}");
    assert_eq!(first, again, "a rerun gives the same values");
    let other = Env {
        seed: Some(43),
        ..TEST_ENV
    };
    let (different, _) = run_on(&program, &host, other, "draws", None);
    assert_ne!(first, different, "another test's seed gives others");
}

#[test]
fn a_recorded_run_on_the_generator_replays_its_values() {
    let program = main(SOURCE);
    let generator = Generator(Cell::new(1));
    let (recorded, tape) = run_on(&program, &generator, LIVE, "draws", None);
    let tape = tape.expect("kept");
    assert!(tape.len() >= 32 + 6 + 8, "{} bytes", tape.len());
    let (replayed, _) = run_on(
        &program,
        &MemoryHost::new(),
        LIVE,
        "draws",
        Some(tape.clone()),
    );
    assert_eq!(recorded, replayed);
    // A recording cut short has no randomness left, so the call faults.
    let short = tape[..10].to_vec();
    let (cut, _) = run_on(&program, &MemoryHost::new(), LIVE, "draws", Some(short));
    let Outcome::Fault(fault) = cut else {
        panic!("a fault: {cut:?}");
    };
    assert_eq!(
        fault.kind,
        FaultKind::Defect("randomness on a host with no generator and no seed".into())
    );
}

#[test]
fn no_generator_and_no_seed_is_a_fault() {
    let program = main(SOURCE);
    let id = program.plan().find("cancel_link").expect("callable");
    let host = MemoryHost::new();
    let mut ctx = Ctx::of(&program, &host, LIVE);
    let outcome = jadpo_runtime::block_on(program.run(&mut ctx, id, Vec::new())).expect("ran");
    let Outcome::Fault(fault) = outcome else {
        panic!("a fault: {outcome:?}");
    };
    assert_eq!(fault.kind.classification(), "internal_fault");
}

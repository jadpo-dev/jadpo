//! Events in tests (area 13, "Caches and events"; area 12, question 15;
//! chunk 6.11): what each step of a test emits is delivered after the
//! step commits and before the next statement, at the test's clock, until
//! none is left, in an order the printed seed shuffles; a delivery that
//! fails for good fails the test as `test.subscriber_failed`.
//!
//! A bakery: ordering a loaf emits `LoafOrdered`, which `bake_ordered`
//! bakes once an oven is lit (refusing a brick for good), emitting
//! `LoafBaked`, which `box_baked` boxes; `label_plain` and `label_seeded`
//! both label an ordered loaf, so which label stays depends on the order
//! they run in.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = crate::support::without_run_time(&output.stdout);
    output
}

/// The bakery, migrated and its records written.
fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("test-events-6.11-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let updated = jadpo(&root, &["records", "update"]);
    assert_eq!(updated.code, 0, "{}{}", updated.stdout, updated.stderr);
    root
}

/// `jadpo test --name <name> --seed <seed>`.
fn test_named(root: &Path, name: &str, seed: u64) -> Output {
    jadpo(root, &["test", "--name", name, "--seed", &seed.to_string()])
}

#[test]
fn what_a_step_emits_is_delivered_before_the_next_statement() {
    let root = project("delivered");
    if !jadpo_test::live() {
        return;
    }
    // Delivered at the test's clock, and what that sets off in turn,
    // before the next line reads the loaf; a schedule's run and a route
    // test's accepted request are steps too.
    for name in [
        "baked and boxed before the next line",
        "once it is lit and the clock moves on",
        "the morning loaf",
        "taken at the counter",
    ] {
        let tested = test_named(&root, name, 1);
        assert_eq!(tested.code, 0, "{name}: {}{}", tested.stdout, tested.stderr);
        assert!(
            tested.stdout.contains("1 passed"),
            "{name}: {}",
            tested.stdout
        );
    }
}

#[test]
fn the_printed_seed_shuffles_the_order_and_reruns_it() {
    let root = project("order");
    if !jadpo_test::live() {
        return;
    }
    let mut passed = Vec::new();
    let mut failed = Vec::new();
    for seed in 1..=16 {
        let tested = test_named(&root, "seeded label is put on last", seed);
        if tested.code == 0 {
            passed.push(seed);
        } else {
            assert!(
                tested.stdout.contains("rule: test.assertion_failed"),
                "{}",
                tested.stdout
            );
            assert!(
                tested.stdout.contains(&format!("--seed {seed}")),
                "the rerun names the seed: {}",
                tested.stdout
            );
            failed.push(seed);
        }
    }
    assert!(
        !passed.is_empty() && !failed.is_empty(),
        "a test that relies on the order its subscribers run in fails under some seeds: passed {passed:?}, failed {failed:?}"
    );
    // The same seed gives the same order.
    for seed in [passed[0], failed[0]] {
        let again = test_named(&root, "seeded label is put on last", seed);
        assert_eq!(again.code == 0, passed.contains(&seed), "seed {seed}");
    }
}

#[test]
fn a_subscriber_that_fails_for_good_fails_the_test() {
    let root = project("failed");
    if !jadpo_test::live() {
        return;
    }
    // Refused at once.
    let brick = test_named(&root, "a brick", 1);
    assert_ne!(brick.code, 0, "{}", brick.stdout);
    for line in [
        "FAIL  a brick is never baked",
        "delivery: `LoafOrdered` to subscriber `bake_ordered` failed for good on attempt 1 of 3",
        "failure: `BadDough` from action bake_loaf",
        "kind: FailureKind.rejected",
        "rule: test.subscriber_failed",
    ] {
        assert!(brick.stdout.contains(line), "{line}\n{}", brick.stdout);
    }
    let json = jadpo(
        &root,
        &["test", "--name", "a brick", "--seed", "1", "--json"],
    );
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("json");
    let failure = &document["data"]["tests"][0]["failure"];
    assert_eq!(failure["code"], "test.subscriber_failed");
    assert_eq!(
        failure["delivery"],
        serde_json::json!({
            "event": "LoafOrdered",
            "subscriber": "bake_ordered",
            "attempt": 1,
            "attempts": 3
        })
    );
    assert_eq!(failure["failure"]["name"], "BadDough");

    // Tried again each time the clock reaches its retry, then given up.
    let cold = test_named(&root, "given up on", 1);
    assert_ne!(cold.code, 0, "{}", cold.stdout);
    for line in [
        "delivery: `LoafOrdered` to subscriber `bake_ordered` failed for good on attempt 3 of 3",
        "failure: `OvenCold` from action bake_loaf",
        "rule: test.subscriber_failed",
    ] {
        assert!(cold.stdout.contains(line), "{line}\n{}", cold.stdout);
    }
}

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/bakery/types/loaf_name.jadpo",
        r#"/// What a loaf is called on the board.
type LoafName = Text {
    min_length: 1
    max_length: 40
}
"#,
    ),
    (
        "domains/bakery/types/order_request.jadpo",
        r#"use bakery/types { LoafName }

/// What the counter sends to order a loaf.
type OrderRequest = Input {
    name: LoafName
}
"#,
    ),
    (
        "domains/bakery/types/order_receipt.jadpo",
        r#"use bakery/entities { Loaf }

/// What the counter is told: which loaf it ordered.
type OrderReceipt = Output {
    loaf_id: Loaf.id
}
"#,
    ),
    (
        "domains/bakery/entities/loaf.jadpo",
        r#"use bakery/types { LoafName }

/// A loaf on order.
entity Loaf {
    id: Uuid
    name: LoafName
    baked_at: Instant?
    boxed: Bool
    label: LoafName?

    policy: {
        create: [Access.public, Access.system]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}
"#,
    ),
    (
        "domains/bakery/entities/oven.jadpo",
        r#"/// An oven, once it is lit.
entity Oven {
    id: Uuid
    lit: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
"#,
    ),
    (
        "domains/bakery/events/loaf_ordered.jadpo",
        r#"use bakery/entities { Loaf }
use bakery/types { LoafName }

/// Someone ordered a loaf.
type LoafOrdered = Event {
    loaf_id: Loaf.id
    name: LoafName
}
"#,
    ),
    (
        "domains/bakery/events/loaf_baked.jadpo",
        r#"use bakery/entities { Loaf }

/// A loaf came out of the oven.
type LoafBaked = Event {
    loaf_id: Loaf.id
}
"#,
    ),
    (
        "domains/bakery/failures/oven_cold.jadpo",
        r#"/// No oven is lit yet.
type OvenCold = Failure {
    kind: FailureKind.unavailable
    message: "The oven isn't lit yet."
}
"#,
    ),
    (
        "domains/bakery/failures/bad_dough.jadpo",
        r#"/// That dough will never rise.
type BadDough = Failure {
    kind: FailureKind.rejected
    message: "That dough won't rise."
}
"#,
    ),
    (
        "domains/bakery/queries/lit_ovens.jadpo",
        r#"use bakery/entities { Oven }

/// The ovens that are lit.
query lit_ovens() -> List<Oven> {
    from: Oven
    where: Oven.lit == true
    order: [Oven.id asc]
    limit: 10
}
"#,
    ),
    (
        "domains/bakery/queries/boxed_loaves.jadpo",
        r#"use bakery/entities { Loaf }

/// The loaves boxed and ready.
query boxed_loaves() -> List<Loaf> {
    from: Loaf
    where: Loaf.boxed == true
    order: [Loaf.id asc]
    limit: 10
}
"#,
    ),
    (
        "domains/bakery/actions/order_loaf.jadpo",
        r#"use bakery/entities { Loaf }
use bakery/types { LoafName }
use bakery/events { LoafOrdered }

/// Orders a loaf; baking it follows in a subscriber.
action order_loaf(name: LoafName) -> Loaf {
    var loaf: Loaf = Loaf.create {
        set: {
            name: name
            baked_at: none
            boxed: false
            label: none
        }
    }
    emit LoafOrdered {
        loaf_id: loaf.id
        name: loaf.name
    }
    return loaf
}
"#,
    ),
    (
        "domains/bakery/actions/take_order.jadpo",
        r#"use bakery/entities { Loaf }
use bakery/types { OrderReceipt, OrderRequest }
use bakery/actions { order_loaf }

/// Takes an order at the counter.
action take_order(input: OrderRequest) -> OrderReceipt {
    var loaf: Loaf = order_loaf(input.name)
    return OrderReceipt {
        loaf_id: loaf.id
    }
}
"#,
    ),
    (
        "domains/bakery/actions/morning_loaf.jadpo",
        r#"use bakery/entities { Loaf }
use bakery/events { LoafOrdered }

/// Orders the first loaf of the day.
action morning_loaf() -> Unit {
    var loaf: Loaf = Loaf.create {
        set: {
            name: "morning"
            baked_at: none
            boxed: false
            label: none
        }
    }
    emit LoafOrdered {
        loaf_id: loaf.id
        name: loaf.name
    }
}
"#,
    ),
    (
        "domains/bakery/actions/light_oven.jadpo",
        r#"use bakery/entities { Oven }

/// Lights an oven.
action light_oven() -> Oven {
    return Oven.create {
        set: {
            lit: true
        }
    }
}
"#,
    ),
    (
        "domains/bakery/actions/bake_loaf.jadpo",
        r#"use bakery/entities { Loaf, LoafNotFound }
use bakery/types { LoafName }
use bakery/failures { BadDough, OvenCold }
use bakery/queries { lit_ovens }
use bakery/events { LoafBaked }

/// Bakes a loaf, once an oven is lit.
action bake_loaf(loaf_id: Loaf.id, name: LoafName) fails LoafNotFound, OvenCold, BadDough -> Unit {
    if (name == "brick") {
        reject BadDough {}
    }
    if (list.count(lit_ovens()) == 0) {
        reject OvenCold {}
    }
    var baked: Loaf = attempt Loaf.update(loaf_id) {
        set: {
            baked_at: clock.now
        }
    }
    emit LoafBaked {
        loaf_id: baked.id
    }
}
"#,
    ),
    (
        "domains/bakery/actions/box_loaf.jadpo",
        r#"use bakery/entities { Loaf, LoafNotFound }

/// Boxes a baked loaf.
action box_loaf(loaf_id: Loaf.id) fails LoafNotFound -> Unit {
    attempt Loaf.update(loaf_id) {
        set: {
            boxed: true
        }
    }
}
"#,
    ),
    (
        "domains/bakery/actions/label_plain.jadpo",
        r#"use bakery/entities { Loaf, LoafNotFound }

/// Labels a loaf plain.
action label_plain(loaf_id: Loaf.id) fails LoafNotFound -> Unit {
    attempt Loaf.update(loaf_id) {
        set: {
            label: "plain"
        }
    }
}
"#,
    ),
    (
        "domains/bakery/actions/label_seeded.jadpo",
        r#"use bakery/entities { Loaf, LoafNotFound }

/// Labels a loaf seeded.
action label_seeded(loaf_id: Loaf.id) fails LoafNotFound -> Unit {
    attempt Loaf.update(loaf_id) {
        set: {
            label: "seeded"
        }
    }
}
"#,
    ),
    (
        "domains/bakery/subscribers/bake_ordered.jadpo",
        r#"use bakery/actions { bake_loaf }
use bakery/events { LoafOrdered }

/// Bakes each loaf ordered.
subscriber bake_ordered {
    on: LoafOrdered
    run: bake_loaf(event.loaf_id, event.name)
    retry: {
        attempts: 3
        within: 1h
    }
}
"#,
    ),
    (
        "domains/bakery/subscribers/box_baked.jadpo",
        r#"use bakery/actions { box_loaf }
use bakery/events { LoafBaked }

/// Boxes each loaf baked.
subscriber box_baked {
    on: LoafBaked
    run: box_loaf(event.loaf_id)
}
"#,
    ),
    (
        "domains/bakery/subscribers/label_plain_loaves.jadpo",
        r#"use bakery/actions { label_plain }
use bakery/events { LoafOrdered }

/// Labels each loaf ordered plain.
subscriber label_plain_loaves {
    on: LoafOrdered
    run: label_plain(event.loaf_id)
}
"#,
    ),
    (
        "domains/bakery/subscribers/label_seeded_loaves.jadpo",
        r#"use bakery/actions { label_seeded }
use bakery/events { LoafOrdered }

/// Labels each loaf ordered seeded.
subscriber label_seeded_loaves {
    on: LoafOrdered
    run: label_seeded(event.loaf_id)
}
"#,
    ),
    (
        "domains/bakery/schedules/morning_bake.jadpo",
        r#"use bakery/actions { morning_loaf }

/// Orders the first loaf every morning.
schedule morning_bake {
    every: 1d
    run: morning_loaf()
}
"#,
    ),
    (
        "domains/bakery/routes/loaves.jadpo",
        r#"use bakery/types { OrderReceipt, OrderRequest }
use bakery/actions { take_order }

/// The counter.
routes Loaves {
    prefix: /loaves

    /// Takes an order, baked after the answer.
    POST / {
        authentication: RouteAuthentication.none
        input: OrderRequest
        output: OrderReceipt
        status: SuccessStatus.accepted
        run: take_order(input)
    }
}
"#,
    ),
    (
        "domains/bakery/actions/order_loaf.test.jadpo",
        r#"use bakery/entities { Loaf }
use bakery/actions { light_oven }

test "an ordered loaf is baked and boxed before the next line" {
    clock: "2026-10-20T06:00:00Z"
    assert light_oven().lit
    var loaf: Loaf = order_loaf("sourdough")
    var ready: Loaf = attempt Loaf.get(loaf.id)
    assert ready.baked_at == "2026-10-20T06:00:00Z"
    assert ready.boxed
}

test "a loaf ordered while the oven is cold is baked once it is lit and the clock moves on" {
    clock: "2026-10-20T06:00:00Z"
    var loaf: Loaf = order_loaf("rye")
    var waiting: Loaf = attempt Loaf.get(loaf.id)
    assert waiting.baked_at == none
    assert light_oven().lit
    var still: Loaf = attempt Loaf.get(loaf.id)
    assert still.baked_at == none
    clock.advance(30s)
    var baked: Loaf = attempt Loaf.get(loaf.id)
    assert baked.baked_at == "2026-10-20T06:00:30Z"
    assert baked.boxed
}

test "the seeded label is put on last" {
    assert light_oven().lit
    var loaf: Loaf = order_loaf("bloomer")
    var labelled: Loaf = attempt Loaf.get(loaf.id)
    assert labelled.label == "seeded"
}

test "a brick is never baked" {
    assert light_oven().lit
    var loaf: Loaf = order_loaf("brick")
    assert loaf.name == "brick"
}

test "a loaf in a cold oven is given up on" {
    var loaf: Loaf = order_loaf("spelt")
    clock.advance(30s)
    clock.advance(1m)
    assert loaf.name == "spelt"
}
"#,
    ),
    (
        "domains/bakery/schedules/morning_bake.test.jadpo",
        r#"use bakery/entities { Loaf }
use bakery/actions { light_oven }
use bakery/queries { boxed_loaves }

test "the morning loaf is boxed once the schedule has run" {
    clock: "2026-10-20T05:00:00Z"
    assert light_oven().lit
    morning_bake.run()
    var boxed: List<Loaf> = boxed_loaves()
    assert list.count(boxed) == 1
}
"#,
    ),
    (
        "domains/bakery/routes/loaves.test.jadpo",
        r#"use bakery/entities { Loaf }
use bakery/types { OrderRequest }
use bakery/actions { light_oven }
use bakery/queries { boxed_loaves }

test "an order taken at the counter is boxed once it is accepted" {
    assert light_oven().lit
    var response: Response = request {
        method: HttpMethod.post
        path: "/loaves"
        input: OrderRequest {
            name: "cob"
        }
    }
    assert response.status == 202
    var boxed: List<Loaf> = boxed_loaves()
    assert list.count(boxed) == 1
}
"#,
    ),
];

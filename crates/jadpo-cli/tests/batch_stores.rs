//! Batch runs kept in several stores, against live PostgreSQL (area 12,
//! "Status", "Cancelling" and "Walking records or a provider's list";
//! chunk J2): a shop keeps its items in `main` and its tallies in
//! `tallies`, and walks each with `batches.each`, a page type allowing
//! more than a batch's 1,000 items split into batches; a club's roll call
//! walks a provider's list, its run kept beside the tallies its
//! `finished:` writes. `batches.status` finds a run in either store, and
//! a cancel, kept beside the events in `main` by an action that writes no
//! store, stops a run kept in `tallies` once its runner copies it there.
//!
//! Its tests run interpreted through `jadpo test` here, and compiled with
//! the interpreter's records in the gate's compiled step (`compiled.rs`,
//! "runs kept in several stores").
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
    run(&args, &env.with_cwd(cwd))
}

/// The shop's two stores, the club's provider, and the tests every
/// executor runs.
pub(crate) fn files() -> Vec<(String, String)> {
    FILES
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect()
}

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        tallies: {
            driver: Driver.postgres
        }
        clubs: {
            package: AcmeClubs.v1
            domains: [club]
            environment: config.clubs_environment
        }
    }
    events: {
        store: main
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    clubs_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
"#,
    ),
    (
        "domains/club/actions/call_roll.jadpo",
        r#"use app { clubs }
use club/entities { RollCall }
use club/actions { roll_called, welcome }
use shared/providers/acme_clubs

/// Starts greeting every member of the club, a page at a time.
action call_roll(roll_call_id: RollCall.id) -> BatchRun {
    var asked_for: AcmeClubs.MembersAsked = AcmeClubs.MembersAsked {
        after: none
    }
    return batches.each(
        from: clubs.members.list(asked_for),
        each: member -> welcome(member),
        finished: summary -> roll_called(summary, roll_call_id),
        key: roll_call_id
    )
}
"#,
    ),
    (
        "domains/club/actions/call_roll.test.jadpo",
        r#"use app { clubs }
use club/entities { RollCall }
use club/actions { open_roll_call, roll_progress, stop_roll }

test "a cancel kept beside the events stops a walk kept beside the tallies" {
    fakes: {
        clubs.members.list: [unavailable, unavailable, unavailable]
    }
    var roll_call: RollCall = open_roll_call()
    var club_walk: BatchRun = call_roll(roll_call.id)
    var waiting: BatchProgress = roll_progress(club_walk)
    assert waiting.state == BatchState.running
    stop_roll(club_walk)
    clock.advance(1m)
    var stopped: BatchProgress = roll_progress(club_walk)
    assert stopped.state == BatchState.cancelled
    assert list.map(clubs.members.list.requests, sent -> sent.after) == [none, none, none]
}
"#,
    ),
    (
        "domains/club/actions/open_roll_call.jadpo",
        r#"use club/entities { RollCall }

/// Opens a roll call, its count still to come.
action open_roll_call() -> RollCall {
    return RollCall.create {
        set: {
            members: none
        }
    }
}
"#,
    ),
    (
        "domains/club/actions/roll_called.jadpo",
        r#"use club/entities { RollCall, RollCallNotFound }

/// Keeps how many members the roll call went through.
action roll_called(
    summary: ImportSummary,
    roll_call_id: RollCall.id
) fails RollCallNotFound -> Unit {
    attempt RollCall.update(roll_call_id) {
        set: {
            members: summary.items
        }
    }
}
"#,
    ),
    (
        "domains/club/actions/roll_progress.jadpo",
        r#"/// How far a roll call's walk has got.
action roll_progress(club_walk: BatchRun) -> BatchProgress {
    return batches.status(club_walk)
}
"#,
    ),
    (
        "domains/club/actions/stop_roll.jadpo",
        r#"/// Stops a roll call's walk after the page in hand.
action stop_roll(club_walk: BatchRun) -> Unit {
    batches.cancel(club_walk)
}
"#,
    ),
    (
        "domains/club/actions/welcome.jadpo",
        r#"use shared/providers/acme_clubs

/// Greets one member.
action welcome(member: AcmeClubs.Member) -> Unit {
    assert member.id == member.id
}
"#,
    ),
    (
        "domains/club/entities/roll_call.jadpo",
        r#"/// One walk through the club's members, kept beside the tallies.
entity RollCall {
    id: Uuid
    members: Int?

    store: tallies

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}
"#,
    ),
    (
        "domains/shop/actions/add_item.jadpo",
        r#"use shop/entities { Item }

/// Adds one, not seen yet.
action add_item() -> Item {
    return Item.create {
        set: {
            seen: false
        }
    }
}
"#,
    ),
    (
        "domains/shop/actions/add_tally.jadpo",
        r#"use shop/entities { Tally }

/// Adds one, not seen yet.
action add_tally() -> Tally {
    return Tally.create {
        set: {
            seen: false
        }
    }
}
"#,
    ),
    (
        "domains/shop/actions/see_item.jadpo",
        r#"use shop/entities { Item }

/// Marks one seen.
action see_item(item: Item) -> Unit {
    var seen: Item = Item.update(item) {
        set: {
            seen: true
        }
    }
    assert seen.seen
}
"#,
    ),
    (
        "domains/shop/actions/see_items.jadpo",
        r#"use shop/types { ItemPage }
use shop/actions { see_item }
use shop/queries { items }

/// Starts marking every one seen.
action see_items(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> see_item(item))
}
"#,
    ),
    (
        "domains/shop/actions/see_items.test.jadpo",
        r#"use shop/entities { Item, Tally }
use shop/types { ItemPage }
use shop/actions { add_item, add_tally, see_tallys, shop_progress }

test "a batch run in either store is found, its page split into batches" {
    var first: Item = add_item()
    var second: Item = add_item()
    var third: Item = add_item()
    var tally: Tally = add_tally()
    var items: BatchRun = see_items(ItemPage {})
    var tallies: BatchRun = see_tallys(ItemPage {})
    var done: BatchProgress = shop_progress(items)
    assert done.items_done == 3
    assert done.state == BatchState.finished
    var roll_called: BatchProgress = shop_progress(tallies)
    assert roll_called.items_done == 1
    assert roll_called.state == BatchState.finished
    assert first.id != second.id and second.id != third.id and tally.id == tally.id
}
"#,
    ),
    (
        "domains/shop/actions/see_tally.jadpo",
        r#"use shop/entities { Tally }

/// Marks one seen.
action see_tally(tally: Tally) -> Unit {
    var seen: Tally = Tally.update(tally) {
        set: {
            seen: true
        }
    }
    assert seen.seen
}
"#,
    ),
    (
        "domains/shop/actions/see_tallys.jadpo",
        r#"use shop/types { ItemPage }
use shop/actions { see_tally }
use shop/queries { tallys }

/// Starts marking every one seen.
action see_tallys(page: ItemPage) -> BatchRun {
    return batches.each(from: tallys(page), each: tally -> see_tally(tally))
}
"#,
    ),
    (
        "domains/shop/actions/shop_progress.jadpo",
        r#"/// How far a batch run has got.
action shop_progress(club_walk: BatchRun) -> BatchProgress {
    return batches.status(club_walk)
}
"#,
    ),
    (
        "domains/shop/entities/item.jadpo",
        r#"/// One thing a batch run marks seen.
entity Item {
    id: Uuid
    seen: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}
"#,
    ),
    (
        "domains/shop/entities/tally.jadpo",
        r#"/// One thing a batch run marks seen.
entity Tally {
    id: Uuid
    seen: Bool

    store: tallies

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}
"#,
    ),
    (
        "domains/shop/queries/items.jadpo",
        r#"use shop/entities { Item }
use shop/types { ItemPage }

/// Every one, a page at a time.
query items(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
"#,
    ),
    (
        "domains/shop/queries/tallys.jadpo",
        r#"use shop/entities { Tally }
use shop/types { ItemPage }

/// Every one, a page at a time.
query tallys(page: ItemPage) -> Page<Tally> {
    from: Tally
    order: [Tally.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
"#,
    ),
    (
        "domains/shop/types/big_page.jadpo",
        r#"/// A page size larger than one batch holds.
type BigPage = Int {
    min: 1
    max: 5000
}
"#,
    ),
    (
        "domains/shop/types/item_page.jadpo",
        r#"use shop/types { BigPage }

/// Where a batch run has got to.
type ItemPage = Input {
    page_size: BigPage {
        default: 1500
    }
    after: PageCursor {
        optional: true
    }
}
"#,
    ),
    (
        "shared/providers/acme_clubs/v1/package.jadpo",
        r#"/// Acme Clubs' membership API.
package AcmeClubs.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
"#,
    ),
    (
        "shared/providers/acme_clubs/v1/provider.jadpo",
        r#"/// Acme Clubs' membership API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-clubs.example"
        sandbox: "https://sandbox.acme-clubs.example"
    }
    error_code: "error.code"
}
"#,
    ),
    (
        "shared/providers/acme_clubs/v1/resources/members.jadpo",
        r#"public resource members {
    list: {
        method: HttpMethod.get
        path: "/v1/members"
        input: MembersAsked
        output: Page<Member>
        changes: false
        source: PackageSource.written
        docs: "https://docs.acme-clubs.example/members"
    }
}
"#,
    ),
    (
        "shared/providers/acme_clubs/v1/types/member.jadpo",
        r#"/// One member of the club.
public type Member = Object {
    id: MemberId
}
"#,
    ),
    (
        "shared/providers/acme_clubs/v1/types/member_id.jadpo",
        r#"/// The club's reference for a member.
public type MemberId = Text {
    max_length: 40
}
"#,
    ),
    (
        "shared/providers/acme_clubs/v1/types/members_asked.jadpo",
        r#"/// Which page of the club's members to list.
public type MembersAsked = Object {
    after: PageCursor?
}
"#,
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("batch-stores-j2-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files() {
        write(&root, &path, &text);
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    root
}

#[test]
fn runs_in_several_stores_pass_their_tests() {
    let root = project("tests");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    if !jadpo_test::live() {
        return;
    }
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("2 passed"), "{}", tested.stdout);
}

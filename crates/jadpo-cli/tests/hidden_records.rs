//! A query that sees what a lifecycle hides, against live PostgreSQL (area
//! 6, "Records a lifecycle hides"; chunk 10.11d), through `jadpo test`:
//! `include_hidden: true` on a query finds the tickets in the bin as well
//! as the open ones, and a query without it doesn't; a bin view is that
//! with a condition on the field the lifecycle sets; `not exists(...,
//! include_hidden: true)` is a delete guard that counts binned tickets, and
//! the `restricted` reference from a binned ticket blocks the delete too;
//! the ticket's read policy still hides another person's binned ticket from
//! the opt-in; an aggregate counts the bin. A person erasure kept a record
//! of stays out of a query saying `include_hidden: true`, which reaches
//! lifecycle hiding only.
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

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use desk/entities { User }

application {}

principal {
    user: {
        subject: User.handle
    }
}
"#,
    ),
    (
        "domains/desk/entities/user.jadpo",
        r#"type Handle = Text {
    min_length: 1
    max_length: 40
}

/// Someone who works the desk.
entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
    }
}
"#,
    ),
    (
        "domains/desk/entities/customer.jadpo",
        r#"type CustomerName = Text {
    min_length: 1
    max_length: 80
}

/// Someone the desk answers.
entity Customer {
    id: Uuid
    name: CustomerName

    policy: {
        create: [Access.authenticated]
        read: [Access.authenticated]
        delete: [Access.authenticated]
    }
}
"#,
    ),
    (
        "domains/desk/entities/ticket.jadpo",
        r#"use desk/entities { Customer, User }

type TicketTitle = Text {
    min_length: 1
    max_length: 200
}

/// Something a customer asked for, binned for 30 days before it goes.
entity Ticket {
    id: Uuid
    title: TicketTitle
    customer_id: Customer.id {
        on_delete: OnDelete.restricted
    }
    owner_id: User.id {
        on_delete: OnDelete.restricted
    }
    binned_at: Instant?

    lifecycle: {
        visible: binned_at == none
        transitions: {
            delete: {
                when: binned_at == none
                set: {
                    binned_at: clock.now
                }
            }
            restore: {
                reaches: hidden
                set: {
                    binned_at: none
                }
            }
        }
        purge: {
            after: 30d
            since: binned_at
        }
    }

    policy: {
        create: [Access.authenticated]
        read: [owner_id]
        update: [owner_id]
        delete: [owner_id]
    }
}
"#,
    ),
    (
        "domains/desk/failures/customer_has_tickets.jadpo",
        r#"/// The customer still has tickets, binned ones included.
type CustomerHasTickets = Failure {
    kind: FailureKind.conflict
}
"#,
    ),
    (
        "domains/desk/types/ticket_view.jadpo",
        r#"use desk/entities { Ticket }

/// A ticket as a list shows it.
type TicketView = Output {
    pick: [Ticket.id, Ticket.title]
}
"#,
    ),
    (
        "domains/desk/types/customer_row.jadpo",
        r#"use desk/entities { Customer }

/// A customer as a list shows them.
type CustomerRow = Output {
    pick: [Customer.id, Customer.name]
}
"#,
    ),
    (
        "domains/desk/types/ticket_count.jadpo",
        r#"/// How many tickets there are.
type TicketCount = Output {
    tickets: Int
}
"#,
    ),
    (
        "domains/desk/queries/open_tickets.jadpo",
        r#"use desk/entities { Ticket }
use desk/types { TicketView }

/// The caller's tickets that aren't in the bin.
query open_tickets(caller: Principal.user) -> List<TicketView> {
    from: Ticket
    order: [Ticket.title asc]
    limit: 50
}
"#,
    ),
    (
        "domains/desk/queries/every_ticket.jadpo",
        r#"use desk/entities { Ticket }
use desk/types { TicketView }

/// Every ticket the caller may read, binned ones included.
query every_ticket(caller: Principal.user) -> List<TicketView> {
    from: Ticket
    order: [Ticket.title asc]
    limit: 50
    include_hidden: true
}
"#,
    ),
    (
        "domains/desk/queries/bin.jadpo",
        r#"use desk/entities { Ticket }
use desk/types { TicketView }

/// The caller's tickets in the bin, which a restore can bring back.
query bin(caller: Principal.user) -> List<TicketView> {
    from: Ticket
    where: Ticket.binned_at != none
    order: [Ticket.title asc]
    limit: 50
    include_hidden: true
}
"#,
    ),
    (
        "domains/desk/queries/ticketless.jadpo",
        r#"use desk/entities { Customer, Ticket }
use desk/types { CustomerRow }

/// The customer, while they have no tickets at all, binned ones included.
query ticketless(customer_id: Customer.id) -> CustomerRow? {
    from: Customer
    where: Customer.id == customer_id
    where: not exists(Ticket.customer_id, include_hidden: true)
}
"#,
    ),
    (
        "domains/desk/queries/no_open_tickets.jadpo",
        r#"use desk/entities { Customer, Ticket }
use desk/types { CustomerRow }

/// The customer, while none of their tickets is open.
query no_open_tickets(customer_id: Customer.id) -> CustomerRow? {
    from: Customer
    where: Customer.id == customer_id
    where: not exists(Ticket.customer_id)
}
"#,
    ),
    (
        "domains/desk/queries/binned_count.jadpo",
        r#"use desk/entities { Ticket }
use desk/types { TicketCount }

/// How many tickets are in the bin, everyone's.
query binned_count() -> TicketCount {
    from: Ticket
    where: Ticket.binned_at != none
    aggregate: {
        tickets: count()
    }
    include_hidden: true
}
"#,
    ),
    (
        "domains/desk/actions/bin_ticket.jadpo",
        r#"use desk/entities { Ticket, TicketNotFound }

/// Moves a ticket to the bin.
action bin_ticket(ticket_id: Ticket.id) fails TicketNotFound -> Unit {
    attempt Ticket.delete(ticket_id)
}
"#,
    ),
    (
        "domains/desk/actions/remove_customer.jadpo",
        r#"use desk/entities { Customer, CustomerNotFound, Ticket }
use desk/failures { CustomerHasTickets }

/// Deletes a customer who has no tickets, binned ones included.
action remove_customer(customer_id: Customer.id) fails CustomerNotFound, CustomerHasTickets -> Unit {
    var customer: Customer = attempt Customer.get(customer_id)
    attempt Customer.delete(customer) {
        conflict: {
            Ticket.customer_id: CustomerHasTickets
        }
    }
}
"#,
    ),
];

const TESTS: &str = r#"use desk/entities { Customer, Ticket, User }
use desk/actions { bin_ticket, remove_customer }
use desk/failures { CustomerHasTickets }
use desk/queries { binned_count, every_ticket, no_open_tickets, open_tickets, ticketless }
use desk/types { CustomerRow, TicketCount }

/// A ticket of `customer`'s that `owner` holds.
fixture held_ticket(customer: Customer, owner: User, title: Ticket.title) -> Ticket {
    return make.Ticket {
        title: title
        customer_id: customer.id
        owner_id: owner.id
    }
}

test "a query saying include_hidden sees the bin, and one without doesn't" {
    var ann: User = make.User { handle: "ann" }
    var acme: Customer = make.Customer { name: "Acme" }
    held_ticket(acme, ann, "a")
    var binned: Ticket = held_ticket(acme, ann, "b")
    attempt bin_ticket(binned.id)
    assert list.map(open_tickets(Principal.user(ann)), t -> t.title) == ["a"]
    assert list.map(every_ticket(Principal.user(ann)), t -> t.title) == ["a", "b"]
    assert list.map(bin(Principal.user(ann)), t -> t.title) == ["b"]
}

test "the read policy still hides another person's binned ticket" {
    var ann: User = make.User { handle: "ann" }
    var bob: User = make.User { handle: "bob" }
    var acme: Customer = make.Customer { name: "Acme" }
    held_ticket(acme, ann, "a")
    var bobs: Ticket = held_ticket(acme, bob, "c")
    attempt bin_ticket(bobs.id)
    assert list.map(every_ticket(Principal.user(ann)), t -> t.title) == ["a"]
    assert list.map(bin(Principal.user(ann)), t -> t.title) == []
    assert list.map(bin(Principal.user(bob)), t -> t.title) == ["c"]
}

test "a customer with only binned tickets still has tickets" {
    var ann: User = make.User { handle: "ann" }
    var acme: Customer = make.Customer { name: "Acme" }
    var globex: Customer = make.Customer { name: "Globex" }
    var binned: Ticket = held_ticket(acme, ann, "a")
    attempt bin_ticket(binned.id)
    assert ticketless(acme.id) == none
    var open: CustomerRow? = no_open_tickets(acme.id)
    assert open != none
    var bare: CustomerRow? = ticketless(globex.id)
    assert bare != none
    assert remove_customer(acme.id) fails CustomerHasTickets
    attempt remove_customer(globex.id)
}

test "an aggregate saying include_hidden counts the bin" {
    var ann: User = make.User { handle: "ann" }
    var bob: User = make.User { handle: "bob" }
    var acme: Customer = make.Customer { name: "Acme" }
    held_ticket(acme, ann, "a")
    var anns: Ticket = held_ticket(acme, ann, "b")
    var bobs: Ticket = held_ticket(acme, bob, "c")
    attempt bin_ticket(anns.id)
    attempt bin_ticket(bobs.id)
    var counted: TicketCount = binned_count()
    assert counted.tickets == 2
}
"#;

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("hidden-records-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    write(&root, "domains/desk/queries/bin.test.jadpo", TESTS);
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

fn replaced(root: &Path, file: &str, old: &str, new: &str) {
    let text = fs::read_to_string(root.join(file)).expect("read");
    assert!(text.contains(old), "{file}: {old}");
    write(root, file, &text.replacen(old, new, 1));
}

/// The statements: the query saying `include_hidden: true`, and the
/// related records `exists` saying it, leave the lifecycle's `visible:`
/// out and keep the policy; the others keep both. The index the query
/// needs holds every record, not only the visible ones.
#[test]
fn include_hidden_leaves_out_visible_and_keeps_policy() {
    let root = project("sql");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let open = jadpo(&root, &["sql", "open_tickets"]);
    assert_eq!(open.code, 0, "{}{}", open.stdout, open.stderr);
    assert!(
        open.stdout.contains(r#""binned_at" IS NULL"#),
        "{}",
        open.stdout
    );
    let every = jadpo(&root, &["sql", "every_ticket"]);
    assert_eq!(every.code, 0, "{}{}", every.stdout, every.stderr);
    assert!(!every.stdout.contains("binned_at"), "{}", every.stdout);
    assert!(
        every.stdout.contains(r#""owner_id" = $1"#),
        "{}",
        every.stdout
    );
    let guard = jadpo(&root, &["sql", "ticketless"]);
    assert_eq!(guard.code, 0, "{}{}", guard.stdout, guard.stderr);
    assert!(
        guard.stdout.contains(r#"NOT EXISTS (SELECT 1 FROM "ticket" WHERE "ticket"."customer_id" = "customer"."id" AND "ticket"."owner_id" = $2)"#),
        "{}",
        guard.stdout
    );
    let open_guard = jadpo(&root, &["sql", "no_open_tickets"]);
    assert!(
        open_guard.stdout.contains(r#""binned_at" IS NULL"#),
        "{}",
        open_guard.stdout
    );
    let migration = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.is_dir())
        .expect("a migration");
    let text = fs::read_to_string(migration.join("postgres.sql")).expect("postgres.sql");
    let index = text
        .lines()
        .find(|line| line.contains(r#"ON "ticket" ("title" COLLATE "C", "id")"#))
        .unwrap_or_else(|| panic!("no index for every_ticket:\n{text}"));
    assert!(!index.contains("WHERE"), "{index}");
}

#[test]
fn include_hidden_passes_its_tests() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("tests");
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("4 passed"), "{}", tested.stdout);
}

/// `include_hidden:` takes only `true`, and only where a lifecycle hides
/// something.
#[test]
fn include_hidden_is_checked() {
    let root = project("false");
    replaced(
        &root,
        "domains/desk/queries/every_ticket.jadpo",
        "include_hidden: true",
        "include_hidden: false",
    );
    let checked = jadpo(&root, &["check"]);
    assert_ne!(checked.code, 0, "{}", checked.stdout);
    assert!(
        checked
            .stdout
            .contains("`include_hidden:` takes only `true`"),
        "{}",
        checked.stdout
    );

    // `include_erased:` reaching for the bin names `include_hidden:`.
    let root = project("erased-for-bin");
    replaced(
        &root,
        "domains/desk/queries/every_ticket.jadpo",
        "include_hidden: true",
        "include_erased: true",
    );
    let checked = jadpo(&root, &["check"]);
    assert_ne!(checked.code, 0, "{}", checked.stdout);
    assert!(
        checked.stdout.contains(
            "next: Write `include_hidden: true` in its place to read the records its lifecycle hides, or remove the line"
        ),
        "{}",
        checked.stdout
    );

    let root = project("unused");
    write(
        &root,
        "domains/desk/queries/every_customer.jadpo",
        "use desk/entities { Customer }\nuse desk/types { CustomerRow }\n\n/// Every customer.\nquery every_customer() -> List<CustomerRow> {\n    from: Customer\n    order: [Customer.name asc]\n    limit: 50\n    include_hidden: true\n}\n",
    );
    let checked = jadpo(&root, &["check"]);
    assert_ne!(checked.code, 0, "{}", checked.stdout);
    assert!(
        checked.stdout.contains(
            "error: `every_customer` includes hidden records, and no lifecycle hides a `Customer`"
        ),
        "{}",
        checked.stdout
    );
    assert!(
        checked.stdout.contains("rule: query.include_hidden_unused"),
        "{}",
        checked.stdout
    );

    let root = project("unused-related");
    write(
        &root,
        "domains/desk/queries/with_open_tickets.jadpo",
        "use desk/entities { Customer, Ticket }\nuse desk/types { TicketView }\n\n/// The tickets of a customer who is there.\nquery with_customer(caller: Principal.user) -> List<TicketView> {\n    from: Ticket\n    where: exists(Ticket.customer_id, include_hidden: true)\n    order: [Ticket.title asc]\n    limit: 50\n}\n",
    );
    let checked = jadpo(&root, &["check"]);
    assert_ne!(checked.code, 0, "{}", checked.stdout);
    assert!(
        checked.stdout.contains("rule: query.include_hidden_unused"),
        "{}",
        checked.stdout
    );
}

/// The words an agent reaches for first name `include_hidden:` as the fix.
#[test]
fn a_guess_at_seeing_the_bin_points_at_include_hidden() {
    for guess in ["include_deleted", "include_binned", "with_deleted"] {
        let root = project(&format!("guess-{guess}"));
        replaced(
            &root,
            "domains/desk/queries/every_ticket.jadpo",
            "include_hidden: true",
            &format!("{guess}: true"),
        );
        let checked = jadpo(&root, &["check"]);
        assert_ne!(checked.code, 0, "{}", checked.stdout);
        assert!(
            checked.stdout.contains("rule: query.unknown_section"),
            "{}",
            checked.stdout
        );
        assert!(
            checked
                .stdout
                .contains("next: Use `include_hidden:` (agent)"),
            "{guess}: {}",
            checked.stdout
        );
    }
}

/// `jadpo explain` says which queries see what a lifecycle hides, beside
/// the entity's lifecycle.
#[test]
fn explain_names_the_queries_that_see_hidden_records() {
    let root = project("explain");
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    let explained = jadpo(&root, &["explain"]);
    assert_eq!(
        explained.code, 0,
        "{}{}",
        explained.stdout, explained.stderr
    );
    assert!(
        explained
            .stdout
            .contains("read by bin, binned_count, every_ticket, ticketless"),
        "{}",
        explained.stdout
    );
}

/// A person erasure kept a record of stays out of a query that says
/// `include_hidden: true`: it reaches lifecycle hiding, and only
/// `include_erased: true` reaches what erasure keeps.
#[test]
fn include_hidden_never_reaches_an_erased_person() {
    if !jadpo_test::live() {
        return;
    }
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hidden-records-erased");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in crate::erasure::kept_files() {
        let text = if path == "domains/cover/entities/customer.jadpo" {
            let text = text.replacen(
                "    plan: Line\n",
                "    plan: Line\n    left_at: Instant?\n",
                1,
            );
            text.replacen(
                "    policy: {\n",
                "    lifecycle: {\n        visible: left_at == none\n        transitions: {\n            leave: {\n                when: left_at == none\n                set: {\n                    left_at: clock.now\n                }\n            }\n        }\n    }\n\n    policy: {\n",
                1,
            )
        } else if path == "domains/cover/actions/forget.test.jadpo" {
            ERASED_TESTS.to_string()
        } else {
            text
        };
        write(&root, &path, &text);
    }
    write(
        &root,
        "domains/cover/actions/leave.jadpo",
        "use cover/entities { Customer, CustomerNotFound }\n\n/// A customer leaves, and is hidden from then on.\naction leave(customer_id: Customer.id) fails CustomerNotFound -> Unit {\n    attempt Customer.leave(customer_id)\n}\n",
    );
    write(
        &root,
        "domains/cover/queries/every_customer.jadpo",
        "use cover/entities { Customer }\nuse cover/types { CustomerLine }\n\n/// Every customer, those who have left included.\nquery every_customer() -> List<CustomerLine> {\n    from: Customer\n    order: [Customer.id asc]\n    limit: 10\n    include_hidden: true\n}\n",
    );
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    let sql = jadpo(&root, &["sql", "every_customer"]);
    assert_eq!(sql.code, 0, "{}{}", sql.stdout, sql.stderr);
    assert!(!sql.stdout.contains("left_at"), "{}", sql.stdout);
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(
        tested.stdout.contains("4 passed, 1 action test"),
        "{}",
        tested.stdout
    );
}

const ERASED_TESTS: &str = r#"use cover/entities { Customer }
use cover/actions { leave, take_out }
use cover/queries { customers, every_customer }

test "include_hidden finds a customer who left, and never an erased one" {
    var ada: Customer = attempt take_out("ada", "Ada Lovelace", "P-100")
    var bob: Customer = attempt take_out("bob", "Bob Babbage", "P-200")
    attempt take_out("cy", "Cy Young", "P-300")
    attempt leave(bob.id)
    attempt forget(ada.id)
    assert list.map(customers(), line -> line.full_name) == ["Cy Young"]
    var every: List<Customer.id> = list.map(every_customer(), line -> line.id)
    assert list.contains(every, bob.id)
    assert not list.contains(every, ada.id)
    assert list.count(every) == 2
}
"#;

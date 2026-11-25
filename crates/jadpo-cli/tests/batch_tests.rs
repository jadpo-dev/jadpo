//! Batches in `jadpo test`, end to end (area 13, "Batches"; area 12,
//! "Which tenant a run reaches"; chunk 10.7d): a help desk raises its VIP
//! tickets. A batch an action starts with its desk saved runs to its end
//! before the test's next line, within that desk alone, and a schedule's
//! own action reaches every desk. A batch naming no desk would reach no
//! desk's records, and quietly walk nothing, so `jadpo check` refuses it
//! before any test runs.
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
        r#"use support/entities { Agent }

application {}

principal {
    user: {
        subject: Agent.handle
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    browser_session: {
        cookie: "desk_session"
    }
}
"#,
    ),
    (
        "domains/support/actions/raise.jadpo",
        r#"use support/entities { Ticket, TicketNotFound }
use support/types { Priority }

/// Makes a ticket urgent.
action raise(ticket: Ticket) fails TicketNotFound -> Unit {
    attempt Ticket.update(ticket.id) {
        set: {
            priority: Priority.urgent
        }
    }
}
"#,
    ),
    (
        "domains/support/actions/raise_desk.jadpo",
        r#"use support/entities { Desk }
use support/types { TicketPage }
use support/actions { raise }
use support/queries { vip_tickets }

/// Makes every VIP ticket of a desk urgent, and keeps the run on the desk.
action raise_desk(help_desk: Desk) -> Desk {
    var raising: BatchRun = batches.each(
        from: vip_tickets(help_desk.id, TicketPage {}),
        each: ticket -> raise(ticket),
        key: help_desk.id
    )
    return Desk.update(help_desk) {
        set: {
            raising: raising
        }
    }
}
"#,
    ),
    (
        "domains/support/actions/raise_desk.test.jadpo",
        r#"use support/entities { Desk, Ticket }
use support/types { Priority }

test "a desk's VIP tickets are raised, and no other desk's" {
    var ash: Desk = make.Desk {
        name: "Ash"
    }
    var birch: Desk = make.Desk {
        name: "Birch"
    }
    var ash_vip: Ticket = make.Ticket {
        desk_id: ash.id
        vip: true
        priority: Priority.normal
    }
    var ash_plain: Ticket = make.Ticket {
        desk_id: ash.id
        vip: false
        priority: Priority.normal
    }
    var birch_vip: Ticket = make.Ticket {
        desk_id: birch.id
        vip: true
        priority: Priority.normal
    }
    var raised_desk: Desk = raise_desk(ash)
    assert raised_desk.raising != none
    var raised: Ticket = attempt Ticket.get(ash_vip.id)
    assert raised.priority == Priority.urgent
    var plain: Ticket = attempt Ticket.get(ash_plain.id)
    assert plain.priority == Priority.normal
    var other: Ticket = attempt Ticket.get(birch_vip.id)
    assert other.priority == Priority.normal
}
"#,
    ),
    (
        "domains/support/actions/raise_every_desk.jadpo",
        r#"use support/entities { Ticket }
use support/types { Priority }
use support/queries { every_vip_ticket }

/// Makes every desk's VIP tickets urgent: a schedule's run reaches every
/// desk.
action raise_every_desk() -> Unit {
    var tickets: List<Ticket> = every_vip_ticket()
    for ticket in tickets {
        var raised: Ticket = Ticket.update(ticket) {
            set: {
                priority: Priority.urgent
            }
        }
        assert raised.priority == Priority.urgent
    }
}
"#,
    ),
    (
        "domains/support/entities/agent.jadpo",
        r#"use support/types { Handle }

/// Someone who signs in.
entity Agent {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
        tenant: Tenancy.shared
    }
}
"#,
    ),
    (
        "domains/support/entities/desk.jadpo",
        r#"use support/types { DeskRole, Subject }

/// A customer's help desk.
entity Desk {
    id: Uuid
    name: Subject
    raising: BatchRun?

    policy: {
        read: [DeskRole.member, Access.system]
        update: [DeskRole.member]
    }
}
"#,
    ),
    (
        "domains/support/entities/desk_membership.jadpo",
        r#"use support/entities { Agent, Desk }
use support/types { DeskRole }

/// Who works on which desk.
entity DeskMembership {
    id: Uuid
    desk_id: Desk.id {
        on_delete: OnDelete.cascade
    }
    agent_id: Agent.id {
        on_delete: OnDelete.cascade
    }
    role: DeskRole

    membership: {
        scope: desk_id
        member: agent_id
        role: role
    }

    policy: {
        read: [agent_id]
    }
}
"#,
    ),
    (
        "domains/support/entities/ticket.jadpo",
        r#"use support/entities { Desk }
use support/types { DeskRole, Priority, Subject }

/// A customer's question.
entity Ticket {
    id: Uuid
    desk_id: Desk.id {
        on_delete: OnDelete.cascade
    }
    subject: Subject
    vip: Bool
    priority: Priority

    policy: {
        create: [DeskRole.member]
        read: [DeskRole.member, Access.system]
        update: [Access.system]
    }
}
"#,
    ),
    (
        "domains/support/queries/every_vip_ticket.jadpo",
        r#"use support/entities { Ticket }

/// Every desk's VIP tickets.
query every_vip_ticket() -> List<Ticket> {
    from: Ticket
    where: Ticket.vip == true
    order: [Ticket.id asc]
    limit: every
}
"#,
    ),
    (
        "domains/support/queries/vip_tickets.jadpo",
        r#"use support/entities { Desk, Ticket }
use support/types { TicketPage }

/// A desk's VIP tickets, a page at a time.
query vip_tickets(desk_id: Desk.id, page: TicketPage) -> Page<Ticket> {
    from: Ticket
    where: Ticket.desk_id == desk_id and Ticket.vip == true
    order: [Ticket.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
"#,
    ),
    (
        "domains/support/schedules/vip_first.jadpo",
        r#"use support/actions { raise_every_desk }

/// Every hour, VIP tickets first.
schedule vip_first {
    every: 1h
    run: raise_every_desk()
}
"#,
    ),
    (
        "domains/support/schedules/vip_first.test.jadpo",
        r#"use support/entities { Desk, Ticket }
use support/types { Priority }

test "every desk's VIP tickets are raised each hour" {
    var ash: Desk = make.Desk {
        name: "Ash"
    }
    var birch: Desk = make.Desk {
        name: "Birch"
    }
    var ash_vip: Ticket = make.Ticket {
        desk_id: ash.id
        vip: true
        priority: Priority.normal
    }
    var birch_vip: Ticket = make.Ticket {
        desk_id: birch.id
        vip: true
        priority: Priority.normal
    }
    vip_first.run()
    var ash_raised: Ticket = attempt Ticket.get(ash_vip.id)
    assert ash_raised.priority == Priority.urgent
    var birch_raised: Ticket = attempt Ticket.get(birch_vip.id)
    assert birch_raised.priority == Priority.urgent
}
"#,
    ),
    (
        "domains/support/types/desk_role.jadpo",
        r#"/// What an agent may do on a desk.
type DeskRole = Enum {
    member
}
"#,
    ),
    (
        "domains/support/types/handle.jadpo",
        r#"type Handle = Text {
    min_length: 3
    max_length: 40
}
"#,
    ),
    (
        "domains/support/types/priority.jadpo",
        r#"/// How soon a ticket is answered.
type Priority = Enum {
    normal
    urgent
}
"#,
    ),
    (
        "domains/support/types/subject.jadpo",
        r#"type Subject = Text {
    min_length: 1
    max_length: 80
}
"#,
    ),
    (
        "domains/support/types/ticket_page.jadpo",
        r#"type TicketPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}
"#,
    ),
    (
        "policy.jadpo",
        r#"use support/entities { Desk }

policy {
    tenant: Desk
}
"#,
    ),
];

/// A batch that names no desk: the helpdesk change's hourly job.
const UNNAMED: &str = r#"use support/actions { raise }
use support/queries { vip_tickets_anywhere }
use support/types { TicketPage }

/// Makes every VIP ticket urgent, naming no desk.
action raise_unnamed() -> BatchRun {
    return batches.each(from: vip_tickets_anywhere(TicketPage {}), each: ticket -> raise(ticket))
}
"#;

const ANYWHERE: &str = r#"use support/entities { Ticket }
use support/types { TicketPage }

/// Every desk's VIP tickets, a page at a time.
query vip_tickets_anywhere(page: TicketPage) -> Page<Ticket> {
    from: Ticket
    where: Ticket.vip == true
    order: [Ticket.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
"#;

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("batch-tests-10-7d-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    root
}

#[test]
fn a_batch_the_code_under_test_starts_runs_before_the_next_line() {
    let root = project("runs");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    if !jadpo_test::live() {
        return;
    }
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("2 passed"), "{}", tested.stdout);
}

#[test]
fn a_batch_naming_no_desk_is_refused_before_a_test_runs() {
    let root = project("unnamed");
    write(
        &root,
        "domains/support/actions/raise_unnamed.jadpo",
        UNNAMED,
    );
    write(
        &root,
        "domains/support/queries/vip_tickets_anywhere.jadpo",
        ANYWHERE,
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}{}", checked.stdout, checked.stderr);
    assert!(
        checked
            .stdout
            .contains("error: `raise_unnamed` starts a batch that names no `Desk`"),
        "{}",
        checked.stdout
    );
    assert!(
        checked.stdout.contains("rule: policy.batch_tenant_missing"),
        "{}",
        checked.stdout
    );
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 1, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("tests not run"), "{}", tested.stdout);
}

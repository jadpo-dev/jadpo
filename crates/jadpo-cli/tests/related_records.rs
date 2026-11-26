//! A condition on a related record against live PostgreSQL (area 6,
//! "Filtering by a related record"; chunk 10.11c), through `jadpo test`: a
//! page of the tickets with a tag, where the tag is a row of the link
//! entity `TicketTag`, in order and across pages; the related entity's
//! `visible:` and policy hiding links from the filter, so each caller's
//! page holds only the tickets they tagged; `not exists` for the tickets
//! the caller hasn't tagged; and an aggregate count with no caller, which
//! runs without policy but still under `visible:`.
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
        "domains/desk/entities/tag.jadpo",
        r#"type TagName = Text {
    min_length: 1
    max_length: 40
}

/// A label tickets are sorted by.
entity Tag {
    id: Uuid
    name: TagName

    policy: {
        read: [Access.authenticated]
    }
}
"#,
    ),
    (
        "domains/desk/entities/ticket.jadpo",
        r#"type TicketTitle = Text {
    min_length: 1
    max_length: 200
}

/// Something someone asked for.
entity Ticket {
    id: Uuid
    title: TicketTitle

    policy: {
        read: [Access.authenticated]
    }
}
"#,
    ),
    (
        "domains/desk/entities/ticket_tag.jadpo",
        r#"use desk/entities { Tag, Ticket, User }

/// A tag someone put on a ticket, seen only by them, until they take it off.
entity TicketTag {
    id: Uuid
    ticket_id: Ticket.id {
        on_delete: OnDelete.cascade
    }
    tag_id: Tag.id {
        on_delete: OnDelete.cascade
    }
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    removed_at: Instant?

    lifecycle: {
        visible: removed_at == none
    }

    policy: {
        read: [owner_id]
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
        "domains/desk/queries/count_tagged.jadpo",
        r#"use desk/entities { Tag, Ticket, TicketTag }
use desk/types { TagCount }

/// Counts the tickets with `tag_id`.
query count_tagged(tag_id: Tag.id) -> TagCount {
    from: Ticket
    where: exists(TicketTag.ticket_id, TicketTag.tag_id == tag_id)
    aggregate: {
        tickets: count()
    }
}
"#,
    ),
    (
        "domains/desk/queries/tagged.jadpo",
        r#"use desk/entities { Tag, Ticket, TicketTag }
use desk/types { TicketView }

/// The tickets the caller tagged with `tag_id`, a page at a time, by title.
query tagged(tag_id: Tag.id, after: PageCursor?, caller: Principal.user) -> Page<TicketView> {
    from: Ticket
    where: exists(TicketTag.ticket_id, TicketTag.tag_id == tag_id)
    order: [Ticket.title asc]
    page: {
        size: 2
        after: after
    }
}
"#,
    ),
    (
        "domains/desk/queries/untagged.jadpo",
        r#"use desk/entities { Ticket, TicketTag }
use desk/types { TicketView }

/// The tickets the caller hasn't tagged at all.
query untagged(caller: Principal.user) -> List<TicketView> {
    from: Ticket
    where: not exists(TicketTag.ticket_id)
    order: [Ticket.title asc]
    limit: 50
}
"#,
    ),
    (
        "domains/desk/types/tag_count.jadpo",
        r#"/// How many tickets carry a tag.
type TagCount = Output {
    tickets: Int
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
];

const TESTS: &str = r#"use desk/entities { Tag, Ticket, TicketTag, User }
use desk/queries { count_tagged, untagged }
use desk/types { TagCount, TicketView }

/// `ticket` tagged `tag` by `owner`.
fixture tagging(ticket: Ticket, tag: Tag, owner: User) -> TicketTag {
    return make.TicketTag {
        ticket_id: ticket.id
        tag_id: tag.id
        owner_id: owner.id
    }
}

/// `ticket` tagged `tag` by `owner`, who has taken it off again.
fixture taken_off(ticket: Ticket, tag: Tag, owner: User) -> TicketTag {
    return make.TicketTag {
        ticket_id: ticket.id
        tag_id: tag.id
        owner_id: owner.id
        removed_at: clock.now
    }
}

test "a page of the tickets with a tag, in order" {
    var ann: User = make.User { handle: "ann" }
    var red: Tag = make.Tag { name: "red" }
    var blue: Tag = make.Tag { name: "blue" }
    var a: Ticket = make.Ticket { title: "a" }
    var b: Ticket = make.Ticket { title: "b" }
    var c: Ticket = make.Ticket { title: "c" }
    var d: Ticket = make.Ticket { title: "d" }
    tagging(c, red, ann)
    tagging(a, red, ann)
    tagging(d, red, ann)
    tagging(b, blue, ann)
    var first: Page<TicketView> = tagged(red.id, none, Principal.user(ann))
    assert list.map(first.items, t -> t.title) == ["a", "c"]
    var second: Page<TicketView> = tagged(red.id, first.next, Principal.user(ann))
    assert list.map(second.items, t -> t.title) == ["d"]
    assert second.next == none
}

test "a tag the caller may not read, or one taken off, doesn't count" {
    var ann: User = make.User { handle: "ann" }
    var bob: User = make.User { handle: "bob" }
    var red: Tag = make.Tag { name: "red" }
    var a: Ticket = make.Ticket { title: "a" }
    var b: Ticket = make.Ticket { title: "b" }
    var c: Ticket = make.Ticket { title: "c" }
    tagging(a, red, ann)
    tagging(b, red, bob)
    taken_off(c, red, ann)
    var anns: Page<TicketView> = tagged(red.id, none, Principal.user(ann))
    assert list.map(anns.items, t -> t.title) == ["a"]
    var bobs: Page<TicketView> = tagged(red.id, none, Principal.user(bob))
    assert list.map(bobs.items, t -> t.title) == ["b"]
    var untouched: List<TicketView> = untagged(Principal.user(bob))
    assert list.map(untouched, t -> t.title) == ["a", "c"]
}

test "a call with no caller counts every tag still on" {
    var ann: User = make.User { handle: "ann" }
    var bob: User = make.User { handle: "bob" }
    var red: Tag = make.Tag { name: "red" }
    var a: Ticket = make.Ticket { title: "a" }
    var b: Ticket = make.Ticket { title: "b" }
    var c: Ticket = make.Ticket { title: "c" }
    tagging(a, red, ann)
    tagging(b, red, bob)
    tagging(b, red, ann)
    taken_off(c, red, ann)
    var counted: TagCount = count_tagged(red.id)
    assert counted.tickets == 2
}
"#;

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("related-records-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    write(&root, "domains/desk/queries/tagged.test.jadpo", TESTS);
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

/// The statement: one `EXISTS` under the link's `visible:` and policy,
/// beside the page's cursor and order, and the index it reads.
#[test]
fn a_related_record_is_one_exists_under_its_policy() {
    let root = project("sql");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let sql = jadpo(&root, &["sql", "tagged"]);
    assert_eq!(sql.code, 0, "{}{}", sql.stdout, sql.stderr);
    assert!(
        sql.stdout.contains(r#"AND EXISTS (SELECT 1 FROM "ticket_tag" WHERE "ticket_tag"."ticket_id" = "ticket"."id" AND "tag_id" = $1 AND "removed_at" IS NULL AND "ticket_tag"."owner_id" = $2)"#),
        "{}",
        sql.stdout
    );
    assert!(
        sql.stdout
            .contains(r#"ORDER BY "title" COLLATE "C" ASC, "id" ASC"#),
        "{}",
        sql.stdout
    );
    let migration = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.is_dir())
        .expect("a migration");
    let text = fs::read_to_string(migration.join("postgres.sql")).expect("postgres.sql");
    assert!(
        text.contains(r#"ON "ticket_tag" ("ticket_id", "tag_id")"#),
        "{text}"
    );
}

#[test]
fn a_page_of_tickets_by_a_related_tag_respects_the_links_policy() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("page");
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("3 passed"), "{}", tested.stdout);
}

/// The obvious way, a condition on the link's field in the ticket query,
/// names `exists` with the reference that links them, as a fix.
#[test]
fn a_related_field_in_where_points_at_exists() {
    let root = project("obvious");
    write(
        &root,
        "domains/desk/queries/tagged.jadpo",
        &FILES
            .iter()
            .find(|(path, _)| *path == "domains/desk/queries/tagged.jadpo")
            .expect("the query")
            .1
            .replace(
                "where: exists(TicketTag.ticket_id, TicketTag.tag_id == tag_id)",
                "where: TicketTag.tag_id == tag_id",
            ),
    );
    let checked = jadpo(&root, &["check"]);
    assert_ne!(checked.code, 0, "{}", checked.stdout);
    assert!(
        checked.stdout.contains(
            "next: Write `exists(TicketTag.ticket_id, TicketTag.tag_id == tag_id)` (agent)"
        ),
        "{}",
        checked.stdout
    );
    let fixed = jadpo(&root, &["fix"]);
    assert_eq!(fixed.code, 0, "{}{}", fixed.stdout, fixed.stderr);
    let text = fs::read_to_string(root.join("domains/desk/queries/tagged.jadpo")).expect("read");
    assert!(
        text.contains("where: exists(TicketTag.ticket_id, TicketTag.tag_id == tag_id)"),
        "{text}"
    );
}

/// A link field no client may read can't be filtered on, as a field of the
/// query's own entity can't (area 9, "What else shows a field").
#[test]
fn a_restricted_field_of_the_related_record_is_refused() {
    let root = project("restricted");
    let entity = "domains/desk/entities/ticket_tag.jadpo";
    let text = fs::read_to_string(root.join(entity)).expect("read");
    write(
        &root,
        entity,
        &text.replace(
            "    tag_id: Tag.id {\n        on_delete: OnDelete.cascade\n    }",
            "    tag_id: Tag.id {\n        on_delete: OnDelete.cascade\n        policy: FieldAccess.server_only\n    }",
        ),
    );
    let checked = jadpo(&root, &["check"]);
    assert_ne!(checked.code, 0, "{}", checked.stdout);
    assert!(
        checked.stdout.contains(
            "error: `tagged` filters on `TicketTag.tag_id`, which not every caller may read"
        ),
        "{}",
        checked.stdout
    );
}

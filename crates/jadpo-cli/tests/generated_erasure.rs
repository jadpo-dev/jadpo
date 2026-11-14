//! The test generated for each person entity no test erases (area 13,
//! "Tests generated for each route"; area 15, "Export"; chunk W2b), end to
//! end: a `User` with a list, a todo on it, a postcard and a badge, whom no
//! test erases, a `Member` whom a test erases, and a `Visitor` whose pass
//! no value can be made for. Only the user gets the test: builders the
//! runner calls make them and a record of each kind of theirs a value can
//! be made for, the list a todo needs among them, then erase them, and
//! their export holds nothing once the erasure's deliveries are done. It
//! sits at the entity, counted apart, and a file, a line, `--name` and
//! `--kind action` pick it.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use crate::http::jadpo;

/// The accounts' project.
const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        erasures: {
            driver: Driver.postgres
        }
    }
    schedules: {
        store: main
    }
    privacy: {
        erasure_log: {
            store: erasures
        }
    }
}
"#,
    ),
    (
        "domains/accounts/actions/join.jadpo",
        r#"use accounts/entities { Member }

/// Adds a member.
action join(email: Member.email) -> Member {
    return Member.create {
        set: {
            email: email
        }
    }
}
"#,
    ),
    (
        "domains/accounts/actions/leave.jadpo",
        r#"use accounts/entities { Member, MemberNotFound }

/// Erases a member.
action leave(member_id: Member.id) fails MemberNotFound -> Unit {
    attempt Member.erase(member_id)
}
"#,
    ),
    (
        "domains/accounts/actions/leave.test.jadpo",
        r#"use accounts/entities { Member, MemberNotFound }
use accounts/actions { join }

test "a member who leaves is erased" {
    var member: Member = join("ada@example.com")
    attempt leave(member.id)
    assert Member.get(member.id) fails MemberNotFound
}
"#,
    ),
    (
        "domains/accounts/actions/sign_up.jadpo",
        r#"use accounts/entities { User }
use accounts/failures { EmailTaken }

/// Adds a user.
action sign_up(email: User.email) fails EmailTaken -> User {
    return attempt User.create {
        set: {
            email: email
        }
        conflict: {
            User.email: EmailTaken
        }
    }
}
"#,
    ),
    (
        "domains/accounts/actions/sign_up.test.jadpo",
        r#"use accounts/entities { User }

test "a user signs up" {
    var ada: User = attempt sign_up("ada@example.com")
    assert ada.email == "ada@example.com"
}
"#,
    ),
    (
        "domains/accounts/entities/badge.jadpo",
        r#"use accounts/entities { User }
use accounts/types { BadgeCode }

/// A badge printed for a user.
entity Badge {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    code: BadgeCode

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
"#,
    ),
    (
        "domains/accounts/entities/member.jadpo",
        r#"/// Someone in the club, who may leave it.
entity Member {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
        delete: [Access.public]
    }

    person: true
}
"#,
    ),
    (
        "domains/accounts/entities/postcard.jadpo",
        r#"use accounts/entities { User }
use accounts/types { CardText }

/// A postcard one user sends another.
entity Postcard {
    id: Uuid
    sender_id: User.id? {
        on_delete: OnDelete.cleared
    }
    recipient_id: User.id? {
        on_delete: OnDelete.cleared
    }
    body: CardText

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
"#,
    ),
    (
        "domains/accounts/entities/todo.jadpo",
        r#"use accounts/entities { TodoList, User }
use accounts/types { TodoTitle }

/// Something a user means to do, on one of their lists.
entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.restricted
        erase: Erasure.deleted
    }
    list_id: TodoList.id {
        on_delete: OnDelete.cascade
    }
    title: TodoTitle

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
"#,
    ),
    (
        "domains/accounts/entities/todo_list.jadpo",
        r#"use accounts/entities { User }
use accounts/types { ListName }

/// A user's list of todos.
entity TodoList {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    name: ListName

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
"#,
    ),
    (
        "domains/accounts/entities/user.jadpo",
        r#"/// Someone with an account.
entity User {
    id: Uuid
    email: Email {
        unique: true
        personal: PersonalData.contact
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }

    person: true
}
"#,
    ),
    (
        "domains/accounts/entities/visitor.jadpo",
        r#"use accounts/types { PassCode }

/// Someone in for the day, known by their pass.
entity Visitor {
    id: Uuid
    pass: PassCode

    policy: {
        create: [Access.public]
        read: [Access.public]
    }

    person: true
}
"#,
    ),
    (
        "domains/accounts/failures/email_taken.jadpo",
        r#"/// Someone has that address already.
type EmailTaken = Failure {
    kind: FailureKind.conflict
}
"#,
    ),
    (
        "domains/accounts/types/badge_code.jadpo",
        r#"/// A badge's printed code.
type BadgeCode = Text {
    pattern: "[A-Z]{3}-[0-9]{4}"
}
"#,
    ),
    (
        "domains/accounts/types/card_text.jadpo",
        r#"/// What a postcard says.
type CardText = Text {
    max_length: 500
}
"#,
    ),
    (
        "domains/accounts/types/list_name.jadpo",
        r#"/// What a list is called.
type ListName = Text {
    min_length: 1
    max_length: 80
}
"#,
    ),
    (
        "domains/accounts/types/pass_code.jadpo",
        r#"/// The code on a visitor's day pass.
type PassCode = Text {
    pattern: "[A-Z]{2}[0-9]{6}"
}
"#,
    ),
    (
        "domains/accounts/types/todo_title.jadpo",
        r#"/// What a todo says.
type TodoTitle = Text {
    max_length: 200
}
"#,
    ),
];

/// The project's files, for the workerd comparison.
pub fn files() -> Vec<(String, String)> {
    FILES
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect()
}

/// The project, written into a folder of its own, migrated and with its
/// records.
fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("generated-erasure-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

fn tested(root: &Path, args: &[&str]) -> (String, u8) {
    let mut all = vec!["test", "--seed", "1"];
    all.extend_from_slice(args);
    let output = jadpo(root, &all);
    (
        crate::support::without_run_time(&output.stdout),
        output.code,
    )
}

/// Each test that ran, as `--json --full` lists it, by key.
fn listed(root: &Path, args: &[&str]) -> Vec<serde_json::Value> {
    let mut all = vec!["test", "--seed", "1", "--json", "--full"];
    all.extend_from_slice(args);
    let output = jadpo(root, &all);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    let mut tests = document["data"]["tests"].as_array().expect("tests").clone();
    tests.sort_by_key(|test| test["key"].as_str().unwrap_or_default().to_string());
    tests
}

#[test]
fn a_person_no_test_erases_is_erased_with_a_record_of_each_kind_of_theirs() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("shown");
    assert_eq!(
        tested(&root, &[]),
        (
            "tested: 3 passed, 2 action tests, 1 generated test\n".to_string(),
            0
        )
    );
    let tests = listed(&root, &[]);
    let shown: Vec<(String, String, u64, bool, String)> = tests
        .iter()
        .map(|test| {
            (
                test["name"].as_str().unwrap_or_default().to_string(),
                test["file"].as_str().unwrap_or_default().to_string(),
                test["line"].as_u64().unwrap_or_default(),
                test["generated"].as_bool().unwrap_or(false),
                test["kind"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let at = |name: &str, file: &str, line: u64, generated: bool| {
        (
            name.to_string(),
            file.to_string(),
            line,
            generated,
            "action".to_string(),
        )
    };
    // The member a test erases and the visitor no builder can make get
    // none.
    assert_eq!(
        shown,
        [
            at(
                "User.erase leaves nothing in User.export",
                "domains/accounts/entities/user.jadpo",
                2,
                true
            ),
            at(
                "a member who leaves is erased",
                "domains/accounts/actions/leave.test.jadpo",
                4,
                false
            ),
            at(
                "a user signs up",
                "domains/accounts/actions/sign_up.test.jadpo",
                3,
                false
            ),
        ]
    );
    // The user first, then a record of each kind of theirs a value can be
    // made for, the list a todo is on made before it: no badge, whose code
    // must match a pattern.
    let generated = tests
        .iter()
        .find(|test| test["generated"].as_bool() == Some(true))
        .expect("the generated test");
    assert_eq!(
        generated["makes"],
        serde_json::json!(["User", "Postcard", "TodoList", "Todo", "TodoList"])
    );
    assert_eq!(generated["requests"], serde_json::Value::Null);
    assert_eq!(generated["outcome"], "passed");
}

#[test]
fn the_entitys_place_its_name_or_its_kind_picks_the_test_generated_for_a_person() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("picked");
    assert_eq!(
        tested(&root, &["domains/accounts/entities/user.jadpo:5"]),
        ("tested: 1 passed, 1 of 3 tests\n".to_string(), 0)
    );
    assert_eq!(
        tested(&root, &["--name", "leaves nothing"]),
        ("tested: 1 passed, 1 of 3 tests\n".to_string(), 0)
    );
    assert_eq!(
        tested(&root, &["--kind", "action"]),
        (
            "tested: 3 passed, 2 action tests, 1 generated test\n".to_string(),
            0
        )
    );
}

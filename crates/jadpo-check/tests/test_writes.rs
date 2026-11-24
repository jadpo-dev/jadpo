//! What a test starts from (area 13, "Action tests"): a write straight to
//! an entity in a test is `semantic.not_callable`, and that one error is
//! the only one its write gives. A create's next step is the data builder,
//! which takes the fields its `set:` gives and needs no `conflict:`
//! (chunk 10.4c: agents copied a fixture's create into a test, and fixed
//! its `conflict:` before learning a test can't write at all).

use jadpo_db::{Database, Layout};

use crate::support;

const ENTITY: &str = r#"type Handle = Text {
    max_length: 40
}

/// A person with an account.
entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
"#;

fn checked(test: &str) -> Vec<jadpo_diag::Diagnostic> {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Project,
        &[
            ("app.jadpo", "application {}\n"),
            ("domains/club/entities/member.jadpo", ENTITY),
            ("domains/club/entities/member.test.jadpo", test),
        ],
    );
    jadpo_check::check(&db, project)
}

#[test]
fn a_create_in_a_test_is_one_error_whose_next_step_is_the_data_builder() {
    let found = checked(
        r#"test "two members have their own handles" {
    var bob: Member = Member.create {
        set: {
            handle: "bob"
        }
    }
    assert bob.handle == "bob"
}
"#,
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].rule.id, "semantic.not_callable", "{found:#?}");
    assert_eq!(found[0].message, "A test can't call `Member.create`");
    assert_eq!(
        found[0].next.as_ref().map(|next| next.text.as_str()),
        Some(
            "Build the record with `make.Member { ... }`, giving it the fields `set:` gives, with no `conflict:`"
        ),
        "{found:#?}"
    );
}

#[test]
fn the_data_builder_checks_where_the_create_was_refused() {
    let found = checked(
        r#"test "two members have their own handles" {
    var bob: Member = make.Member {
        handle: "bob"
    }
    assert bob.handle == "bob"
}
"#,
    );
    assert!(found.is_empty(), "{found:#?}");
}

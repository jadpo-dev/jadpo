//! `jadpo seed` (area 5, "Seeds"; area 13, "Seeds and fixtures"; area 14;
//! chunk A5): a seed gives the development database its first staff
//! through the app's own actions, and is refused against a database that
//! isn't on this machine.
//!
//! A library's desk: `register` makes a librarian, the app's user;
//! `appoint` gives one a role on the staff with no caller, so it runs
//! without policy, which is how a seed makes the first head; `appoint_by`
//! takes the caller, so policy applies to it as to a request: only a head
//! may appoint.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! tests that need it.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_runtime::{Database, SqlValue};
use jadpo_term::Environment;

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use desk/entities { Librarian }

application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
}

principal {
    user: {
        subject: Librarian.email
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    database_url: PostgresUrl
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    browser_session: {
        cookie: "library_session"
    }
}
"#,
    ),
    (
        "domains/desk/types/staff_role.jadpo",
        r#"/// What a librarian may do at the desk.
type StaffRole = Enum {
    head
    desk
}
"#,
    ),
    (
        "domains/desk/entities/librarian.jadpo",
        r#"/// Someone who signs in to the library's desk.
entity Librarian {
    id: Uuid
    email: Email {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}
"#,
    ),
    (
        "domains/desk/entities/staff_membership.jadpo",
        r#"use desk/entities { Librarian }
use desk/types { StaffRole }

/// A librarian's role on the library's staff.
entity StaffMembership {
    id: Uuid
    librarian_id: Librarian.id {
        on_delete: OnDelete.cascade
    }
    role: StaffRole

    membership: {
        scope: MembershipScope.application
        member: librarian_id
        role: role
    }

    policy: {
        create: [StaffRole.head]
        read: [librarian_id, StaffRole.head]
    }
}
"#,
    ),
    (
        "domains/desk/failures/already_registered.jadpo",
        r#"type AlreadyRegistered = Failure {
    kind: FailureKind.conflict
    message: "That address is already registered."
}
"#,
    ),
    (
        "domains/desk/actions/register.jadpo",
        r#"use desk/entities { Librarian }
use desk/failures { AlreadyRegistered }

/// Makes someone a librarian.
action register(email: Email) fails AlreadyRegistered -> Librarian {
    return attempt Librarian.create {
        set: {
            email: email
        }
        conflict: {
            Librarian.email: AlreadyRegistered
        }
    }
}
"#,
    ),
    (
        "domains/desk/actions/appoint.jadpo",
        r#"use desk/entities { Librarian, LibrarianNotFound, StaffMembership }
use desk/types { StaffRole }

/// Gives a librarian a role on the staff.
action appoint(librarian_id: Librarian.id, role: StaffRole) fails LibrarianNotFound -> StaffMembership {
    return attempt StaffMembership.create {
        set: {
            librarian_id: librarian_id
            role: role
        }
    }
}
"#,
    ),
    (
        "domains/desk/actions/appoint_by.jadpo",
        r#"use desk/entities { Librarian, LibrarianNotFound, StaffMembership }
use desk/types { StaffRole }

/// Gives a librarian a role on the staff, as the head appointing them.
action appoint_by(librarian_id: Librarian.id, role: StaffRole, by: Principal.user) fails LibrarianNotFound -> StaffMembership {
    return attempt StaffMembership.create {
        set: {
            librarian_id: librarian_id
            role: role
        }
    }
}
"#,
    ),
    // The page's own form: `seed { }`, named by its file.
    (
        "database/seeds/first_staff.jadpo",
        r#"use desk
use desk/entities { Librarian }
use desk/types { StaffRole }

/// The library's first staff, for local development.
seed {
    var ada: Librarian = attempt desk.register("ada@example.com")
    attempt desk.appoint(ada.id, StaffRole.head)
    var ben: Librarian = attempt desk.register("ben@example.com")
    attempt desk.appoint_by(ben.id, StaffRole.desk, Principal.user(ada))
}
"#,
    ),
    // A desk librarian appointing someone: policy refuses it, as it
    // would a request.
    (
        "database/seeds/desk_appoints.jadpo",
        r#"use desk
use desk/entities { Librarian }
use desk/types { StaffRole }

/// Ben, at the desk, tries to appoint Cal.
seed {
    var ben: Librarian = attempt desk.register("ben@example.com")
    attempt desk.appoint(ben.id, StaffRole.desk)
    var cal: Librarian = attempt desk.register("cal@example.com")
    attempt desk.appoint_by(cal.id, StaffRole.desk, Principal.user(ben))
}
"#,
    ),
];

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &Environment::empty().with_cwd(cwd))
}

/// What a run printed, on either stream.
fn printed(output: &Output) -> String {
    format!("{}{}", output.stdout, output.stderr)
}

/// A database the test server gives, as the URL a `PostgresUrl` setting
/// holds: the disposable server answers with `host=… dbname=…` pairs, an
/// external one with a URL.
fn as_url(connection: &str) -> String {
    if connection.starts_with("postgres") {
        return connection.to_string();
    }
    let part = |key: &str| {
        connection
            .split_whitespace()
            .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
            .map(|value| value.trim_matches('\'').to_string())
            .unwrap_or_default()
    };
    format!(
        "postgresql://{}@/{}?host={}&port={}",
        part("user"),
        part("dbname"),
        part("host"),
        part("port")
    )
}

/// The library, with its first migration written, in a folder of its own.
fn library(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("seeds-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

/// Runs `sql`, giving each row's columns as text.
fn rows(url: &str, sql: &str) -> Vec<Vec<String>> {
    let store = jadpo_host_native::postgres(url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        let rows = transaction.query(sql, &[]).await.expect("query");
        transaction.commit().await.expect("commit");
        rows.into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|value| match value {
                        SqlValue::Text(text) => text,
                        SqlValue::Int8(n) => n.to_string(),
                        other => format!("{other:?}"),
                    })
                    .collect()
            })
            .collect()
    })
}

/// Each librarian's address and role, by address.
fn staff(url: &str) -> Vec<Vec<String>> {
    rows(
        url,
        "SELECT l.email, m.role FROM librarian l \
         LEFT JOIN staff_membership m ON m.librarian_id = l.id ORDER BY l.email",
    )
}

/// A database on the test server, or `None` when live PostgreSQL is
/// skipped.
fn database(stem: &str) -> Option<String> {
    if !jadpo_test::live() {
        return None;
    }
    let server = jadpo_test::database::server().expect("a PostgreSQL server");
    Some(server.create_database(stem).expect("a database"))
}

#[test]
fn a_seed_gives_the_dev_database_its_first_staff() {
    let root = library("first-staff");
    let Some(url) = database("seed_first_staff") else {
        return;
    };
    // The address comes from the connection's setting, in `.env.local`,
    // as a developer's machine gives it.
    write(
        &root,
        ".env.local",
        &format!("DATABASE_URL={}\n", as_url(&url)),
    );
    let seeded = jadpo(&root, &["seed", "first_staff"]);
    assert_eq!(seeded.code, 0, "{}{}", seeded.stdout, seeded.stderr);
    // The migrations were pending, so they went first.
    assert!(
        seeded.stdout.contains("Applied 1 migration to `main`"),
        "{}",
        seeded.stdout
    );
    assert!(
        seeded.stdout.contains("Seeded `first_staff` in "),
        "{}",
        seeded.stdout
    );
    // Where the data went, without a credential.
    assert!(seeded.stdout.contains("main"), "{}", seeded.stdout);
    assert!(!seeded.stdout.contains("password"), "{}", seeded.stdout);
    assert_eq!(
        staff(&url),
        [
            vec!["ada@example.com".to_string(), "head".to_string()],
            vec!["ben@example.com".to_string(), "desk".to_string()],
        ]
    );
}

#[test]
fn policy_applies_to_a_seeds_call_with_a_caller() {
    let root = library("desk-appoints");
    let Some(url) = database("seed_desk_appoints") else {
        return;
    };
    let seeded = jadpo(&root, &["seed", "desk_appoints", "--url", &url, "--json"]);
    assert_eq!(seeded.code, 1, "{}{}", seeded.stdout, seeded.stderr);
    let document: serde_json::Value = serde_json::from_str(&seeded.stdout).expect("json");
    let diagnostic = &document["diagnostics"][0];
    assert_eq!(diagnostic["rule"], "seed.failed", "{document}");
    assert_eq!(document["data"]["outcome"], "failed", "{document}");
    assert_eq!(document["data"]["migrationsApplied"], 1, "{document}");
    // What it committed before the refusal stays: Cal was registered, and
    // never appointed.
    assert_eq!(
        staff(&url),
        [
            vec!["ben@example.com".to_string(), "desk".to_string()],
            vec!["cal@example.com".to_string(), "Null(Text)".to_string()],
        ]
    );
}

#[test]
fn a_seed_that_fails_says_where_and_keeps_what_it_committed() {
    let root = library("fails");
    let Some(url) = database("seed_fails") else {
        return;
    };
    let first = jadpo(&root, &["seed", "first_staff", "--url", &url]);
    assert_eq!(first.code, 0, "{}{}", first.stdout, first.stderr);
    // Again: Ada is registered already.
    let again = jadpo(&root, &["seed", "first_staff", "--url", &url]);
    assert_eq!(again.code, 1, "{}{}", again.stdout, again.stderr);
    for words in [
        "Seed `first_staff` stopped with `AlreadyRegistered` from action register",
        "Its code is `already_registered`, conflict (409): \"That address is already registered.\"",
        "The calls were seed first_staff -> action register.",
        "domains/desk/actions/register.jadpo",
        "What the seed's earlier calls committed stays in the database.",
        "seed.failed",
        "seed failed",
    ] {
        assert!(again.stdout.contains(words), "{words}: {}", again.stdout);
    }
    assert_eq!(staff(&url).len(), 2);
}

#[test]
fn a_database_that_isnt_on_this_machine_is_refused_before_anything_runs() {
    let root = library("production");
    for url in [
        "postgresql://app:secret@db.example.com:5432/library",
        "postgresql://10.0.0.7/library",
    ] {
        let refused = jadpo(&root, &["seed", "first_staff", "--url", url]);
        assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
        for words in [
            "isn't on this machine",
            "Nothing was run.",
            "seed.production_refused",
            "(human)",
        ] {
            assert!(
                printed(&refused).contains(words),
                "{words}: {}",
                printed(&refused)
            );
        }
        assert!(
            !printed(&refused).contains("secret"),
            "{}",
            printed(&refused)
        );
    }
    // From the connection's setting too.
    write(
        &root,
        ".env.local",
        "DATABASE_URL=postgresql://app:secret@db.example.com/library\n",
    );
    let refused = jadpo(&root, &["seed", "first_staff", "--json"]);
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    let document: serde_json::Value = serde_json::from_str(&refused.stdout).expect("json");
    assert_eq!(
        document["diagnostics"][0]["rule"], "seed.production_refused",
        "{document}"
    );
}

#[test]
fn a_seed_is_named_by_its_file() {
    let root = library("names");
    let missing = jadpo(&root, &["seed"]);
    assert_eq!(missing.code, 2, "{}{}", missing.stdout, missing.stderr);
    for words in [
        "`jadpo seed` needs the seed to run",
        "`desk_appoints` and `first_staff`",
        "cli.seed_arguments",
    ] {
        assert!(
            printed(&missing).contains(words),
            "{words}: {}",
            printed(&missing)
        );
    }
    let misspelt = jadpo(&root, &["seed", "first_staf"]);
    assert_eq!(misspelt.code, 2, "{}{}", misspelt.stdout, misspelt.stderr);
    assert!(
        printed(&misspelt).contains("Use `first_staff` instead of `first_staf`"),
        "{}",
        printed(&misspelt)
    );
}

#[test]
fn a_seed_is_checked_like_a_test() {
    let root = library("checked");
    // `seed { }` has no name of its own, and its body is still checked.
    write(
        &root,
        "database/seeds/first_staff.jadpo",
        r#"use desk
use desk/entities { Librarian }

/// The library's first staff, for local development.
seed {
    var ada: Librarian = attempt desk.register("ada@example.com")
    attempt desk.appoint(ada.id, 42)
}
"#,
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}{}", checked.stdout, checked.stderr);
    assert!(
        checked.stdout.contains("type.mismatch"),
        "{}",
        checked.stdout
    );
    assert!(
        checked
            .stdout
            .contains("database/seeds/first_staff.jadpo:7"),
        "{}",
        checked.stdout
    );
}

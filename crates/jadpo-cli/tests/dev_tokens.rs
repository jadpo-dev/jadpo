//! External tokens in `jadpo dev` (area 8, "External tokens in tests and
//! `jadpo dev`"; area 14; chunk A3): a session takes tokens from its own
//! local issuer in the configured one's place, fetching nothing from the
//! provider, and `jadpo dev token <user-id>` beside it prints one for that
//! user's subject, which signs the user in on an API that takes only
//! external tokens; a user who isn't active is still refused. The key is
//! kept under `build/` while the session runs, readable by its owner
//! alone, and removed when it stops.

use std::path::{Path, PathBuf};

use super::http::{Dev, jadpo, tempdir, write};

const FILES: &[(&str, &str)] = &[
    (
        "config.jadpo",
        r#"config {
    issuer: Url {
        default: "https://id.lending.example/"
    }
    audience: Text {
        default: "lending-desk"
    }
}
"#,
    ),
    (
        "app.jadpo",
        r#"use desk/entities { Librarian }

application {
    display_name: "Lending desk"
    apis: {
        main: {
            prefix: /
            authentication: [AuthenticationMethod.browser_session]
        }
        staff: {
            prefix: /staff
            authentication: [AuthenticationMethod.external_token]
        }
    }
}

principal {
    user: {
        subject: Librarian.subject
        active: Librarian.active == true
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
    external_token: {
        issuer: config.issuer
        audience: config.audience
    }
}
"#,
    ),
    (
        "domains/desk/entities/librarian.jadpo",
        r#"entity Librarian {
    id: Uuid
    subject: Text {
        unique: true
    }
    name: Text
    active: Bool

    policy: {
        create: [Access.system]
        read: [id]
    }
}
"#,
    ),
    (
        "domains/desk/failures/not_a_librarian.jadpo",
        r#"/// The caller isn't on the staff.
type NotALibrarian = Failure {
    kind: FailureKind.not_found
}
"#,
    ),
    (
        "domains/desk/failures/subject_taken.jadpo",
        r#"/// Someone else already signs in with that subject.
type SubjectTaken = Failure {
    kind: FailureKind.conflict
}
"#,
    ),
    (
        "domains/desk/types/librarian_card.jadpo",
        r#"use desk/entities { Librarian }

type LibrarianCard = Output {
    pick: [Librarian.id, Librarian.name]
}
"#,
    ),
    (
        "domains/desk/types/new_librarian.jadpo",
        r#"use desk/entities { Librarian }

/// Someone to take on, under the subject their provider gives them.
type NewLibrarian = Input {
    subject: Librarian.subject
    name: Librarian.name
    active: Librarian.active
}
"#,
    ),
    (
        "domains/desk/actions/take_on.jadpo",
        r#"use desk/entities { Librarian }
use desk/failures { SubjectTaken }
use desk/types { NewLibrarian }

/// Takes someone on.
action take_on(input: NewLibrarian) fails SubjectTaken -> Librarian {
    return attempt Librarian.create {
        set: {
            subject: input.subject
            name: input.name
            active: input.active
        }
        conflict: {
            Librarian.subject: SubjectTaken
        }
    }
}
"#,
    ),
    (
        "domains/desk/queries/me.jadpo",
        r#"use desk/entities { Librarian }
use desk/failures { NotALibrarian }
use desk/types { LibrarianCard }

/// The signed-in member of staff.
query me(user: Principal.user) fails NotALibrarian -> LibrarianCard {
    from: Librarian
    where: Librarian.id == user.id
    missing: NotALibrarian
}
"#,
    ),
    (
        "domains/desk/routes/staff.jadpo",
        r#"use desk/queries { me }
use desk/types { LibrarianCard }

/// What staff reach with their provider's token.
routes Staff {
    api: staff
    prefix: /staff
    GET /me {
        output: LibrarianCard
        run: me(current_principal.user)
    }
}
"#,
    ),
];

fn project() -> (tempdir::Dir, PathBuf) {
    let dir = tempdir::Dir::new("dev-tokens");
    let root = dir.path().join("desk");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    for args in [
        &["migrate", "new", "initial"][..],
        &["records", "update"],
        &["check"],
    ] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    (dir, root)
}

/// A librarian with `subject`, written straight into the session's
/// database; its id.
fn librarian(url: &str, subject: &str, active: bool) -> String {
    use jadpo_runtime::{Database, SqlValue};
    let id = format!(
        "00000000-0000-4000-8000-{:012}",
        subject.len() + usize::from(active)
    );
    let database = jadpo_host_native::postgres(url).expect("the database");
    let runtime = jadpo_host_native::Runtime::new().expect("a runtime");
    runtime
        .block_on(database.query_once(
            "INSERT INTO librarian (id, subject, name, active) VALUES ($1::uuid, $2, $3, $4)",
            &[
                SqlValue::Text(id.clone()),
                SqlValue::Text(subject.to_string()),
                SqlValue::Text(subject.to_string()),
                SqlValue::Bool(active),
            ],
        ))
        .expect("inserted");
    id
}

/// `jadpo dev <args>` on the system clock, as the session reads it, not
/// the fixed time [`jadpo`] gives.
fn dev_command(root: &Path, args: &[&str]) -> jadpo_cli::Output {
    let mut all = vec!["dev".to_string()];
    all.extend(args.iter().map(|arg| arg.to_string()));
    jadpo_cli::run(&all, &jadpo_term::Environment::empty().with_cwd(root))
}

fn token(root: &Path, url: &str, id: &str) -> jadpo_cli::Output {
    dev_command(root, &["token", id, "--url", url])
}

#[test]
fn a_token_from_jadpo_dev_signs_a_librarian_in_on_the_staff_api() {
    if !jadpo_test::live() {
        return;
    }
    let (_dir, root) = project();
    // Before a session, there is no issuer to sign with.
    let none = dev_command(&root, &["token", "00000000-0000-4000-8000-000000000001"]);
    assert_eq!(none.code, 2, "{}{}", none.stdout, none.stderr);
    assert!(
        none.stderr.contains("No `jadpo dev` is running"),
        "{}",
        none.stderr
    );
    let dev = Dev::start(&root);
    assert!(dev.log().contains("\"tokens\":\"local\""), "{}", dev.log());
    let kept = root.join("build/dev/issuer.json");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&kept)
            .expect("the key is kept")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let ann = librarian(dev.url(), "google-ann", true);
    let printed = token(&root, dev.url(), &ann);
    assert_eq!(printed.code, 0, "{}{}", printed.stdout, printed.stderr);
    let bearer = printed.stdout.trim_end().to_string();
    assert_eq!(printed.stdout, format!("{bearer}\n"), "the token alone");
    assert_eq!(bearer.split('.').count(), 3, "{bearer}");
    let authorization = format!("Bearer {bearer}");
    let own_card = dev.send("GET", "/staff/me", &[("authorization", &authorization)], "");
    assert_eq!(own_card.status, 200, "{own_card:?}");
    assert_eq!(own_card.json()["name"], "google-ann");
    // `--json`: the token, its subject and when it expires.
    let json = dev_command(&root, &["token", &ann, "--url", dev.url(), "--json"]);
    assert_eq!(json.code, 0, "{}{}", json.stdout, json.stderr);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("json");
    assert_eq!(document["data"]["subject"], "google-ann");
    assert!(document["data"]["expiresAt"].is_string(), "{document}");
    // Someone no longer active is refused, as a provider's token would be.
    let bo = librarian(dev.url(), "google-bo", false);
    let printed = token(&root, dev.url(), &bo);
    assert_eq!(printed.code, 0, "{}{}", printed.stdout, printed.stderr);
    let authorization = format!("Bearer {}", printed.stdout.trim_end());
    let refused = dev.send("GET", "/staff/me", &[("authorization", &authorization)], "");
    assert_eq!(refused.status, 403, "{refused:?}");
    // No such user, and not an id.
    let unknown = token(&root, dev.url(), "00000000-0000-4000-8000-999999999999");
    assert_eq!(unknown.code, 2);
    assert!(
        unknown.stderr.contains("No user has the id"),
        "{}",
        unknown.stderr
    );
    let not_an_id = token(&root, dev.url(), "ann");
    assert_eq!(not_an_id.code, 2);
    assert!(
        not_an_id.stderr.contains("isn't a user's id"),
        "{}",
        not_an_id.stderr
    );
    // A token signed with another key, as from the real provider's
    // impostor, is refused.
    let forged = format!("{}x", &bearer[..bearer.len() - 1]);
    let forged = dev.send(
        "GET",
        "/staff/me",
        &[("authorization", &format!("Bearer {forged}"))],
        "",
    );
    assert_eq!(forged.status, 401, "{forged:?}");
    drop(dev);
    assert!(!kept.exists(), "the key goes with the session");
}

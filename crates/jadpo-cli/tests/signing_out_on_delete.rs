//! Deleting someone who signs in takes their sign-in state with them
//! (area 8, "Deleting someone who signs in"; chunk 10.13b), against live
//! PostgreSQL: `Member.delete(id)`, and deleting a club its members
//! cascade from, delete each member's sessions, API tokens and sign-in
//! links in the same statement, as erasure and retention do, with their
//! passkeys and kept AI content where the app keeps them.
//!
//! A member who left and signs in again through the same provider is a
//! person the app hasn't seen (area 8, "A person the app has not seen"):
//! the link went with them, so `on_first_sign_in:` runs again and makes
//! one new member, linked as the first was, so the sign-in after that
//! goes through the new link. A link left behind would point at no one:
//! each sign-in would run `on_first_sign_in:` again, and once the new
//! member held the address, refuse.
//!
//! The tests run interpreted through `jadpo test`, and the rows are read
//! after a run through the interpreter. The server is the one
//! `JADPO_TEST_DATABASE_URL` names or one started from the machine's
//! PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::Program;
use jadpo_runtime::{Executor, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

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

const FILES: [(&str, &str); 16] = [
    (
        "app.jadpo",
        r#"use club/entities { Member }

application {
    display_name: "Book club"
    connections: {
        google: {
            driver: Driver.oidc
            issuer: config.google_issuer
            client_id: config.google_client_id
            credential: config.google_client_secret
        }
    }
}

principal {
    user: {
        subject: Member.email
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    google_client_id: Text {
        test: "club-google"
    }
    google_client_secret: Text {
        secret: true
    }
    google_issuer: Url {
        default: "https://accounts.google.com"
    }
    site: Url {
        test: "https://club.example"
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"use club

authentication {
    browser_session: {
        cookie: "club_session"
        origin: config.site
    }
    api_token: {}
    sign_in: {
        providers: [google]
    }
    on_first_sign_in: club.join
}
"#,
    ),
    (
        "domains/club/entities/club.jadpo",
        r#"use club/types { ClubName }

/// A book club.
entity Club {
    id: Uuid
    name: ClubName

    policy: {
        create: [Access.public]
        read: [Access.public]
        delete: [Access.public]
    }
}
"#,
    ),
    (
        "domains/club/entities/member.jadpo",
        r#"use club/entities { Club }

/// Someone who signs in with Google.
entity Member {
    id: Uuid
    email: Email {
        unique: true
    }
    club_id: Club.id? {
        on_delete: OnDelete.cascade
    }

    policy: {
        create: [Access.system, Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }
}
"#,
    ),
    (
        "domains/club/failures/cant_join.jadpo",
        r#"/// Google didn't give a verified address.
type CantJoin = Failure {
    kind: FailureKind.not_permitted
}
"#,
    ),
    (
        "domains/club/types/club_name.jadpo",
        r#"/// What a club is called.
type ClubName = Text {
    max_length: 80
}
"#,
    ),
    (
        "domains/club/failures/already_a_member.jadpo",
        r#"/// Someone with the address is a member already.
type AlreadyAMember = Failure {
    kind: FailureKind.conflict
}
"#,
    ),
    (
        "domains/club/types/app_signed_in.jadpo",
        r#"/// A token for the club's app.
type AppSignedIn = Output {
    token: ApiToken
}
"#,
    ),
    (
        "domains/club/actions/join.jadpo",
        r#"use club/entities { Member }
use club/failures { AlreadyAMember, CantJoin }

/// Makes a member of someone Google has verified the address of.
action join(first: FirstSignIn) fails CantJoin, AlreadyAMember -> Member.id {
    if (not first.address_verified) {
        reject CantJoin {}
    }
    match first.address {
        none -> {
            reject CantJoin {}
        }
        some(address) -> {
            var member: Member = attempt Member.create {
                set: {
                    email: address
                    club_id: none
                }
                conflict: {
                    Member.email: AlreadyAMember
                }
            }
            return member.id
        }
    }
}
"#,
    ),
    (
        "domains/club/actions/enrol.jadpo",
        r#"use club/entities { Club, ClubNotFound, Member }
use club/failures { AlreadyAMember }
use club/types { AppSignedIn }

/// A member of a club, with a token for the club's app.
action enrol(club_id: Club.id, email: Member.email) fails AlreadyAMember, ClubNotFound -> AppSignedIn {
    var member: Member = attempt Member.create {
        set: {
            email: email
            club_id: club_id
        }
        conflict: {
            Member.email: AlreadyAMember
        }
    }
    sessions.start(member.id)
    return AppSignedIn {
        token: sessions.issue_token(member.id)
    }
}
"#,
    ),
    (
        "domains/club/actions/create_club.jadpo",
        r#"use club/entities { Club }

/// Starts a club.
action create_club(name: Club.name) -> Club {
    return Club.create {
        set: {
            name: name
        }
    }
}
"#,
    ),
    (
        "domains/club/actions/leave.jadpo",
        r#"use club/entities { Member, MemberNotFound }

/// Removes a member for good.
action leave(member_id: Member.id) fails MemberNotFound -> Unit {
    attempt Member.delete(member_id)
}
"#,
    ),
    (
        "domains/club/actions/close.jadpo",
        r#"use club/entities { Club, ClubNotFound }

/// Closes a club, and its members go with it.
action close(club_id: Club.id) fails ClubNotFound -> Unit {
    attempt Club.delete(club_id)
}
"#,
    ),
    (
        "domains/club/queries/members.jadpo",
        r#"use club/entities { Member }

/// Every member.
query members() -> List<Member> {
    from: Member
    order: [Member.email asc]
    limit: every
}
"#,
    ),
    (
        "domains/club/actions/leave.test.jadpo",
        r#"use club/entities { Member }
use club/queries { members }

test "a member who left and signs in again with Google joins as someone new" {
    var ann: TestAccount = TestAccount {
        subject: "110248495921238986420"
        email: "ann@club.example"
        email_verified: true
        name: "Ann"
    }
    var first: Response = test_providers.sign_in(SignInMethod.google, ann)
    assert first.status == 303
    assert first.code == none
    var joined: List<Member> = members()
    assert list.count(joined) == 1
    match list.first(joined) {
        none -> {
            assert false
        }
        some(member) -> {
            attempt leave(member.id)
            assert list.count(members()) == 0
            var again: Response = test_providers.sign_in(SignInMethod.google, ann)
            assert again.status == 303
            assert again.code == none
            var rejoined: List<Member> = members()
            assert list.count(rejoined) == 1
            match list.first(rejoined) {
                none -> {
                    assert false
                }
                some(newcomer) -> {
                    assert newcomer.id != member.id
                    assert newcomer.email == member.email
                }
            }
            var third: Response = test_providers.sign_in(SignInMethod.google, ann)
            assert third.status == 303
            assert third.code == none
            assert list.count(members()) == 1
        }
    }
}
"#,
    ),
];

/// The project, for the compiled run (`compiled.rs`).
pub(crate) fn files() -> Vec<(String, String)> {
    FILES
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect()
}

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("signing-out-{name}"));
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
fn a_member_who_left_signs_in_again_as_someone_new() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("tests");
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("1 passed"), "{}", tested.stdout);
}

async fn call(program: &Program, host: &Harness, action: &str, args: Vec<Value>) -> Value {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = host.ctx(program);
    match program.run(&mut ctx, id, args).await {
        Outcome::Value(value) => value,
        other => panic!("{action}: expected a value, got {other:?}"),
    }
}

/// How many rows `sql` counts.
async fn count(host: &Harness, sql: &str) -> i64 {
    match host.rows(sql).await.first().and_then(|row| row.first()) {
        Some(SqlValue::Int8(n)) => *n,
        other => panic!("not a count: {other:?}"),
    }
}

/// `principal_id`'s rows in the sign-in tables a member's sign-in keeps.
async fn sign_in_rows(host: &Harness, member: &str) -> i64 {
    let mut total = 0;
    for (table, column) in [
        ("jadpo_sessions", "principal_id"),
        ("jadpo_sign_in_links", "user_id"),
    ] {
        total += count(
            host,
            &format!("SELECT count(*) FROM \"{table}\" WHERE \"{column}\"::text = '{member}'"),
        )
        .await;
    }
    total
}

fn member_id(host: &Harness, runtime: &Runtime, email: &str) -> String {
    runtime.block_on(async {
        match host
            .rows(&format!(
                "SELECT \"id\"::text FROM \"member\" WHERE \"email\" = '{email}'"
            ))
            .await
            .first()
            .and_then(|row| row.first())
        {
            Some(SqlValue::Text(id)) => id.clone(),
            other => panic!("no member {email}: {other:?}"),
        }
    })
}

/// The rows after a delete and a cascading delete run through the
/// interpreter: each deleted member's sessions, tokens and links are
/// gone, and a member still there keeps theirs.
#[test]
fn deleting_a_member_deletes_their_sign_in_state() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("rows");
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url)
        .at(1_792_400_400_000)
        .random_from(0);
    let runtime = Runtime::new().expect("runtime");
    let (readers, writers) = runtime.block_on(async {
        let readers = call(
            &program,
            &host,
            "create_club",
            vec![Value::text("Readers")],
        )
        .await;
        let writers = call(
            &program,
            &host,
            "create_club",
            vec![Value::text("Writers")],
        )
        .await;
        (readers, writers)
    });
    let id = |club: &Value| match club {
        Value::Record(record) => record.get("id").cloned().expect("an id"),
        other => panic!("not a club: {other:?}"),
    };
    runtime.block_on(async {
        for (club, email) in [
            (&readers, "ann@club.example"),
            (&readers, "bo@club.example"),
            (&writers, "cy@club.example"),
        ] {
            call(
                &program,
                &host,
                "enrol",
                vec![id(club), Value::text(email)],
            )
            .await;
        }
    });
    let ann = member_id(&host, &runtime, "ann@club.example");
    let bo = member_id(&host, &runtime, "bo@club.example");
    let cy = member_id(&host, &runtime, "cy@club.example");
    runtime.block_on(async {
        // A link each, as a first sign-in through Google keeps it.
        for member in [&ann, &bo, &cy] {
            host.rows(&format!(
                "INSERT INTO \"jadpo_sign_in_links\" (\"method\", \"subject\", \"user_id\", \"linked_at\") VALUES ('google', '{member}', '{member}', now()) RETURNING 1"
            ))
            .await;
        }
        // Two sessions each, a browser session and an API token, and the
        // link.
        assert_eq!(sign_in_rows(&host, &ann).await, 3);
        let member = Value::uuid(&ann);
        call(&program, &host, "leave", vec![member]).await;
        assert_eq!(sign_in_rows(&host, &ann).await, 0);
        assert_eq!(sign_in_rows(&host, &bo).await, 3);
        // Closing a club deletes its members, and their sign-in state goes
        // with each of them; another club's members keep theirs.
        call(&program, &host, "close", vec![id(&readers)]).await;
        assert_eq!(sign_in_rows(&host, &bo).await, 0);
        assert_eq!(sign_in_rows(&host, &cy).await, 3);
        assert_eq!(
            count(&host, "SELECT count(*) FROM \"member\"").await,
            1
        );
    });
}

//! `on_first_sign_in:` for external tokens (area 8, "A person the app has
//! not seen"; chunk A7): a token whose subject no user has runs the
//! action once, as the system, and the link it makes signs every later
//! request in through one authority read, over real PostgreSQL with the
//! local issuer in the configured one's place.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_host_native::{NativeHost, Postgres, Runtime};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::Host;
use jadpo_runtime::http::local_issuer::LocalIssuer;
use jadpo_runtime::http::{Request, Response, Worker, serve_request};

use super::external_tokens::WireCounting;

const ISSUER: &str = "https://id.lending.example/";
const AUDIENCE: &str = "lending-desk";

const FILES: [(&str, &str); 10] = [
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
        subject: Librarian.subject
        active: Librarian.active == true
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    database_url: PostgresUrl
    issuer: Url
    audience: Text
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"use desk

authentication {
    external_token: {
        issuer: config.issuer
        audience: config.audience
    }
    on_first_sign_in: desk.take_on_newcomer
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
        "domains/desk/failures/sign_up_closed.jadpo",
        r#"/// The desk takes no one on that way.
type SignUpClosed = Failure {
    kind: FailureKind.conflict
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
        "domains/desk/actions/take_on_newcomer.jadpo",
        r#"use desk/entities { Librarian }
use desk/failures { SignUpClosed }

/// Takes on someone the provider vouches for, under a staff subject of
/// the desk's own, so they sign in again through the link; held back
/// when the provider's subject says they're pending.
action take_on_newcomer(first: FirstSignIn) fails SignUpClosed -> Librarian.id {
    match first.subject {
        none -> {
            reject SignUpClosed {}
        }
        some(subject) -> {
            if (subject == "closed" or first.method != SignInMethod.external_token) {
                reject SignUpClosed {}
            }
            var librarian: Librarian = attempt Librarian.create {
                set: {
                    subject: "staff-" + subject
                    name: "Newcomer"
                    active: subject != "pending"
                }
                conflict: {
                    Librarian.subject: SignUpClosed
                }
            }
            return librarian.id
        }
    }
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
        "domains/desk/queries/me.jadpo",
        r#"use desk/entities { Librarian }
use desk/types { LibrarianCard }
use desk/failures { NotALibrarian }

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
        r#"use desk/types { LibrarianCard }
use desk/queries { me }

/// What staff reach with their provider's token.
routes Staff {
    prefix: /staff
    GET /me {
        output: LibrarianCard
        run: me(current_principal.user)
    }
}
"#,
    ),
];

/// Runs `futures` together on this thread, each to its end: requests
/// that arrive at once.
async fn all<F: std::future::Future>(futures: Vec<F>) -> Vec<F::Output> {
    let mut pending: Vec<std::pin::Pin<Box<F>>> = futures.into_iter().map(Box::pin).collect();
    let mut done: Vec<Option<F::Output>> = pending.iter().map(|_| None).collect();
    std::future::poll_fn(|cx| {
        for (future, slot) in pending.iter_mut().zip(done.iter_mut()) {
            if slot.is_none()
                && let std::task::Poll::Ready(output) = future.as_mut().poll(cx)
            {
                *slot = Some(output);
            }
        }
        if done.iter().all(Option::is_some) {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    })
    .await;
    done.into_iter().flatten().collect()
}

fn project() -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("external-first-sign-in");
    let _ = std::fs::remove_dir_all(&root);
    for (path, source) in FILES {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, source).expect("source");
    }
    for args in [&["migrate", "new", "initial"][..], &["api", "update"][..]] {
        let output = super::http::jadpo(&root, args);
        assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    }
    root
}

#[test]
fn an_external_subject_no_user_has_is_taken_on_once_and_linked() {
    if !jadpo_test::live() {
        return;
    }
    let root = project();
    let loaded = jadpo_db::load(&root);
    let prepared = Prepared::load(&loaded.db, loaded.project).expect("checks");
    let server = jadpo_test::database::server().expect("server");
    let url = server
        .create_database("external_first_sign_in")
        .expect("database");
    super::deployable::apply(&url, &super::deployable::migrations(&root));
    let counting = WireCounting::default();
    let store = Postgres::new(&url, Rc::new(counting.clone())).expect("store");
    let random = |bytes: &mut [u8]| NativeHost::new().random(bytes);
    let issuer = LocalIssuer::new(&random).expect("a local issuer");
    let tokens = LocalIssuer::from_seed(issuer.seed()).expect("the same issuer");
    let host = NativeHost::new()
        .with_store("main", store)
        .with_local_issuer(issuer);
    let mut variables = BTreeMap::new();
    variables.insert(
        "DATABASE_URL".into(),
        "postgresql://fixture@127.0.0.1:5432/local".into(),
    );
    variables.insert("ISSUER".into(), ISSUER.into());
    variables.insert("AUDIENCE".into(), AUDIENCE.into());
    let config = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|bytes| host.random(bytes),
        )
        .expect("config");
    let program = Program::new(prepared).with_config(&config);
    let runtime = Runtime::new().expect("runtime");
    let worker = Worker::default();
    let request = |subject: &str| Request {
        method: "GET".into(),
        path: "/staff/me".into(),
        headers: vec![(
            "authorization".into(),
            format!(
                "Bearer {}",
                tokens
                    .token(ISSUER, AUDIENCE, subject, host.now_ms().expect("clock"))
                    .expect("a token")
            ),
        )],
        ..Request::default()
    };
    let send = |subject: &str| -> Response {
        runtime.block_on(serve_request(
            &program.app(),
            &program,
            &host,
            &worker,
            &request(subject),
        ))
    };
    let shown = |response: &Response| {
        format!(
            "{} {:?} {}",
            response.status,
            response.code,
            String::from_utf8_lossy(&response.body)
        )
    };
    let rows = |sql: &str| super::deployable::rows(&url, sql);

    // The first token for a subject no user has: the action takes them
    // on, and the request is theirs.
    let first = send("ann");
    assert_eq!(first.status, 200, "{}", shown(&first));
    let card: serde_json::Value = serde_json::from_slice(&first.body).expect("JSON");
    assert_eq!(card["name"], "Newcomer");
    assert_eq!(
        rows(
            "SELECT l.\"method\", l.\"subject\", r.\"subject\" FROM \"jadpo_sign_in_links\" AS l JOIN \"librarian\" AS r ON r.\"id\" = l.\"user_id\""
        ),
        [["external_token", "ann", "staff-ann"]]
    );

    // Later requests sign in through the link, in one authority read,
    // and the action never runs again.
    let before = counting.trips.get();
    let again = send("ann");
    assert_eq!(again.status, 200, "{}", shown(&again));
    assert_eq!(
        counting.trips.get() - before,
        3,
        "one authority read plus the query's two exchanges"
    );
    assert_eq!(super::deployable::count(&url, "\"librarian\""), 1);

    // Several first requests at once, as a page's first load sends them:
    // one takes the newcomer on, and every one is theirs.
    let together: Vec<Request> = (0..4).map(|_| request("bo")).collect();
    let app = program.app();
    let answered = runtime.block_on(all(together
        .iter()
        .map(|request| serve_request(&app, &program, &host, &worker, request))
        .collect()));
    for response in &answered {
        assert_eq!(response.status, 200, "{}", shown(response));
    }
    assert_eq!(
        rows("SELECT count(*) FROM \"librarian\" WHERE \"subject\" = 'staff-bo'"),
        [["1"]]
    );

    // A refusal is the same `sign_in_refused` whatever the reason, and
    // links no one.
    let refused = send("closed");
    assert_eq!(refused.status, 403, "{}", shown(&refused));
    assert_eq!(refused.code.as_deref(), Some("sign_in_refused"));
    assert_eq!(
        rows("SELECT count(*) FROM \"jadpo_sign_in_links\" WHERE \"subject\" = 'closed'"),
        [["0"]]
    );

    // A user the action made but left inactive is linked and disabled,
    // and signs in once they are made active.
    let pending = send("pending");
    assert_eq!(
        pending.code.as_deref(),
        Some("user_disabled"),
        "{}",
        shown(&pending)
    );
    rows(
        "UPDATE \"librarian\" SET \"active\" = true WHERE \"subject\" = 'staff-pending' RETURNING 1",
    );
    let approved = send("pending");
    assert_eq!(approved.status, 200, "{}", shown(&approved));
    assert_eq!(super::deployable::count(&url, "\"librarian\""), 3);

    // A user provisioned under the token's subject signs in directly, with
    // no link.
    rows(
        "INSERT INTO \"librarian\" (\"id\", \"subject\", \"name\", \"active\") VALUES ('00000000-0000-4000-8000-000000000007', 'cy', 'Cy', true) RETURNING 1",
    );
    let direct = send("cy");
    assert_eq!(direct.status, 200, "{}", shown(&direct));
    let card: serde_json::Value = serde_json::from_slice(&direct.body).expect("JSON");
    assert_eq!(card["name"], "Cy");
    assert_eq!(
        rows("SELECT count(*) FROM \"jadpo_sign_in_links\" WHERE \"subject\" = 'cy'"),
        [["0"]]
    );
}

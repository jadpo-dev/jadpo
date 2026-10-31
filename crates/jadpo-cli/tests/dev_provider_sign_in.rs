//! Signing in through a provider in `jadpo dev` (area 8, "Providers", "In
//! `jadpo dev`"; area 14, `jadpo dev`; chunk A7c): with none of Google's
//! settings given, the session starts, the start route sends the browser
//! to the local issuer's account-chooser page, and choosing or adding a
//! test account there comes back to the callback signed in, as Google's
//! own page would; cancelling there comes back refused. The test accounts
//! are kept under `build/dev/` for the next session.

use std::path::PathBuf;

use super::http::{Dev, Reply, jadpo, tempdir, write};

/// The browser session's origin, as `config.site` gives it.
const SITE: &str = "http://localhost:3000";

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use desk/entities { Librarian }

application {
    display_name: "Lending desk"
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
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
        subject: Librarian.email
        active: Librarian.active == true
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    database_url: PostgresUrl
    google_client_id: Text
    google_client_secret: Text {
        secret: true
    }
    google_issuer: Url
    site: Url {
        default: "http://localhost:3000"
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"use desk

authentication {
    browser_session: {
        cookie: "desk_session"
        origin: config.site
    }
    sign_in: {
        providers: [google]
    }
    on_first_sign_in: desk.link_staff
}
"#,
    ),
    (
        "domains/desk/entities/librarian.jadpo",
        r#"entity Librarian {
    id: Uuid
    email: Email {
        unique: true
    }
    name: Text
    active: Bool

    policy: {
        create: [Access.system]
        read: [id, Access.system]
    }
}
"#,
    ),
    (
        "domains/desk/failures/not_on_the_staff.jadpo",
        r#"/// The person isn't on the staff.
type NotOnTheStaff = Failure {
    kind: FailureKind.not_found
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
        "domains/desk/queries/staff_member.jadpo",
        r#"use desk/entities { Librarian }
use desk/failures { NotOnTheStaff }

/// The member of staff with an address.
query staff_member(address: Email) fails NotOnTheStaff -> Librarian {
    from: Librarian
    where: Librarian.email == address
    missing: NotOnTheStaff
}
"#,
    ),
    (
        "domains/desk/actions/link_staff.jadpo",
        r#"use desk/entities { Librarian }
use desk/failures { NotOnTheStaff }
use desk/queries { staff_member }

/// Links a Google account to the member of staff with its address, once
/// Google has verified it.
action link_staff(first: FirstSignIn) fails NotOnTheStaff -> Librarian.id {
    if (first.method != SignInMethod.google or not first.address_verified) {
        reject NotOnTheStaff {}
    }
    match first.address {
        none -> {
            reject NotOnTheStaff {}
        }
        some(address) -> {
            var librarian: Librarian = attempt staff_member(address)
            return librarian.id
        }
    }
}
"#,
    ),
    (
        "domains/desk/queries/me.jadpo",
        r#"use desk/entities { Librarian }
use desk/types { LibrarianCard }
use desk/failures { NotOnTheStaff }

/// The signed-in member of staff.
query me(user: Principal.user) fails NotOnTheStaff -> LibrarianCard {
    from: Librarian
    where: Librarian.id == user.id
    missing: NotOnTheStaff
}
"#,
    ),
    (
        "domains/desk/routes/staff.jadpo",
        r#"use desk/types { LibrarianCard }
use desk/queries { me }

/// What a signed-in member of staff reaches.
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

fn project() -> (tempdir::Dir, PathBuf) {
    let dir = tempdir::Dir::new("dev-provider-sign-in");
    let root = dir.path().join("desk");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    for args in [
        &["migrate", "new", "initial"][..],
        &["api", "update"],
        &["check"],
    ] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    (dir, root)
}

/// A librarian taken on under `email`, written straight into the
/// session's database.
fn librarian(url: &str, email: &str, name: &str) {
    use jadpo_runtime::{Database, SqlValue};
    let database = jadpo_host_native::postgres(url).expect("the database");
    let runtime = jadpo_host_native::Runtime::new().expect("a runtime");
    runtime
        .block_on(database.query_once(
            "INSERT INTO librarian (id, email, name, active) VALUES (gen_random_uuid(), $1, $2, true)",
            &[
                SqlValue::Text(email.to_string()),
                SqlValue::Text(name.to_string()),
            ],
        ))
        .expect("inserted");
}

/// The value `reply` sets cookie `name` to.
fn cookie(reply: &Reply, name: &str) -> Option<String> {
    reply.headers("set-cookie").into_iter().find_map(|value| {
        let (key, value) = value.split(';').next()?.split_once('=')?;
        (key.trim() == name && !value.is_empty()).then(|| value.to_string())
    })
}

/// A sign-in started from the app's own page: the chooser's path and
/// query, and the binding cookie.
fn start(dev: &Dev) -> (String, String) {
    let started = dev.send(
        "POST",
        "/auth/google/start",
        &[("origin", SITE)],
        r#"{"return_to":"/books"}"#,
    );
    assert_eq!(started.status, 200, "{started:?}");
    let address = started.json()["address"]
        .as_str()
        .expect("an address")
        .to_string();
    let page = dev.sign_in_page();
    let path = address
        .strip_prefix(&page)
        .unwrap_or_else(|| panic!("{address} isn't on the chooser at {page}"))
        .to_string();
    assert!(path.starts_with("/google/authorize?"), "{path}");
    let binding = cookie(&started, "desk_session_provider").expect("the binding cookie");
    (path, binding)
}

/// The choice `form` posted from the chooser page itself, answered with
/// a `303` back to the app: the callback's path and query.
fn choose(dev: &Dev, path: &str, form: &str) -> String {
    let query = path.split_once('?').expect("a query").1;
    let origin = dev.sign_in_page();
    let chosen = dev.send_sign_in(
        "POST",
        "/google/authorize",
        &[
            ("origin", origin.as_str()),
            ("content-type", "application/x-www-form-urlencoded"),
        ],
        &format!("{query}&{form}"),
    );
    assert_eq!(chosen.status, 303, "{chosen:?}");
    let location = chosen.header("location").expect("back to the app");
    location
        .strip_prefix(SITE)
        .unwrap_or_else(|| panic!("{location} isn't the app's callback"))
        .to_string()
}

#[test]
fn jadpo_dev_signs_a_member_of_staff_in_with_google_through_the_chooser() {
    if !jadpo_test::live() {
        return;
    }
    let (_dir, root) = project();
    // No Google settings at all: the session starts with stand-ins and
    // its local issuer in Google's place.
    let dev = Dev::start(&root);
    let page = dev.sign_in_page();
    assert!(page.starts_with("http://localhost:"), "{page}");
    librarian(dev.url(), "ann@desk.example", "Ann");

    // The first time, there is no test account: the page offers to add one.
    let (path, binding) = start(&dev);
    let empty = dev.send_sign_in("GET", &path, &[], "");
    assert_eq!(empty.status, 200, "{empty:?}");
    assert!(
        empty
            .body()
            .contains("No test accounts for <code>google</code> yet"),
        "{}",
        empty.body()
    );
    let callback = choose(
        &dev,
        &path,
        "choice=add&name=Ann%20Example&email=ann%40desk.example&email_verified=true&subject=104829365172093846251",
    );
    assert!(
        callback.starts_with("/auth/google/callback?code="),
        "{callback}"
    );
    let back = dev.send(
        "GET",
        &callback,
        &[("cookie", &format!("desk_session_provider={binding}"))],
        "",
    );
    assert_eq!(back.status, 303, "{back:?}");
    assert_eq!(back.header("location"), Some("/books"));
    let session = cookie(&back, "desk_session").expect("a browser session");
    let me = dev.send(
        "GET",
        "/staff/me",
        &[("cookie", &format!("desk_session={session}"))],
        "",
    );
    assert_eq!(me.status, 200, "{me:?}");
    assert_eq!(me.json()["name"], "Ann");
    let kept = std::fs::read_to_string(root.join("build/dev/accounts.json")).expect("kept");
    assert!(kept.contains("104829365172093846251"), "{kept}");

    // The next time Ann is on the page, and choosing her signs in through
    // the link the first sign-in made.
    let (path, binding) = start(&dev);
    let listed = dev.send_sign_in("GET", &path, &[], "");
    assert!(listed.body().contains("Ann Example"), "{}", listed.body());
    let callback = choose(&dev, &path, "account=104829365172093846251");
    let back = dev.send(
        "GET",
        &callback,
        &[("cookie", &format!("desk_session_provider={binding}"))],
        "",
    );
    assert_eq!(back.header("location"), Some("/books"), "{back:?}");
    assert!(cookie(&back, "desk_session").is_some(), "{back:?}");

    // Cancelling comes back refused, as the provider's own refusal does.
    let (path, binding) = start(&dev);
    let callback = choose(&dev, &path, "choice=cancel");
    assert!(callback.contains("error=access_denied"), "{callback}");
    let back = dev.send(
        "GET",
        &callback,
        &[("cookie", &format!("desk_session_provider={binding}"))],
        "",
    );
    assert_eq!(
        back.header("location"),
        Some("/books?error=sign_in_provider_failed"),
        "{back:?}"
    );
    assert!(cookie(&back, "desk_session").is_none(), "{back:?}");

    // A choice from another site's page is refused, and nothing comes back.
    let (path, _) = start(&dev);
    let query = path.split_once('?').expect("a query").1;
    let forged = dev.send_sign_in(
        "POST",
        "/google/authorize",
        &[
            ("origin", "http://evil.example"),
            ("content-type", "application/x-www-form-urlencoded"),
        ],
        &format!("{query}&account=104829365172093846251"),
    );
    assert_eq!(forged.status, 403, "{forged:?}");
    assert!(forged.header("location").is_none());
    drop(dev);

    // The accounts outlast the session.
    let kept = std::fs::read_to_string(root.join("build/dev/accounts.json")).expect("kept");
    assert!(kept.contains("ann@desk.example"), "{kept}");
}

//! Passkeys against live PostgreSQL (area 8, "Passkeys"; chunk 5.5z): the
//! runtime answers the generated routes as a deployable does
//! (`http::serve_request`) on the native host, whose ceremonies are
//! `webauthn-rs`'s, with a clock the test moves and a software
//! authenticator (`jadpo_host_native::test_authenticator`) standing in for
//! the browser and the device:
//!
//! - a person registers a passkey, then signs in with it;
//! - a replayed answer, one from another site, one whose challenge has
//!   expired and one from another browser are refused;
//! - a removed passkey can't sign in;
//! - a counter that goes backwards is refused, and a passkey that keeps
//!   none is not;
//! - a person has several passkeys, renames and removes their own and
//!   never another's; a device that holds one already makes no second;
//! - a disabled person signs in with none, and every public write needs
//!   the application's own `Origin`.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live tests.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use jadpo_host_native::test_authenticator::TestAuthenticator;
use jadpo_host_native::{NativeHost, Postgres, Runtime};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::http::{Request, Response, Worker, serve_request};
use jadpo_runtime::{Database, Event, Host, LocalFuture};

fn skipped() -> bool {
    let skip = std::env::var("JADPO_SKIP_POSTGRES").is_ok_and(|value| value == "1");
    if skip {
        println!("skipped: JADPO_SKIP_POSTGRES is set");
    }
    skip
}

/// The native host, with a wall clock the test sets.
struct Clocked {
    inner: NativeHost,
    now: Cell<i64>,
}

impl Host for Clocked {
    fn passkeys(&self) -> Option<&dyn jadpo_runtime::Passkeys> {
        self.inner.passkeys()
    }

    fn database(&self, store: &str) -> Option<&dyn Database> {
        self.inner.database(store)
    }

    fn deliver(&self, events: Vec<Event>) -> LocalFuture<'_, ()> {
        self.inner.deliver(events)
    }

    fn random(&self, bytes: &mut [u8]) -> bool {
        self.inner.random(bytes)
    }

    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        self.inner.sleep(millis)
    }

    fn monotonic_ms(&self) -> Option<u64> {
        self.inner.monotonic_ms()
    }

    fn now_ms(&self) -> Option<i64> {
        Some(self.now.get())
    }
}

/// 2026-10-18T09:00:00Z.
const START: i64 = 1_792_314_000_000;
const MINUTE: i64 = 60_000;
const ORIGIN: &str = "https://notes.example";

const FILES: &[(&str, &str)] = &[
    ("config.jadpo", "config {\n    browser_origin: Url\n}\n"),
    (
        "app.jadpo",
        r#"use notes/entities { Member }
use notes/types { MemberStatus }

application {}

principal {
    user: {
        subject: Member.email
        active: Member.status == MemberStatus.active
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    browser_session: {
        cookie: "notes_session"
        origin: config.browser_origin
    }
    sign_in: {
        passkey: {
            expires_after: 2m
        }
    }
}
"#,
    ),
    (
        "policy.jadpo",
        "use notes\n\npolicy {\n    actions: {\n        notes.sign_up: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/notes/entities/member.jadpo",
        r#"use notes/types { MemberStatus }

entity Member {
    id: Uuid
    email: Email {
        unique: true
    }
    status: MemberStatus

    policy: {
        create: [Access.public]
        read: [id]
        update: [id]
    }
}
"#,
    ),
    (
        "domains/notes/actions/sign_up.jadpo",
        r#"use notes/entities { Member }
use notes/types { MemberStatus }
use notes/failures { EmailTaken }

/// Registers a member and signs this browser in.
action sign_up(input: SignUp) fails EmailTaken -> Unit {
    var member: Member = attempt Member.create {
        set: {
            email: input.email
            status: MemberStatus.active
        }
        conflict: {
            Member.email: EmailTaken
        }
    }
    sessions.start(member.id)
}

type SignUp = Input {
    email: Email
}
"#,
    ),
    (
        "domains/notes/failures/email_taken.jadpo",
        "type EmailTaken = Failure {\n    kind: FailureKind.conflict\n    message: \"That address has an account.\"\n}\n",
    ),
    (
        "domains/notes/routes/account.jadpo",
        r#"use notes/actions { SignUp, sign_up }

routes Account {
    prefix: /account

    POST /sign-up {
        authentication: RouteAuthentication.none
        input: SignUp
        status: SuccessStatus.no_content
        run: sign_up(input)
    }
}
"#,
    ),
    (
        "domains/notes/types/member_status.jadpo",
        "type MemberStatus = Enum {\n    active\n    suspended\n}\n",
    ),
];

fn jadpo(cwd: &Path, args: &[&str]) {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = jadpo_term::Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", "1792504800")
        .with_cwd(cwd);
    let done = jadpo_cli::run(&args, &env);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
}

/// The project, migrated into a database of its own and served by the
/// runtime at [`ORIGIN`].
struct Live {
    dir: PathBuf,
    program: Program,
    host: Clocked,
    runtime: Runtime,
    worker: Worker,
}

impl Live {
    fn new(name: &str) -> Live {
        let dir =
            std::env::temp_dir().join(format!("jadpo-passkeys-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("notes");
        for (path, text) in FILES {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
            std::fs::write(path, text).expect("write");
        }
        jadpo(&root, &["migrate", "new", "initial"]);
        jadpo(&root, &["api", "update"]);
        jadpo(&root, &["check"]);
        let loaded = jadpo_db::load(&root);
        let prepared = Prepared::load(&loaded.db, loaded.project).expect("checks");
        let set = jadpo_check::data::migrations::migrations(&loaded.db, loaded.project);
        let steps = jadpo_check::data::migrations::store_steps(set);
        let server = jadpo_test::database::server().expect("a PostgreSQL server");
        let url = server
            .create_database(&format!("passkeys_{name}"))
            .expect("a database");
        let store = Postgres::new(&url, std::rc::Rc::new(jadpo_host_native::NativeTransport))
            .expect("a store");
        let runtime = Runtime::new().expect("a runtime");
        runtime
            .block_on(jadpo_runtime::migrate(
                &store,
                steps.get("main").expect("main's migrations"),
            ))
            .expect("migrates");
        let mut variables = BTreeMap::new();
        variables.insert("BROWSER_ORIGIN".to_string(), ORIGIN.to_string());
        let values = prepared
            .config(
                &variables,
                jadpo_runtime::config::Mode::Environment,
                &|bytes: &mut [u8]| NativeHost::new().random(bytes),
            )
            .expect("the settings check");
        let program = Program::new(Arc::clone(&prepared)).with_config(&values);
        Live {
            dir,
            program,
            host: Clocked {
                inner: NativeHost::new().with_store("main", store),
                now: Cell::new(START),
            },
            runtime,
            worker: Worker::default(),
        }
    }

    fn at(&self, now: i64) {
        self.host.now.set(now);
    }

    fn send(&self, request: &Request) -> Response {
        let app = self.program.app();
        self.runtime.block_on(serve_request(
            &app,
            &self.program,
            &self.host,
            &self.worker,
            request,
        ))
    }

    fn rows(&self, statement: &str) -> Vec<jadpo_runtime::Row> {
        let database = self.host.database("main").expect("main");
        self.runtime
            .block_on(database.query_once(statement, &[]))
            .expect("runs")
    }

    /// Signs a new member up in a browser of their own.
    fn sign_up(&self, email: &str) -> Browser {
        let response = self.send(&request(
            "POST",
            "/account/sign-up",
            &[("origin", ORIGIN)],
            &format!("{{\"email\":\"{email}\"}}"),
        ));
        assert_eq!(response.status, 204, "{}", text(&response));
        let mut browser = Browser::default();
        browser.keep(&response);
        assert!(browser.signed_in(), "{:?}", response.headers);
        browser
    }

    /// Adds a passkey on `device`, from `browser`, named `name`.
    fn register(
        &self,
        browser: &mut Browser,
        device: &mut TestAuthenticator,
        name: &str,
    ) -> Response {
        let options = self.send(&browser.write("POST", "/auth/passkey/register/options", ""));
        assert_eq!(options.status, 200, "{}", text(&options));
        let credential = device
            .create(&text(&options))
            .expect("the device makes one");
        let response = self.send(&browser.write(
            "POST",
            "/auth/passkey/register",
            &format!("{{\"name\":\"{name}\",\"credential\":{credential}}}"),
        ));
        browser.keep(&response);
        response
    }

    /// The sign-in options `browser` is given, and the device's answer.
    fn answer(&self, browser: &mut Browser, device: &mut TestAuthenticator) -> String {
        let options = self.send(&browser.public("POST", "/auth/passkey/sign-in/options", ""));
        assert_eq!(options.status, 200, "{}", text(&options));
        browser.keep(&options);
        device.get(&text(&options)).expect("the device answers")
    }

    /// Posts `answer` from `browser`.
    fn finish(&self, browser: &mut Browser, answer: &str) -> Response {
        let response = self.send(&browser.public(
            "POST",
            "/auth/passkey/sign-in",
            &format!("{{\"credential\":{answer}}}"),
        ));
        browser.keep(&response);
        response
    }

    /// Signs in on `device` from a new browser.
    fn sign_in(&self, device: &mut TestAuthenticator) -> (Browser, Response) {
        let mut browser = Browser::default();
        let answer = self.answer(&mut browser, device);
        let response = self.finish(&mut browser, &answer);
        (browser, response)
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A browser's cookies, as it keeps them from what responses set.
#[derive(Debug, Default, Clone)]
struct Browser {
    cookies: BTreeMap<String, String>,
}

impl Browser {
    fn keep(&mut self, response: &Response) {
        for (name, value) in &response.headers {
            if !name.eq_ignore_ascii_case("set-cookie") {
                continue;
            }
            let pair = value.split(';').next().unwrap_or_default();
            if let Some((key, value)) = pair.split_once('=') {
                if value.is_empty() {
                    self.cookies.remove(key);
                } else {
                    self.cookies.insert(key.to_string(), value.to_string());
                }
            }
        }
    }

    fn signed_in(&self) -> bool {
        self.cookies.contains_key("notes_session")
    }

    fn cookie_header(&self) -> String {
        self.cookies
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// A request from the application's own page, with the cookies.
    fn public(&self, method: &str, path: &str, body: &str) -> Request {
        let cookies = self.cookie_header();
        let mut headers = vec![("origin", ORIGIN)];
        if !cookies.is_empty() {
            headers.push(("cookie", cookies.as_str()));
        }
        request(method, path, &headers, body)
    }

    /// A signed-in write: the cookies, the CSRF token and the origin.
    fn write(&self, method: &str, path: &str, body: &str) -> Request {
        let cookies = self.cookie_header();
        let csrf = self
            .cookies
            .get("notes_session_csrf")
            .cloned()
            .unwrap_or_default();
        request(
            method,
            path,
            &[
                ("origin", ORIGIN),
                ("cookie", cookies.as_str()),
                ("x-jadpo-csrf", csrf.as_str()),
            ],
            body,
        )
    }
}

fn request(method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Request {
    let mut headers: Vec<(String, String)> = headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    if !body.is_empty() {
        headers.push(("content-type".into(), "application/json".into()));
    }
    Request {
        method: method.into(),
        path: path.into(),
        headers,
        body: body.as_bytes().to_vec(),
        address: Some("10.0.0.1".into()),
        ..Request::default()
    }
}

fn text(response: &Response) -> String {
    String::from_utf8_lossy(&response.body).into_owned()
}

fn json(response: &Response) -> serde_json::Value {
    serde_json::from_slice(&response.body)
        .unwrap_or_else(|_| panic!("not JSON ({}): {}", response.status, text(response)))
}

fn code(response: &Response) -> String {
    json(response)["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn refused(response: &Response, status: u16, wanted: &str) {
    assert_eq!(
        (response.status, code(response).as_str()),
        (status, wanted),
        "{}",
        text(response)
    );
}

#[test]
fn a_person_registers_a_passkey_and_signs_in_with_it() {
    if skipped() {
        return;
    }
    let live = Live::new("register");
    let mut ada = live.sign_up("ada@example.com");
    let mut phone = TestAuthenticator::new(ORIGIN);

    // The options name the configured relying party, never the request's.
    let options = live.send(&ada.write("POST", "/auth/passkey/register/options", ""));
    assert_eq!(options.status, 200, "{}", text(&options));
    assert_eq!(options.header("cache-control"), Some("no-store"));
    let key = &json(&options)["publicKey"];
    assert_eq!(key["rp"]["id"], "notes.example");
    assert_eq!(key["user"]["name"], "ada@example.com");
    assert_eq!(key["authenticatorSelection"]["residentKey"], "required");
    assert_eq!(
        key["authenticatorSelection"]["userVerification"],
        "required"
    );
    assert_eq!(key["attestation"], "none");
    assert_eq!(key["timeout"], 120_000);

    let added = live.register(&mut ada, &mut phone, "Work phone");
    assert_eq!(added.status, 201, "{}", text(&added));
    let view = json(&added);
    assert_eq!(view["name"], "Work phone");
    assert_eq!(view["registered_at"], "2026-10-18T09:00:00Z");
    assert_eq!(view["last_used_at"], serde_json::Value::Null);
    assert!(view.get("credential_id").is_none() && view.get("public_key").is_none());

    // What is kept is a `WebAuthnCredential`: a public key, never a secret.
    let kept = live.rows("SELECT \"credential\"::text FROM \"jadpo_passkeys\"");
    let [row] = kept.as_slice() else {
        panic!("{kept:?}")
    };
    let jadpo_runtime::SqlValue::Text(credential) = &row[0] else {
        panic!("{row:?}")
    };
    let credential: serde_json::Value = serde_json::from_str(credential).expect("json");
    let mut parts: Vec<&str> = credential
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    parts.sort_unstable();
    assert_eq!(
        parts,
        [
            "backed_up",
            "backup_eligible",
            "credential_id",
            "name",
            "public_key",
            "registered_at",
            "sign_count",
            "transports"
        ]
    );

    // Signing in in another browser starts a session there.
    live.at(START + MINUTE);
    let (mut laptop, signed) = live.sign_in(&mut phone);
    assert_eq!(signed.status, 204, "{}", text(&signed));
    assert!(laptop.signed_in(), "{:?}", signed.headers);
    assert!(!laptop.cookies.contains_key("notes_session_passkey"));
    let listed = live.send(&laptop.public("GET", "/auth/passkey", ""));
    assert_eq!(listed.status, 200, "{}", text(&listed));
    let listed = json(&listed);
    assert_eq!(listed[0]["name"], "Work phone");
    assert_eq!(listed[0]["last_used_at"], "2026-10-18T09:01:00Z");
    // The counter went up.
    let count = live.rows("SELECT \"credential\" ->> 'sign_count' FROM \"jadpo_passkeys\"");
    assert_eq!(
        count[0][0],
        jadpo_runtime::SqlValue::Text("1".to_string()),
        "{count:?}"
    );
    laptop.cookies.clear();
}

#[test]
fn a_replayed_answer_is_refused() {
    if skipped() {
        return;
    }
    let live = Live::new("replay");
    let mut ada = live.sign_up("ada@example.com");
    let mut phone = TestAuthenticator::new(ORIGIN);
    assert_eq!(live.register(&mut ada, &mut phone, "Phone").status, 201);
    let mut browser = Browser::default();
    let answer = live.answer(&mut browser, &mut phone);
    let binding = browser.clone();
    assert_eq!(live.finish(&mut browser, &answer).status, 204);
    // The same answer again, with the same cookie: the challenge is gone.
    let mut again = binding;
    refused(
        &live.finish(&mut again, &answer),
        401,
        "passkey_challenge_expired",
    );
    assert!(!again.signed_in());
}

#[test]
fn an_answer_made_for_another_site_is_refused() {
    if skipped() {
        return;
    }
    let live = Live::new("origin");
    let mut ada = live.sign_up("ada@example.com");
    let mut phone = TestAuthenticator::new(ORIGIN);
    assert_eq!(live.register(&mut ada, &mut phone, "Phone").status, 201);
    // A look-alike site relays the real options, and the device answers
    // for the page it is on.
    phone.at("https://notes-example.evil");
    let (browser, response) = live.sign_in(&mut phone);
    refused(&response, 401, "passkey_rejected");
    assert!(!browser.signed_in());
    // A passkey can't be registered from another site either.
    let mut laptop = TestAuthenticator::new("https://notes.example:8443");
    let added = live.register(&mut ada, &mut laptop, "Laptop");
    refused(&added, 401, "passkey_rejected");
}

#[test]
fn an_expired_challenge_or_one_from_another_browser_is_refused() {
    if skipped() {
        return;
    }
    let live = Live::new("expired");
    let mut ada = live.sign_up("ada@example.com");
    let mut phone = TestAuthenticator::new(ORIGIN);
    assert_eq!(live.register(&mut ada, &mut phone, "Phone").status, 201);

    // `expires_after: 2m`.
    let mut browser = Browser::default();
    let answer = live.answer(&mut browser, &mut phone);
    live.at(START + 2 * MINUTE + 1);
    refused(
        &live.finish(&mut browser, &answer),
        401,
        "passkey_challenge_expired",
    );

    // Asked for in one browser, answered from another.
    let mut asked = Browser::default();
    let answer = live.answer(&mut asked, &mut phone);
    let mut other = Browser::default();
    let theirs = live.send(&other.public("POST", "/auth/passkey/sign-in/options", ""));
    other.keep(&theirs);
    refused(
        &live.finish(&mut other, &answer),
        401,
        "passkey_challenge_expired",
    );
    // And with no cookie at all.
    let answer = live.answer(&mut Browser::default(), &mut phone);
    refused(
        &live.finish(&mut Browser::default(), &answer),
        401,
        "passkey_challenge_expired",
    );
}

#[test]
fn a_removed_passkey_cant_sign_in() {
    if skipped() {
        return;
    }
    let live = Live::new("removed");
    let mut ada = live.sign_up("ada@example.com");
    let mut phone = TestAuthenticator::new(ORIGIN);
    let added = live.register(&mut ada, &mut phone, "Phone");
    assert_eq!(added.status, 201, "{}", text(&added));
    let id = json(&added)["id"].as_str().expect("an id").to_string();
    let removed = live.send(&ada.write("DELETE", &format!("/auth/passkey/{id}"), ""));
    assert_eq!(removed.status, 204, "{}", text(&removed));
    let (browser, response) = live.sign_in(&mut phone);
    refused(&response, 401, "passkey_rejected");
    assert!(!browser.signed_in());
    // Removed already.
    let again = live.send(&ada.write("DELETE", &format!("/auth/passkey/{id}"), ""));
    refused(&again, 404, "passkey_not_found");
}

#[test]
fn a_counter_that_goes_backwards_is_refused() {
    if skipped() {
        return;
    }
    let live = Live::new("counter");
    let mut ada = live.sign_up("ada@example.com");
    let mut key = TestAuthenticator::new(ORIGIN);
    assert_eq!(
        live.register(&mut ada, &mut key, "Security key").status,
        201
    );
    assert_eq!(live.sign_in(&mut key).1.status, 204);
    assert_eq!(live.sign_in(&mut key).1.status, 204);
    // A copy of the key, two uses behind.
    key.set_counter(0);
    refused(&live.sign_in(&mut key).1, 401, "passkey_rejected");
    let count = live.rows("SELECT \"credential\" ->> 'sign_count' FROM \"jadpo_passkeys\"");
    assert_eq!(count[0][0], jadpo_runtime::SqlValue::Text("2".to_string()));

    // A synced passkey keeps no counter and sends 0 every time.
    let mut bob = live.sign_up("bob@example.com");
    let mut synced = TestAuthenticator::new(ORIGIN);
    synced.counting(false);
    assert_eq!(live.register(&mut bob, &mut synced, "Phone").status, 201);
    assert_eq!(live.sign_in(&mut synced).1.status, 204);
    assert_eq!(live.sign_in(&mut synced).1.status, 204);
}

#[test]
fn a_person_keeps_several_passkeys_and_manages_only_their_own() {
    if skipped() {
        return;
    }
    let live = Live::new("several");
    let mut ada = live.sign_up("ada@example.com");
    let mut phone = TestAuthenticator::new(ORIGIN);
    let mut laptop = TestAuthenticator::new(ORIGIN);
    assert_eq!(live.register(&mut ada, &mut phone, "Phone").status, 201);
    live.at(START + MINUTE);
    assert_eq!(live.register(&mut ada, &mut laptop, "Laptop").status, 201);

    // A device that holds one already is excluded, so it makes no second.
    let options = live.send(&ada.write("POST", "/auth/passkey/register/options", ""));
    assert_eq!(
        json(&options)["publicKey"]["excludeCredentials"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert!(phone.create(&text(&options)).is_err());

    let listed = json(&live.send(&ada.public("GET", "/auth/passkey", "")));
    let names: Vec<&str> = listed
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|passkey| passkey["name"].as_str())
        .collect();
    assert_eq!(names, ["Phone", "Laptop"]);
    let laptop_id = listed[1]["id"].as_str().expect("an id").to_string();

    let renamed = live.send(&ada.write(
        "PATCH",
        &format!("/auth/passkey/{laptop_id}"),
        "{\"name\":\"Old laptop\"}",
    ));
    assert_eq!(renamed.status, 200, "{}", text(&renamed));
    assert_eq!(json(&renamed)["name"], "Old laptop");
    let blank = live.send(&ada.write(
        "PATCH",
        &format!("/auth/passkey/{laptop_id}"),
        "{\"name\":\" \"}",
    ));
    assert_eq!(blank.status, 422, "{}", text(&blank));

    // Bob sees none of Ada's, and can't rename or remove them.
    let bob = live.sign_up("bob@example.com");
    let theirs = live.send(&bob.public("GET", "/auth/passkey", ""));
    assert_eq!(json(&theirs), serde_json::json!([]));
    refused(
        &live.send(&bob.write(
            "PATCH",
            &format!("/auth/passkey/{laptop_id}"),
            "{\"name\":\"Mine now\"}",
        )),
        404,
        "passkey_not_found",
    );
    refused(
        &live.send(&bob.write("DELETE", &format!("/auth/passkey/{laptop_id}"), "")),
        404,
        "passkey_not_found",
    );
    // Each device signs Ada in.
    assert_eq!(live.sign_in(&mut laptop).1.status, 204);
    assert_eq!(live.sign_in(&mut phone).1.status, 204);
}

#[test]
fn a_registration_is_bound_to_the_session_that_asked() {
    if skipped() {
        return;
    }
    let live = Live::new("bound");
    let mut ada = live.sign_up("ada@example.com");
    let mut phone = TestAuthenticator::new(ORIGIN);
    assert_eq!(live.register(&mut ada, &mut phone, "Phone").status, 201);
    // Ada's second browser, signed in with her passkey.
    let (mut second, signed) = live.sign_in(&mut phone);
    assert_eq!(signed.status, 204);
    let options = live.send(&ada.write("POST", "/auth/passkey/register/options", ""));
    let mut laptop = TestAuthenticator::new(ORIGIN);
    let credential = laptop.create(&text(&options)).expect("made");
    let finished = live.send(&second.write(
        "POST",
        "/auth/passkey/register",
        &format!("{{\"name\":\"Laptop\",\"credential\":{credential}}}"),
    ));
    refused(&finished, 401, "passkey_challenge_expired");
    second.cookies.clear();
    // Signed out, nothing registers.
    let anonymous = live.send(&request(
        "POST",
        "/auth/passkey/register/options",
        &[("origin", ORIGIN)],
        "",
    ));
    refused(&anonymous, 401, "authentication_required");
}

#[test]
fn a_disabled_person_signs_in_with_no_passkey_and_writes_need_the_origin() {
    if skipped() {
        return;
    }
    let live = Live::new("disabled");
    let mut ada = live.sign_up("ada@example.com");
    let mut phone = TestAuthenticator::new(ORIGIN);
    assert_eq!(live.register(&mut ada, &mut phone, "Phone").status, 201);
    live.rows("UPDATE \"member\" SET \"status\" = 'suspended'");
    refused(&live.sign_in(&mut phone).1, 403, "user_disabled");

    // From another site, or with no `Origin`, the public routes refuse.
    for origin in [&[("origin", "https://evil.example")][..], &[]] {
        let asked = live.send(&request(
            "POST",
            "/auth/passkey/sign-in/options",
            origin,
            "",
        ));
        refused(&asked, 403, "csrf_rejected");
    }
    // A body the options route doesn't read is refused.
    let extra = live.send(&request(
        "POST",
        "/auth/passkey/sign-in/options",
        &[("origin", ORIGIN)],
        "{\"address\":\"ada@example.com\"}",
    ));
    assert_eq!(extra.status, 422, "{}", text(&extra));
}

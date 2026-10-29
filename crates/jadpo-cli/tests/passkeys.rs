//! Passkeys against live PostgreSQL (area 8, "Passkeys"; chunk 5.5z): the
//! runtime answers the generated routes as a deployable does
//! (`http::serve_request`) on the native host, with the runtime's own
//! ceremonies, a clock the test moves and a software authenticator
//! (`jadpo_runtime::test_authenticator`) standing in for the browser and
//! the device:
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

use jadpo_host_native::{NativeHost, Postgres, Runtime};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::capability::{Capability, Fake};
use jadpo_runtime::http::{Request, Response, Worker, serve_request};
use jadpo_runtime::test_authenticator::TestAuthenticator;
use jadpo_runtime::{Database, Host, LocalFuture};

/// The native host, with a wall clock the test sets and the `email`
/// connection faked.
struct Clocked {
    inner: NativeHost,
    now: Cell<i64>,
    mail: Fake,
}

impl Host for Clocked {
    fn capability(&self, connection: &str) -> Option<&dyn Capability> {
        (connection == "mail").then_some(&self.mail as &dyn Capability)
    }

    fn passkeys(&self) -> Option<&dyn jadpo_runtime::Passkeys> {
        self.inner.passkeys()
    }

    fn database(&self, store: &str) -> Option<&dyn Database> {
        self.inner.database(store)
    }

    fn defer(&self, work: LocalFuture<'static, ()>) {
        self.inner.defer(work)
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
    (
        "config.jadpo",
        r#"config {
    browser_origin: Url {
        test: "https://notes.example"
    }
    mail_sender: Email {
        default: "hello@notes.example"
    }
    smtp_host: Hostname {
        default: "localhost"
    }
}
"#,
    ),
    (
        "app.jadpo",
        r#"use notes/entities { Member }
use notes/types { MemberStatus }

application {
    display_name: "Notes"
    connections: {
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
}

principal {
    user: {
        subject: Member.handle
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
        magic_link: {
            link: "/sign-in/confirm"
        }
        passkey: {
            expires_after: 2m
        }
    }
}
"#,
    ),
    (
        "policy.jadpo",
        "use notes\n\npolicy {\n    actions: {\n        notes.sign_up: [Access.public]\n        notes.add_member: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/notes/entities/member.jadpo",
        r#"use notes/types { Handle, MemberStatus }

entity Member {
    id: Uuid
    email: Email {
        unique: true
    }
    handle: Handle {
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
use notes/types { MemberStatus, NewMember }
use notes/failures { EmailTaken }

/// Registers a member and signs this browser in.
action sign_up(input: NewMember) fails EmailTaken -> Unit {
    var member: Member = attempt Member.create {
        set: {
            email: input.email
            handle: input.handle
            status: MemberStatus.active
        }
        conflict: {
            Member.email: EmailTaken
            Member.handle: EmailTaken
        }
    }
    sessions.start(member.id)
}
"#,
    ),
    (
        "domains/notes/types/new_member.jadpo",
        "use notes/types { Handle }\n\ntype NewMember = Input {\n    email: Email\n    handle: Handle\n}\n",
    ),
    (
        "domains/notes/actions/add_member.jadpo",
        r#"use notes/entities { Member }
use notes/types { MemberStatus, NewMember }
use notes/failures { EmailTaken }

/// Registers a member, signing no one in.
action add_member(input: NewMember) fails EmailTaken -> Member {
    return attempt Member.create {
        set: {
            email: input.email
            handle: input.handle
            status: MemberStatus.active
        }
        conflict: {
            Member.email: EmailTaken
            Member.handle: EmailTaken
        }
    }
}
"#,
    ),
    (
        "domains/notes/failures/email_taken.jadpo",
        "type EmailTaken = Failure {\n    kind: FailureKind.conflict\n    message: \"That address has an account.\"\n}\n",
    ),
    (
        "domains/notes/failures/member_not_found.jadpo",
        "type MemberNotFound = Failure {\n    kind: FailureKind.not_found\n}\n",
    ),
    (
        "domains/notes/routes/account.jadpo",
        r#"use notes/actions { sign_up }
use notes/types { NewMember }

routes Account {
    prefix: /account

    POST /sign-up {
        authentication: RouteAuthentication.none
        input: NewMember
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
    (
        "domains/notes/types/handle.jadpo",
        "type Handle = Text {\n    max_length: 40\n}\n",
    ),
    (
        "domains/notes/types/note_kind.jadpo",
        "type NoteKind = Enum {\n    signed_in\n    refused\n    passkey_added\n    passkey_removed\n}\n",
    ),
    // What the application hears of sign-ins and passkeys (chunk 5.5z2).
    (
        "domains/notes/entities/sign_in_note.jadpo",
        r#"use notes/entities { Member }
use notes/types { NoteKind }

/// What the application heard of a sign-in or a passkey.
entity SignInNote {
    id: Uuid
    kind: NoteKind
    member_id: Member.id? {
        on_delete: OnDelete.cleared
    }
    method: SignInMethod?
    reason: SignInRefusal?

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
"#,
    ),
    (
        "domains/notes/actions/note_signed_in.jadpo",
        r#"use notes/entities { SignInNote }
use notes/failures { MemberNotFound }
use notes/types { NoteKind }

/// Notes a sign-in.
action note_signed_in(event: SignedIn) fails MemberNotFound -> SignInNote {
    return attempt SignInNote.create {
        set: {
            kind: NoteKind.signed_in
            member_id: event.user_id
            method: event.method
            reason: none
        }
    }
}
"#,
    ),
    (
        "domains/notes/actions/note_refused.jadpo",
        r#"use notes/entities { SignInNote }
use notes/failures { MemberNotFound }
use notes/types { NoteKind }

/// Notes a refusal.
action note_refused(event: SignInRefused) fails MemberNotFound -> SignInNote {
    return attempt SignInNote.create {
        set: {
            kind: NoteKind.refused
            member_id: event.user_id
            method: event.method
            reason: event.reason
        }
    }
}
"#,
    ),
    (
        "domains/notes/actions/note_added.jadpo",
        r#"use notes/entities { SignInNote }
use notes/failures { MemberNotFound }
use notes/types { NoteKind }

/// Notes a passkey added.
action note_added(event: PasskeyAdded) fails MemberNotFound -> SignInNote {
    return attempt SignInNote.create {
        set: {
            kind: NoteKind.passkey_added
            member_id: event.user_id
            method: none
            reason: none
        }
    }
}
"#,
    ),
    (
        "domains/notes/actions/note_removed.jadpo",
        r#"use notes/entities { SignInNote }
use notes/failures { MemberNotFound }
use notes/types { NoteKind }

/// Notes a passkey removed.
action note_removed(event: PasskeyRemoved) fails MemberNotFound -> SignInNote {
    return attempt SignInNote.create {
        set: {
            kind: NoteKind.passkey_removed
            member_id: event.user_id
            method: none
            reason: none
        }
    }
}
"#,
    ),
    (
        "domains/notes/subscribers/note_sign_ins.jadpo",
        "use notes/actions { note_signed_in }\n\n/// Notes each sign-in.\nsubscriber note_sign_ins {\n    on: SignedIn\n    run: note_signed_in(event)\n}\n",
    ),
    (
        "domains/notes/subscribers/note_refusals.jadpo",
        "use notes/actions { note_refused }\n\n/// Notes each refusal.\nsubscriber note_refusals {\n    on: SignInRefused\n    run: note_refused(event)\n}\n",
    ),
    (
        "domains/notes/subscribers/note_additions.jadpo",
        "use notes/actions { note_added }\n\n/// Notes each passkey added.\nsubscriber note_additions {\n    on: PasskeyAdded\n    run: note_added(event)\n}\n",
    ),
    (
        "domains/notes/subscribers/note_removals.jadpo",
        "use notes/actions { note_removed }\n\n/// Notes each passkey removed.\nsubscriber note_removals {\n    on: PasskeyRemoved\n    run: note_removed(event)\n}\n",
    ),
];

/// A route test driving the passkey routes with `test_passkeys` (chunk
/// 5.5z2): Ada adds a passkey and signs in with it; when it is lost she
/// is signed in another way, adds a new one, and that signs her in. Until
/// `email.sent` reads the magic link's email in a test (chunk 7.3c), `as:`
/// signs her in in its place, as it does for every route test.
const ROUTE_TESTS: &[(&str, &str)] = &[
    (
        "domains/notes/fixtures/signed_up_ada.jadpo",
        r#"use notes/actions { add_member }
use notes/entities { Member }
use notes/types { NewMember }

/// Ada, signed up, with no passkey.
public fixture signed_up_ada() -> Principal.user {
    var member: Member = attempt add_member(NewMember {
        email: "ada@example.com"
        handle: "ada"
    })
    return Principal.user(member)
}
"#,
    ),
    (
        "tests/passkeys.test.jadpo",
        r#"use notes

test "a passkey signs Ada in" {
    var ada: Principal.user = notes.signed_up_ada()
    var phone: TestPasskey = test_passkeys.register(ada, "Phone")
    var response: Response = test_passkeys.sign_in(phone)
    assert response.status == 204
    assert response.code == none
}

test "with her phone lost, Ada adds a new passkey and signs in with it" {
    var ada: Principal.user = notes.signed_up_ada()
    var phone: TestPasskey = test_passkeys.register(ada, "Phone")
    var first: Response = test_passkeys.sign_in(phone)
    assert first.status == 204
    var new_phone: TestPasskey = test_passkeys.register(ada, "New phone")
    var response: Response = test_passkeys.sign_in(new_phone)
    assert response.status == 204
}
"#,
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
        jadpo(&root, &["records", "update"]);
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
                mail: Fake::new(),
            },
            runtime,
            worker: Worker::default(),
        }
    }

    fn at(&self, now: i64) {
        self.host.now.set(now);
    }

    /// Answers `request`, then runs what it left for after the response:
    /// its events' deliveries and any email.
    fn send(&self, request: &Request) -> Response {
        let app = self.program.app();
        let response = self.runtime.block_on(serve_request(
            &app,
            &self.program,
            &self.host,
            &self.worker,
            request,
        ));
        let work = response.deferred.clone();
        self.runtime.block_on(jadpo_runtime::runner::run_deferred(
            &self.program,
            &self.host,
            work,
        ));
        response
    }

    /// What the subscribers heard, oldest first: each note's kind, the
    /// member's address, and the way in or the reason.
    fn notes(&self) -> Vec<String> {
        self.rows(
            "SELECT concat_ws(' ', n.\"kind\", coalesce(m.\"email\", '-'), n.\"method\", n.\"reason\") FROM \"sign_in_note\" AS n LEFT JOIN \"member\" AS m ON m.\"id\" = n.\"member_id\" ORDER BY n.\"ctid\"",
        )
        .into_iter()
        .map(|row| match row.first() {
            Some(jadpo_runtime::SqlValue::Text(text)) => text.clone(),
            other => panic!("not a note: {other:?}"),
        })
        .collect()
    }

    /// The six-digit code of the last email, which must have gone to
    /// `address`.
    fn code_sent_to(&self, address: &str) -> String {
        let call = self.host.mail.sent().pop().expect("an email");
        let jadpo_value::Value::Record(record) = &call.input else {
            panic!("not a message: {:?}", call.input);
        };
        let text = |name: &str| match record.get(name) {
            Some(jadpo_value::Value::Text(text)) => text.to_string(),
            other => panic!("{name}: {other:?}"),
        };
        assert_eq!(text("to"), address);
        let body = text("text");
        let (_, rest) = body.split_once("code: ").expect("a code");
        rest.chars().take(6).collect()
    }

    fn rows(&self, statement: &str) -> Vec<jadpo_runtime::Row> {
        let database = self.host.database("main").expect("main");
        self.runtime
            .block_on(database.query_once(statement, &[]))
            .expect("runs")
    }

    /// Signs a new member up in a browser of their own, their handle the
    /// address's local part.
    fn sign_up(&self, email: &str) -> Browser {
        let handle = email.split('@').next().unwrap_or_default();
        let response = self.send(&request(
            "POST",
            "/account/sign-up",
            &[("origin", ORIGIN)],
            &format!("{{\"email\":\"{email}\",\"handle\":\"{handle}\"}}"),
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
    if !jadpo_test::live() {
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
    // The device lists it under her address, the one a magic link gets
    // her back in with, rather than her handle, the principal's subject.
    assert_eq!(key["user"]["name"], "ada@example.com");
    assert_eq!(key["user"]["displayName"], "ada@example.com");
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

    // In the contract's JSON, in this release's encoding (chunk 5.9).
    let encoding = live.rows("SELECT \"encoding\"::text FROM \"jadpo_passkeys\"");
    assert_eq!(
        encoding[0][0],
        jadpo_runtime::SqlValue::Text("1".to_string())
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

/// A passkey kept before encodings were recorded (area 19, chunk 5.9),
/// with no `encoding` and its count as text, as chunk 5.5z kept it, is
/// still listed, renamed and signed in with, and is written in this
/// release's encoding once it is used.
#[test]
fn a_passkey_kept_before_encodings_were_recorded_still_signs_in() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("encodings");
    let mut ada = live.sign_up("ada@example.com");
    let mut phone = TestAuthenticator::new(ORIGIN);
    let added = live.register(&mut ada, &mut phone, "Work phone");
    assert_eq!(added.status, 201, "{}", text(&added));
    let id = json(&added)["id"].as_str().expect("an id").to_string();
    live.rows(
        "UPDATE \"jadpo_passkeys\" SET \"encoding\" = NULL, \"credential\" = jsonb_set(\"credential\", '{sign_count}', to_jsonb(\"credential\" ->> 'sign_count'))",
    );
    let before = live.rows("SELECT \"credential\" ->> 'sign_count', jsonb_typeof(\"credential\" -> 'sign_count') FROM \"jadpo_passkeys\"");
    assert_eq!(
        before[0],
        [
            jadpo_runtime::SqlValue::Text("0".to_string()),
            jadpo_runtime::SqlValue::Text("string".to_string())
        ]
    );

    let listed = live.send(&ada.public("GET", "/auth/passkey", ""));
    assert_eq!(listed.status, 200, "{}", text(&listed));
    assert_eq!(json(&listed)[0]["name"], "Work phone");
    let renamed = live.send(&ada.write(
        "PATCH",
        &format!("/auth/passkey/{id}"),
        "{\"name\":\"Old phone\"}",
    ));
    assert_eq!(renamed.status, 200, "{}", text(&renamed));
    assert_eq!(json(&renamed)["name"], "Old phone");

    live.at(START + MINUTE);
    let (laptop, signed) = live.sign_in(&mut phone);
    assert_eq!(signed.status, 204, "{}", text(&signed));
    assert!(laptop.signed_in(), "{:?}", signed.headers);
    let after = live.rows("SELECT \"encoding\"::text, \"credential\" ->> 'sign_count', jsonb_typeof(\"credential\" -> 'sign_count'), \"credential\" ->> 'name' FROM \"jadpo_passkeys\"");
    assert_eq!(
        after[0],
        [
            jadpo_runtime::SqlValue::Text("1".to_string()),
            jadpo_runtime::SqlValue::Text("1".to_string()),
            jadpo_runtime::SqlValue::Text("number".to_string()),
            jadpo_runtime::SqlValue::Text("Old phone".to_string()),
        ]
    );
    // An encoding a later release writes is never guessed at.
    live.rows("UPDATE \"jadpo_passkeys\" SET \"encoding\" = 2");
    refused(&live.sign_in(&mut phone).1, 401, "passkey_rejected");
}

#[test]
fn a_replayed_answer_is_refused() {
    if !jadpo_test::live() {
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
    if !jadpo_test::live() {
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
    if !jadpo_test::live() {
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
    if !jadpo_test::live() {
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
    if !jadpo_test::live() {
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
    key.set_sign_count(0);
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
    if !jadpo_test::live() {
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
    if !jadpo_test::live() {
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
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("disabled");
    let mut ada = live.sign_up("ada@example.com");
    let mut phone = TestAuthenticator::new(ORIGIN);
    assert_eq!(live.register(&mut ada, &mut phone, "Phone").status, 201);
    live.rows("UPDATE \"member\" SET \"status\" = 'suspended'");
    let (_, response) = live.sign_in(&mut phone);
    refused(&response, 403, "user_disabled");
    // The log says why, and the application hears it, with whose passkey.
    assert_eq!(
        response.refused.as_ref().map(|refused| refused.reason),
        Some("user_disabled")
    );
    assert_eq!(
        live.notes(),
        [
            "passkey_added ada@example.com",
            "refused ada@example.com passkey user_disabled"
        ]
    );

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

/// Recovery (chunk 5.5z2): a person with no passkey left signs in with a
/// magic link's code and adds a new passkey, which then signs her in; the
/// application hears every step, and no event names a credential.
#[test]
fn a_person_with_no_passkey_left_recovers_through_a_magic_link() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("recovery");
    let mut ada = live.sign_up("ada@example.com");
    let mut phone = TestAuthenticator::new(ORIGIN);
    let added = live.register(&mut ada, &mut phone, "Phone");
    assert_eq!(added.status, 201, "{}", text(&added));
    let (_, signed) = live.sign_in(&mut phone);
    assert_eq!(signed.status, 204, "{}", text(&signed));

    // She removes her only passkey, and it signs no one in.
    let id = json(&added)["id"].as_str().expect("an id").to_string();
    let removed = live.send(&ada.write("DELETE", &format!("/auth/passkey/{id}"), ""));
    assert_eq!(removed.status, 204, "{}", text(&removed));
    let (mut laptop, response) = live.sign_in(&mut phone);
    refused(&response, 401, "passkey_rejected");
    assert_eq!(
        response.refused.as_ref().map(|refused| refused.reason),
        Some("passkey_rejected")
    );
    assert!(!laptop.signed_in());

    // A magic link's code signs her in on the laptop.
    let asked = live.send(&laptop.public(
        "POST",
        "/auth/magic-link",
        "{\"address\":\"ada@example.com\"}",
    ));
    assert_eq!(asked.status, 202, "{}", text(&asked));
    let code = live.code_sent_to("ada@example.com");
    let confirmed = live.send(&laptop.public(
        "POST",
        "/auth/magic-link/confirm",
        &format!("{{\"address\":\"ada@example.com\",\"code\":\"{code}\"}}"),
    ));
    assert_eq!(confirmed.status, 200, "{}", text(&confirmed));
    laptop.keep(&confirmed);
    assert!(laptop.signed_in());

    // From there she adds a new passkey, which signs her in.
    let mut new_phone = TestAuthenticator::new(ORIGIN);
    let added = live.register(&mut laptop, &mut new_phone, "New phone");
    assert_eq!(added.status, 201, "{}", text(&added));
    let (browser, signed) = live.sign_in(&mut new_phone);
    assert_eq!(signed.status, 204, "{}", text(&signed));
    assert!(browser.signed_in());
    let listed = live.send(&browser.public("GET", "/auth/passkey", ""));
    let listed = json(&listed);
    let names: Vec<&str> = listed
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|view| view["name"].as_str())
        .collect();
    assert_eq!(names, ["New phone"]);

    assert_eq!(
        live.notes(),
        [
            "passkey_added ada@example.com",
            "signed_in ada@example.com passkey",
            "passkey_removed ada@example.com",
            "refused - passkey passkey_rejected",
            "signed_in ada@example.com magic_link",
            "passkey_added ada@example.com",
            "signed_in ada@example.com passkey",
        ]
    );
    // What the outbox kept names the user and the passkey by `id` only.
    for row in live.rows("SELECT \"payload\"::text FROM \"jadpo_outbox\"") {
        let jadpo_runtime::SqlValue::Text(payload) = &row[0] else {
            panic!("{row:?}")
        };
        assert!(
            !payload.contains("credential") && !payload.contains("challenge"),
            "{payload}"
        );
    }
}

/// `jadpo test` runs a route test written with `test_passkeys`, which
/// drives the generated routes with the test host's own authenticator.
#[test]
fn a_route_test_signs_in_with_test_passkeys() {
    if !jadpo_test::live() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("jadpo-passkeys-route-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let root = dir.join("notes");
    for (path, text) in FILES.iter().chain(ROUTE_TESTS) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    jadpo(&root, &["migrate", "new", "initial"]);
    jadpo(&root, &["records", "update"]);
    let env = jadpo_term::Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", "1792504800")
        .with_cwd(&root);
    let done = jadpo_cli::run(&["test".to_string()], &env);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert!(done.stdout.contains("2 passed"), "{}", done.stdout);
}

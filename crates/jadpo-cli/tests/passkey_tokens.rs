//! Passkeys and API tokens (area 8, "Passkeys"): adding and managing
//! passkeys need the person signed in with a browser session, even where
//! the API serving the passkey routes also accepts API tokens, as an
//! application with both methods and one API does (security audit S1).
//! Against live PostgreSQL, on the harness of `tests/passkeys.rs`.

#![allow(dead_code)]

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
    api_token: {}
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
        "use notes\n\npolicy {\n    actions: {\n        notes.sign_up: [Access.public]\n        notes.add_member: [Access.public]\n        notes.app_sign_up: [Access.public]\n    }\n}\n",
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
        "domains/notes/actions/app_sign_up.jadpo",
        r#"use notes/entities { Member }
use notes/types { AppSignedIn, MemberStatus, NewMember }
use notes/failures { EmailTaken }

/// Registers a member and gives the app's own client an API token.
action app_sign_up(input: NewMember) fails EmailTaken -> AppSignedIn {
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
    return AppSignedIn {
        token: sessions.issue_token(member.id)
    }
}
"#,
    ),
    (
        "domains/notes/types/app_signed_in.jadpo",
        "type AppSignedIn = Output {\n    token: ApiToken\n}\n",
    ),
    (
        "domains/notes/routes/app.jadpo",
        r#"use notes/actions { app_sign_up }
use notes/types { AppSignedIn, NewMember }

routes App {
    prefix: /app

    POST /sign-up {
        authentication: RouteAuthentication.none
        input: NewMember
        output: AppSignedIn
        run: app_sign_up(input)
    }
}
"#,
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
            std::env::temp_dir().join(format!("jadpo-s1-identity-{name}-{}", std::process::id()));
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
            .create_database(&format!("s1_identity_{name}"))
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

/// A user's API token (the app's own mobile or command-line client's, or
/// one leaked from it) adds, lists, renames and removes no passkey: a
/// passkey it added would start a browser session that outlives revoking
/// the token. Each is 403 `not_permitted`, and the browser session still
/// adds one.
#[test]
fn an_api_token_adds_and_manages_no_passkey() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("token_passkey");
    let signed_up = live.send(&request(
        "POST",
        "/app/sign-up",
        &[],
        "{\"email\":\"ada@example.com\",\"handle\":\"ada\"}",
    ));
    assert_eq!(signed_up.status, 200, "{}", text(&signed_up));
    let token = json(&signed_up)["token"]
        .as_str()
        .expect("a token")
        .to_string();
    assert!(token.starts_with("jadpo_token_"), "{token}");
    let bearer = format!("Bearer {token}");
    let with_token = |method: &str, path: &str, body: &str| {
        live.send(&request(
            method,
            path,
            &[("authorization", bearer.as_str())],
            body,
        ))
    };
    let options = with_token("POST", "/auth/passkey/register/options", "");
    refused(&options, 403, "not_permitted");
    // An answer to options a browser session asked for doesn't register
    // through the token either, and the token reaches no passkey: each
    // refusal comes before any is looked up.
    let mut bea = live.sign_up("bea@example.com");
    let mut phone = TestAuthenticator::new(ORIGIN);
    assert_eq!(live.register(&mut bea, &mut phone, "Phone").status, 201);
    let asked = live.send(&bea.write("POST", "/auth/passkey/register/options", ""));
    assert_eq!(asked.status, 200, "{}", text(&asked));
    let mut device = TestAuthenticator::new(ORIGIN);
    let credential = device.create(&text(&asked)).expect("the device makes one");
    let added = with_token(
        "POST",
        "/auth/passkey/register",
        &format!("{{\"name\":\"Other\",\"credential\":{credential}}}"),
    );
    refused(&added, 403, "not_permitted");
    refused(
        &with_token("GET", "/auth/passkey", ""),
        403,
        "not_permitted",
    );
    let listed = json(&live.send(&bea.public("GET", "/auth/passkey", "")));
    let id = listed[0]["id"].as_str().expect("an id").to_string();
    let path = format!("/auth/passkey/{id}");
    refused(
        &with_token("PATCH", &path, "{\"name\":\"Mine\"}"),
        403,
        "not_permitted",
    );
    refused(&with_token("DELETE", &path, ""), 403, "not_permitted");
    // Nothing was added, renamed or removed.
    let still = json(&live.send(&bea.public("GET", "/auth/passkey", "")));
    assert_eq!(still, listed);
}

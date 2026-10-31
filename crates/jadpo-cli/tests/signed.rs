//! Signed sessions against live PostgreSQL (area 8, chunk 5.1b): the
//! runtime answers requests as a deployable does (`http::serve_request`)
//! on the native host, through a transport that counts round trips, with
//! a clock the test moves. With `revocation_delay: 5m`:
//!
//! - a browser session's or an API token's first request reads the record
//!   and renews the credential with a signed part, which signs every
//!   request after it in without a read;
//! - a member suspended, or a session ended, keeps working until the
//!   signed part expires, at most the delay, and is refused once renewing
//!   reads the record;
//! - a `fresh` route reads the record whatever the signed part says;
//! - a key rotated to `SESSION_SIGNING_KEY_PREVIOUS` still verifies, and
//!   renewing signs with the current one;
//! - `POST /auth/exchange` trades a service key for a short-lived token;
//! - a cookie write's `Origin` must be the configured `origin:`.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live tests.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll};

use jadpo_drivers::{Endpoint, Stream, Transport};
use jadpo_host_native::{NativeHost, NativeTransport, Postgres, Runtime};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::http::{Request, Response, Worker, serve_request};
use jadpo_runtime::{Database, Host, LocalFuture};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

// --- counting round trips ----------------------------------------------------------

/// Counts round trips across every connection it opens.
#[derive(Clone, Default)]
struct Counting {
    trips: Rc<Cell<u32>>,
}

impl Transport for Counting {
    fn connect(&self, endpoint: &Endpoint) -> LocalFuture<'_, std::io::Result<Box<dyn Stream>>> {
        let endpoint = endpoint.clone();
        let trips = Rc::clone(&self.trips);
        Box::pin(async move {
            let inner = NativeTransport.connect(&endpoint).await?;
            Ok(Box::new(Counted {
                inner,
                trips,
                answered: true,
            }) as Box<dyn Stream>)
        })
    }

    fn spawn(&self, task: LocalFuture<'static, ()>) {
        NativeTransport.spawn(task);
    }

    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        NativeTransport.sleep(millis)
    }
}

/// A connection whose first write after an answer counts one round trip.
struct Counted {
    inner: Box<dyn Stream>,
    trips: Rc<Cell<u32>>,
    answered: bool,
}

impl Counted {
    fn wrote(&mut self, n: usize) {
        if n > 0 && self.answered {
            self.answered = false;
            self.trips.set(self.trips.get() + 1);
        }
    }
}

impl AsyncRead for Counted {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let read = Pin::new(&mut self.inner).poll_read(cx, buf);
        if matches!(read, Poll::Ready(Ok(()))) && buf.filled().len() > before {
            self.answered = true;
        }
        read
    }
}

impl AsyncWrite for Counted {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let written = Pin::new(&mut self.inner).poll_write(cx, bytes);
        if let Poll::Ready(Ok(n)) = written {
            self.wrote(n);
        }
        written
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        let written = Pin::new(&mut self.inner).poll_write_vectored(cx, bufs);
        if let Poll::Ready(Ok(n)) = written {
            self.wrote(n);
        }
        written
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

// --- a host whose clock the test moves --------------------------------------------

/// The native host, with a wall clock the test sets.
struct Clocked {
    inner: NativeHost,
    now: Cell<i64>,
}

impl Host for Clocked {
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

    fn yield_now(&self) -> LocalFuture<'_, ()> {
        self.inner.yield_now()
    }

    fn monotonic_ms(&self) -> Option<u64> {
        self.inner.monotonic_ms()
    }

    fn now_ms(&self) -> Option<i64> {
        Some(self.now.get())
    }
}

/// 2026-10-16T09:00:00Z.
const START: i64 = 1_792_141_200_000;
const MINUTE: i64 = 60_000;

const KEY: &str = "0123456789abcdef0123456789abcdef-current";
const OLD_KEY: &str = "fedcba9876543210fedcba9876543210-previous";
const ORIGIN: &str = "https://notes.example";

// --- the project --------------------------------------------------------------------

const FILES: &[(&str, &str)] = &[
    (
        "config.jadpo",
        "config {\n    session_signing_key: SigningKey\n    browser_origin: Url\n}\n",
    ),
    (
        "app.jadpo",
        r#"use notes/entities { Member, Partner }
use notes/types { MemberStatus, PartnerStatus }

application {
    revocation_delay: 5m
}

principal {
    user: {
        subject: Member.handle
        active: Member.status == MemberStatus.active
    }
    service: {
        subject: Partner.name
        active: Partner.status == PartnerStatus.active
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    browser_session: {
        cookie: "notes_session"
        signing_key: config.session_signing_key
        origin: config.browser_origin
    }
    api_token: {
        signing_key: config.session_signing_key
    }
    service_key: {
        signing_key: config.session_signing_key
    }
}
"#,
    ),
    (
        "policy.jadpo",
        r#"use notes

policy {
    actions: {
        notes.say_who: [Access.authenticated]
        notes.say_who_fresh: [Access.authenticated]
        notes.say_partner: [Access.authenticated]
    }
}
"#,
    ),
    (
        "domains/notes/entities/member.jadpo",
        r#"use notes/types { Handle, MemberStatus }

entity Member {
    id: Uuid
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
        "domains/notes/entities/partner.jadpo",
        r#"use notes/entities { Member }
use notes/types { Handle, PartnerStatus }

entity Partner {
    id: Uuid
    owner_id: Member.id {
        on_delete: OnDelete.restricted
    }
    name: Handle {
        unique: true
    }
    status: PartnerStatus

    policy: {
        create: [owner_id]
        read: [id, owner_id]
        update: [owner_id]
    }
}
"#,
    ),
    (
        "domains/notes/actions/sign_up.jadpo",
        r#"use notes/entities { Member }
use notes/types { Handle, MemberStatus }
use notes/failures { NameTaken }

/// Registers a member, signs this browser in and gives their app a token.
action sign_up(input: SignUp) fails NameTaken -> SignedUp {
    var member: Member = attempt Member.create {
        set: {
            handle: input.handle
            status: MemberStatus.active
        }
        conflict: {
            Member.handle: NameTaken
        }
    }
    sessions.start(member.id)
    return SignedUp {
        token: sessions.issue_token(member.id)
    }
}

type SignUp = Input {
    handle: Handle
}

type SignedUp = Output {
    token: ApiToken
}
"#,
    ),
    (
        "domains/notes/actions/sign_out.jadpo",
        "action sign_out() -> Unit {\n    sessions.end()\n}\n",
    ),
    (
        "domains/notes/actions/say_who.jadpo",
        r#"use notes/types { Said }

/// Says who is signed in, reading nothing.
action say_who(caller: Principal.user) -> Said {
    return Said {
        who: caller.subject
    }
}
"#,
    ),
    (
        "domains/notes/actions/say_who_fresh.jadpo",
        r#"use notes/types { Said }

/// The same, on a route that reads the caller's record.
action say_who_fresh(caller: Principal.user) -> Said {
    return Said {
        who: caller.subject
    }
}
"#,
    ),
    (
        "domains/notes/actions/say_partner.jadpo",
        r#"use notes/types { Said }

/// Says which partner is signed in, reading nothing.
action say_partner(caller: Principal.service) -> Said {
    return Said {
        who: caller.subject
    }
}
"#,
    ),
    (
        "domains/notes/failures/name_taken.jadpo",
        "type NameTaken = Failure {\n    kind: FailureKind.conflict\n    message: \"That name is taken.\"\n}\n",
    ),
    (
        "domains/notes/actions/add_partner.jadpo",
        r#"use notes/entities { Partner }
use notes/types { Handle, PartnerStatus }
use notes/failures { NameTaken }

/// Adds a partner and gives its first key.
action add_partner(input: AddPartner, owner: Principal.user) fails NameTaken -> PartnerAdded {
    var partner: Partner = attempt Partner.create {
        set: {
            owner_id: owner.id
            name: input.name
            status: PartnerStatus.active
        }
        conflict: {
            Partner.name: NameTaken
        }
    }
    return PartnerAdded {
        key: Partner.issue_key(partner.id, input.key_expires_at)
    }
}

type AddPartner = Input {
    name: Handle
    key_expires_at: Instant
}

type PartnerAdded = Output {
    key: ServiceKey
}
"#,
    ),
    (
        "domains/notes/routes/account.jadpo",
        r#"use notes/actions { SignUp, SignedUp, sign_out, sign_up }

routes Account {
    prefix: /account

    POST /sign-up {
        authentication: RouteAuthentication.none
        input: SignUp
        output: SignedUp
        status: SuccessStatus.created
        run: sign_up(input)
    }

    POST /sign-out {
        status: SuccessStatus.no_content
        run: sign_out()
    }
}
"#,
    ),
    (
        "domains/notes/routes/echo.jadpo",
        r#"use notes/types { Said }
use notes/actions { say_partner, say_who, say_who_fresh }

routes Echo {
    prefix: /echo

    POST / {
        output: Said
        run: say_who(current_principal.user)
    }

    POST /fresh {
        authentication: RouteAuthentication.fresh
        output: Said
        run: say_who_fresh(current_principal.user)
    }

    POST /partner {
        output: Said
        run: say_partner(current_principal.service)
    }
}
"#,
    ),
    (
        "domains/notes/routes/partners.jadpo",
        r#"use notes/actions { AddPartner, PartnerAdded, add_partner }

routes Partners {
    prefix: /partners

    POST / {
        input: AddPartner
        output: PartnerAdded
        status: SuccessStatus.created
        run: add_partner(input, current_principal.user)
    }
}
"#,
    ),
    (
        "domains/notes/types/handle.jadpo",
        "type Handle = Text {\n    min_length: 3\n    max_length: 40\n}\n",
    ),
    (
        "domains/notes/types/member_status.jadpo",
        "type MemberStatus = Enum {\n    active\n    suspended\n}\n",
    ),
    (
        "domains/notes/types/partner_status.jadpo",
        "type PartnerStatus = Enum {\n    active\n    suspended\n}\n",
    ),
    (
        "domains/notes/types/said.jadpo",
        "use notes/types { Handle }\n\ntype Said = Output {\n    who: Handle\n}\n",
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

/// The project, migrated into a database of its own, served by the
/// runtime with the settings `variables` give.
struct Live {
    dir: PathBuf,
    prepared: Arc<Prepared>,
    program: Program,
    host: Clocked,
    runtime: Runtime,
    counting: Counting,
    worker: Worker,
}

impl Live {
    fn new(name: &str) -> Live {
        let dir = std::env::temp_dir().join(format!("jadpo-signed-{name}-{}", std::process::id()));
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
        let url = server.create_database(name).expect("a database");
        let counting = Counting::default();
        let store = Postgres::new(&url, Rc::new(counting.clone())).expect("a store");
        let runtime = Runtime::new().expect("a runtime");
        runtime
            .block_on(jadpo_runtime::migrate(
                &store,
                steps.get("main").expect("main's migrations"),
            ))
            .expect("migrates");
        let program = Self::program(&prepared, KEY, None);
        Live {
            dir,
            prepared,
            program,
            host: Clocked {
                inner: NativeHost::new().with_store("main", store),
                now: Cell::new(START),
            },
            runtime,
            counting,
            worker: Worker::default(),
        }
    }

    /// The program, started with `key` as the signing key and `previous`
    /// as the one before it.
    fn program(prepared: &Arc<Prepared>, key: &str, previous: Option<&str>) -> Program {
        let mut variables = BTreeMap::new();
        variables.insert("SESSION_SIGNING_KEY".to_string(), key.to_string());
        variables.insert("BROWSER_ORIGIN".to_string(), ORIGIN.to_string());
        if let Some(previous) = previous {
            variables.insert(
                "SESSION_SIGNING_KEY_PREVIOUS".to_string(),
                previous.to_string(),
            );
        }
        let values = prepared
            .config(
                &variables,
                jadpo_runtime::config::Mode::Environment,
                &|bytes: &mut [u8]| NativeHost::new().random(bytes),
            )
            .expect("the settings check");
        Program::new(Arc::clone(prepared)).with_config(&values)
    }

    /// Starts again with other keys, as a deployment rotating its key does.
    fn restart(&mut self, key: &str, previous: Option<&str>) {
        self.program = Self::program(&self.prepared, key, previous);
    }

    fn at(&self, now: i64) {
        self.host.now.set(now);
    }

    /// The response, and how many round trips to PostgreSQL it took.
    fn send(&self, request: &Request) -> (Response, u32) {
        let before = self.counting.trips.get();
        let app = self.program.app();
        let response = self.runtime.block_on(serve_request(
            &app,
            &self.program,
            &self.host,
            &self.worker,
            request,
        ));
        (response, self.counting.trips.get() - before)
    }

    fn sql(&self, statement: &str) {
        let database = self.host.database("main").expect("main");
        self.runtime
            .block_on(database.query_once(statement, &[]))
            .expect("runs");
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
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

fn json(response: &Response) -> serde_json::Value {
    serde_json::from_slice(&response.body).unwrap_or_else(|_| {
        panic!(
            "not JSON ({}): {}",
            response.status,
            String::from_utf8_lossy(&response.body)
        )
    })
}

fn code(response: &Response) -> String {
    json(response)["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// A cookie a response sets, by name: its value.
fn cookie(response: &Response, name: &str) -> Option<String> {
    response
        .headers
        .iter()
        .filter(|(header, _)| header == "set-cookie")
        .find_map(|(_, value)| {
            let (pair, _) = value.split_once(';')?;
            let (key, value) = pair.split_once('=')?;
            (key == name).then(|| value.to_string())
        })
}

/// What a browser signed in as `handle` sends: its session's cookie, its
/// CSRF token and the application's origin.
struct Browser {
    session: String,
    csrf: String,
}

impl Browser {
    fn post(&self, path: &str) -> Request {
        self.post_from(path, ORIGIN)
    }

    fn post_from(&self, path: &str, origin: &str) -> Request {
        let cookie = format!(
            "notes_session={}; notes_session_csrf={}",
            self.session, self.csrf
        );
        request(
            "POST",
            path,
            &[
                ("cookie", &cookie),
                ("x-jadpo-csrf", &self.csrf),
                ("origin", origin),
                ("host", "10.0.0.5:8080"),
            ],
            "",
        )
    }
}

/// Signs `handle` up: the browser's session and the app's API token, each
/// as issued, with no signed part.
fn sign_up(live: &Live, handle: &str) -> (Browser, String) {
    let (response, _) = live.send(&request(
        "POST",
        "/account/sign-up",
        &[],
        &format!(r#"{{"handle":"{handle}"}}"#),
    ));
    assert_eq!(response.status, 201, "{}", json(&response));
    let session = cookie(&response, "notes_session").expect("a session");
    let csrf = cookie(&response, "notes_session_csrf").expect("a CSRF token");
    assert_eq!(session.len(), 64, "a session starts without a signed part");
    let token = json(&response)["token"]
        .as_str()
        .expect("a token")
        .to_string();
    assert_eq!(token.len(), "jadpo_token_".len() + 64);
    (Browser { session, csrf }, token)
}

#[test]
fn a_signed_session_signs_in_without_a_read_until_the_delay_bites() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("sessions");
    let (mut ann, _) = sign_up(&live, "ann");

    // The first request reads the session with Ann's record and renews
    // the cookie with a signed part.
    let (first, trips) = live.send(&ann.post("/echo"));
    assert_eq!(first.status, 200, "{}", json(&first));
    assert_eq!(json(&first)["who"], "ann");
    assert_eq!(trips, 1, "the sign-in lookup");
    let renewed = cookie(&first, "notes_session").expect("a renewed cookie");
    assert!(
        renewed.starts_with(&format!("{}.", ann.session)),
        "{renewed}"
    );
    ann.session = renewed;

    // Every request after it signs in without a read.
    let (second, trips) = live.send(&ann.post("/echo"));
    assert_eq!(second.status, 200, "{}", json(&second));
    assert_eq!(trips, 0, "a signed session reads nothing to sign in");
    assert_eq!(cookie(&second, "notes_session"), None);

    // A `fresh` route reads the record anyway.
    let (fresh, trips) = live.send(&ann.post("/echo/fresh"));
    assert_eq!(fresh.status, 200, "{}", json(&fresh));
    assert_eq!(trips, 1, "a fresh route reads the record");

    // Suspended, Ann keeps working until the signed part expires, within
    // the delay, and is refused once renewing reads her record; a fresh
    // route refuses her at once.
    live.sql("UPDATE \"member\" SET \"status\" = 'suspended'");
    live.at(START + 4 * MINUTE);
    let (within, trips) = live.send(&ann.post("/echo"));
    assert_eq!((within.status, trips), (200, 0), "{}", json(&within));
    let (fresh, _) = live.send(&ann.post("/echo/fresh"));
    assert_eq!(fresh.status, 403);
    assert_eq!(code(&fresh), "user_disabled");
    live.at(START + 5 * MINUTE + 1);
    let (after, trips) = live.send(&ann.post("/echo"));
    assert_eq!(after.status, 403, "{}", json(&after));
    assert_eq!(code(&after), "user_disabled");
    assert_eq!(trips, 1, "renewing reads the record");

    // Reinstated, she renews again; ending the session stops a copy of
    // the cookie once its signed part expires.
    live.sql("UPDATE \"member\" SET \"status\" = 'active'");
    let (back, _) = live.send(&ann.post("/echo"));
    assert_eq!(back.status, 200, "{}", json(&back));
    ann.session = cookie(&back, "notes_session").expect("renewed");
    let copy = Browser {
        session: ann.session.clone(),
        csrf: ann.csrf.clone(),
    };
    let (ended, _) = live.send(&ann.post("/account/sign-out"));
    assert_eq!(ended.status, 204, "{}", json(&ended));
    let (still, trips) = live.send(&copy.post("/echo"));
    assert_eq!((still.status, trips), (200, 0), "within the delay");
    live.at(START + 11 * MINUTE);
    let (gone, _) = live.send(&copy.post("/echo"));
    assert_eq!(gone.status, 401, "{}", json(&gone));
    assert_eq!(code(&gone), "invalid_credentials");
}

#[test]
fn an_api_token_renews_in_a_header_and_signs_in_without_a_read() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("tokens");
    let (_, token) = sign_up(&live, "bob");
    let bearer = |token: &str| format!("Bearer {token}");
    let (first, trips) = live.send(&request(
        "POST",
        "/echo",
        &[("authorization", &bearer(&token))],
        "",
    ));
    assert_eq!(first.status, 200, "{}", json(&first));
    assert_eq!(trips, 1);
    let renewed = first.header("x-jadpo-token").expect("a renewed token");
    assert!(renewed.starts_with(&format!("{token}.")), "{renewed}");
    let (second, trips) = live.send(&request(
        "POST",
        "/echo",
        &[("authorization", &bearer(renewed))],
        "",
    ));
    assert_eq!((second.status, trips), (200, 0), "{}", json(&second));
    assert_eq!(second.header("x-jadpo-token"), None);
    // The token as issued still signs in, reading its record each time.
    let (plain, trips) = live.send(&request(
        "POST",
        "/echo",
        &[("authorization", &bearer(&token))],
        "",
    ));
    assert_eq!((plain.status, trips), (200, 1));
    // A signed part with another token's secret signs no one in without a
    // read: it is renewed from the secret's own row.
    let (_, other) = sign_up(&live, "cat");
    let (_, signed) = renewed.split_once('.').expect("a signed part");
    let mixed = format!("{other}.{signed}");
    let (crossed, trips) = live.send(&request(
        "POST",
        "/echo",
        &[("authorization", &bearer(&mixed))],
        "",
    ));
    assert_eq!((crossed.status, trips), (200, 1), "{}", json(&crossed));
    assert_eq!(json(&crossed)["who"], "cat");
}

/// A signed part records the encoding it was written in (area 19, chunk
/// 5.9): one written before encodings were recorded still signs in
/// without a read, and one of an encoding this release doesn't know is
/// renewed by reading the record, as an expired one is.
#[test]
fn a_part_signed_before_encodings_were_recorded_still_signs_in() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("encodings");
    let (mut eve, _) = sign_up(&live, "eve");
    let (first, _) = live.send(&eve.post("/echo"));
    let renewed = cookie(&first, "notes_session").expect("a renewed cookie");
    let (secret, signed) = renewed.split_once('.').expect("a signed part");
    let key = jadpo_runtime::http::signed::SigningKey::new(KEY, None);
    let payload = key.open(signed).expect("signed with the current key");
    let mut claims: serde_json::Value = serde_json::from_str(&payload).expect("JSON");
    assert_eq!(claims["enc"], jadpo_std::standard::ENCODING, "{payload}");

    // As chunk 5.1b signed it, with no `enc`.
    let fields = claims.as_object_mut().expect("an object");
    fields.remove("enc");
    let before = serde_json::Value::Object(fields.clone()).to_string();
    eve.session = format!("{secret}.{}", key.sign(&before));
    let (old, trips) = live.send(&eve.post("/echo"));
    assert_eq!((old.status, trips), (200, 0), "{}", json(&old));
    assert_eq!(json(&old)["who"], "eve");
    assert_eq!(cookie(&old, "notes_session"), None, "nothing to renew");

    // One 0.1.0 signed in encoding 1, before `Jwk` held `key_ops` (chunk
    // A3): it signs in as it did, with no read.
    fields.insert("enc".into(), 1.into());
    let earlier = serde_json::Value::Object(fields.clone()).to_string();
    eve.session = format!("{secret}.{}", key.sign(&earlier));
    let (old, trips) = live.send(&eve.post("/echo"));
    assert_eq!((old.status, trips), (200, 0), "{}", json(&old));
    assert_eq!(json(&old)["who"], "eve");

    // An encoding a later release writes: the record is read, and the
    // part renewed in this release's.
    fields.insert("enc".into(), (jadpo_std::standard::ENCODING + 1).into());
    let later = serde_json::Value::Object(fields.clone()).to_string();
    eve.session = format!("{secret}.{}", key.sign(&later));
    let (read, trips) = live.send(&eve.post("/echo"));
    assert_eq!((read.status, trips), (200, 1), "{}", json(&read));
    let again = cookie(&read, "notes_session").expect("renewed");
    let (_, signed) = again.split_once('.').expect("a signed part");
    let payload = key.open(signed).expect("signed with the current key");
    let claims: serde_json::Value = serde_json::from_str(&payload).expect("JSON");
    assert_eq!(claims["enc"], jadpo_std::standard::ENCODING, "{payload}");
}

#[test]
fn a_rotated_key_still_verifies_and_new_parts_sign_with_the_current_one() {
    if !jadpo_test::live() {
        return;
    }
    let mut live = Live::new("rotation");
    live.restart(OLD_KEY, None);
    let (mut dan, _) = sign_up(&live, "dan");
    let (first, _) = live.send(&dan.post("/echo"));
    dan.session = cookie(&first, "notes_session").expect("signed with the old key");
    let old_signed = dan.session.clone();

    // The old key moves to `_PREVIOUS`: what it signed still verifies.
    live.restart(KEY, Some(OLD_KEY));
    let (rotated, trips) = live.send(&dan.post("/echo"));
    assert_eq!((rotated.status, trips), (200, 0), "{}", json(&rotated));

    // Renewing signs with the current key alone: once the old one is
    // removed, the new part verifies and the old one is renewed by a read.
    live.at(START + 6 * MINUTE);
    let (renewed, trips) = live.send(&dan.post("/echo"));
    assert_eq!((renewed.status, trips), (200, 1));
    let new_signed = cookie(&renewed, "notes_session").expect("signed with the new key");
    live.restart(KEY, None);
    dan.session = new_signed;
    let (current, trips) = live.send(&dan.post("/echo"));
    assert_eq!((current.status, trips), (200, 0), "{}", json(&current));
    live.at(START);
    dan.session = old_signed;
    let (old, trips) = live.send(&dan.post("/echo"));
    assert_eq!(
        (old.status, trips),
        (200, 1),
        "the old key no longer verifies"
    );
}

#[test]
fn a_cookie_write_must_come_from_the_configured_origin() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("origin");
    let (eve, _) = sign_up(&live, "eve");
    // Behind a proxy, `Host` names the server inside; the configured
    // origin is what a write must come from.
    let (here, _) = live.send(&eve.post("/echo"));
    assert_eq!(here.status, 200, "{}", json(&here));
    let (foreign, _) = live.send(&eve.post_from("/echo", "https://evil.example"));
    assert_eq!(foreign.status, 403);
    assert_eq!(code(&foreign), "csrf_rejected");
    let (host, _) = live.send(&eve.post_from("/echo", "http://10.0.0.5:8080"));
    assert_eq!(
        host.status, 403,
        "the server's own address isn't the origin"
    );
    assert_eq!(code(&host), "csrf_rejected");
}

#[test]
fn the_exchange_trades_a_service_key_for_a_short_lived_token() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("exchange");
    let (owner, token) = sign_up(&live, "fay");
    let (added, _) = live.send(&{
        let mut request = owner.post("/partners");
        request.body = br#"{"name":"acme","key_expires_at":"2027-01-01T00:00:00Z"}"#.to_vec();
        request
            .headers
            .push(("content-type".into(), "application/json".into()));
        request
    });
    assert_eq!(added.status, 201, "{}", json(&added));
    let key = json(&added)["key"].as_str().expect("a key").to_string();
    let exchange = |credential: &str| {
        live.send(&request(
            "POST",
            "/auth/exchange",
            &[("authorization", &format!("Bearer {credential}"))],
            "",
        ))
    };

    // The key is read with its partner's record, and traded for a token
    // that expires within the delay.
    let (traded, trips) = exchange(&key);
    assert_eq!(traded.status, 200, "{}", json(&traded));
    assert_eq!(trips, 1, "the key's lookup");
    let body = json(&traded);
    let exchanged = body["token"].as_str().expect("a token").to_string();
    assert!(exchanged.starts_with("jadpo_service_token_"), "{exchanged}");
    assert_eq!(body["expires_at"], "2026-10-16T09:05:00Z");

    let partner = |credential: &str| {
        live.send(&request(
            "POST",
            "/echo/partner",
            &[("authorization", &format!("Bearer {credential}"))],
            "",
        ))
    };
    let (signed_in, trips) = partner(&exchanged);
    assert_eq!((signed_in.status, trips), (200, 0), "{}", json(&signed_in));
    assert_eq!(json(&signed_in)["who"], "acme");
    // The key used directly always reads the record.
    let (direct, trips) = partner(&key);
    assert_eq!((direct.status, trips), (200, 1));

    // Only a key is exchanged: not the token it gives, nor a member's.
    let (again, _) = exchange(&exchanged);
    assert_eq!(again.status, 401);
    assert_eq!(code(&again), "invalid_credentials");
    let (member, _) = exchange(&token);
    assert_eq!(member.status, 403);
    assert_eq!(code(&member), "not_permitted");
    // It takes nothing but the key.
    let (with_body, _) = live.send(&request(
        "POST",
        "/auth/exchange",
        &[("authorization", &format!("Bearer {key}"))],
        r#"{"scope":"all"}"#,
    ));
    assert_eq!(with_body.status, 422, "{}", json(&with_body));

    // Suspended, the partner's token works until it expires; then it is
    // refused, and exchanging the key again says why.
    live.sql("UPDATE \"partner\" SET \"status\" = 'suspended'");
    live.at(START + 4 * MINUTE);
    let (within, trips) = partner(&exchanged);
    assert_eq!((within.status, trips), (200, 0));
    live.at(START + 5 * MINUTE);
    let (expired, _) = partner(&exchanged);
    assert_eq!(expired.status, 401);
    assert_eq!(code(&expired), "invalid_credentials");
    let (refused, _) = exchange(&key);
    assert_eq!(refused.status, 403);
    assert_eq!(code(&refused), "service_disabled");
}

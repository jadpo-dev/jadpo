//! What the system reaches inside a request, against live PostgreSQL
//! (area 9, "Background work"; area 8, "Magic links"; area 11, "Webhooks
//! that need a reply"; Phase 7 review, chunk 7.9b): the magic-link
//! `send:` action runs with its delivery's tenant, which for the sign-in
//! email is none, and a webhook's reply action and `on_first_sign_in:`'s
//! run with no tenant, so none of them reaches a tenant's records. "No
//! tenant" is never "every tenant", which only a schedule's run has.
//!
//! A household application: Ash has three chores and Birch two, which
//! the system may read. Each of the three actions notes how many chores
//! the system sees from where it runs, in a record every tenant shares.
//! The runtime answers the requests as a deployable does
//! (`http::serve_request`), the sign-in link is handed to a fake of the
//! application's own provider, and the webhook is signed as Twilio signs
//! one, so nothing leaves the machine.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use jadpo_host_native::{NativeHost, Postgres, Runtime};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::capability::{Answer, Capability, Fake};
use jadpo_runtime::http::{Request, Response, Worker, serve_request};
use jadpo_runtime::{Database, Host, LocalFuture, SqlValue};
use jadpo_value::Value;

/// The native host with the application's own provider faked.
struct Faked {
    inner: NativeHost,
    texter: Fake,
}

impl Host for Faked {
    fn capability(&self, connection: &str) -> Option<&dyn Capability> {
        match connection {
            "texter" => Some(&self.texter as &dyn Capability),
            _ => None,
        }
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
        self.inner.now_ms()
    }
}

const ORIGIN: &str = "https://home.example";
/// Twilio's example auth token.
const VOICE_TOKEN: &str = "12345";

const FILES: &[(&str, &str)] = &[
    (
        "config.jadpo",
        r#"config {
    browser_origin: Url
    texter_key: Text {
        secret: true
    }
    texter_environment: ProviderEnvironment
    voice_key: Text {
        secret: true
    }
    voice_address: Url
    voice_auth_token: Text {
        secret: true
    }
}
"#,
    ),
    (
        "app.jadpo",
        r#"use home
use home/entities { Person }

application {
    display_name: "Home"
    connections: {
        texter: {
            package: Texter.v1
            domains: [home]
            credential: config.texter_key
            environment: config.texter_environment
        }
        voice: {
            package: AcmeVoice.v1
            domains: [home]
            credential: config.voice_key
            host: config.voice_address
            webhooks: {
                secret: config.voice_auth_token
                replies: {
                    incoming_call: home.answer_call
                }
            }
        }
    }
}

principal {
    user: {
        subject: Person.email
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"use home

authentication {
    browser_session: {
        cookie: "home_session"
        origin: config.browser_origin
    }
    sign_in: {
        magic_link: {
            link: "/sign-in/confirm"
            send: home.send_sign_in_link
        }
    }
    on_first_sign_in: home.welcome_person
}
"#,
    ),
    (
        "policy.jadpo",
        r#"use home/entities { Household }

policy {
    tenant: Household
}
"#,
    ),
    (
        "domains/home/types/home_role.jadpo",
        "type HomeRole = Enum {\n    member\n}\n",
    ),
    (
        "domains/home/types/title.jadpo",
        "type Title = Text {\n    max_length: 80\n}\n",
    ),
    (
        "domains/home/failures/sign_up_closed.jadpo",
        "/// Signing up is closed for this address.\ntype SignUpClosed = Failure {\n    kind: FailureKind.conflict\n}\n",
    ),
    (
        "domains/home/entities/household.jadpo",
        r#"use home/types { HomeRole, Title }

entity Household {
    id: Uuid
    name: Title

    policy: {
        read: [HomeRole.member]
    }
}
"#,
    ),
    (
        "domains/home/entities/home_membership.jadpo",
        r#"use home/entities { Household, Person }
use home/types { HomeRole }

entity HomeMembership {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    person_id: Person.id {
        on_delete: OnDelete.cascade
    }
    role: HomeRole

    membership: {
        scope: household_id
        member: person_id
        role: role
    }

    policy: {
        create: [HomeRole.member]
        read: [person_id]
    }
}
"#,
    ),
    (
        "domains/home/entities/person.jadpo",
        r#"entity Person {
    id: Uuid
    email: Email {
        unique: true
    }

    policy: {
        create: [Access.system]
        read: [id]
        tenant: Tenancy.shared
    }
}
"#,
    ),
    (
        "domains/home/entities/chore.jadpo",
        r#"use home/entities { Household }
use home/types { HomeRole, Title }

entity Chore {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    title: Title

    policy: {
        create: [HomeRole.member]
        read: [HomeRole.member, Access.system]
    }
}
"#,
    ),
    (
        "domains/home/entities/sighting.jadpo",
        r#"use home/types { Title }

/// How many chores the system saw, and what it was running.
entity Sighting {
    id: Uuid
    seen_by: Title
    chores: Int

    policy: {
        create: [Access.system]
        read: [Access.system]
        tenant: Tenancy.shared
    }
}
"#,
    ),
    (
        "domains/home/queries/every_chore.jadpo",
        r#"use home/entities { Chore }

/// Every chore the caller may read.
query every_chore() -> List<Chore> {
    from: Chore
    order: [Chore.title asc]
    limit: 50
}
"#,
    ),
    (
        "domains/home/actions/note_sighting.jadpo",
        r#"use home/entities { Sighting }
use home/queries { every_chore }
use home/types { Title }

/// Notes how many chores the system reads from here.
action note_sighting(seen_by: Title) -> Unit {
    var noted: Sighting = Sighting.create {
        set: {
            seen_by: seen_by
            chores: list.count(every_chore())
        }
    }
    assert noted.seen_by == seen_by
}
"#,
    ),
    (
        "domains/home/actions/send_sign_in_link.jadpo",
        r#"use app { texter }
use home/actions { note_sighting }
use shared/providers/texter

/// Hands the sign-in link and code to Texter.
action send_sign_in_link(request: MagicLinkRequest) -> Unit {
    texter.messages.send(Texter.SignInMessage {
        to: request.address
        link: request.sign_in_link
        code: request.code
    })
    note_sighting("send")
}
"#,
    ),
    (
        "domains/home/actions/welcome_person.jadpo",
        r#"use home/actions { note_sighting }
use home/entities { Person }
use home/failures { SignUpClosed }

/// Makes a person of someone signing in for the first time.
action welcome_person(first: FirstSignIn) fails SignUpClosed -> Person.id {
    note_sighting("first")
    match first.address {
        none -> {
            reject SignUpClosed {}
        }
        some(address) -> {
            var person: Person = attempt Person.create {
                set: {
                    email: address
                }
                conflict: {
                    Person.email: SignUpClosed
                }
            }
            return person.id
        }
    }
}
"#,
    ),
    (
        "domains/home/actions/answer_call.jadpo",
        r#"use home/actions { note_sighting }
use shared/providers/acme_voice

/// Answers a call Acme Voice rings through.
action answer_call(event: AcmeVoice.IncomingCall) -> AcmeVoice.CallReply {
    note_sighting("reply")
    return AcmeVoice.CallReply {
        say: "Thanks for calling."
        hang_up: event.from != ""
    }
}
"#,
    ),
    (
        "shared/providers/texter/v1/package.jadpo",
        "/// Texter's messaging API.\npackage Texter.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
    ),
    (
        "shared/providers/texter/v1/provider.jadpo",
        r#"/// Texter's API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.texter.example"
        sandbox: "https://sandbox.texter.example"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: "Idempotency-Key"
        window: 24h
        max_length: 255
    }
    error_code: "error.code"
}
"#,
    ),
    (
        "shared/providers/texter/v1/resources/messages.jadpo",
        r#"public resource messages {
    send: {
        method: HttpMethod.post
        path: "/v1/messages"
        input: SignInMessage
        changes: true
        idempotent: true
        source: PackageSource.written
    }
}
"#,
    ),
    (
        "shared/providers/texter/v1/types/sign_in_message.jadpo",
        r#"/// A sign-in link and code to send.
public type SignInMessage = Object {
    to: Email
    link: Url
    code: SignInCode
}
"#,
    ),
    (
        "shared/providers/acme_voice/v1/package.jadpo",
        "/// Acme Voice's calls API.\npackage AcmeVoice.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
    ),
    (
        "shared/providers/acme_voice/v1/provider.jadpo",
        r#"/// Acme Voice's API, which signs its webhooks as Twilio does.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
    webhooks: {
        check: SignatureScheme.twilio
        event_id: "CallSid"
        event_type: "CallStatus"
    }
}
"#,
    ),
    (
        "shared/providers/acme_voice/v1/types/call_reply.jadpo",
        r#"/// What a caller hears.
public type CallReply = Object {
    say: Text
    hang_up: Bool {
        wire_name: "hangUp"
    }
}
"#,
    ),
    (
        "shared/providers/acme_voice/v1/events/incoming_call.jadpo",
        r#"/// A call is ringing, and waits to be told what to do.
public type IncomingCall = Event {
    wire_name: "ringing"
    reply: CallReply
    occurred_at: Instant
    call_sid: Text {
        wire_name: "CallSid"
    }
    from: Text {
        wire_name: "From"
    }
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
    assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
}

/// The project, migrated into a database of its own, with Ash's and
/// Birch's chores.
struct Live {
    dir: PathBuf,
    program: Program,
    host: Faked,
    runtime: Runtime,
    worker: Worker,
}

impl Live {
    fn new(name: &str) -> Live {
        let dir = std::env::temp_dir().join(format!(
            "jadpo-system-tenants-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("home");
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
            .create_database(&format!("system_tenants_{name}"))
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
        variables.insert("TEXTER_KEY".to_string(), "tk_test_123".to_string());
        variables.insert("TEXTER_ENVIRONMENT".to_string(), "sandbox".to_string());
        variables.insert("VOICE_KEY".to_string(), "key_test".to_string());
        variables.insert(
            "VOICE_ADDRESS".to_string(),
            "https://api.example".to_string(),
        );
        variables.insert("VOICE_AUTH_TOKEN".to_string(), VOICE_TOKEN.to_string());
        let values = prepared
            .config(
                &variables,
                jadpo_runtime::config::Mode::Environment,
                &|bytes: &mut [u8]| NativeHost::new().random(bytes),
            )
            .expect("the settings check");
        let program = Program::new(Arc::clone(&prepared)).with_config(&values);
        let live = Live {
            dir,
            program,
            host: Faked {
                inner: NativeHost::new().with_store("main", store),
                texter: Fake::answering(Answer::Generated),
            },
            runtime,
            worker: Worker::default(),
        };
        live.execute(
            "INSERT INTO \"household\" (\"id\", \"name\") VALUES ('0a000000-0000-4000-8000-000000000001', 'Ash'), ('0b000000-0000-4000-8000-000000000002', 'Birch')",
        );
        live.execute(
            "INSERT INTO \"chore\" (\"id\", \"household_id\", \"title\") VALUES ('0c000000-0000-4000-8000-000000000001', '0a000000-0000-4000-8000-000000000001', 'ash-1'), ('0c000000-0000-4000-8000-000000000002', '0a000000-0000-4000-8000-000000000001', 'ash-2'), ('0c000000-0000-4000-8000-000000000003', '0a000000-0000-4000-8000-000000000001', 'ash-3'), ('0c000000-0000-4000-8000-000000000004', '0b000000-0000-4000-8000-000000000002', 'birch-1'), ('0c000000-0000-4000-8000-000000000005', '0b000000-0000-4000-8000-000000000002', 'birch-2')",
        );
        live
    }

    fn execute(&self, statement: &str) {
        let database = self.host.database("main").expect("main");
        self.runtime
            .block_on(database.query_once(statement, &[]))
            .expect("runs");
    }

    /// Answers `request`, then runs what it left for after the response.
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

    /// What each action saw, by what it was running.
    fn sightings(&self) -> Vec<(String, i64)> {
        let database = self.host.database("main").expect("main");
        self.runtime
            .block_on(database.query_once(
                "SELECT \"seen_by\", \"chores\"::int8 FROM \"sighting\" ORDER BY \"seen_by\"",
                &[],
            ))
            .expect("runs")
            .into_iter()
            .map(|row| match row.as_slice() {
                [SqlValue::Text(seen_by), SqlValue::Int8(chores)] => (seen_by.clone(), *chores),
                other => panic!("a sighting's row: {other:?}"),
            })
            .collect()
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

fn text(response: &Response) -> String {
    String::from_utf8_lossy(&response.body).into_owned()
}

/// A call ringing, as Acme Voice sends it: a form, signed as Twilio signs
/// over `https://<host>/webhooks/voice` and the form's members by name.
fn ringing(sid: &str, from: &str) -> Request {
    let members = [("CallSid", sid), ("CallStatus", "ringing"), ("From", from)];
    let body: Vec<String> = members
        .iter()
        .map(|(name, value)| format!("{name}={}", value.replace('+', "%2B")))
        .collect();
    let mut sorted = members.to_vec();
    sorted.sort();
    let mut signed = String::from("https://home.example/webhooks/voice");
    for (name, value) in sorted {
        signed.push_str(name);
        signed.push_str(value);
    }
    let key = openssl::pkey::PKey::hmac(VOICE_TOKEN.as_bytes()).expect("a key");
    let mut signer =
        openssl::sign::Signer::new(openssl::hash::MessageDigest::sha1(), &key).expect("a signer");
    signer.update(signed.as_bytes()).expect("signed");
    let mac = signer.sign_to_vec().expect("a signature");
    Request {
        method: "POST".into(),
        path: "/webhooks/voice".into(),
        headers: vec![
            (
                "content-type".into(),
                "application/x-www-form-urlencoded".into(),
            ),
            ("host".into(), "home.example".into()),
            (
                "x-twilio-signature".into(),
                jadpo_value::encoding::to_base64(&mac),
            ),
        ],
        body: body.join("&").into_bytes(),
        address: Some("192.0.2.20".into()),
        ..Request::default()
    }
}

/// The link the application's own sending last handed Texter.
fn handed_link(live: &Live) -> String {
    let sent = live.host.texter.sent();
    let call = sent.last().expect("a message");
    let Value::Record(record) = &call.input else {
        panic!("not a message: {:?}", call.input);
    };
    match record.get("link") {
        Some(Value::Text(link)) => link.to_string(),
        other => panic!("link: {other:?}"),
    }
}

/// The sign-in email's `send:` action, `on_first_sign_in:`'s and the
/// webhook's reply action each reach no tenant's records: the system
/// reads none of the five chores from any of them.
#[test]
fn the_system_reaches_no_tenant_from_a_sign_in_or_a_reply() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("reach");
    let asked = live.send(&request(
        "POST",
        "/auth/magic-link",
        &[],
        "{\"address\":\"ada@example.com\"}",
    ));
    assert_eq!(asked.status, 202, "{}", text(&asked));
    assert_eq!(live.sightings(), [("send".to_string(), 0)]);
    let link = handed_link(&live);
    let token = link.rsplit('=').next().unwrap_or_default().to_string();
    let confirmed = live.send(&request(
        "POST",
        "/auth/magic-link/confirm",
        &[("origin", ORIGIN)],
        &format!("{{\"token\":\"{token}\"}}"),
    ));
    assert_eq!(confirmed.status, 200, "{}", text(&confirmed));
    assert_eq!(
        live.sightings(),
        [("first".to_string(), 0), ("send".to_string(), 0)]
    );
    let answered = live.send(&ringing("CA100", "+447700900123"));
    assert_eq!(answered.status, 200, "{}", text(&answered));
    assert_eq!(
        live.sightings(),
        [
            ("first".to_string(), 0),
            ("reply".to_string(), 0),
            ("send".to_string(), 0)
        ]
    );
}

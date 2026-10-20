//! Magic links against live PostgreSQL (area 8, "Magic links"; chunk
//! 7.3b): the runtime answers the generated routes as a deployable does
//! (`http::serve_request`) on the native host, with a clock the test moves,
//! and Jadpo's own email is rendered and handed to a fake of the `email`
//! connection, so nothing leaves the machine:
//!
//! - a person asks, the email carries a link to the frontend's page and a
//!   six-digit code, and either signs them in once, ending the other;
//! - a first sign-in runs `on_first_sign_in:`'s action, which may refuse;
//! - sends are limited per address alike for every address, wrong codes
//!   end a code and, past the day's, stop codes but not links;
//! - a `same_browser:` link works only in the browser that asked;
//! - the code is redeemed for an API token, a disabled person signs in
//!   with nothing, and the confirm needs the application's own `Origin`;
//! - neither the outbox nor the table ever holds a link or a code.
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
use jadpo_runtime::capability::{Answer, Capability, Fake};
use jadpo_runtime::http::{Request, Response, Worker, serve_request};
use jadpo_runtime::{Database, Host, LocalFuture};
use jadpo_value::Value;

/// The native host, with a wall clock the test sets, the `email`
/// connection faked, and the provider the application's own sending
/// calls faked too.
struct Clocked {
    inner: NativeHost,
    now: Cell<i64>,
    mail: Fake,
    texter: Fake,
}

impl Host for Clocked {
    fn capability(&self, connection: &str) -> Option<&dyn Capability> {
        match connection {
            "mail" => Some(&self.mail as &dyn Capability),
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
        Some(self.now.get())
    }
}

/// 2026-10-20T09:00:00Z.
const START: i64 = 1_792_486_800_000;
const MINUTE: i64 = 60_000;
const ORIGIN: &str = "https://notes.example";

/// Which way the project's sign-in email is sent.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Variant {
    /// Jadpo's own email, plain.
    Plain,
    /// Jadpo's own email, branded, with `same_browser: true`.
    SameBrowser,
    /// The application's own email (area 16, "Built-in emails", level 4).
    OwnEmail,
    /// The application's own sending, through a provider (level 5).
    OwnSending,
}

fn files(variant: Variant) -> Vec<(&'static str, String)> {
    let same_browser = variant == Variant::SameBrowser;
    let same = if same_browser {
        "\n            same_browser: true"
    } else {
        ""
    };
    let mut files = vec![
        (
            "config.jadpo",
            r#"config {
    browser_origin: Url
    mail_sender: Email {
        default: "hello@notes.example"
    }
    smtp_host: Hostname {
        default: "localhost"
    }
}
"#
            .to_string(),
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
        subject: Member.email
        active: Member.status == MemberStatus.active
    }
}
"#
            .to_string(),
        ),
        (
            "authentication.jadpo",
            format!(
                r#"use notes

authentication {{
    browser_session: {{
        cookie: "notes_session"
        origin: config.browser_origin
    }}
    api_token: {{}}
    sign_in: {{
        magic_link: {{
            link: "/sign-in/confirm"
            code_attempts: 3
            codes_per_hour: 3
            failed_codes_per_day: 5{same}{level}
        }}
    }}
    on_first_sign_in: notes.create_member
}}
"#,
                level = match variant {
                    Variant::OwnEmail => "\n            email: notes.SignInEmail",
                    Variant::OwnSending => "\n            send: notes.send_sign_in_link",
                    _ => "",
                }
            ),
        ),
        (
            "domains/notes/types/member_status.jadpo",
            "type MemberStatus = Enum {\n    active\n    suspended\n}\n".to_string(),
        ),
        (
            "domains/notes/types/member_view.jadpo",
            "use notes/entities { Member }\n\ntype MemberView = Output {\n    pick: [Member.email]\n}\n"
                .to_string(),
        ),
        (
            "domains/notes/failures/sign_up_closed.jadpo",
            "/// Signing up is closed for this address.\ntype SignUpClosed = Failure {\n    kind: FailureKind.conflict\n}\n"
                .to_string(),
        ),
        (
            "domains/notes/failures/member_not_found.jadpo",
            "type MemberNotFound = Failure {\n    kind: FailureKind.not_found\n}\n".to_string(),
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
        create: [Access.system]
        read: [id]
    }
}
"#
            .to_string(),
        ),
        (
            "domains/notes/actions/create_member.jadpo",
            r#"use notes/entities { Member }
use notes/failures { SignUpClosed }
use notes/types { MemberStatus }

/// Signs a new member up the first time they sign in, unless their
/// address is one the club has closed.
action create_member(first: FirstSignIn) fails SignUpClosed -> Member.id {
    match first.address {
        none -> {
            reject SignUpClosed {}
        }
        some(address) -> {
            if (address == "closed@notes.example") {
                reject SignUpClosed {}
            }
            var member: Member = attempt Member.create {
                set: {
                    email: address
                    status: MemberStatus.active
                }
                conflict: {
                    Member.email: SignUpClosed
                }
            }
            return member.id
        }
    }
}
"#
            .to_string(),
        ),
        (
            "domains/notes/queries/whoami.jadpo",
            r#"use notes/entities { Member }
use notes/failures { MemberNotFound }
use notes/types { MemberView }

/// The signed-in member.
query whoami(caller: Principal.user) fails MemberNotFound -> MemberView {
    from: Member
    where: Member.id == caller.id
    missing: MemberNotFound
}
"#
            .to_string(),
        ),
        (
            "domains/notes/routes/account.jadpo",
            r#"use notes/queries { whoami }
use notes/types { MemberView }

routes Account {
    prefix: /account

    /// Who is signed in.
    GET /me {
        output: MemberView
        run: whoami(current_principal.user)
    }
}
"#
            .to_string(),
        ),
    ];
    // The application's own email (level 4): its words, its templates and
    // its subject, filled with the link and the code Jadpo makes.
    if variant == Variant::OwnEmail {
        files.push((
            "domains/notes/emails/sign_in_email.jadpo",
            r#"/// The sign-in email, in Notes' own words.
type SignInEmail = EmailMessage {
    pick: [MagicLinkEmail.sign_in_link, MagicLinkEmail.code, MagicLinkEmail.expires_in]
}
"#
            .to_string(),
        ));
        files.push((
            "domains/notes/emails/sign_in_email.html",
            "<h1>Welcome back to Notes</h1>\n<p><a href=\"{{ sign_in_link }}\">Open Notes</a></p>\n<p>Or type {{ code }} on the sign-in page.</p>\n<p>Both work for {{ expires_in }}.</p>\n"
                .to_string(),
        ));
        files.push((
            "domains/notes/emails/sign_in_email.txt",
            "Welcome back to Notes\n\nOpen Notes: {{ sign_in_link }}\n\nOr type code: {{ code }} on the sign-in page.\n\nBoth work for {{ expires_in }}.\n"
                .to_string(),
        ));
    }
    // The application's own sending (level 5): no email connection, and
    // an action handing the link and the code to a provider of its own.
    if variant == Variant::OwnSending {
        if let Some((_, app)) = files.iter_mut().find(|(path, _)| *path == "app.jadpo") {
            *app = app.replace(
                r#"        mail: {
            driver: Driver.smtp
            host: config.smtp_host
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }"#,
                r#"        texter: {
            package: Texter.v1
            domains: [notes]
            credential: config.texter_key
            environment: config.texter_environment
        }
    }"#,
            );
        }
        if let Some((_, config)) = files.iter_mut().find(|(path, _)| *path == "config.jadpo") {
            *config = config.replace(
                "    browser_origin: Url\n",
                "    browser_origin: Url\n    texter_key: Text {\n        secret: true\n    }\n    texter_environment: ProviderEnvironment\n",
            );
        }
        files.extend([
            (
                "shared/providers/texter/v1/package.jadpo",
                "/// Texter's messaging API.\npackage Texter.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n"
                    .to_string(),
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
    error_code: "error.code"
}
"#
                .to_string(),
            ),
            (
                "shared/providers/texter/v1/resources/messages.jadpo",
                r#"public resource messages {
    send: {
        method: HttpMethod.post
        path: "/v1/messages"
        input: SignInMessage
        changes: true
        source: PackageSource.written
    }
}
"#
                .to_string(),
            ),
            (
                "shared/providers/texter/v1/types/sign_in_message.jadpo",
                r#"/// A sign-in link and code to send.
public type SignInMessage = Object {
    to: Email
    link: Url
    code: SignInCode
}
"#
                .to_string(),
            ),
            (
                "domains/notes/actions/send_sign_in_link.jadpo",
                r#"use app { texter }
use shared/providers/texter

/// Hands the sign-in link and code to Texter.
action send_sign_in_link(request: MagicLinkRequest) -> Unit {
    texter.messages.send(Texter.SignInMessage {
        to: request.address
        link: request.sign_in_link
        code: request.code
    })
}
"#
                .to_string(),
            ),
        ]);
    }
    // The browser project is branded: its layout wraps Jadpo's email, and
    // it rewords the subject (area 16, "Built-in emails", levels 2 and 3).
    if same_browser || variant == Variant::OwnEmail {
        files.push((
            "shared/emails/layout.html",
            "<html dir=\"{{ locale.direction }}\"><head><title>{{ subject }}</title></head><body><header>{{ app.display_name }}</header>{{ content }}</body></html>\n"
                .to_string(),
        ));
        files.push((
            "shared/emails/layout.txt",
            "{{ app.display_name }}\n\n{{ content }}".to_string(),
        ));
        files.push((
            "translations/en_gb.jadpo",
            "translations {\n    jadpo: {\n        magic_link_email: \"Your link to {app_name}\"\n    }\n}\n"
                .to_string(),
        ));
        let subject = if variant == Variant::OwnEmail {
            "    sign_in_email: \"Your Notes sign-in link\"\n"
        } else {
            ""
        };
        files.push((
            "domains/notes/translations/en_gb.jadpo",
            format!(
                "translations {{\n    member_not_found: \"We couldn't find you.\"\n{subject}    sign_up_closed: \"Signing up is closed.\"\n}}\n"
            ),
        ));
        if let Some((_, app)) = files.iter_mut().find(|(path, _)| *path == "app.jadpo") {
            app.push_str(
                "\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb]\n}\n",
            );
        }
    }
    files
}

fn jadpo(cwd: &Path, args: &[&str]) {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = jadpo_term::Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", "1792504800")
        .with_cwd(cwd);
    let done = jadpo_cli::run(&args, &env);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
}

/// One email the fake caught: its subject, its text, its HTML and who it
/// went to.
#[derive(Debug)]
struct Mail {
    to: String,
    subject: String,
    text: String,
    html: String,
}

impl Mail {
    /// The link's token.
    fn token(&self) -> String {
        let (_, rest) = self
            .text
            .split_once("/sign-in/confirm?token=")
            .expect("a link");
        rest.chars().take(64).collect()
    }

    /// The six-digit code.
    fn code(&self) -> String {
        let (_, rest) = self.text.split_once("code: ").expect("a code");
        rest.chars().take(6).collect()
    }
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
    fn new(name: &str, variant: Variant) -> Live {
        let dir =
            std::env::temp_dir().join(format!("jadpo-magic-links-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("notes");
        for (path, text) in files(variant) {
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
            .create_database(&format!("magic_links_{name}"))
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
                texter: Fake::answering(Answer::Generated),
            },
            runtime,
            worker: Worker::default(),
        }
    }

    fn at(&self, now: i64) {
        self.host.now.set(now);
    }

    /// Answers `request`, then runs what it left for after the response:
    /// the email's delivery.
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

    fn rows(&self, statement: &str) -> Vec<jadpo_runtime::Row> {
        let database = self.host.database("main").expect("main");
        self.runtime
            .block_on(database.query_once(statement, &[]))
            .expect("runs")
    }

    /// Every email sent so far.
    fn mails(&self) -> Vec<Mail> {
        self.host
            .mail
            .sent()
            .into_iter()
            .map(|call| {
                let Value::Record(record) = &call.input else {
                    panic!("not a message: {:?}", call.input);
                };
                let text = |name: &str| match record.get(name) {
                    Some(Value::Text(text)) => text.to_string(),
                    other => panic!("{name}: {other:?}"),
                };
                Mail {
                    to: text("to"),
                    subject: text("subject"),
                    text: text("text"),
                    html: text("html"),
                }
            })
            .collect()
    }

    /// Asks for a link for `address` from `browser`.
    fn ask(&self, browser: &mut Browser, address: &str) -> Response {
        let response = self.send(&browser.public(
            "POST",
            "/auth/magic-link",
            &format!("{{\"address\":\"{address}\",\"return_to\":\"/notes\"}}"),
        ));
        browser.keep(&response);
        response
    }

    /// The last email, which must have gone to `address`.
    fn last_mail(&self, address: &str) -> Mail {
        let mail = self.mails().pop().expect("an email");
        assert_eq!(mail.to, address);
        mail
    }

    fn confirm(&self, browser: &mut Browser, body: &str) -> Response {
        let response = self.send(&browser.public("POST", "/auth/magic-link/confirm", body));
        browser.keep(&response);
        response
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

    /// A request from the application's own page, with the cookies.
    fn public(&self, method: &str, path: &str, body: &str) -> Request {
        let cookies: String = self
            .cookies
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        let mut headers = vec![("origin", ORIGIN)];
        if !cookies.is_empty() {
            headers.push(("cookie", cookies.as_str()));
        }
        request(method, path, &headers, body)
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

/// A code that isn't `code`.
fn other_code(code: &str) -> String {
    if code == "000000" {
        "000001".to_string()
    } else {
        "000000".to_string()
    }
}

#[test]
fn a_link_signs_a_new_person_in_once_and_ends_its_code() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("link", Variant::Plain);
    let mut browser = Browser::default();
    let asked = live.ask(&mut browser, "ada@example.com");
    assert_eq!(asked.status, 202, "{}", text(&asked));
    assert!(asked.body.is_empty());
    let mail = live.last_mail("ada@example.com");
    assert_eq!(mail.subject, "Sign in to Notes");
    assert!(
        mail.text
            .contains("https://notes.example/sign-in/confirm?token="),
        "{}",
        mail.text
    );
    assert!(
        mail.text
            .contains("Someone asked to sign in as ada@example.com.")
    );
    assert!(
        mail.text
            .contains("The link and the code work for 15 minutes.")
    );
    assert!(mail.html.starts_with("<!DOCTYPE html>"), "{}", mail.html);
    assert!(mail.html.contains(&mail.code()));
    let (token, six) = (mail.token(), mail.code());
    assert!(six.bytes().all(|b| b.is_ascii_digit()) && six.len() == 6);
    // Neither the outbox nor the table holds the link or the code.
    for row in live.rows("SELECT \"payload\"::text FROM \"jadpo_outbox\"") {
        let found = format!("{row:?}");
        assert!(!found.contains(&token) && !found.contains(&six), "{found}");
    }
    for row in live.rows("SELECT row_to_json(r)::text FROM \"jadpo_sign_in_requests\" AS r") {
        let found = format!("{row:?}");
        assert!(
            !found.contains(&token) && !found.contains(&six) && !found.contains("ada@example"),
            "{found}"
        );
    }
    // Loading the page posts the token: signed in, as a new member.
    let confirmed = live.confirm(&mut browser, &format!("{{\"token\":\"{token}\"}}"));
    assert_eq!(confirmed.status, 200, "{}", text(&confirmed));
    assert_eq!(json(&confirmed)["return_to"], "/notes");
    assert!(browser.signed_in());
    let me = live.send(&browser.public("GET", "/account/me", ""));
    assert_eq!(me.status, 200, "{}", text(&me));
    assert_eq!(json(&me)["email"], "ada@example.com");
    // Used once; its code went with it.
    let again = live.confirm(
        &mut Browser::default(),
        &format!("{{\"token\":\"{token}\"}}"),
    );
    refused(&again, 401, "sign_in_link_expired");
    let by_code = live.confirm(
        &mut Browser::default(),
        &format!("{{\"address\":\"ada@example.com\",\"code\":\"{six}\"}}"),
    );
    refused(&by_code, 401, "sign_in_code_invalid");
}

#[test]
fn a_code_signs_in_and_wrong_codes_end_it() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("code", Variant::Plain);
    let mut browser = Browser::default();
    assert_eq!(live.ask(&mut browser, "bo@example.com").status, 202);
    let six = live.last_mail("bo@example.com").code();
    let wrong = format!(
        "{{\"address\":\"bo@example.com\",\"code\":\"{}\"}}",
        other_code(&six)
    );
    let right = format!("{{\"address\":\"bo@example.com\",\"code\":\"{six}\"}}");
    // A wrong code, and the right one for another address, say the same.
    refused(
        &live.confirm(&mut browser, &wrong),
        401,
        "sign_in_code_invalid",
    );
    refused(
        &live.confirm(
            &mut browser,
            &format!("{{\"address\":\"Bo@example.com\",\"code\":\"{six}\"}}"),
        ),
        401,
        "sign_in_code_invalid",
    );
    let signed = live.confirm(&mut browser, &right);
    assert_eq!(signed.status, 200, "{}", text(&signed));
    assert!(browser.signed_in());
    // The next day, a new code guessed wrong `code_attempts:` times is
    // ended.
    live.at(START + 24 * 60 * MINUTE);
    let mut other = Browser::default();
    assert_eq!(live.ask(&mut other, "bo@example.com").status, 202);
    let six = live.last_mail("bo@example.com").code();
    let wrong = format!(
        "{{\"address\":\"bo@example.com\",\"code\":\"{}\"}}",
        other_code(&six)
    );
    for _ in 0..3 {
        refused(
            &live.confirm(&mut other, &wrong),
            401,
            "sign_in_code_invalid",
        );
    }
    let late = live.confirm(
        &mut other,
        &format!("{{\"address\":\"bo@example.com\",\"code\":\"{six}\"}}"),
    );
    refused(&late, 401, "sign_in_code_invalid");
    assert!(!other.signed_in());
}

#[test]
fn sends_are_limited_alike_for_every_address() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("limits", Variant::Plain);
    let mut browser = Browser::default();
    for address in ["cy@example.com", "nobody@example.com"] {
        assert_eq!(live.ask(&mut browser, address).status, 202);
        let soon = live.ask(&mut browser, address);
        refused(&soon, 429, "rate_limited");
        assert_eq!(soon.header("retry-after"), Some("60"));
    }
    // `codes_per_hour: 3`.
    live.at(START + 2 * MINUTE);
    assert_eq!(live.ask(&mut browser, "cy@example.com").status, 202);
    live.at(START + 4 * MINUTE);
    assert_eq!(live.ask(&mut browser, "cy@example.com").status, 202);
    live.at(START + 6 * MINUTE);
    refused(
        &live.ask(&mut browser, "cy@example.com"),
        429,
        "rate_limited",
    );
    live.at(START + 61 * MINUTE);
    assert_eq!(live.ask(&mut browser, "cy@example.com").status, 202);
    // Only the latest link works.
    let mails: Vec<Mail> = live
        .mails()
        .into_iter()
        .filter(|mail| mail.to == "cy@example.com")
        .collect();
    let first = mails.first().expect("a first email").token();
    let last = mails.last().expect("a last email").token();
    refused(
        &live.confirm(&mut browser, &format!("{{\"token\":\"{first}\"}}")),
        401,
        "sign_in_link_expired",
    );
    // Past 15 minutes, the latest has expired too.
    live.at(START + 77 * MINUTE);
    refused(
        &live.confirm(&mut browser, &format!("{{\"token\":\"{last}\"}}")),
        401,
        "sign_in_link_expired",
    );
}

#[test]
fn past_the_days_wrong_codes_codes_stop_but_links_work() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("day", Variant::Plain);
    let mut browser = Browser::default();
    let mut minute = 0;
    // Two codes' worth of wrong guesses: 3 and 2, the day's 5.
    for guesses in [3, 2] {
        live.at(START + minute * MINUTE);
        minute += 2;
        assert_eq!(live.ask(&mut browser, "di@example.com").status, 202);
        let six = live.last_mail("di@example.com").code();
        let wrong = format!(
            "{{\"address\":\"di@example.com\",\"code\":\"{}\"}}",
            other_code(&six)
        );
        for _ in 0..guesses {
            refused(
                &live.confirm(&mut browser, &wrong),
                401,
                "sign_in_code_invalid",
            );
        }
    }
    let mail = live.last_mail("di@example.com");
    let disabled = live.confirm(
        &mut browser,
        &format!(
            "{{\"address\":\"di@example.com\",\"code\":\"{}\"}}",
            mail.code()
        ),
    );
    refused(&disabled, 429, "sign_in_codes_disabled");
    assert!(disabled.header("retry-after").is_some());
    let linked = live.confirm(&mut browser, &format!("{{\"token\":\"{}\"}}", mail.token()));
    assert_eq!(linked.status, 200, "{}", text(&linked));
}

#[test]
fn a_same_browser_link_works_only_where_it_was_asked_for() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("browser", Variant::SameBrowser);
    let mut asker = Browser::default();
    let asked = live.ask(&mut asker, "ed@example.com");
    assert_eq!(asked.status, 202);
    let cookie = asked.header("set-cookie").expect("the binding cookie");
    assert!(
        cookie.starts_with("notes_session_sign_in=")
            && cookie.contains("HttpOnly")
            && cookie.contains("SameSite=Lax"),
        "{cookie}"
    );
    let mail = live.last_mail("ed@example.com");
    // Worded by the application, in its layout.
    assert_eq!(mail.subject, "Your link to Notes");
    assert!(
        mail.html.starts_with("<html dir=\"ltr\"><head><title>Your link to Notes</title></head><body><header>Notes</header><div dir=\"ltr\""),
        "{}",
        mail.html
    );
    assert!(mail.text.starts_with("Notes\n\nSign in\n"), "{}", mail.text);
    let link = format!("{{\"token\":\"{}\"}}", mail.token());
    refused(
        &live.confirm(&mut Browser::default(), &link),
        401,
        "sign_in_link_other_browser",
    );
    // Left as it was: the browser that asked still signs in.
    let signed = live.confirm(&mut asker, &link);
    assert_eq!(signed.status, 200, "{}", text(&signed));
    assert!(asker.signed_in());
    assert!(!asker.cookies.contains_key("notes_session_sign_in"));
}

#[test]
fn a_code_is_redeemed_for_an_api_token() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("redeem", Variant::Plain);
    let mut browser = Browser::default();
    assert_eq!(live.ask(&mut browser, "fi@example.com").status, 202);
    let six = live.last_mail("fi@example.com").code();
    // The app's own client sends no `Origin`.
    let redeemed = live.send(&request(
        "POST",
        "/auth/magic-link/redeem",
        &[],
        &format!("{{\"address\":\"fi@example.com\",\"code\":\"{six}\"}}"),
    ));
    assert_eq!(redeemed.status, 200, "{}", text(&redeemed));
    let token = json(&redeemed)["token"]
        .as_str()
        .expect("a token")
        .to_string();
    assert!(token.starts_with("jadpo_token_"), "{token}");
    let bearer = format!("Bearer {token}");
    let me = live.send(&request(
        "GET",
        "/account/me",
        &[("authorization", bearer.as_str())],
        "",
    ));
    assert_eq!(me.status, 200, "{}", text(&me));
    assert_eq!(json(&me)["email"], "fi@example.com");
}

#[test]
fn a_refused_first_sign_in_and_a_disabled_person_get_nothing() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("refused", Variant::Plain);
    let mut browser = Browser::default();
    assert_eq!(live.ask(&mut browser, "closed@notes.example").status, 202);
    let mail = live.last_mail("closed@notes.example");
    let refusal = live.confirm(&mut browser, &format!("{{\"token\":\"{}\"}}", mail.token()));
    refused(&refusal, 403, "sign_in_refused");
    assert!(!browser.signed_in());
    // A member who is suspended signs in with nothing.
    assert_eq!(live.ask(&mut browser, "gu@example.com").status, 202);
    let mail = live.last_mail("gu@example.com");
    assert_eq!(
        live.confirm(&mut browser, &format!("{{\"token\":\"{}\"}}", mail.token()))
            .status,
        200
    );
    let suspended = live.rows(
        "UPDATE \"member\" SET \"status\" = 'suspended' WHERE \"email\" = 'gu@example.com' RETURNING \"id\"",
    );
    assert_eq!(suspended.len(), 1);
    live.at(START + 2 * MINUTE);
    let mut later = Browser::default();
    assert_eq!(live.ask(&mut later, "gu@example.com").status, 202);
    let mail = live.last_mail("gu@example.com");
    refused(
        &live.confirm(&mut later, &format!("{{\"token\":\"{}\"}}", mail.token())),
        403,
        "user_disabled",
    );
    assert!(!later.signed_in());
}

#[test]
fn the_confirm_needs_the_sites_origin_and_return_to_stays_on_it() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("origin", Variant::Plain);
    let mut browser = Browser::default();
    for to in [
        "//evil.example/",
        "https://evil.example/",
        "/\\\\evil.example",
    ] {
        let asked = live.send(&browser.public(
            "POST",
            "/auth/magic-link",
            &format!("{{\"address\":\"ha@example.com\",\"return_to\":\"{to}\"}}"),
        ));
        refused(&asked, 422, "invalid_input");
    }
    assert!(live.mails().is_empty());
    assert_eq!(live.ask(&mut browser, "ha@example.com").status, 202);
    let mail = live.last_mail("ha@example.com");
    let elsewhere = live.send(&request(
        "POST",
        "/auth/magic-link/confirm",
        &[("origin", "https://evil.example")],
        &format!("{{\"token\":\"{}\"}}", mail.token()),
    ));
    refused(&elsewhere, 403, "csrf_rejected");
    // Refused before it was used: it still works from the site.
    let signed = live.confirm(&mut browser, &format!("{{\"token\":\"{}\"}}", mail.token()));
    assert_eq!(signed.status, 200, "{}", text(&signed));
}

#[test]
fn the_apps_own_email_carries_the_link_and_the_code() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("own_email", Variant::OwnEmail);
    let mut browser = Browser::default();
    let asked = live.ask(&mut browser, "ada@example.com");
    assert_eq!(asked.status, 202, "{}", text(&asked));
    // Jadpo fills the application's own email, wrapped in its layout, in
    // place of its own.
    let mail = live.last_mail("ada@example.com");
    assert_eq!(mail.subject, "Your Notes sign-in link");
    assert!(
        mail.text.starts_with("Notes\n\nWelcome back to Notes"),
        "{}",
        mail.text
    );
    assert!(
        mail.text.contains("Both work for 15 minutes."),
        "{}",
        mail.text
    );
    assert!(
        mail.html.contains("<header>Notes</header>"),
        "{}",
        mail.html
    );
    assert!(
        mail.html.contains("<h1>Welcome back to Notes</h1>"),
        "{}",
        mail.html
    );
    assert!(!mail.text.contains("Someone asked to sign in"));
    let (token, six) = (mail.token(), mail.code());
    assert!(
        mail.html.contains(&format!(
            "https://notes.example/sign-in/confirm?token={token}"
        )),
        "{}",
        mail.html
    );
    assert!(
        mail.html.contains(&format!("Or type {six} on")),
        "{}",
        mail.html
    );
    // Neither the outbox nor the table holds the link or the code.
    for row in live.rows("SELECT \"payload\"::text FROM \"jadpo_outbox\"") {
        let found = format!("{row:?}");
        assert!(!found.contains(&token) && !found.contains(&six), "{found}");
    }
    // The code signs Ada in, once, and ends the link.
    let confirmed = live.confirm(
        &mut browser,
        &format!("{{\"address\":\"ada@example.com\",\"code\":\"{six}\"}}"),
    );
    assert_eq!(confirmed.status, 200, "{}", text(&confirmed));
    assert!(browser.signed_in());
    let mut other = Browser::default();
    refused(
        &live.confirm(&mut other, &format!("{{\"token\":\"{token}\"}}")),
        401,
        "sign_in_link_expired",
    );
}

/// What the application's own sending handed its provider: the address,
/// the link and the code.
fn handed(call: &jadpo_runtime::capability::Call) -> (String, String, String) {
    let Value::Record(record) = &call.input else {
        panic!("not a message: {:?}", call.input);
    };
    let text = |name: &str| match record.get(name) {
        Some(Value::Text(text)) => text.to_string(),
        other => panic!("{name}: {other:?}"),
    };
    (text("to"), text("link"), text("code"))
}

#[test]
fn the_apps_own_sending_is_given_a_fresh_link_and_code_each_attempt() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("own_sending", Variant::OwnSending);
    // Texter is down for the whole first attempt: the call is sent again
    // as its connection's `retry:` allows, three times, then the action
    // fails `unavailable`.
    for _ in 0..3 {
        live.host.texter.script(
            "messages.send",
            jadpo_runtime::capability::Scripted::Answer(Answer::Unavailable),
        );
    }
    let mut browser = Browser::default();
    let asked = live.ask(&mut browser, "ada@example.com");
    assert_eq!(asked.status, 202, "{}", text(&asked));
    // Jadpo sends no email of its own.
    assert!(live.mails().is_empty());
    let sent = live.host.texter.sent();
    assert_eq!(sent.len(), 3, "{sent:?}");
    let (to, first_link, first_code) = handed(&sent[0]);
    assert_eq!(to, "ada@example.com");
    assert!(
        first_link.starts_with("https://notes.example/sign-in/confirm?token="),
        "{first_link}"
    );
    assert!(first_code.len() == 6 && first_code.bytes().all(|b| b.is_ascii_digit()));
    assert_eq!(handed(&sent[2]).1, first_link);
    // The delivery is retried as a subscriber's is, with a link and a code
    // of its own.
    live.at(START + 5 * MINUTE);
    live.runtime
        .block_on(jadpo_runtime::runner::sweep(
            &live.program,
            &live.host,
            "main",
            START + 5 * MINUTE,
        ))
        .expect("sweeps");
    let sent = live.host.texter.sent();
    assert_eq!(sent.len(), 4, "{sent:?}");
    let (_, link, code) = handed(&sent[3]);
    assert_ne!(link, first_link);
    // Only the latest works.
    let first_token = first_link
        .rsplit('=')
        .next()
        .unwrap_or_default()
        .to_string();
    let token = link.rsplit('=').next().unwrap_or_default().to_string();
    let mut other = Browser::default();
    refused(
        &live.confirm(&mut other, &format!("{{\"token\":\"{first_token}\"}}")),
        401,
        "sign_in_link_expired",
    );
    // The outbox never held either.
    for row in live.rows("SELECT \"payload\"::text FROM \"jadpo_outbox\"") {
        let found = format!("{row:?}");
        assert!(
            !found.contains(&token) && !found.contains(&code) && !found.contains(&first_token),
            "{found}"
        );
    }
    let confirmed = live.confirm(&mut browser, &format!("{{\"token\":\"{token}\"}}"));
    assert_eq!(confirmed.status, 200, "{}", text(&confirmed));
    assert!(browser.signed_in());
}

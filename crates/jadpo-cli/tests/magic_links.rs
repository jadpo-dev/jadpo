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
    /// The application's own routes, with `generate_routes: false` and
    /// `same_browser: true` (chunk 7.3b3).
    OwnRoutes,
}

fn files(variant: Variant) -> Vec<(&'static str, String)> {
    let same_browser = variant == Variant::SameBrowser;
    let same = match variant {
        Variant::SameBrowser => "\n            same_browser: true",
        Variant::OwnRoutes => {
            "\n            same_browser: true\n            generate_routes: false"
        }
        _ => "",
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
    // What the application hears of each sign-in (area 8, "Events"; chunk
    // 7.3b4): a note for every event, kept by a subscriber.
    files.extend(listeners());
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
    idempotency: {
        header: "Idempotency-Key"
        window: 24h
        max_length: 255
    }
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
        idempotent: true
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
    // The application's own routes over magic links' operations (area 8,
    // "Writing the routes yourself").
    if variant == Variant::OwnRoutes {
        files.extend([
            (
                "domains/notes/types/return_path.jadpo",
                "type ReturnPath = Text {\n    max_length: 2000\n}\n".to_string(),
            ),
            (
                "domains/notes/types/link_asked.jadpo",
                "use notes/types { ReturnPath }\n\ntype LinkAsked = Input {\n    address: Email\n    locale: Locale\n    return_to: ReturnPath?\n}\n"
                    .to_string(),
            ),
            (
                "domains/notes/types/link_given.jadpo",
                "type LinkGiven = Input {\n    token: SignInToken\n}\n".to_string(),
            ),
            (
                "domains/notes/types/code_given.jadpo",
                "type CodeGiven = Input {\n    address: Email\n    code: SignInCode\n}\n"
                    .to_string(),
            ),
            (
                "domains/notes/types/signed_in_view.jadpo",
                "type SignedInView = Output {\n    return_to: Text\n}\n".to_string(),
            ),
            (
                "domains/notes/types/app_signed_in.jadpo",
                "type AppSignedIn = Output {\n    token: ApiToken\n}\n".to_string(),
            ),
            (
                "domains/notes/actions/ask_for_link.jadpo",
                r#"use notes/types { LinkAsked }

/// Sends a link and a code to the address asked for.
action ask_for_link(input: LinkAsked) -> Unit {
    magic_links.request(input.address, input.locale, input.return_to)
}
"#
                .to_string(),
            ),
            (
                "domains/notes/actions/confirm_link.jadpo",
                r#"use notes/types { LinkGiven, SignedInView }

/// Signs this browser in with a link's token.
action confirm_link(input: LinkGiven) -> SignedInView {
    var redeemed: MagicLinkRedeemed = magic_links.redeem(input.token)
    sessions.start(redeemed.user_id)
    return SignedInView {
        return_to: redeemed.return_to
    }
}
"#
                .to_string(),
            ),
            (
                "domains/notes/actions/confirm_code.jadpo",
                r#"use notes/types { CodeGiven, SignedInView }

/// Signs this browser in with a code.
action confirm_code(input: CodeGiven) -> SignedInView {
    var redeemed: MagicLinkRedeemed = magic_links.redeem_code(input.address, input.code)
    sessions.start(redeemed.user_id)
    return SignedInView {
        return_to: redeemed.return_to
    }
}
"#
                .to_string(),
            ),
            (
                "domains/notes/actions/token_for_code.jadpo",
                r#"use notes/types { AppSignedIn, CodeGiven }

/// Gives the application's own client a token for a code.
action token_for_code(input: CodeGiven) -> AppSignedIn {
    var redeemed: MagicLinkRedeemed = magic_links.redeem_code(input.address, input.code)
    return AppSignedIn {
        token: sessions.issue_token(redeemed.user_id)
    }
}
"#
                .to_string(),
            ),
            (
                "domains/notes/routes/sign_in.jadpo",
                r#"use notes/actions { ask_for_link, confirm_code, confirm_link, token_for_code }
use notes/types { AppSignedIn, CodeGiven, LinkAsked, LinkGiven, SignedInView }

routes SignIn {
    prefix: /sign-in

    /// Asks for a link and a code.
    POST /ask {
        authentication: RouteAuthentication.none
        input: LinkAsked
        status: SuccessStatus.no_content
        run: ask_for_link(input)
    }

    /// Signs in with a link's token.
    POST /link {
        authentication: RouteAuthentication.none
        input: LinkGiven
        output: SignedInView
        run: confirm_link(input)
    }

    /// Signs in with a code.
    POST /code {
        authentication: RouteAuthentication.none
        input: CodeGiven
        output: SignedInView
        run: confirm_code(input)
    }

    /// Trades a code for an API token.
    POST /token {
        authentication: RouteAuthentication.none
        input: CodeGiven
        output: AppSignedIn
        run: token_for_code(input)
    }
}
"#
                .to_string(),
            ),
            (
                "policy.jadpo",
                r#"use notes

policy {
    actions: {
        notes.ask_for_link: [Access.public]
        notes.confirm_link: [Access.public]
        notes.confirm_code: [Access.public]
        notes.token_for_code: [Access.public]
    }
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

/// A subscriber of each sign-in event, keeping what it heard as a note.
fn listeners() -> Vec<(&'static str, String)> {
    let note = |action: &str, event: &str, set: &str| {
        format!(
            "use notes/entities {{ SignInNote }}\nuse notes/failures {{ MemberNotFound }}\n\n/// Notes what it heard.\naction {action}(event: {event}) fails MemberNotFound -> SignInNote {{\n    return attempt SignInNote.create {{\n        set: {{\n{set}        }}\n    }}\n}}\n"
        )
    };
    let listen = |name: &str, event: &str, action: &str| {
        format!(
            "use notes/actions {{ {action} }}\n\n/// Notes each `{event}`.\nsubscriber {name} {{\n    on: {event}\n    run: {action}(event)\n}}\n"
        )
    };
    vec![
        (
            "domains/notes/entities/sign_in_note.jadpo",
            r#"use notes/entities { Member }

/// What the application heard of a sign-in.
entity SignInNote {
    id: Uuid
    member_id: Member.id? {
        on_delete: OnDelete.cleared
    }
    method: SignInMethod?
    reason: SignInRefusal?
    first_time: Bool
    until: Instant?

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
"#
            .to_string(),
        ),
        (
            "domains/notes/actions/note_signed_in.jadpo",
            note(
                "note_signed_in",
                "SignedIn",
                "            member_id: event.user_id\n            method: event.method\n            reason: none\n            first_time: event.first_time\n            until: none\n",
            ),
        ),
        (
            "domains/notes/actions/note_refused.jadpo",
            note(
                "note_refused",
                "SignInRefused",
                "            member_id: event.user_id\n            method: event.method\n            reason: event.reason\n            first_time: false\n            until: none\n",
            ),
        ),
        (
            "domains/notes/actions/note_codes_disabled.jadpo",
            note(
                "note_codes_disabled",
                "SignInCodesDisabled",
                "            member_id: event.user_id\n            method: none\n            reason: none\n            first_time: false\n            until: event.until\n",
            ),
        ),
        (
            "domains/notes/subscribers/note_sign_ins.jadpo",
            listen("note_sign_ins", "SignedIn", "note_signed_in"),
        ),
        (
            "domains/notes/subscribers/note_refusals.jadpo",
            listen("note_refusals", "SignInRefused", "note_refused"),
        ),
        (
            "domains/notes/subscribers/note_disabled_codes.jadpo",
            listen(
                "note_disabled_codes",
                "SignInCodesDisabled",
                "note_codes_disabled",
            ),
        ),
    ]
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

    /// What the subscribers heard, oldest first: each event's name, the
    /// member's address, the way in or the reason, and whether it was a
    /// first time; codes stopping gives when they start again.
    fn notes(&self) -> Vec<String> {
        self.rows(
            "SELECT concat_ws(' ', CASE WHEN n.\"until\" IS NOT NULL THEN 'codes_disabled_until ' || to_char(n.\"until\" AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI') WHEN n.\"reason\" IS NOT NULL THEN 'refused ' || n.\"reason\" ELSE 'signed_in ' || n.\"method\" END, coalesce(m.\"email\", '-'), CASE WHEN n.\"first_time\" THEN 'first' END) FROM \"sign_in_note\" AS n LEFT JOIN \"member\" AS m ON m.\"id\" = n.\"member_id\" ORDER BY n.\"ctid\"",
        )
        .into_iter()
        .map(|row| match row.first() {
            Some(jadpo_runtime::SqlValue::Text(text)) => text.clone(),
            other => panic!("not a note: {other:?}"),
        })
        .collect()
    }

    /// Why the request was refused, as its log line says.
    fn reason(response: &Response) -> Option<String> {
        response
            .refused
            .as_ref()
            .map(|refused| match &refused.failure {
                Some(failure) => format!("{} {failure}", refused.reason),
                None => refused.reason.to_string(),
            })
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
    // Each was published, and the log line says why each was refused.
    assert_eq!(Live::reason(&confirmed), None);
    assert_eq!(Live::reason(&again).as_deref(), Some("link_expired"));
    assert_eq!(Live::reason(&by_code).as_deref(), Some("code_invalid"));
    assert_eq!(
        live.notes(),
        [
            "signed_in magic_link ada@example.com first",
            "refused link_expired -",
            "refused code_invalid ada@example.com",
        ]
    );
    // No sign-in event holds an address, a link or a code.
    let kept = live.rows(
        "SELECT \"event\" || ' ' || \"payload\"::text FROM \"jadpo_outbox\" WHERE \"event\" LIKE 'jadpo.Sign%'",
    );
    assert_eq!(kept.len(), 3);
    for row in kept {
        let found = format!("{row:?}");
        assert!(
            !found.contains('@') && !found.contains(&token) && !found.contains(&six),
            "{found}"
        );
    }
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
    assert_eq!(Live::reason(&disabled).as_deref(), Some("codes_disabled"));
    let linked = live.confirm(&mut browser, &format!("{{\"token\":\"{}\"}}", mail.token()));
    assert_eq!(linked.status, 200, "{}", text(&linked));
    // The fifth wrong code stopped codes until midnight UTC, once.
    let notes = live.notes();
    assert_eq!(
        notes[4..],
        [
            "codes_disabled_until 2026-10-21T00:00 -",
            "refused code_invalid -",
            "refused codes_disabled -",
            "signed_in magic_link di@example.com first",
        ],
        "{notes:?}"
    );
    assert_eq!(
        notes
            .iter()
            .filter(|note| note.starts_with("codes_disabled_until"))
            .count(),
        1
    );
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
    let elsewhere = live.confirm(&mut Browser::default(), &link);
    refused(&elsewhere, 401, "sign_in_link_other_browser");
    assert_eq!(
        Live::reason(&elsewhere).as_deref(),
        Some("link_other_browser")
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
    assert_eq!(live.notes(), ["signed_in magic_link fi@example.com first"]);
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
    // The person sees only `sign_in_refused`; the log and the event keep
    // the real reason.
    assert_eq!(
        Live::reason(&refusal).as_deref(),
        Some("refused_by_app SignUpClosed")
    );
    assert!(!text(&refusal).contains("SignUpClosed"));
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
    let disabled = live.confirm(&mut later, &format!("{{\"token\":\"{}\"}}", mail.token()));
    refused(&disabled, 403, "user_disabled");
    assert!(!later.signed_in());
    assert_eq!(Live::reason(&disabled).as_deref(), Some("user_disabled"));
    assert_eq!(
        live.notes(),
        [
            "refused refused_by_app -",
            "signed_in magic_link gu@example.com first",
            "refused user_disabled gu@example.com",
        ]
    );
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

/// The key the application's own sending's call went under.
fn key_of(call: &jadpo_runtime::capability::Call) -> String {
    call.key.as_ref().expect("a keyed call").key.clone()
}

/// Each attempt of the application's own sending makes a link and a code
/// of its own, so it goes under a key of its own: a provider that replays
/// a key's first answer still sends the latest link, rather than the
/// person keeping one the retry ended (chunk 7.9a).
#[test]
fn the_apps_own_sending_sends_each_attempts_link_under_a_key_of_its_own() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("own_sending_keys", Variant::OwnSending);
    // Texter's answers are lost for the whole first attempt: it may have
    // sent the first link.
    for _ in 0..3 {
        live.host.texter.script(
            "messages.send",
            jadpo_runtime::capability::Scripted::Answer(Answer::Unknown),
        );
    }
    let mut browser = Browser::default();
    let asked = live.ask(&mut browser, "ada@example.com");
    assert_eq!(asked.status, 202, "{}", text(&asked));
    let sent = live.host.texter.sent();
    assert_eq!(sent.len(), 3, "{sent:?}");
    let first_key = key_of(&sent[0]);
    assert!(
        sent.iter().all(|call| key_of(call) == first_key),
        "one attempt's link goes under one key"
    );
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
    assert_ne!(handed(&sent[3]).1, handed(&sent[0]).1, "a link of its own");
    assert_ne!(key_of(&sent[3]), first_key, "under a key of its own");
    let token = handed(&sent[3])
        .1
        .rsplit('=')
        .next()
        .unwrap_or_default()
        .to_string();
    let confirmed = live.confirm(&mut browser, &format!("{{\"token\":\"{token}\"}}"));
    assert_eq!(confirmed.status, 200, "{}", text(&confirmed));
}

// --- the application's own routes (chunk 7.3b3) ------------------------------

impl Live {
    /// Asks for a link through the application's own route.
    fn ask_own(&self, browser: &mut Browser, address: &str, return_to: &str) -> Response {
        let response = self.send(&browser.public(
            "POST",
            "/sign-in/ask",
            &format!(
                "{{\"address\":\"{address}\",\"locale\":\"en-GB\",\"return_to\":\"{return_to}\"}}"
            ),
        ));
        browser.keep(&response);
        response
    }

    fn post_own(&self, browser: &mut Browser, path: &str, body: &str) -> Response {
        let response = self.send(&browser.public("POST", path, body));
        browser.keep(&response);
        response
    }
}

#[test]
fn the_apps_own_routes_sign_in_with_a_link_once_in_the_browser_that_asked() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("own_link", Variant::OwnRoutes);
    // No generated routes.
    let generated = live.send(&Browser::default().public(
        "POST",
        "/auth/magic-link",
        "{\"address\":\"ada@example.com\"}",
    ));
    assert_eq!(generated.status, 404, "{}", text(&generated));
    let mut asker = Browser::default();
    let asked = live.ask_own(&mut asker, "ada@example.com", "/notes");
    assert_eq!(asked.status, 204, "{}", text(&asked));
    let cookie = asked.header("set-cookie").expect("the binding cookie");
    assert!(
        cookie.starts_with("notes_session_sign_in=")
            && cookie.contains("HttpOnly")
            && cookie.contains("SameSite=Lax")
            && cookie.contains("Max-Age=900"),
        "{cookie}"
    );
    let mail = live.last_mail("ada@example.com");
    let (token, six) = (mail.token(), mail.code());
    // Neither the outbox nor the table holds the link or the code.
    for row in live.rows("SELECT \"payload\"::text FROM \"jadpo_outbox\"") {
        let found = format!("{row:?}");
        assert!(!found.contains(&token) && !found.contains(&six), "{found}");
    }
    let link = format!("{{\"token\":\"{token}\"}}");
    // Another browser is refused, and the link is left as it was.
    let mut other = Browser::default();
    let elsewhere = live.post_own(&mut other, "/sign-in/link", &link);
    refused(&elsewhere, 401, "sign_in_link_other_browser");
    assert!(!other.signed_in());
    // The browser that asked signs in, as a new member, and the binding
    // cookie is cleared.
    let signed = live.post_own(&mut asker, "/sign-in/link", &link);
    assert_eq!(signed.status, 200, "{}", text(&signed));
    assert_eq!(json(&signed)["return_to"], "/notes");
    assert!(asker.signed_in());
    assert!(!asker.cookies.contains_key("notes_session_sign_in"));
    // Once: the link, and the code it ended.
    let again = live.post_own(&mut Browser::default(), "/sign-in/link", &link);
    refused(&again, 401, "sign_in_link_expired");
    let code = live.post_own(
        &mut Browser::default(),
        "/sign-in/code",
        &format!("{{\"address\":\"ada@example.com\",\"code\":\"{six}\"}}"),
    );
    refused(&code, 401, "sign_in_code_invalid");
    // The failure's text never holds the token or the code.
    assert!(!text(&again).contains(&token) && !text(&code).contains(&six));
    // The events and the log are the generated routes' own.
    assert_eq!(
        Live::reason(&elsewhere).as_deref(),
        Some("link_other_browser")
    );
    assert_eq!(Live::reason(&code).as_deref(), Some("code_invalid"));
    assert_eq!(
        live.notes(),
        [
            "refused link_other_browser -",
            "signed_in magic_link ada@example.com first",
            "refused link_expired -",
            "refused code_invalid ada@example.com",
        ]
    );
}

#[test]
fn the_apps_own_routes_keep_the_limits_and_wrong_codes() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("own_limits", Variant::OwnRoutes);
    let mut browser = Browser::default();
    // A path off the site is refused, and nothing is sent.
    let off = live.ask_own(&mut browser, "bo@example.com", "//evil.example/");
    refused(&off, 422, "sign_in_return_invalid");
    assert!(live.mails().is_empty());
    assert_eq!(
        live.ask_own(&mut browser, "bo@example.com", "/").status,
        204
    );
    // One send per `resend_after:`, alike for an address no one has.
    for address in ["bo@example.com", "nobody@example.com"] {
        if address == "nobody@example.com" {
            assert_eq!(live.ask_own(&mut browser, address, "/").status, 204);
        }
        let again = live.ask_own(&mut browser, address, "/");
        refused(&again, 429, "rate_limited");
        assert_eq!(again.header("retry-after"), Some("60"), "{address}");
    }
    let six = live.last_mail("nobody@example.com").code();
    let six_bo = live
        .mails()
        .into_iter()
        .find(|mail| mail.to == "bo@example.com")
        .expect("bo's email")
        .code();
    // Wrong codes count even though the action fails: `code_attempts: 3`
    // ends the code.
    for _ in 0..3 {
        let wrong = live.post_own(
            &mut browser,
            "/sign-in/code",
            &format!(
                "{{\"address\":\"bo@example.com\",\"code\":\"{}\"}}",
                other_code(&six_bo)
            ),
        );
        refused(&wrong, 401, "sign_in_code_invalid");
    }
    let right = live.post_own(
        &mut browser,
        "/sign-in/code",
        &format!("{{\"address\":\"bo@example.com\",\"code\":\"{six_bo}\"}}"),
    );
    refused(&right, 401, "sign_in_code_invalid");
    assert!(!browser.signed_in());
    // Another address's right code signs in, as a new member.
    let signed = live.post_own(
        &mut browser,
        "/sign-in/code",
        &format!("{{\"address\":\"nobody@example.com\",\"code\":\"{six}\"}}"),
    );
    assert_eq!(signed.status, 200, "{}", text(&signed));
    assert_eq!(json(&signed)["return_to"], "/");
    assert!(browser.signed_in());
}

#[test]
fn the_apps_own_route_trades_a_code_for_an_api_token() {
    if !jadpo_test::live() {
        return;
    }
    let live = Live::new("own_token", Variant::OwnRoutes);
    let mut browser = Browser::default();
    assert_eq!(
        live.ask_own(&mut browser, "cy@example.com", "/").status,
        204
    );
    let six = live.last_mail("cy@example.com").code();
    let redeemed = live.send(&request(
        "POST",
        "/sign-in/token",
        &[],
        &format!("{{\"address\":\"cy@example.com\",\"code\":\"{six}\"}}"),
    ));
    assert_eq!(redeemed.status, 200, "{}", text(&redeemed));
    let token = json(&redeemed)["token"]
        .as_str()
        .expect("a token")
        .to_string();
    let bearer = format!("Bearer {token}");
    let me = live.send(&request(
        "GET",
        "/account/me",
        &[("authorization", bearer.as_str())],
        "",
    ));
    assert_eq!(me.status, 200, "{}", text(&me));
    assert_eq!(json(&me)["email"], "cy@example.com");
    // A refused first sign-in signs no one in.
    assert_eq!(
        live.ask_own(&mut browser, "closed@notes.example", "/")
            .status,
        204
    );
    let six = live.last_mail("closed@notes.example").code();
    let closed = live.post_own(
        &mut browser,
        "/sign-in/code",
        &format!("{{\"address\":\"closed@notes.example\",\"code\":\"{six}\"}}"),
    );
    refused(&closed, 403, "sign_in_refused");
    assert!(!browser.signed_in());
    // The token's action committed, so its `SignedIn` was delivered; the
    // refusal was kept on its own though its action failed.
    assert_eq!(
        Live::reason(&closed).as_deref(),
        Some("refused_by_app SignUpClosed")
    );
    assert_eq!(
        live.notes(),
        [
            "signed_in magic_link cy@example.com first",
            "refused refused_by_app -",
        ]
    );
}

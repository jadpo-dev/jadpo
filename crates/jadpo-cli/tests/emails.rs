//! Sending email (area 16, "Email"; chunk 7.3), against live PostgreSQL:
//! an action emits an email, and once it commits the email's delivery
//! renders it whole (its templates run by the interpreter with its
//! fields, wrapped in the shared layout, its subject worded in its
//! locale, its `File` fields read from their store as attachments) and
//! the SMTP driver hands it to an SMTP server started in the test, over
//! the native host's sockets. A send whose outcome is unknown is parked
//! for an operator, a refused recipient fails for good, a server that
//! can't be reached is retried, and an action that rolls back sends
//! nothing. Nothing reaches a network: the server is on this machine.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_drivers::smtp::{Server, Smtp};
use jadpo_host_native::Runtime;
use jadpo_host_native::smtp::NativeRelay;
use jadpo_interp::Program;
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::files::MemoryFiles;
use jadpo_runtime::runner::{Deferred, run_deferred};
use jadpo_runtime::{Executor, Host, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

use crate::support::fake_provider::FakeProvider;
use crate::support::fake_smtp::{Behaviour, FakeSmtp, base64_text};

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

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {
    display_name: "Acme"
    connections: {
        cards: {
            driver: Driver.disk
        }
        mail: {
            driver: Driver.smtp
            host: config.smtp_host
            credential: {
                username: config.smtp_username
                password: config.smtp_password
            }
        }
    }
    email: {
        connection: mail
        from: config.mail_sender
    }
    events: {
        store: main
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.ar_eg]
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    mail_sender: Email {
        default: "hello@acme.test"
    }
    smtp_host: Text {
        default: "localhost"
    }
    smtp_password: Text {
        secret: true
    }
    smtp_username: Text {
        default: "acme"
    }
}
"#,
    ),
    (
        "domains/members/types/member_name.jadpo",
        r#"type MemberName = Text {
    min_length: 1
    max_length: 80
}
"#,
    ),
    (
        "domains/members/types/membership_plan.jadpo",
        r#"/// How much a member pays for.
type MembershipPlan = Enum {
    basic
    premium
}
"#,
    ),
    (
        "domains/members/entities/member.jadpo",
        r#"use members/types { MemberName }

entity Member {
    id: Uuid
    address: Email
    name: MemberName

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
"#,
    ),
    (
        "domains/members/failures/joining_closed.jadpo",
        r#"/// Joining is closed for now.
type JoiningClosed = Failure {
    kind: FailureKind.conflict
}
"#,
    ),
    (
        "domains/members/emails/welcome_email.jadpo",
        r#"use members/types { MemberName, MembershipPlan }

/// Sent once someone joins.
type WelcomeEmail = EmailMessage {
    name: MemberName
    plan: MembershipPlan
    visits: Int
    perks: List<Text>
}
"#,
    ),
    (
        "domains/members/emails/visit_line.jadpo",
        r#"use members/types { MembershipPlan }

/// The line naming a member's plan and how often they have visited.
type VisitLine = EmailPartial {
    plan: MembershipPlan
    visits: Int
}
"#,
    ),
    (
        "domains/members/emails/visit_line.html",
        "<p>{{ plan }}, visit {{ visits }}</p>",
    ),
    (
        "domains/members/emails/visit_line.txt",
        "{{ plan }}, visit {{ visits }}",
    ),
    (
        "domains/members/emails/welcome_email.html",
        r#"<h1>{{ translations.members.welcome_heading(name: name) }}</h1>
{{ include visit_line(plan: plan, visits: visits) }}
{{ if (visits > 1) }}<p>Back again</p>{{ else }}<p>First visit</p>{{ end }}
<ul>{{ for perk in perks }}<li>{{ perk }}</li>{{ end }}</ul>
"#,
    ),
    (
        "domains/members/emails/welcome_email.txt",
        r#"{{ translations.members.welcome_heading(name: name) }}
{{ include visit_line(plan: plan, visits: visits) }}
"#,
    ),
    (
        "domains/members/emails/card_email.jadpo",
        r#"use members/types { MemberName }

/// Sends a member their card.
type CardEmail = EmailMessage {
    name: MemberName
    card: File {
        max_size: 5mb
    }
}
"#,
    ),
    (
        "domains/members/emails/card_email.txt",
        r#"{{ name }}, your card is attached.
"#,
    ),
    (
        "shared/emails/layout.html",
        r#"<html dir="{{ locale.direction }}"><head><title>{{ subject }}</title></head><body><header>{{ app.display_name }}</header>{{ content }}</body></html>
"#,
    ),
    (
        "shared/emails/layout.txt",
        r#"{{ app.display_name }}

{{ content }}"#,
    ),
    (
        "domains/members/translations/welcome_heading.jadpo",
        r#"/// The heading of the welcome email.
type WelcomeHeading = Message {
    name: Text
}
"#,
    ),
    (
        "domains/members/translations/en_gb.jadpo",
        r#"translations {
    card_email: "Your card, {name}"
    joining_closed: "Joining is closed for now."
    membership_plan: {
        basic: "Basic"
        premium: "Premium <gold>"
    }
    welcome_email: "Welcome to the club, {name}"
    welcome_heading: "Hello, {name}"
}
"#,
    ),
    (
        "domains/members/translations/ar_eg.jadpo",
        r#"translations {
    card_email: "بطاقتك يا {name}"
    joining_closed: "الانضمام مغلق الآن."
    membership_plan: {
        basic: "أساسي"
        premium: "مميز"
    }
    welcome_email: "أهلاً بك في النادي يا {name}"
    welcome_heading: "مرحباً يا {name}"
}
"#,
    ),
    (
        "domains/members/actions/join.jadpo",
        r#"use members/emails { WelcomeEmail }
use members/entities { Member }
use members/types { MemberName, MembershipPlan }

/// Someone joins, and is welcomed once they have.
action join(address: Email, name: MemberName, plan: MembershipPlan, language: Locale) -> Member {
    var member: Member = Member.create {
        set: {
            address: address
            name: name
        }
    }
    emit WelcomeEmail {
        to: address
        locale: language
        name: name
        plan: plan
        visits: 3
        perks: ["Free coffee", "Tom & Jerry's <pass>"]
    }
    return member
}
"#,
    ),
    (
        "domains/members/actions/join.test.jadpo",
        r#"use members/entities { Member }
use members/types { MembershipPlan }

test "joining sends one welcome, its plan's line from a partial" {
    var member: Member = join("ana@example.test", "Ana", MembershipPlan.premium, Locale.en_gb)
    assert member.name == "Ana"
    var sent: List<SentEmail> = email.sent
    assert list.count(sent) == 1
    assert list.any(sent, item -> item.to == "ana@example.test" and item.from == "hello@acme.test" and item.subject == "Welcome to the club, Ana")
    assert list.any(sent, item -> item.text == "Acme\n\nHello, Ana\nPremium <gold>, visit 3\n")
    assert list.all(sent, item -> list.count(item.links) == 0 and list.count(item.attachments) == 0)
}

test "an action that rolls back sends nothing a test can read" {
    var before: List<SentEmail> = email.sent
    assert list.count(before) == 0
}
"#,
    ),
    (
        "domains/members/actions/join_then_close.jadpo",
        r#"use members/emails { WelcomeEmail }
use members/entities { Member }
use members/failures { JoiningClosed }
use members/types { MemberName, MembershipPlan }

/// Starts to let someone join, then finds joining closed: nothing kept,
/// nothing sent.
action join_then_close(address: Email, name: MemberName) fails JoiningClosed -> Unit {
    var member: Member = Member.create {
        set: {
            address: address
            name: name
        }
    }
    emit WelcomeEmail {
        to: address
        locale: Locale.en_gb
        name: member.name
        plan: MembershipPlan.basic
        visits: 1
        perks: []
    }
    reject JoiningClosed {}
}
"#,
    ),
    (
        "domains/members/actions/send_card.jadpo",
        r#"use members/emails { CardEmail }
use members/types { MemberName }

/// Sends a member the card they asked for.
action send_card(address: Email, name: MemberName, card: File) -> Unit {
    emit CardEmail {
        to: address
        locale: Locale.en_gb
        name: name
        card: card
    }
}
"#,
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("emails-7.3-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;

struct Loaded {
    _root: PathBuf,
    program: Program,
    _databases: jadpo_test::database::TestDatabases,
    url: String,
}

fn load(name: &str) -> Option<Loaded> {
    let root = project(name);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    if !jadpo_test::live() {
        return None;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    Some(Loaded {
        _root: root,
        program,
        _databases: databases,
        url,
    })
}

/// The SMTP driver for `mail`, at `port` on this machine, signing in; and
/// the store of cards in memory.
fn faked<'b>(harness: &'b Harness, port: u16, cards: &Rc<MemoryFiles>) -> Faked<&'b Harness> {
    let server = Server::new("127.0.0.1", port).signing_in("acme", "s3cret");
    let mail: Rc<dyn Capability> = Rc::new(Smtp::new(NativeRelay, server));
    Faked::new(harness)
        .with("mail", mail)
        .with("cards", Rc::clone(cards) as Rc<dyn Capability>)
}

/// Runs the action called `action` with `args` on `host`, giving the work
/// it left for after its response.
async fn act(
    harness: &Harness,
    host: &dyn Host,
    program: &Program,
    action: &str,
    args: Vec<Value>,
) -> (Outcome, Vec<Deferred>) {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = harness.ctx_on(program.places(), host);
    let outcome = program.run(&mut ctx, id, args).await;
    (outcome, ctx.take_deferred())
}

fn locale(name: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::LOCALE, name).expect("a locale");
    Value::variant(jadpo_std::LOCALE, ordinal, name, Vec::new())
}

fn plan(name: &str) -> Value {
    let ordinal = u32::from(name == "premium");
    Value::variant("MembershipPlan", ordinal, name, Vec::new())
}

/// Each delivery of an email sent to `address`: its state and its last
/// failure's code.
async fn deliveries(harness: &Harness, address: &str) -> Vec<(String, Option<String>)> {
    let sql = format!(
        "SELECT \"d\".\"status\", \"d\".\"last_failure\" ->> 'code' FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" WHERE \"d\".\"subscriber\" LIKE 'email %' AND \"o\".\"payload\" ->> 'to' = '{address}'"
    );
    harness
        .rows(&sql)
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Text(status), SqlValue::Text(code)] => (status.clone(), Some(code.clone())),
            [SqlValue::Text(status), _] => (status.clone(), None),
            other => panic!("not a delivery: {other:?}"),
        })
        .collect()
}

/// A MIME message's headers, and each part's headers and decoded text,
/// for the one level of nesting the driver writes.
struct Parsed {
    headers: String,
    parts: Vec<(String, String)>,
}

fn parse(data: &str) -> Parsed {
    let (headers, _) = data.split_once("\r\n\r\n").expect("headers");
    let mut parts = Vec::new();
    for piece in data.split("\r\n--jadpo-part-") {
        let Some((head, body)) = piece.split_once("\r\n\r\n") else {
            continue;
        };
        if !head.contains("Content-Transfer-Encoding: base64") {
            continue;
        }
        let encoded: String = body
            .lines()
            .take_while(|line| !line.starts_with("--"))
            .collect::<Vec<_>>()
            .join("");
        let head = head
            .rsplit_once("Content-Type:")
            .map_or(head.to_string(), |(_, rest)| format!("Content-Type:{rest}"));
        parts.push((head, base64_text(&encoded)));
    }
    Parsed {
        headers: headers.to_string(),
        parts,
    }
}

/// An email is rendered whole once its action commits, wrapped in the
/// layout and worded in its locale, and handed over SMTP, signing in, as
/// one message with its text and its HTML; its delivery's record alone
/// says it went.
#[test]
fn an_email_is_rendered_whole_and_sent_over_smtp_after_its_action_commits() {
    let Some(loaded) = load("sent") else {
        return;
    };
    let server = FakeSmtp::start();
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let cards = Rc::new(MemoryFiles::new());
    let host = faked(&harness, server.port, &cards);
    Runtime::new().expect("runtime").block_on(async {
        let (joined, work) = act(
            &harness,
            &host,
            &loaded.program,
            "join",
            vec![
                Value::text("ana@example.test"),
                Value::text("Ana"),
                plan("premium"),
                locale("en_gb"),
            ],
        )
        .await;
        assert!(matches!(joined, Outcome::Value(_)), "{joined:?}");
        let ran = run_deferred(&loaded.program, &host, work).await;
        assert!(matches!(ran.as_slice(), [Outcome::Value(_)]), "{ran:?}");
        assert_eq!(
            deliveries(&harness, "ana@example.test").await,
            vec![("succeeded".to_string(), None)]
        );
        let received = server.received();
        assert_eq!(received.len(), 1);
        let message = &received[0];
        assert_eq!(message.from, "hello@acme.test");
        assert_eq!(message.to, vec!["ana@example.test".to_string()]);
        assert_eq!(
            message.signed_in,
            Some(("acme".to_string(), "s3cret".to_string()))
        );
        let parsed = parse(&message.data);
        assert!(parsed.headers.contains("From: hello@acme.test\r\n"), "{}", parsed.headers);
        assert!(parsed.headers.contains("To: ana@example.test\r\n"));
        assert!(parsed.headers.contains("Subject: Welcome to the club, Ana\r\n"));
        assert!(parsed.headers.starts_with("Date: "));
        assert!(parsed.headers.contains("multipart/alternative"));
        let [(text_head, text), (html_head, html)] = parsed.parts.as_slice() else {
            panic!("a text and an HTML part: {:?}", parsed.parts);
        };
        assert!(text_head.starts_with("Content-Type: text/plain; charset=utf-8"));
        assert_eq!(text, "Acme\n\nHello, Ana\nPremium <gold>, visit 3\n");
        assert!(html_head.starts_with("Content-Type: text/html; charset=utf-8"));
        assert_eq!(
            html,
            "<html dir=\"ltr\"><head><title>Welcome to the club, Ana</title></head><body><header>Acme</header><h1>Hello, Ana</h1>\n<p>Premium &lt;gold&gt;, visit 3</p>\n<p>Back again</p>\n<ul><li>Free coffee</li><li>Tom &amp; Jerry&#39;s &lt;pass&gt;</li></ul>\n</body></html>\n"
        );
    });
}

/// A test reads the emails it sent with `email.sent` (chunk 7.3c), each
/// rendered whole, its partials included, and a test that sends none
/// reads none.
#[test]
fn a_test_reads_the_emails_it_sent() {
    let root = project("sent-in-tests");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    if !jadpo_test::live() {
        return;
    }
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("2 passed"), "{}", tested.stdout);
}

/// An email in a language written right to left is laid out that way, and
/// its subject, outside ASCII, is sent as encoded words.
#[test]
fn an_email_is_written_in_its_own_locale() {
    let Some(loaded) = load("locale") else {
        return;
    };
    let server = FakeSmtp::start();
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let cards = Rc::new(MemoryFiles::new());
    let host = faked(&harness, server.port, &cards);
    Runtime::new().expect("runtime").block_on(async {
        let (_, work) = act(
            &harness,
            &host,
            &loaded.program,
            "join",
            vec![
                Value::text("rana@example.test"),
                Value::text("رنا"),
                plan("basic"),
                locale("ar_eg"),
            ],
        )
        .await;
        run_deferred(&loaded.program, &host, work).await;
        let received = server.received();
        assert_eq!(received.len(), 1);
        let parsed = parse(&received[0].data);
        assert!(
            parsed.headers.contains("Subject: =?utf-8?B?"),
            "{}",
            parsed.headers
        );
        let (_, html) = &parsed.parts[1];
        assert!(
            html.starts_with("<html dir=\"rtl\"><head><title>أهلاً بك في النادي يا رنا</title>"),
            "{html}"
        );
        assert!(html.contains("<h1>مرحباً يا رنا</h1>"));
        assert!(
            html.contains("<p>أساسي, visit ٣</p>") || html.contains("<p>أساسي, visit 3</p>"),
            "{html}"
        );
    });
}

/// A `File` field is an attachment, read from its store as the email is
/// sent and written beside its body.
#[test]
fn an_attachment_is_read_from_its_store_and_sent_beside_the_body() {
    let Some(loaded) = load("attachment") else {
        return;
    };
    let server = FakeSmtp::start();
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let cards = Rc::new(MemoryFiles::new());
    let host = faked(&harness, server.port, &cards);
    let card = cards.keep("cards", b"%PDF-1.7 the card", "pdf");
    Runtime::new().expect("runtime").block_on(async {
        let (sent, work) = act(
            &harness,
            &host,
            &loaded.program,
            "send_card",
            vec![Value::text("ana@example.test"), Value::text("Ana"), card],
        )
        .await;
        assert!(matches!(sent, Outcome::Value(_)), "{sent:?}");
        run_deferred(&loaded.program, &host, work).await;
        let received = server.received();
        assert_eq!(received.len(), 1);
        let parsed = parse(&received[0].data);
        assert!(parsed.headers.contains("Subject: Your card, Ana\r\n"));
        assert!(parsed.headers.contains("multipart/mixed"));
        let [(text_head, text), (card_head, card)] = parsed.parts.as_slice() else {
            panic!("the text and the card: {:?}", parsed.parts);
        };
        assert!(text_head.starts_with("Content-Type: text/plain"));
        assert_eq!(text, "Acme\n\nAna, your card is attached.\n");
        assert!(
            card_head.starts_with("Content-Type: application/pdf"),
            "{card_head}"
        );
        assert!(card_head.contains("Content-Disposition: attachment; filename*=utf-8''"));
        assert_eq!(card, "%PDF-1.7 the card");
    });
}

/// A send whose end went out with no answer may have been taken: parked
/// as `outcome_unknown` for an operator, never sent again by itself. A
/// recipient the server refuses for good fails; a server that can't be
/// reached surely took nothing, so it is retried.
#[test]
fn a_send_whose_outcome_is_unknown_is_parked_and_a_refused_one_fails() {
    let Some(loaded) = load("outcomes") else {
        return;
    };
    let server = FakeSmtp::start();
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let cards = Rc::new(MemoryFiles::new());
    let host = faked(&harness, server.port, &cards);
    // A port nothing listens on.
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("a port")
        .local_addr()
        .expect("address")
        .port();
    let down = faked(&harness, closed, &cards);
    Runtime::new().expect("runtime").block_on(async {
        let join = |address: &str| {
            vec![
                Value::text(address),
                Value::text("Ana"),
                plan("basic"),
                locale("en_gb"),
            ]
        };
        server.next(Behaviour::DropAfterData);
        let (_, work) = act(
            &harness,
            &host,
            &loaded.program,
            "join",
            join("lost@example.test"),
        )
        .await;
        run_deferred(&loaded.program, &host, work).await;
        server.next(Behaviour::RejectRecipient);
        let (_, work) = act(
            &harness,
            &host,
            &loaded.program,
            "join",
            join("nobody@example.test"),
        )
        .await;
        run_deferred(&loaded.program, &host, work).await;
        let (_, work) = act(
            &harness,
            &down,
            &loaded.program,
            "join",
            join("later@example.test"),
        )
        .await;
        run_deferred(&loaded.program, &down, work).await;
        assert_eq!(
            deliveries(&harness, "lost@example.test").await,
            vec![(
                "outcome_unknown".to_string(),
                Some("outcome_unknown".to_string())
            )]
        );
        assert_eq!(
            deliveries(&harness, "nobody@example.test").await,
            vec![("failed".to_string(), Some("recipient_rejected".to_string()))]
        );
        assert_eq!(
            deliveries(&harness, "later@example.test").await,
            vec![("pending".to_string(), Some("unavailable".to_string()))]
        );
        assert!(server.received().is_empty(), "nothing taken");
        assert_eq!(server.conversations(), 2);
    });
}

/// An email is sent only once its action commits: one that rolls back
/// sends nothing.
#[test]
fn an_action_that_rolls_back_sends_nothing() {
    let Some(loaded) = load("rolled-back") else {
        return;
    };
    let server = FakeSmtp::start();
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let cards = Rc::new(MemoryFiles::new());
    let host = faked(&harness, server.port, &cards);
    Runtime::new().expect("runtime").block_on(async {
        let (outcome, work) = act(
            &harness,
            &host,
            &loaded.program,
            "join_then_close",
            vec![Value::text("ana@example.test"), Value::text("Ana")],
        )
        .await;
        assert!(matches!(outcome, Outcome::Failed(_)), "{outcome:?}");
        run_deferred(&loaded.program, &host, work).await;
        assert!(deliveries(&harness, "ana@example.test").await.is_empty());
        assert_eq!(server.conversations(), 0);
    });
}

// --- the swap: the same emails through a mail package (chunk 7.3c) -------

/// The app sending through Resend instead: only `app.jadpo` and
/// `config.jadpo` change.
const RESEND_APP: &str = r#"application {
    display_name: "Acme"
    connections: {
        cards: {
            driver: Driver.disk
        }
        resend: {
            package: Resend.v1
            domains: []
            credential: config.resend_key
            host: config.resend_host
        }
    }
    email: {
        connection: resend
        from: config.mail_sender
    }
    events: {
        store: main
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.ar_eg]
}
"#;

const RESEND_CONFIG: &str = r#"config {
    mail_sender: Email {
        default: "hello@acme.test"
    }
    resend_host: Url {
        default: "https://api.resend.com"
    }
    resend_key: Text {
        secret: true
    }
}
"#;

/// The Resend package as this repository holds it.
fn resend_package() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/resend")
}

/// The project sending through Resend, with the package installed as an
/// app installs it.
fn resend_project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("emails-7.3c-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    write(&root, "app.jadpo", RESEND_APP);
    write(&root, "config.jadpo", RESEND_CONFIG);
    let package = resend_package();
    let package = package.to_str().expect("a path");
    for args in [
        &["package", "add", package][..],
        &["migrate", "new", "initial"],
        &["records", "update"],
        &["check"],
    ] {
        let out = jadpo(&root, args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    root
}

/// The Resend project loaded with Resend at `host`, and a test database.
fn load_resend(name: &str, host: &str) -> Option<Loaded> {
    let root = resend_project(name);
    if !jadpo_test::live() {
        return None;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let prepared = jadpo_interp::Prepared::load(&loaded.db, loaded.project)
        .unwrap_or_else(|problems| panic!("the project checks: {} problems", problems.len()));
    let host = host.to_string();
    let variables = move |name: &str| -> Option<String> {
        match name {
            "RESEND_HOST" => Some(host.clone()),
            "RESEND_KEY" => Some("re_test_key".into()),
            _ => None,
        }
    };
    let values = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .unwrap_or_else(|problems| panic!("the settings read: {problems:?}"));
    let program = Program::new(prepared).with_config(&values);
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    Some(Loaded {
        _root: root,
        program,
        _databases: databases,
        url,
    })
}

/// The package driver for `resend`, over this machine's HTTP, and the
/// store of cards in memory.
fn through_resend<'b>(harness: &'b Harness, cards: &Rc<MemoryFiles>) -> Faked<&'b Harness> {
    let driver: Rc<dyn Capability> = Rc::new(jadpo_drivers::package::JsonHttp::new(
        jadpo_host_native::fetch::NativeFetch,
    ));
    Faked::new(harness)
        .with("resend", driver)
        .with("cards", Rc::clone(cards) as Rc<dyn Capability>)
}

/// The same email, rendered the same way, goes through the mail package
/// `email:` names: its mapping gives Resend's `POST /emails` the message,
/// with the account's key as a bearer credential, and the delivery's
/// record alone says it went.
#[test]
fn the_same_email_goes_through_a_mail_package() {
    let resend = FakeProvider::start();
    let Some(loaded) = load_resend("sent", &resend.url) else {
        return;
    };
    resend.answer(200, r#"{"id":"49a3999c-0ce1-4ea6-ab68-afcd6dc2e794"}"#);
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let cards = Rc::new(MemoryFiles::new());
    let host = through_resend(&harness, &cards);
    Runtime::new().expect("runtime").block_on(async {
        let (joined, work) = act(
            &harness,
            &host,
            &loaded.program,
            "join",
            vec![
                Value::text("ana@example.test"),
                Value::text("Ana"),
                plan("premium"),
                locale("en_gb"),
            ],
        )
        .await;
        assert!(matches!(joined, Outcome::Value(_)), "{joined:?}");
        let ran = run_deferred(&loaded.program, &host, work).await;
        assert!(matches!(ran.as_slice(), [Outcome::Value(_)]), "{ran:?}");
        assert_eq!(
            deliveries(&harness, "ana@example.test").await,
            vec![("succeeded".to_string(), None)]
        );
    });
    let requests = resend.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(
        (request.method.as_str(), request.path.as_str()),
        ("POST", "/emails")
    );
    assert_eq!(request.header("authorization"), Some("Bearer re_test_key"));
    assert_eq!(
        request.json(),
        serde_json::json!({
            "from": "hello@acme.test",
            "to": "ana@example.test",
            "subject": "Welcome to the club, Ana",
            "html": "<html dir=\"ltr\"><head><title>Welcome to the club, Ana</title></head><body><header>Acme</header><h1>Hello, Ana</h1>\n<p>Premium &lt;gold&gt;, visit 3</p>\n<p>Back again</p>\n<ul><li>Free coffee</li><li>Tom &amp; Jerry&#39;s &lt;pass&gt;</li></ul>\n</body></html>\n",
            "text": "Acme\n\nHello, Ana\nPremium <gold>, visit 3\n",
            "attachments": [],
        })
    );
}

/// An attachment goes in base64 with its name and media type, and an email
/// with no `.html` sends none.
#[test]
fn an_attachment_goes_through_a_mail_package_in_base64() {
    let resend = FakeProvider::start();
    let Some(loaded) = load_resend("attachment", &resend.url) else {
        return;
    };
    resend.answer(200, r#"{"id":"re-2"}"#);
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let cards = Rc::new(MemoryFiles::new());
    let host = through_resend(&harness, &cards);
    let card = cards.keep("cards", b"%PDF-1.7 the card", "pdf");
    Runtime::new().expect("runtime").block_on(async {
        let (sent, work) = act(
            &harness,
            &host,
            &loaded.program,
            "send_card",
            vec![Value::text("ana@example.test"), Value::text("Ana"), card],
        )
        .await;
        assert!(matches!(sent, Outcome::Value(_)), "{sent:?}");
        run_deferred(&loaded.program, &host, work).await;
    });
    let requests = resend.requests();
    assert_eq!(requests.len(), 1);
    let body = requests[0].json();
    assert_eq!(body["subject"], "Your card, Ana");
    assert_eq!(body["html"], serde_json::Value::Null);
    assert_eq!(body["text"], "Acme\n\nAna, your card is attached.\n");
    let attachment = &body["attachments"][0];
    assert_eq!(attachment["content"], "JVBERi0xLjcgdGhlIGNhcmQ=");
    assert_eq!(attachment["content_type"], "application/pdf");
    assert!(
        attachment["filename"]
            .as_str()
            .is_some_and(|name| !name.is_empty()),
        "{attachment}"
    );
}

/// The package's failures the mapping names are an email's: a message
/// Resend refuses fails for good as `message_rejected`; a server's error
/// may come after it took the message, so the send is parked.
#[test]
fn a_mail_packages_failures_are_an_emails() {
    let resend = FakeProvider::start();
    let Some(loaded) = load_resend("outcomes", &resend.url) else {
        return;
    };
    resend.answer(
        422,
        r#"{"statusCode":422,"name":"validation_error","message":"Invalid `to` field."}"#,
    );
    resend.answer(
        500,
        r#"{"statusCode":500,"name":"application_error","message":"Something went wrong."}"#,
    );
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let cards = Rc::new(MemoryFiles::new());
    let host = through_resend(&harness, &cards);
    Runtime::new().expect("runtime").block_on(async {
        for address in ["refused@example.test", "lost@example.test"] {
            let (_, work) = act(
                &harness,
                &host,
                &loaded.program,
                "join",
                vec![
                    Value::text(address),
                    Value::text("Ana"),
                    plan("basic"),
                    locale("en_gb"),
                ],
            )
            .await;
            run_deferred(&loaded.program, &host, work).await;
        }
        assert_eq!(
            deliveries(&harness, "refused@example.test").await,
            vec![("failed".to_string(), Some("message_rejected".to_string()))]
        );
        assert_eq!(
            deliveries(&harness, "lost@example.test").await,
            vec![(
                "outcome_unknown".to_string(),
                Some("outcome_unknown".to_string())
            )]
        );
    });
    assert_eq!(resend.requests().len(), 2);
}

/// A test catches mail sent through a mail package as it does any other:
/// the capability's fake stands in for every connection that fills it.
#[test]
fn a_test_catches_mail_sent_through_a_mail_package() {
    let root = resend_project("sent-in-tests");
    if !jadpo_test::live() {
        return;
    }
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("2 passed"), "{}", tested.stdout);
}

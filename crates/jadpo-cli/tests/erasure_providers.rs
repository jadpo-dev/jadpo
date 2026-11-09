//! A person's erasure reaching the companies that hold their data (area 15,
//! "Erasure at providers"; area 11, "Erasing a person at a provider"): each
//! connection's key is copied into `PersonErased` as the person's records
//! go, and after the commit a generated subscriber calls the package's
//! `erasure:` operation, retried until the provider confirms.
//!
//! A club keeps its members' keys at two providers: `cards` takes the key in
//! its path, `mail` in its body under a name of its own, and the key `mail`
//! takes, an email address, is itself personal data.
//!
//! Its tests run interpreted through `jadpo test` here, and compiled with
//! the interpreter's records in the gate's compiled step (`compiled.rs`,
//! "an erasure at providers"). What only rows can show, that nothing is
//! called until the commit, that a key leaves the outbox once it is
//! delivered and that one that gives up is left for an operator, is read
//! here after runs through the interpreter.

use std::cell::{Cell, RefCell};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::capability::{Answer, Call, Capability, Faked};
use jadpo_runtime::files::MemoryFiles;
use jadpo_runtime::runner::sweep;
use jadpo_runtime::{Database, Executor, Host, LocalFuture, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

const PROVIDER: &str = "    protocol: Protocol.json_http
    hosts: {
        live: \"https://api.acme.example\"
        sandbox: \"https://sandbox.acme.example\"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: \"Idempotency-Key\"
        window: 24h
    }
    error_code: \"error.code\"
";

fn package(folder: &str, name: &str) -> (String, String) {
    (
        format!("shared/providers/{folder}/v1/package.jadpo"),
        format!(
            "/// {name}'s API.\npackage {name}.v1 {{\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}}\n"
        ),
    )
}

/// The project: a club, its members' keys, its two providers.
pub(crate) fn files() -> Vec<(String, String)> {
    let file = |path: &str, text: &str| (path.to_string(), text.to_string());
    let open = "    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }
";
    vec![
        file(
            "app.jadpo",
            "use club/entities { Member }

application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        scans: {
            driver: Driver.disk
            path: \"/var/lib/club/scans\"
        }
        cards: {
            package: AcmeCards.v1
            domains: [club]
            credential: config.cards_api_key
            environment: config.provider_environment
            erasure: {
                key: Member.cards_id
            }
        }
        mail: {
            package: AcmeMail.v1
            domains: [club]
            credential: config.mail_api_key
            environment: config.provider_environment
            erasure: {
                key: Member.mail_address
            }
        }
    }
    privacy: {
        erasure_log: {
            store: scans
        }
    }
}
",
        ),
        file(
            "config.jadpo",
            "config {
    cards_api_key: Text {
        secret: true
    }
    mail_api_key: Text {
        secret: true
    }
    provider_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
",
        ),
        package("acme_cards", "AcmeCards"),
        file(
            "shared/providers/acme_cards/v1/provider.jadpo",
            &format!(
                "/// Acme Cards.\nprovider {{\n{PROVIDER}    erasure: {{\n        call: customers.delete\n        by: CustomerId\n    }}\n}}\n"
            ),
        ),
        file(
            "shared/providers/acme_cards/v1/types/customer_id.jadpo",
            "/// Acme Cards' id for a customer.\npublic type CustomerId = Text {}\n",
        ),
        file(
            "shared/providers/acme_cards/v1/types/customer_key.jadpo",
            "/// A customer to delete.\npublic type CustomerKey = Object {\n    /// Their id.\n    id: CustomerId\n}\n",
        ),
        file(
            "shared/providers/acme_cards/v1/types/removed.jadpo",
            "/// A customer deleted.\npublic type Removed = Object {\n    deleted: Bool\n}\n",
        ),
        file(
            "shared/providers/acme_cards/v1/failures/customer_gone.jadpo",
            "/// No customer has that id.\npublic type CustomerGone = Failure {\n    kind: FailureKind.not_found\n    message: \"No such customer.\"\n}\n",
        ),
        file(
            "shared/providers/acme_cards/v1/failures/card_on_hold.jadpo",
            "/// The customer can't go yet.\npublic type CardOnHold = Failure {\n    kind: FailureKind.conflict\n    message: \"The card is on hold.\"\n}\n",
        ),
        file(
            "shared/providers/acme_cards/v1/resources/customers.jadpo",
            "/// Customers.\npublic resource customers {\n    /// Deletes a customer.\n    delete: {\n        method: HttpMethod.delete\n        path: \"/v1/customers/{id}\"\n        input: CustomerKey\n        output: Removed\n        changes: true\n        idempotent: true\n        failures: [CustomerGone, CardOnHold]\n        source: PackageSource.written\n    }\n}\n",
        ),
        package("acme_mail", "AcmeMail"),
        file(
            "shared/providers/acme_mail/v1/provider.jadpo",
            &format!(
                "/// Acme Mail.\nprovider {{\n{PROVIDER}    erasure: {{\n        call: contacts.forget\n        by: ContactAddress\n    }}\n}}\n"
            ),
        ),
        file(
            "shared/providers/acme_mail/v1/types/contact_address.jadpo",
            "/// The address a contact is kept under.\npublic type ContactAddress = Email {}\n",
        ),
        file(
            "shared/providers/acme_mail/v1/types/forget_contact.jadpo",
            "/// A contact to forget.\npublic type ForgetContact = Object {\n    /// Who.\n    address: ContactAddress {\n        wire_name: \"emailAddress\"\n    }\n    /// Why, if the app says.\n    note: Text?\n}\n",
        ),
        file(
            "shared/providers/acme_mail/v1/types/forgotten.jadpo",
            "/// A contact forgotten.\npublic type Forgotten = Object {\n    forgotten: Bool\n}\n",
        ),
        file(
            "shared/providers/acme_mail/v1/resources/contacts.jadpo",
            "/// Contacts.\npublic resource contacts {\n    /// Forgets a contact.\n    forget: {\n        method: HttpMethod.post\n        path: \"/v1/contacts/forget\"\n        input: ForgetContact\n        output: Forgotten\n        changes: true\n        idempotent: true\n        source: PackageSource.written\n    }\n}\n",
        ),
        file(
            "domains/club/types/handle.jadpo",
            "/// What someone goes by.\ntype Handle = Text {\n    max_length: 80\n}\n",
        ),
        file(
            "domains/club/types/done.jadpo",
            "/// That it is done.\ntype Done = Output {\n    okay: Bool\n}\n",
        ),
        file(
            "domains/club/entities/member.jadpo",
            &format!(
                "use club/types {{ Handle }}\nuse shared/providers/acme_cards\nuse shared/providers/acme_mail\n\n/// Someone in the club.\nentity Member {{\n    id: Uuid\n    handle: Handle {{\n        personal: PersonalData.identifying\n    }}\n    cards_id: AcmeCards.CustomerId?\n    mail_address: AcmeMail.ContactAddress?\n\n{open}\n    person: true\n}}\n"
            ),
        ),
        file(
            "domains/club/entities/guest.jadpo",
            &format!(
                "use club/types {{ Handle }}\n\n/// Someone visiting, whom neither provider knows.\nentity Guest {{\n    id: Uuid\n    handle: Handle {{\n        personal: PersonalData.identifying\n    }}\n\n{open}\n    person: true\n}}\n"
            ),
        ),
        file(
            "domains/club/actions/join.jadpo",
            "use club/entities { Member }\nuse club/types { Handle }\nuse shared/providers/acme_cards\nuse shared/providers/acme_mail\n\n/// Adds someone.\naction join(handle: Handle, cards_id: AcmeCards.CustomerId?, mail_address: AcmeMail.ContactAddress?) -> Member {\n    return Member.create {\n        set: {\n            handle: handle\n            cards_id: cards_id\n            mail_address: mail_address\n        }\n    }\n}\n",
        ),
        file(
            "domains/club/actions/visit.jadpo",
            "use club/entities { Guest }\nuse club/types { Handle }\n\n/// Lets someone in.\naction visit(handle: Handle) -> Guest {\n    return Guest.create {\n        set: {\n            handle: handle\n        }\n    }\n}\n",
        ),
        file(
            "domains/club/actions/close.jadpo",
            "use club/entities { Member, MemberNotFound }\nuse club/types { Done }\n\n/// Erases someone.\naction close(member_id: Member.id) fails MemberNotFound -> Done {\n    attempt Member.erase(member_id)\n    return Done {\n        okay: true\n    }\n}\n",
        ),
        file(
            "domains/club/actions/see_out.jadpo",
            "use club/entities { Guest, GuestNotFound }\nuse club/types { Done }\n\n/// Erases a guest.\naction see_out(guest_id: Guest.id) fails GuestNotFound -> Done {\n    attempt Guest.erase(guest_id)\n    return Done {\n        okay: true\n    }\n}\n",
        ),
        file("domains/club/actions/close.test.jadpo", CLOSE_TESTS),
    ]
}

const CLOSE_TESTS: &str = r#"use app { cards, mail }
use club/actions { join, see_out, visit }
use club/entities { Guest, GuestNotFound, Member, MemberNotFound }
use shared/providers/acme_cards
use shared/providers/acme_mail

test "erasing a person asks each provider to erase them, once" {
    fakes: {
        cards.customers.delete: [AcmeCards.Removed {
            deleted: true
        }]
        mail.contacts.forget: [AcmeMail.Forgotten {
            forgotten: true
        }]
    }
    var ada: Member = join("ada", "cus_ada", "ada@example.test")
    attempt close(ada.id)
    assert Member.get(ada.id) fails MemberNotFound
    assert list.map(cards.customers.delete.requests, sent -> sent.id) == ["cus_ada"]
    assert list.map(mail.contacts.forget.requests, sent -> sent.address) == ["ada@example.test"]
    assert list.map(mail.contacts.forget.requests, sent -> sent.note) == [none]
}

test "a person with no key at a provider isn't asked about there" {
    var ada: Member = join("ada", none, "ada@example.test")
    attempt close(ada.id)
    assert list.count(cards.customers.delete.requests) == 0
    assert list.map(mail.contacts.forget.requests, sent -> sent.address) == ["ada@example.test"]
}

test "erasing a person of another entity asks no provider" {
    var grace: Guest = visit("grace")
    attempt see_out(grace.id)
    assert Guest.get(grace.id) fails GuestNotFound
    assert list.count(cards.customers.delete.requests) == 0
    assert list.count(mail.contacts.forget.requests) == 0
}

test "a provider that no longer knows the person has confirmed" {
    fakes: {
        cards.customers.delete: [customer_gone]
    }
    var ada: Member = join("ada", "cus_ada", none)
    attempt close(ada.id)
    assert list.count(cards.customers.delete.requests) == 1
}

test "a provider that can't be reached leaves the person erased and is tried again" {
    fakes: {
        cards.customers.delete: [unavailable, unavailable, unavailable, AcmeCards.Removed {
            deleted: true
        }]
    }
    var ada: Member = join("ada", "cus_ada", none)
    attempt close(ada.id)
    assert Member.get(ada.id) fails MemberNotFound
    assert list.count(cards.customers.delete.requests) == 3
    clock.advance(1h)
    var bob: Member = join("bob", none, none)
    var kept: Member = attempt Member.get(bob.id)
    assert kept.id == bob.id
    assert list.map(cards.customers.delete.requests, sent -> sent.id) == ["cus_ada", "cus_ada", "cus_ada", "cus_ada"]
}
"#;

/// The club written out, migrated and its records updated.
fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("erasure-providers-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files() {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let out = jadpo(&root, args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    root
}

#[test]
fn erasing_a_person_reaches_each_provider_in_a_test() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("tests");
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert_eq!(
        crate::support::without_run_time(&tested.stdout),
        "tested: 5 passed, 5 action tests\n",
        "{}",
        tested.stderr
    );
}

/// The report says where a key that is personal data waits, and takes back
/// that Jadpo doesn't call the provider.
#[test]
fn the_report_says_a_personal_key_waits_in_the_outbox() {
    let root = project("report");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let report =
        fs::read_to_string(root.join("build/report/personal-data.md")).expect("the report");
    assert!(
        !report.contains("Jadpo doesn't call a provider's `erasure:`"),
        "{report}"
    );
    assert!(
        report.contains("| `mail` | `Member.mail_address` |"),
        "{report}"
    );
    assert!(!report.contains("| `cards` |"), "{report}");
}

const T0: i64 = 1_792_400_400_000;
const HOUR: i64 = 3_600_000;
const DAY: i64 = 24 * HOUR;

/// What a provider gives every call, and how many members `main` held as
/// each came.
struct Fake {
    database: jadpo_host_native::Postgres,
    answer: RefCell<Answer>,
    calls: RefCell<Vec<(String, Value, i64)>>,
    made: Rc<Cell<u32>>,
}

impl Capability for Fake {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        Box::pin(async move {
            let rows = self
                .database
                .query_once("SELECT count(*)::int FROM \"member\"", &[])
                .await
                .expect("the members");
            let members = match rows.first().and_then(|row| row.first()) {
                Some(SqlValue::Int4(count)) => i64::from(*count),
                other => panic!("not a count: {other:?}"),
            };
            self.calls
                .borrow_mut()
                .push((call.name(), call.input.clone(), members));
            self.made.set(self.made.get() + 1);
            self.answer.borrow().clone()
        })
    }
}

struct Club {
    program: Program,
    harness: Harness<jadpo_host_native::Postgres>,
    url: String,
    _databases: jadpo_test::database::TestDatabases,
}

fn in_club(
    name: &str,
    run: impl AsyncFnOnce(&Program, &Harness<jadpo_host_native::Postgres>, &str),
) {
    let root = project(name);
    if !jadpo_test::live() {
        return;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let prepared = Prepared::load(&loaded.db, loaded.project).expect("the project checks");
    let variables = |name: &str| -> Option<String> {
        match name {
            "CARDS_API_KEY" | "MAIL_API_KEY" => Some("key".into()),
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
    let club = Club {
        program,
        harness: Harness::postgres(&url).at(T0).random_from(0),
        url,
        _databases: databases,
    };
    Runtime::new()
        .expect("runtime")
        .block_on(run(&club.program, &club.harness, &club.url));
}

fn id_of(value: &Value) -> Value {
    match value {
        Value::Record(record) => record.get("id").cloned().expect("an id"),
        other => panic!("not a record: {other:?}"),
    }
}

async fn call(
    harness: &Harness<jadpo_host_native::Postgres>,
    program: &Program,
    host: &dyn Host,
    action: &str,
    args: Vec<Value>,
) -> Value {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = harness.ctx_on(program, host);
    match program.run(&mut ctx, id, args).await {
        Outcome::Value(value) => value,
        other => panic!("{action}: expected a value, got {other:?}"),
    }
}

async fn texts(harness: &Harness<jadpo_host_native::Postgres>, sql: &str) -> Vec<String> {
    harness
        .rows(sql)
        .await
        .iter()
        .map(|row| match row.first() {
            Some(SqlValue::Text(text)) => text.clone(),
            Some(SqlValue::Null(_)) => "none".to_string(),
            other => format!("{other:?}"),
        })
        .collect()
}

const PAYLOAD: &str =
    "SELECT \"payload\"::text FROM \"jadpo_outbox\" WHERE \"event\" = 'jadpo.PersonErased'";
const STATES: &str = "SELECT \"subscriber\" || ' ' || \"status\" FROM \"jadpo_deliveries\" WHERE \"subscriber\" LIKE 'erasure at%' ORDER BY 1";

fn fake(url: &str, answer: Answer, made: &Rc<Cell<u32>>) -> Rc<Fake> {
    Rc::new(Fake {
        database: jadpo_host_native::postgres(url).expect("a store"),
        answer: RefCell::new(answer),
        calls: RefCell::default(),
        made: made.clone(),
    })
}

/// Nothing is called until the erasure has committed; then each provider
/// is called once, with its key, with the person already gone, and each key
/// leaves the outbox as it is delivered.
#[test]
fn each_provider_is_called_once_after_the_commit() {
    in_club("order", async |program, harness, url| {
        let made = Rc::new(Cell::new(0));
        let cards = fake(url, Answer::Wire("{\"deleted\":true}".into()), &made);
        let mail = fake(url, Answer::Wire("{\"forgotten\":true}".into()), &made);
        let files: Rc<dyn Capability> = Rc::new(MemoryFiles::new());
        let as_cards: Rc<dyn Capability> = cards.clone();
        let as_mail: Rc<dyn Capability> = mail.clone();
        let host = Faked::new(harness)
            .with("cards", as_cards)
            .with("mail", as_mail)
            .with("scans", files);
        let ada = call(
            harness,
            program,
            &host,
            "join",
            vec![
                Value::text("ada"),
                Value::text("cus_ada"),
                Value::text("ada@example.test"),
            ],
        )
        .await;
        call(
            harness,
            program,
            &host,
            "join",
            vec![Value::text("bob"), Value::None, Value::None],
        )
        .await;
        call(harness, program, &host, "close", vec![id_of(&ada)]).await;

        // Committed, with the keys in the event and no provider asked yet.
        assert_eq!(made.get(), 0, "nothing goes out before the commit");
        assert_eq!(
            harness.count("SELECT count(*) FROM \"member\"").await,
            1,
            "the erasure has committed"
        );
        let payloads = texts(harness, PAYLOAD).await;
        let read: serde_json::Value =
            serde_json::from_str(payloads.first().expect("one event")).expect("JSON");
        assert_eq!(read["keys"]["cards"], "cus_ada");
        assert_eq!(read["keys"]["mail"], "ada@example.test");
        assert_eq!(
            texts(harness, STATES).await,
            ["erasure at cards pending", "erasure at mail pending"]
        );

        sweep(program, &host, "main", harness.now())
            .await
            .expect("a sweep");

        assert_eq!(cards.calls.borrow().len(), 1, "{:?}", cards.calls.borrow());
        assert_eq!(mail.calls.borrow().len(), 1, "{:?}", mail.calls.borrow());
        let (name, input, members) = cards.calls.borrow()[0].clone();
        assert_eq!(name, "cards.customers.delete");
        assert_eq!(members, 1, "the person is gone as the provider is called");
        match &input {
            Value::Record(record) => assert_eq!(record.get("id"), Some(&Value::text("cus_ada"))),
            other => panic!("not its input: {other:?}"),
        }
        assert_eq!(mail.calls.borrow()[0].2, 1);
        assert_eq!(
            texts(harness, STATES).await,
            ["erasure at cards succeeded", "erasure at mail succeeded"]
        );
        // The keys are gone from the outbox: only what the event always
        // held is left.
        let payloads = texts(harness, PAYLOAD).await;
        let after: serde_json::Value =
            serde_json::from_str(payloads.first().expect("one event")).expect("JSON");
        assert_eq!(after["keys"], serde_json::json!({}), "{after}");

        // A sweep later nothing goes out again.
        sweep(program, &host, "main", harness.now())
            .await
            .expect("a sweep");
        assert_eq!(made.get(), 2);
    });
}

/// A provider that keeps refusing leaves the delivery pending, then, out of
/// time, failed for an operator, with its key still there to send; and one
/// operator's retry finishes it and takes the key.
#[test]
fn a_delivery_that_gives_up_is_parked_and_keeps_its_key() {
    in_club("parked", async |program, harness, url| {
        let made = Rc::new(Cell::new(0));
        let cards = fake(url, Answer::Unavailable, &made);
        let mail = fake(url, Answer::Wire("{\"forgotten\":true}".into()), &made);
        let files: Rc<dyn Capability> = Rc::new(MemoryFiles::new());
        let as_cards: Rc<dyn Capability> = cards.clone();
        let as_mail: Rc<dyn Capability> = mail.clone();
        let host = Faked::new(harness)
            .with("cards", as_cards)
            .with("mail", as_mail)
            .with("scans", files);
        let ada = call(
            harness,
            program,
            &host,
            "join",
            vec![
                Value::text("ada"),
                Value::text("cus_ada"),
                Value::text("ada@example.test"),
            ],
        )
        .await;
        call(harness, program, &host, "close", vec![id_of(&ada)]).await;

        sweep(program, &host, "main", harness.now())
            .await
            .expect("a sweep");
        assert_eq!(
            texts(harness, STATES).await,
            ["erasure at cards pending", "erasure at mail succeeded"],
            "a provider that is down leaves its delivery to try again"
        );
        let held = texts(harness, PAYLOAD).await;
        let read: serde_json::Value = serde_json::from_str(&held[0]).expect("JSON");
        assert_eq!(read["keys"]["cards"], "cus_ada");
        assert_eq!(
            read["keys"].get("mail"),
            None,
            "the other was delivered: {read}"
        );

        harness.advance(45 * DAY);
        sweep(program, &host, "main", harness.now())
            .await
            .expect("a sweep");
        assert_eq!(
            texts(harness, STATES).await,
            ["erasure at cards failed", "erasure at mail succeeded"],
            "past the month it is left for an operator, not dropped"
        );
        let parked = texts(harness, PAYLOAD).await;
        let read: serde_json::Value = serde_json::from_str(&parked[0]).expect("JSON");
        assert_eq!(
            read["keys"]["cards"], "cus_ada",
            "kept for an operator's retry"
        );
    });
}

//! Events published outward, against live PostgreSQL and a receiver on
//! this machine (area 12, "Where events travel"; area 16, "Outgoing
//! webhooks"): a dispatch desk publishes `ParcelPacked` through a
//! connection with `driver: Driver.webhook`. Once the action that emits it
//! commits, its delivery posts the event to the connection's address,
//! signed as Standard Webhooks signs with the connection's secret, so the
//! runtime's own check of a webhook it receives takes it, with the trace
//! context of the work that delivered it. A receiver that answers `503` is
//! sent the same event again, under the same id, once the delivery's
//! backoff has passed; one that answers `204` ends it.
//!
//! The same project runs compiled: its `jadpo test` both ways in the
//! gate's compiled step (`compiled.rs`, "an event published outward"),
//! where a test's host catches each event, and built as a native
//! deployable here, which posts to the receiver itself, signed, and again
//! after a `503`.
//!
//! Nothing reaches a network: the receiver is a server on this machine
//! (`support::fake_hooks`), and every secret is made as the test runs. The
//! database is the one `JADPO_TEST_DATABASE_URL` names or one started from
//! the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_drivers::package::Fetch;
use jadpo_drivers::webhook::Webhook;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::outbox::Trace;
use jadpo_runtime::runner::{run_deferred, sweep};
use jadpo_runtime::{Executor, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

use crate::support::fake_hooks::{FakeHooks, Received};

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
/// An hour, past the first retry's wait.
const HOUR: i64 = 3_600_000;

/// The trace the packing runs in.
const TRACE: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

/// The dispatch desk.
pub(crate) fn files() -> Vec<(String, String)> {
    let file = |path: &str, text: &str| (path.to_string(), text.to_string());
    vec![
        file(
            "app.jadpo",
            "use dispatch/events { ParcelPacked }

application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        partner_hooks: {
            driver: Driver.webhook
            url: config.partner_hook_url
            secret: config.partner_hook_secret
        }
    }
    publishing: {
        partner_hooks: {
            events: [ParcelPacked]
        }
    }
}
",
        ),
        file(
            "config.jadpo",
            "config {
    /// The dispatch desk's database.
    database_url: PostgresUrl
    /// Where each packed parcel is announced.
    partner_hook_url: Url
    /// What each announcement is signed with.
    partner_hook_secret: Text {
        secret: true
    }
}
",
        ),
        file(
            "domains/dispatch/types/label.jadpo",
            "/// What a parcel's label says.
public type Label = Text {
    min_length: 1
    max_length: 40
}
",
        ),
        file(
            "domains/dispatch/types/pack_request.jadpo",
            "use dispatch/types { Label }

/// A parcel to pack.
type PackRequest = Input {
    label: Label
}
",
        ),
        file(
            "domains/dispatch/types/parcel_view.jadpo",
            "use dispatch/entities { Parcel }

/// A packed parcel.
type ParcelView = Output {
    pick: [Parcel.id, Parcel.label]
}
",
        ),
        file(
            "domains/dispatch/entities/parcel.jadpo",
            "use dispatch/types { Label }

/// A parcel the desk has packed.
entity Parcel {
    id: Uuid
    label: Label

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
",
        ),
        file(
            "domains/dispatch/events/parcel_packed.jadpo",
            "use dispatch/types { Label }

/// A parcel was packed.
public type ParcelPacked = Event {
    label: Label
    packed_at: Instant
}
",
        ),
        file(
            "domains/dispatch/actions/pack_parcel.jadpo",
            "use dispatch/entities { Parcel }
use dispatch/events { ParcelPacked }
use dispatch/types { PackRequest, ParcelView }

/// Packs a parcel, and announces it.
action pack_parcel(request: PackRequest) -> ParcelView {
    var parcel: Parcel = Parcel.create {
        set: {
            label: request.label
        }
    }
    emit ParcelPacked {
        label: parcel.label
        packed_at: clock.now
    }
    return ParcelView {
        id: parcel.id
        label: parcel.label
    }
}
",
        ),
        file(
            "domains/dispatch/actions/pack_parcel.test.jadpo",
            "use dispatch/types { PackRequest, ParcelView }

test \"packing a parcel announces it\" {
    var packed: ParcelView = pack_parcel(PackRequest {
        label: \"Blue box\"
    })
    assert packed.label == \"Blue box\"
}
",
        ),
        file(
            "domains/dispatch/routes/parcels.jadpo",
            "use dispatch/types { PackRequest, ParcelView }
use dispatch/actions { pack_parcel }

/// The desk's parcels.
routes Parcels {
    prefix: /parcels

    /// Packs a parcel.
    POST / {
        authentication: RouteAuthentication.none
        input: PackRequest
        output: ParcelView
        status: SuccessStatus.created
        run: pack_parcel(input)
    }
}
",
        ),
    ]
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

/// The desk, written into a folder of its own, migrated and recorded.
fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("publishing-{name}"));
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

/// A Standard Webhooks secret made for this run, never one written in
/// source.
fn made_secret(from: u8) -> String {
    let bytes: Vec<u8> = (0u8..32)
        .map(|at| at.wrapping_mul(53).wrapping_add(from) ^ 0x5a)
        .collect();
    format!("whsec_{}", jadpo_value::encoding::to_base64(&bytes))
}

struct Loaded {
    _root: PathBuf,
    program: Program,
    _databases: jadpo_test::database::TestDatabases,
    url: String,
}

/// The desk's program, its settings naming `hooks` and `secret`.
fn load(name: &str, hooks: &FakeHooks, secret: &str) -> Option<Loaded> {
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
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:?}",
            problems.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let address = format!("{}/hooks/dispatch", hooks.url);
    let (database, secret) = (crate::deployable::as_url(&url), secret.to_string());
    let variables = move |name: &str| -> Option<String> {
        match name {
            "DATABASE_URL" => Some(database.clone()),
            "PARTNER_HOOK_URL" => Some(address.clone()),
            "PARTNER_HOOK_SECRET" => Some(secret.clone()),
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
    Some(Loaded {
        _root: root,
        program,
        _databases: databases,
        url,
    })
}

/// The publish delivery's state, attempts and last failure's code.
async fn deliveries(
    harness: &Harness<jadpo_host_native::Postgres>,
) -> Vec<(String, i32, Option<String>)> {
    harness
        .rows(
            "SELECT \"status\", \"attempts\", \"last_failure\" ->> 'code' FROM \"jadpo_deliveries\" WHERE \"subscriber\" LIKE 'publish %'",
        )
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Text(status), SqlValue::Int4(attempts), code] => (
                status.clone(),
                *attempts,
                match code {
                    SqlValue::Text(code) => Some(code.clone()),
                    _ => None,
                },
            ),
            other => panic!("not a delivery: {other:?}"),
        })
        .collect()
}

/// Checks `received` is the desk's `ParcelPacked` for `label`, signed
/// with `secret` at a time within the check's tolerance of `now`, and
/// gives its id and body.
fn announced(
    received: &Received,
    secret: &str,
    label: &str,
    now: i64,
) -> (String, serde_json::Value) {
    assert_eq!(received.method, "POST");
    assert_eq!(received.path, "/hooks/dispatch");
    assert_eq!(received.header("content-type"), Some("application/json"));
    let signed = jadpo_runtime::webhooks::check(
        &jadpo_std::provider::SignatureScheme::StandardWebhooks,
        &[secret],
        &received.as_request(),
        now,
    );
    assert!(matches!(signed, Ok(Some(_))), "signed: {signed:?}");
    let body: serde_json::Value = serde_json::from_slice(&received.body).expect("JSON");
    let id = received.header("webhook-id").expect("an id").to_string();
    assert_eq!(body["id"], serde_json::json!(id), "{body}");
    assert_eq!(body["type"], "dispatch.ParcelPacked", "{body}");
    assert_eq!(body["data"]["label"], label, "{body}");
    assert!(body["data"]["packed_at"].is_string(), "{body}");
    (id, body)
}

/// A packed parcel is posted to the receiver once its action commits,
/// signed, in the packing's trace; a receiver that answers `503` is sent
/// it again after the backoff, under the same id and body, and the
/// delivery succeeds on the second attempt and is never sent again.
#[test]
fn a_published_event_reaches_its_receiver_signed_and_is_retried() {
    let hooks = FakeHooks::start(&[503]);
    let secret = made_secret(17);
    let Some(loaded) = load("interpreted", &hooks, &secret) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    // The driver signs at its own clock, which the test moves with the
    // harness's.
    let clock = Rc::new(Cell::new(T0));
    let signing = Rc::clone(&clock);
    let driver: Rc<dyn Capability> = Rc::new(
        Webhook::new(
            Rc::new(NativeFetch::new()) as Rc<dyn Fetch>,
            Rc::new(move || signing.get()),
            &format!("{}/hooks/dispatch", hooks.url),
            &secret,
        )
        .expect("a webhook"),
    );
    let host = Faked::new(&harness).with("partner_hooks", driver);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let program = &loaded.program;
        let id = program.plan().find("pack_parcel").expect("the action");
        let mut ctx = harness.ctx_on(program, &host);
        ctx.set_trace(Trace::parse(TRACE));
        let request = Value::record(
            "PackRequest",
            vec![(Rc::from("label"), Value::text("Blue box"))],
        );
        match program.run(&mut ctx, id, vec![request]).await {
            Outcome::Value(_) => {}
            other => panic!("the parcel is packed: {other:?}"),
        }
        let work = ctx.take_deferred();
        drop(ctx);
        // Nothing is posted until the commit, and then at once.
        let ran = run_deferred(program, &host, work).await;
        assert_eq!(ran.len(), 1, "{ran:?}");
        let first = hooks.received();
        assert_eq!(first.len(), 1, "{first:?}");
        let (first_id, first_body) = announced(&first[0], &secret, "Blue box", T0);
        let outbox = harness
            .rows("SELECT \"id\"::text FROM \"jadpo_outbox\"")
            .await;
        assert_eq!(
            outbox,
            vec![vec![SqlValue::Text(first_id.clone())]],
            "its id is the event's"
        );
        let traceparent = first[0].header("traceparent").expect("a trace");
        let carried = Trace::parse(traceparent).expect("a trace context");
        let packing = Trace::parse(TRACE).expect("the packing's");
        assert_eq!(carried.trace_id, packing.trace_id, "{traceparent}");
        assert_eq!(
            deliveries(&harness).await,
            vec![("pending".to_string(), 1, Some("unavailable".to_string()))]
        );

        // An hour on, the sweep sends it again, the same event under the
        // same id, signed at the new time; `204` ends it.
        harness.set_now(T0 + HOUR);
        clock.set(T0 + HOUR);
        sweep(program, &host, "main", T0 + HOUR)
            .await
            .expect("the sweep runs");
        let both = hooks.received();
        assert_eq!(both.len(), 2, "{both:?}");
        let (again_id, again_body) = announced(&both[1], &secret, "Blue box", T0 + HOUR);
        assert_eq!(again_id, first_id);
        assert_eq!(again_body, first_body);
        assert_ne!(
            both[0].header("webhook-timestamp"),
            both[1].header("webhook-timestamp"),
            "each attempt is signed as it is sent"
        );
        // Signed an hour ago, the first is stale now.
        assert!(
            jadpo_runtime::webhooks::check(
                &jadpo_std::provider::SignatureScheme::StandardWebhooks,
                &[secret.as_str()],
                &both[0].as_request(),
                T0 + HOUR,
            )
            .is_err()
        );
        // Another secret doesn't check it.
        assert!(
            jadpo_runtime::webhooks::check(
                &jadpo_std::provider::SignatureScheme::StandardWebhooks,
                &[made_secret(18).as_str()],
                &both[1].as_request(),
                T0 + HOUR,
            )
            .is_err()
        );
        assert_eq!(
            deliveries(&harness).await,
            vec![("succeeded".to_string(), 2, None)]
        );
        harness.set_now(T0 + 2 * HOUR);
        clock.set(T0 + 2 * HOUR);
        sweep(program, &host, "main", T0 + 2 * HOUR)
            .await
            .expect("the sweep runs");
        assert_eq!(
            hooks.received().len(),
            2,
            "sent once it succeeded, never again"
        );
    });
}

/// A receiver that refuses the event, `422`, fails it for good, as the
/// same event would be refused again; one whose address or secret is
/// wrong, `401`, fails it as `misconfigured`. Each is left for an
/// operator, never sent again by itself.
#[test]
fn a_refused_event_fails_and_is_left_for_an_operator() {
    let hooks = FakeHooks::start(&[422, 401]);
    let secret = made_secret(29);
    let Some(loaded) = load("refused", &hooks, &secret) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let driver: Rc<dyn Capability> = Rc::new(
        Webhook::new(
            Rc::new(NativeFetch::new()) as Rc<dyn Fetch>,
            Rc::new(|| T0),
            &format!("{}/hooks/dispatch", hooks.url),
            &secret,
        )
        .expect("a webhook"),
    );
    let host = Faked::new(&harness).with("partner_hooks", driver);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let program = &loaded.program;
        let id = program.plan().find("pack_parcel").expect("the action");
        for label in ["Red box", "Green box"] {
            let mut ctx = harness.ctx_on(program, &host);
            let request =
                Value::record("PackRequest", vec![(Rc::from("label"), Value::text(label))]);
            match program.run(&mut ctx, id, vec![request]).await {
                Outcome::Value(_) => {}
                other => panic!("the parcel is packed: {other:?}"),
            }
            let work = ctx.take_deferred();
            drop(ctx);
            run_deferred(program, &host, work).await;
        }
        let mut states = deliveries(&harness).await;
        states.sort();
        assert_eq!(
            states,
            vec![
                ("failed".to_string(), 1, Some("misconfigured".to_string())),
                ("failed".to_string(), 1, Some("refused".to_string())),
            ]
        );
        harness.set_now(T0 + HOUR);
        sweep(program, &host, "main", T0 + HOUR)
            .await
            .expect("the sweep runs");
        assert_eq!(hooks.received().len(), 2, "neither sent again");
    });
}

/// Built as a native deployable, the desk posts each packed parcel to the
/// receiver itself, signed with the secret its environment gives, and
/// again, under the same id, after the receiver's `503`, on its own tick.
#[test]
fn a_native_deployable_publishes_signed_and_retries() {
    if !jadpo_test::live() {
        return;
    }
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let base = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/deployable");
    let root = base.join("publishing-desk");
    for (path, text) in files() {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    // Made afresh each run, so a cached project's old migration can't
    // disagree with entities this test has since gained.
    let _ = fs::remove_dir_all(root.join("database"));
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let out = jadpo(&root, args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    let output = run(
        &["build".to_string(), "--json".to_string()],
        &crate::deployable::at(&root, &crate::deployable::shared_cache()),
    );
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);

    let server = jadpo_test::postgres::Server::start().expect("a server");
    let connection = server
        .create_database("publishing_desk")
        .expect("a database");
    crate::deployable::apply(&connection, &crate::deployable::migrations(&root));
    let url = crate::deployable::as_url(&connection);
    let hooks = FakeHooks::start(&[503]);
    let secret = made_secret(41);
    let address = format!("{}/hooks/dispatch", hooks.url);
    let settings = [
        ("DATABASE_URL", url.as_str()),
        ("PARTNER_HOOK_URL", address.as_str()),
        ("PARTNER_HOOK_SECRET", secret.as_str()),
        ("HOST", "127.0.0.1"),
        ("PORT", "0"),
    ];
    let binary = root.join("build/target/app/app");
    let (mut child, lines) = crate::deployable::start(&binary, &settings);
    let started = crate::deployable::next(&lines, 60);
    assert_eq!(started["event"], "runtime.started", "{started}");
    assert_eq!(started["unbuilt"], serde_json::json!([]), "{started}");
    let served = started["address"]
        .as_str()
        .expect("its address")
        .to_string();

    let (status, body) =
        crate::deployable::exchange(&served, "POST", "/parcels", r#"{"label":"Blue box"}"#);
    assert_eq!(status, 201, "{body}");
    // Posted after the response, then again on a tick once its backoff,
    // at most 30 seconds, has passed: a tick comes each minute.
    let received = hooks.wait_for(2, 180);
    assert_eq!(received.len(), 2, "{received:?}");
    let now = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| {
                i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
            })
    };
    let (first, first_body) = announced(&received[0], &secret, "Blue box", now());
    let (again, again_body) = announced(&received[1], &secret, "Blue box", now());
    assert_eq!(first, again, "sent again under its id");
    assert_eq!(first_body, again_body);
    let outbox = crate::deployable::rows(&connection, "SELECT id::text FROM jadpo_outbox");
    assert_eq!(outbox, vec![vec![first.clone()]]);
    // Its record is settled once the receiver's answer is read.
    let settled = || {
        crate::deployable::rows(
            &connection,
            "SELECT status, attempts::text FROM jadpo_deliveries",
        )
    };
    assert!(
        crate::disk_files::eventually(30, || settled()
            == vec![vec!["succeeded".to_string(), "2".to_string()]]),
        "{:?}",
        settled()
    );
    let stopped = std::process::Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(stopped.success());
    assert_eq!(crate::deployable::ended(&mut child, 30), Some(0));
}

/// `jadpo dev` logs each published event in place of sending it, as its
/// inbox catches mail: its line names the event's id, its type and its
/// connection, never its payload, and nothing reaches the address its
/// settings give.
#[test]
fn jadpo_dev_logs_each_published_event_and_sends_nothing() {
    if !jadpo_test::live() {
        return;
    }
    let hooks = FakeHooks::start(&[]);
    let root = project("dev");
    // The address is given; the secret is set nowhere, and `jadpo dev`
    // starts without it, since nothing is sent.
    fs::write(
        root.join(".env.local"),
        format!("PARTNER_HOOK_URL={}/hooks/dispatch\n", hooks.url),
    )
    .expect("write");
    let dev = crate::http::Dev::start(&root);
    let packed = dev.send("POST", "/parcels", &[], r#"{"label":"Blue box"}"#);
    assert_eq!(packed.status, 201, "{packed:?}");
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !dev.log().contains("\"event\":\"published\"") {
        assert!(std::time::Instant::now() < until, "{}", dev.log());
        std::thread::park_timeout(std::time::Duration::from_millis(50));
    }
    let log = dev.log();
    let line: serde_json::Value = log
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .map(|line| line["data"].clone())
        .find(|data| data["event"] == "published")
        .expect("the published line");
    assert_eq!(line["type"], "dispatch.ParcelPacked", "{line}");
    assert_eq!(line["connection"], "partner_hooks", "{line}");
    assert_eq!(line["id"].as_str().map(str::len), Some(36), "{line}");
    assert!(!log.contains("Blue box"), "{log}");
    assert!(hooks.received().is_empty(), "{:?}", hooks.received());
}

/// The desk, its parcels kept with an audit trail whose entries it also
/// publishes.
fn audited_files() -> Vec<(String, String)> {
    files()
        .into_iter()
        .map(|(path, text)| {
            let text = match path.as_str() {
                "app.jadpo" => text.replace(
                    "events: [ParcelPacked]",
                    "events: [ParcelPacked, AuditEntryRecorded]",
                ),
                "domains/dispatch/entities/parcel.jadpo" => {
                    text.replace("\n    policy: {", "\n    audit: {}\n\n    policy: {")
                }
                _ => text,
            };
            (path, text)
        })
        .collect()
}

/// Each audit entry is published as `jadpo.AuditEntryRecorded` once its
/// change commits, beside the action's own event, signed, its data the
/// entry's fields.
#[test]
fn an_audit_entry_is_published_beside_the_event() {
    let hooks = FakeHooks::start(&[]);
    let secret = made_secret(53);
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("publishing-audited");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in audited_files() {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    for args in [
        &["migrate", "new", "initial"][..],
        &["records", "update"],
        &["check"],
    ] {
        let out = jadpo(&root, args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    if !jadpo_test::live() {
        return;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let prepared = Prepared::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let address = format!("{}/hooks/dispatch", hooks.url);
    let database = crate::deployable::as_url(&url);
    let given = secret.clone();
    let variables = move |name: &str| -> Option<String> {
        match name {
            "DATABASE_URL" => Some(database.clone()),
            "PARTNER_HOOK_URL" => Some(address.clone()),
            "PARTNER_HOOK_SECRET" => Some(given.clone()),
            _ => None,
        }
    };
    let values = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .expect("the settings read");
    let program = Program::new(prepared).with_config(&values);
    let harness = Harness::postgres(&url).at(T0).random_from(0);
    let driver: Rc<dyn Capability> = Rc::new(
        Webhook::new(
            Rc::new(NativeFetch::new()) as Rc<dyn Fetch>,
            Rc::new(|| T0),
            &format!("{}/hooks/dispatch", hooks.url),
            &secret,
        )
        .expect("a webhook"),
    );
    let host = Faked::new(&harness).with("partner_hooks", driver);
    Runtime::new().expect("runtime").block_on(async {
        let id = program.plan().find("pack_parcel").expect("the action");
        let mut ctx = harness.ctx_on(&program, &host);
        let request = Value::record(
            "PackRequest",
            vec![(Rc::from("label"), Value::text("Blue box"))],
        );
        match program.run(&mut ctx, id, vec![request]).await {
            Outcome::Value(_) => {}
            other => panic!("the parcel is packed: {other:?}"),
        }
        let work = ctx.take_deferred();
        drop(ctx);
        run_deferred(&program, &host, work).await;
    });
    let received = hooks.received();
    assert_eq!(received.len(), 2, "{received:?}");
    let bodies: Vec<serde_json::Value> = received
        .iter()
        .map(|one| {
            let signed = jadpo_runtime::webhooks::check(
                &jadpo_std::provider::SignatureScheme::StandardWebhooks,
                &[secret.as_str()],
                &one.as_request(),
                T0,
            );
            assert!(matches!(signed, Ok(Some(_))), "signed: {signed:?}");
            serde_json::from_slice(&one.body).expect("JSON")
        })
        .collect();
    let entry = bodies
        .iter()
        .find(|body| body["type"] == "jadpo.AuditEntryRecorded")
        .expect("the audit entry");
    assert_eq!(entry["data"]["entity"], "Parcel", "{entry}");
    assert_eq!(entry["data"]["change"], "created", "{entry}");
    assert!(entry["data"]["record_id"].is_string(), "{entry}");
    assert!(entry["data"]["at"].is_string(), "{entry}");
    assert!(
        bodies
            .iter()
            .any(|body| body["type"] == "dispatch.ParcelPacked"),
        "{bodies:?}"
    );
}
